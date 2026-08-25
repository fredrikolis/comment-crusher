// Concern: walks the targets and collects a finding and a total for every file measured | Non-concern: what a rule measures, or how a report prints | IO: (targets, Config) -> Report

use std::path::{Path, PathBuf};

use anyhow::Result;
use ignore::WalkBuilder;
use rayon::prelude::*;
use serde::Serialize;

use crate::config::Config;
use crate::diagnostic::{Diagnostic, Level};
use crate::scan::scan_in;
use crate::syntax::Syntax;

/// A NUL means an encoding this tool does not read: a binary, or UTF-16, which is text.
fn is_binary(bytes: &[u8]) -> bool {
    bytes.iter().take(8192).any(|b| *b == 0)
}

#[derive(Debug, Clone, Serialize)]
pub struct FileStat {
    pub path: PathBuf,
    pub language: String,
    pub lines: usize,
    /// A prose document has no code, so no ratio is taken.
    pub prose: bool,
    pub code_chars: usize,
    pub comment_chars: usize,
}

#[derive(Debug, Default, Serialize)]
pub struct Report {
    pub files: Vec<FileStat>,
    pub diagnostics: Vec<Diagnostic>,
}

impl Report {
    pub fn worst(&self) -> Option<Level> {
        self.diagnostics.iter().map(|d| d.level).max()
    }

    /// Code files only: including a document would make this a statement about prose volume.
    pub fn totals(&self) -> (usize, usize) {
        self.files
            .iter()
            .filter(|f| !f.prose)
            .fold((0, 0), |(c, k), f| (c + f.comment_chars, k + f.code_chars))
    }
}

pub struct Engine<'a> {
    config: &'a Config,
    root: PathBuf,
}

impl<'a> Engine<'a> {
    /// Without `--root` the budget's own directory is the base, so one repo answer holds.
    pub fn new(config: &'a Config, override_root: Option<&Path>) -> Self {
        let root = override_root.unwrap_or_else(|| config.root());
        Self {
            config,
            root: root.canonicalize().unwrap_or_else(|_| root.to_path_buf()),
        }
    }

    pub fn run(&self, targets: &[PathBuf]) -> Report {
        let checked: Vec<_> = self
            .collect(targets)
            .par_iter()
            .filter_map(|f| self.check(f))
            .collect();
        let mut report = checked
            .into_iter()
            .fold(Report::default(), |mut acc, (stat, diags)| {
                acc.files.push(stat);
                acc.diagnostics.extend(diags);
                acc
            });
        report.files.sort_by(|a, b| a.path.cmp(&b.path));
        report.diagnostics.sort_by(|a, b| {
            a.file
                .cmp(&b.file)
                .then(a.line.cmp(&b.line))
                .then(a.rule.cmp(b.rule))
        });
        let measured: Vec<PathBuf> = report.files.iter().map(|f| f.path.clone()).collect();
        for glob in self.config.argv_globs_matching_none(&measured) {
            // The tree that was measured, not the glob: `location.file` is a path to open.
            report.diagnostics.push(Diagnostic::new(
                "allowance.unused",
                Level::Warn,
                self.config.root(),
                format!(
                    "--allow `{glob}` matched none of the files measured, so it widened nothing"
                ),
                "Write the glob relative to the budget file, or to where you invoked the tool.",
            ));
        }
        report
    }

    /// A named file is measured whatever the walk would have said: naming it is an instruction.
    fn collect(&self, targets: &[PathBuf]) -> Vec<PathBuf> {
        let mut out = Vec::new();
        for t in targets {
            if t.is_file() {
                out.push(t.clone());
                continue;
            }
            let mut b = WalkBuilder::new(t);
            b.hidden(false).git_ignore(true).require_git(false);
            // Pruned, not filtered afterwards: a committed vendor tree is never descended.
            let names: Vec<String> = self.config.exclude.clone();
            b.filter_entry(move |e| {
                !e.file_type().is_some_and(|f| f.is_dir())
                    || !e
                        .path()
                        .file_name()
                        .is_some_and(|n| n == ".git" || names.iter().any(|x| n == x.as_str()))
            });
            out.extend(
                b.build()
                    .filter_map(Result::ok)
                    .filter(|e| e.file_type().is_some_and(|f| f.is_file()))
                    .map(ignore::DirEntry::into_path)
                    .filter(|p| !self.excluded(p)),
            );
        }
        out.sort();
        out.dedup();
        out
    }

    fn excluded(&self, path: &Path) -> bool {
        self.relative(path)
            .components()
            .any(|c| self.pruned_name(c.as_os_str()))
    }

    fn pruned_name(&self, name: &std::ffi::OsStr) -> bool {
        name == ".git" || self.config.exclude.iter().any(|e| name == e.as_str())
    }

    /// Never fatal: one bad file must not cost the whole report.
    fn unreadable(
        &self,
        file: &Path,
        syn: &Syntax,
        error: &std::io::Error,
    ) -> (FileStat, Vec<Diagnostic>) {
        let rel = self.relative(file);
        let (rules, _) = self.config.rules_for(&rel);
        let diags = crate::rules::unreadable::check(&rules.unreadable, &rel, error)
            .into_iter()
            .collect();
        (Self::empty_stat(rel, syn), diags)
    }

    fn binary(&self, file: &Path, syn: &Syntax) -> (FileStat, Vec<Diagnostic>) {
        let rel = self.relative(file);
        let (rules, _) = self.config.rules_for(&rel);
        let diags = crate::rules::unreadable::binary(&rules.unreadable, &rel)
            .into_iter()
            .collect();
        (Self::empty_stat(rel, syn), diags)
    }

    fn empty_stat(path: PathBuf, syn: &Syntax) -> FileStat {
        FileStat {
            path,
            language: syn.name.clone(),
            prose: syn.prose,
            lines: 0,
            code_chars: 0,
            comment_chars: 0,
        }
    }

    /// A path outside the budget's directory is returned whole and matches no allowance.
    fn relative(&self, path: &Path) -> PathBuf {
        path.canonicalize()
            .ok()
            .and_then(|p| p.strip_prefix(&self.root).ok().map(Path::to_path_buf))
            .unwrap_or_else(|| path.to_path_buf())
    }

    fn check(&self, file: &Path) -> Option<(FileStat, Vec<Diagnostic>)> {
        // A resolved path must be read or reported; an unresolved one is only a candidate.
        let (syn, content) = if let Some(syn) = self.config.language(file) {
            match std::fs::read(file) {
                Ok(bytes) if is_binary(&bytes) => return Some(self.binary(file, syn)),
                // Lossy on purpose: legacy-encoded source is still source, and markers are ASCII.
                Ok(bytes) => (syn, String::from_utf8_lossy(&bytes).into_owned()),
                Err(e) => return Some(self.unreadable(file, syn, &e)),
            }
        } else {
            let bytes = std::fs::read(file).ok()?;
            if is_binary(&bytes) {
                return None;
            }
            // Lossy here too: one decode policy, whichever way the language resolved.
            let text = String::from_utf8_lossy(&bytes).into_owned();
            let head = text.lines().next().unwrap_or_default();
            let syn = self.config.language_of_shebang(head)?;
            (syn, text)
        };
        if !syn.measurable() {
            return None;
        }
        let rel = self.relative(file);
        let (rules, reasons) = self.config.rules_for(&rel);
        let result = scan_in(&content, syn, self.config);
        let allowance = (!reasons.is_empty()).then(|| reasons.join("; "));
        let stat = FileStat {
            path: rel.clone(),
            language: syn.name.clone(),
            prose: syn.prose,
            lines: result.total_lines,
            code_chars: result.code_chars,
            comment_chars: result.charged_chars(true, None),
        };
        let diags = rules
            .check(&rel, syn, &result)
            .into_iter()
            .map(|mut d| {
                d.allowance.clone_from(&allowance);
                d
            })
            .collect();
        Some((stat, diags))
    }
}
