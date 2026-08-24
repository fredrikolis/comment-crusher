// Concern: walks the targets and collects a finding and a total for every file measured | Non-concern: what a rule measures, or how a report prints | IO: (targets, Config) -> Report

use std::path::{Path, PathBuf};

use anyhow::Result;
use ignore::WalkBuilder;
use rayon::prelude::*;
use serde::Serialize;

use crate::config::Config;
use crate::diagnostic::{Diagnostic, Level};
use crate::scan::scan;

#[derive(Debug, Clone, Serialize)]
pub struct FileStat {
    pub path: PathBuf,
    pub language: String,
    pub lines: usize,
    /// A prose document has no code, so its whole weight is reported here and no ratio is taken.
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

    /// Comment and code characters across the code files only. A document has no code, so
    /// including one would make the ratio a statement about how much prose the repo ships.
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
    pub fn new(config: &'a Config, root: &Path) -> Self {
        Self {
            config,
            root: root.canonicalize().unwrap_or_else(|_| root.to_path_buf()),
        }
    }

    pub fn run(&self, targets: &[PathBuf]) -> Result<Report> {
        let mut report = self
            .collect(targets)
            .par_iter()
            .filter_map(|f| self.check(f))
            .fold(Report::default, |mut acc, (stat, diags)| {
                acc.files.push(stat);
                acc.diagnostics.extend(diags);
                acc
            })
            .reduce(Report::default, |mut a, b| {
                a.files.extend(b.files);
                a.diagnostics.extend(b.diagnostics);
                a
            });
        report.files.sort_by(|a, b| a.path.cmp(&b.path));
        report.diagnostics.sort_by(|a, b| {
            a.file
                .cmp(&b.file)
                .then(a.line.cmp(&b.line))
                .then(a.rule.cmp(b.rule))
        });
        Ok(report)
    }

    /// A named file is measured whatever the walk would have said about it: pointing the tool
    /// at one path is an explicit instruction, not a suggestion.
    fn collect(&self, targets: &[PathBuf]) -> Vec<PathBuf> {
        let mut out = Vec::new();
        for t in targets {
            if t.is_file() {
                out.push(t.clone());
                continue;
            }
            let mut b = WalkBuilder::new(t);
            b.hidden(false).git_ignore(true).require_git(false);
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
        self.relative(path).components().any(|c| {
            let name = c.as_os_str();
            name == ".git" || self.config.exclude.iter().any(|e| name == e.as_str())
        })
    }

    fn relative(&self, path: &Path) -> PathBuf {
        path.canonicalize()
            .ok()
            .and_then(|p| p.strip_prefix(&self.root).ok().map(Path::to_path_buf))
            .unwrap_or_else(|| path.to_path_buf())
    }

    fn check(&self, file: &Path) -> Option<(FileStat, Vec<Diagnostic>)> {
        let content = std::fs::read_to_string(file).ok()?;
        let syn = self.config.language(file).or_else(|| {
            self.config
                .language_of_shebang(content.lines().next().unwrap_or_default())
        })?;
        if !syn.measurable() {
            return None;
        }
        let rel = self.relative(file);
        let (rules, reasons) = self.config.rules_for(&rel).ok()?;
        let result = scan(&content, syn);
        let allowance = (!reasons.is_empty()).then(|| reasons.join("; "));
        let stat = FileStat {
            path: rel.clone(),
            language: syn.name.clone(),
            prose: syn.prose,
            lines: result.total_lines,
            code_chars: result.code_chars,
            comment_chars: result.comment_chars(true, false),
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
