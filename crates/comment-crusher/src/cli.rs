// Concern: declares the command-line surface and turns one invocation into a printed report and an exit code | Non-concern: walking, scanning or judging | IO: (argv, stdout/stderr) -> exit code

use std::path::PathBuf;

use anyhow::{Result, bail};
use clap::{Parser, ValueEnum};

use crate::config::Config;
use crate::diagnostic::Level;
use crate::engine::{Engine, Report};

#[derive(Copy, Clone, PartialEq, Eq, ValueEnum)]
pub enum Format {
    Human,
    Json,
}

#[derive(Parser)]
#[command(
    name = "comment-crusher",
    version,
    about = "Language-agnostic comment budget.",
    long_about = "Measures the comment characters, the longest single comment, and the length of \
every document under the paths given, and fails the ones over budget.\n\n\
The budget lives in .comment-crusher.toml, found by walking up from the target, so one repo \
answer holds whether the tool runs in CI, in a pre-commit hook, or against a single file an \
agent just edited."
)]
pub struct Cli {
    /// Files or directories to measure.
    #[arg(default_value = ".")]
    pub paths: Vec<PathBuf>,

    /// Config file to use instead of the nearest .comment-crusher.toml.
    #[arg(long, value_name = "FILE")]
    pub config: Option<PathBuf>,

    /// Widen a budget for the paths a glob matches, e.g.
    /// --allow 'docs/**/*.md' doc-length.max_lines=2000
    #[arg(long, num_args = 2, value_names = ["GLOB", "RULE.FIELD=VALUE"], action = clap::ArgAction::Append)]
    pub allow: Vec<String>,

    /// Directory globs and reported paths are relative to. Defaults to the working directory.
    #[arg(long, value_name = "DIR")]
    pub root: Option<PathBuf>,

    #[arg(long, value_enum, default_value_t = Format::Human)]
    pub format: Format,

    /// Print per-language totals instead of findings.
    #[arg(long)]
    pub stats: bool,

    /// Exit nonzero on a warning as well as an error.
    #[arg(short = 'W', long)]
    pub warnings_as_errors: bool,
}

impl Cli {
    fn allowances(&self) -> Result<Vec<(String, String)>> {
        if !self.allow.len().is_multiple_of(2) {
            bail!("--allow takes two values: <GLOB> <RULE.FIELD=VALUE>");
        }
        Ok(self
            .allow
            .chunks_exact(2)
            .map(|p| (p[0].clone(), p[1].clone()))
            .collect())
    }

    pub fn run(&self) -> Result<i32> {
        let root = self.root.clone().map_or_else(std::env::current_dir, Ok)?;
        for p in &self.paths {
            if !p.exists() {
                bail!("no such path: {}", p.display());
            }
        }
        let anchor = self.paths.first().cloned().unwrap_or_else(|| root.clone());
        let config = Config::load(&anchor, self.config.as_deref(), &self.allowances()?)?;
        let report = Engine::new(&config, &root).run(&self.paths)?;
        self.print(&report);
        Ok(self.exit_code(&report))
    }

    fn print(&self, report: &Report) {
        match (self.format, self.stats) {
            (Format::Json, _) => println!(
                "{}",
                serde_json::to_string_pretty(report).unwrap_or_else(|_| "{}".into())
            ),
            (Format::Human, true) => print_stats(report),
            (Format::Human, false) => print_findings(report),
        }
    }

    fn exit_code(&self, report: &Report) -> i32 {
        match report.worst() {
            Some(Level::Deny) => 1,
            Some(Level::Warn) if self.warnings_as_errors => 1,
            _ => 0,
        }
    }
}

fn print_findings(report: &Report) {
    for d in &report.diagnostics {
        let note = d
            .allowance
            .as_ref()
            .map_or_else(String::new, |a| format!(" (allowance: {a})"));
        println!("{}{note}", d.human());
    }
    let (comment, code) = report.totals();
    let total = comment + code;
    let pct = percent(comment, total);
    println!(
        "\n{} files, {pct:.1}% comment ({comment}/{total} chars), {} findings",
        report.files.len(),
        report.diagnostics.len()
    );
}

fn print_stats(report: &Report) {
    let mut langs: Vec<&str> = report.files.iter().map(|f| f.language.as_str()).collect();
    langs.sort_unstable();
    langs.dedup();
    println!(
        "{:<16} {:>6} {:>7} {:>10} {:>10} {:>7}",
        "language", "files", "lines", "comment", "code", "share"
    );
    for lang in langs {
        let files: Vec<_> = report.files.iter().filter(|f| f.language == lang).collect();
        let prose = files.first().is_some_and(|f| f.prose);
        let (lines, comment, code) = files.iter().fold((0, 0, 0), |(l, c, k), f| {
            (l + f.lines, c + f.comment_chars, k + f.code_chars)
        });
        let share = if prose {
            "  prose".to_string()
        } else {
            format!("{:>6.1}%", percent(comment, comment + code))
        };
        println!(
            "{lang:<16} {:>6} {lines:>7} {comment:>10} {code:>10} {share}",
            files.len()
        );
    }
}

#[expect(
    clippy::cast_precision_loss,
    reason = "character counts are far below f64 precision"
)]
fn percent(part: usize, whole: usize) -> f64 {
    if whole == 0 {
        0.0
    } else {
        part as f64 / whole as f64 * 100.0
    }
}
