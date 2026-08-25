// Concern: declares the command-line surface and turns one invocation into a rendered report and an exit code | Non-concern: walking, scanning or judging | IO: (argv, stdout) -> exit code

use std::path::{Path, PathBuf};

use anyhow::{Result, bail};
use clap::{Parser, ValueEnum};
use serde::Serialize;

use crate::config::{CONFIG_FILE, Config};
use crate::diagnostic::Level;
use crate::engine::{Engine, FileStat, Report};

/// Exit codes an agent branches on, from the CLI standard.
const EXIT_VALIDATION: i32 = 3;
const EXIT_NOT_FOUND: i32 = 24;
pub const EXIT_BAD_ARGS: i32 = 2;
const EXIT_INTERNAL: i32 = 1;

const AFTER_HELP: &str = r##"EXAMPLES
  comment-crusher .                                  measure a tree
  comment-crusher src/parser.rs --format json        one file, for a machine
  comment-crusher . --stats                          per-language totals
  comment-crusher . --allow 'docs/**/*.md' doc-length.max_lines=2000

OUTPUT (--format json)
  {"status":"success"|"error",
   "error":{"code":..., "message":...},
   "data":{"files":[{path,language,prose,lines,code_chars,comment_chars}],
           "languages":[{language,files,lines,comment_chars,code_chars}],
           "diagnostics":[{code,severity,message,location,help}],
           "pagination":{"files":{count,has_more},"diagnostics":{count,has_more}}}}

  `error` appears only when status is error. A field with no value is omitted, never null.
  `comment_chars` counts every comment in a file; a rule may judge a subset and says so.
  `location.span` is a byte offset and length; `location.start`/`end` are 1-based line and
  character column.

EXIT CODES
  0   nothing over budget
  2   bad_arguments: argv itself was rejected
  3   validation_error: a file is over budget, or the configuration is invalid
  24  not_found: a path does not exist
  1   internal error

LANGUAGE TABLE (crates/comment-crusher/src/default_config.toml)
  A file resolves by exact filename, then extension, then the `#!` interpreter. Per entry:
    line / doc_line      line-comment markers; doc_line wins when both match
    exceptions           text that cancels a comment: `#[` is a PHP attribute, `{$` a
                         Pascal directive
    line_anchored        a line comment only opens where nothing but whitespace precedes it,
                         so a URL cannot open one where there are no strings to hide in
    block / doc_block    [open, close] pairs
    nested_block         `/* /* */ */` closes once, not twice
    strings              [open, close] regions whose contents are CODE, not comment
      multiline          may cross a newline; without it a bad open self-heals at end of line
      docstring          opening a line makes it a doc comment (Python, Elixir)
      char_literal       only a string if it closes within a few chars on the same line
    hash_raw_strings     r"..." and r#"..."# raw strings
    heredoc              <<WORD and <<-'WORD' bodies are code until a line equal to WORD
    prose                measured by doc-length, never by comment-ratio
    embed / embed_use    a region holding another language, inline or from [embed_sets]
      open / close       delimiters; one starting with `<` is a tag, body starts past the `>`
      default            the child language when no attribute names one
      attrs / map        tag attributes that may name the child, and value -> language
      at_start           matches only at byte zero, which is what makes a `---` fence safe
      balanced           ends at the `close` balancing this `open`, not the first one
      skip               text after `open` that cancels the match: `{#if` is a directive

SEE ALSO
  .comment-crusher.toml            the repo's budget and its allowances"##;

#[derive(Copy, Clone, PartialEq, Eq, ValueEnum)]
pub enum Format {
    /// Findings one per line, then a summary.
    Human,
    /// One envelope an agent can branch on.
    Json,
}

#[derive(Parser)]
#[command(
    name = "comment-crusher",
    disable_version_flag = true,
    about = "Language-agnostic comment budget.",
    long_about = "Measures the comment characters, the longest single comment, and the length \
of every document under the paths given, and fails the ones over budget.\n\n\
The budget lives in .comment-crusher.toml, found by walking up from the target, so one repo \
answer holds whether the tool runs in CI, in a pre-commit hook, or against a single file an \
agent just edited.",
    after_help = AFTER_HELP,
    after_long_help = AFTER_HELP
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

    /// Directory globs and reported paths are relative to. Defaults to the directory the
    /// budget file was found in, so one answer holds from anywhere.
    #[arg(long, value_name = "DIR")]
    pub root: Option<PathBuf>,

    /// How to render the report.
    #[arg(long, value_enum, default_value_t = Format::Human)]
    pub format: Format,

    /// Print per-language totals instead of findings. Human format only; JSON always carries
    /// them under `data.languages`.
    #[arg(long)]
    pub stats: bool,

    /// Exit nonzero on a warning as well as an error.
    #[arg(long)]
    pub warnings_as_errors: bool,

    /// Print the version and exit. Read before any other argument, so a rejected invocation
    /// can still say what it is; `main` answers it before clap runs.
    #[arg(short = 'V', long)]
    pub version: bool,
}

#[derive(Serialize)]
struct ErrorBody {
    code: String,
    message: String,
}

/// Per collection, because one `has_more` shared between two says nothing about either. A
/// run reports whole trees, so neither is ever truncated.
#[derive(Serialize)]
struct Page {
    count: usize,
    has_more: bool,
}

#[derive(Serialize)]
struct Pagination {
    files: Page,
    diagnostics: Page,
}

#[derive(Serialize)]
struct LanguageTotal<'a> {
    language: &'a str,
    files: usize,
    lines: usize,
    comment_chars: usize,
    code_chars: usize,
}

#[derive(Serialize)]
struct Data<'a> {
    files: &'a [FileStat],
    languages: Vec<LanguageTotal<'a>>,
    diagnostics: &'a [crate::Diagnostic],
    pagination: Pagination,
}

#[derive(Serialize)]
struct Envelope<'a> {
    status: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<ErrorBody>,
    data: Data<'a>,
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

    /// Renders its own failures, so a JSON caller gets an envelope on stdout rather than prose
    /// on stderr it cannot parse.
    pub fn run(&self) -> i32 {
        match self.report() {
            Ok(report) => self.print(&report),
            Err((code, e)) => self.print_error(code, &format!("{e:#}")),
        }
    }

    pub fn version_only(json: bool) -> i32 {
        let version = env!("CARGO_PKG_VERSION");
        if json {
            println!(
                "{}",
                serde_json::json!({
                    "status": "success",
                    "data": { "name": "comment-crusher", "version": version },
                })
            );
        } else {
            println!("comment-crusher {version}");
        }
        0
    }

    fn report(&self) -> Result<Report, (&'static str, anyhow::Error)> {
        for p in &self.paths {
            if !p.exists() {
                return Err((
                    "not_found",
                    anyhow::anyhow!("no such path: {}", p.display()),
                ));
            }
        }
        let allow = self.allowances().map_err(|e| ("validation_error", e))?;
        let anchor = self
            .paths
            .first()
            .cloned()
            .unwrap_or_else(|| PathBuf::from("."));
        if let Some(path) = &self.config
            && !path.exists()
        {
            return Err((
                "not_found",
                anyhow::anyhow!("no such config: {}", path.display()),
            ));
        }
        let config = match Config::load(&anchor, self.config.as_deref(), &allow) {
            Ok(config) => config,
            Err(e) => {
                let path = Config::source_path(&anchor, self.config.as_deref())
                    .unwrap_or_else(|| PathBuf::from(CONFIG_FILE));
                return Ok(Report {
                    files: Vec::new(),
                    diagnostics: vec![config_diagnostic(&path, &format!("{e:#}"))],
                });
            }
        };
        Ok(Engine::new(&config, self.root.as_deref()).run(&self.paths))
    }

    fn print(&self, report: &Report) -> i32 {
        let rejected = match report.worst() {
            Some(Level::Deny) => true,
            Some(Level::Warn) => self.warnings_as_errors,
            _ => false,
        };
        match self.format {
            Format::Json => self.print_json(report, rejected),
            Format::Human if self.stats => {
                print_stats(report);
                i32::from(rejected) * EXIT_VALIDATION
            }
            Format::Human => {
                print_findings(report);
                i32::from(rejected) * EXIT_VALIDATION
            }
        }
    }

    fn print_json(&self, report: &Report, rejected: bool) -> i32 {
        let envelope = Envelope {
            status: if rejected { "error" } else { "success" },
            error: rejected.then(|| {
                let n = report
                    .diagnostics
                    .iter()
                    .filter(|d| d.level == Level::Deny || self.warnings_as_errors)
                    .count();
                ErrorBody {
                    code: "validation_error".to_string(),
                    message: format!("{n} findings over budget"),
                }
            }),
            data: Data {
                files: &report.files,
                languages: language_totals(report),
                diagnostics: &report.diagnostics,
                pagination: Pagination {
                    files: Page {
                        count: report.files.len(),
                        has_more: false,
                    },
                    diagnostics: Page {
                        count: report.diagnostics.len(),
                        has_more: false,
                    },
                },
            },
        };
        match serde_json::to_string_pretty(&envelope) {
            Ok(text) => {
                println!("{text}");
                i32::from(rejected) * EXIT_VALIDATION
            }
            Err(e) => self.print_error("internal_error", &e.to_string()),
        }
    }

    /// Every channel a caller reads is stdout, this one and clap's rejection alike.
    fn print_error(&self, code: &'static str, message: &str) -> i32 {
        match self.format {
            Format::Human => println!("comment-crusher: {message}"),
            Format::Json => println!("{}", error_json(code, message)),
        }
        match code {
            "not_found" => EXIT_NOT_FOUND,
            "bad_arguments" => EXIT_BAD_ARGS,
            "validation_error" => EXIT_VALIDATION,
            _ => EXIT_INTERNAL,
        }
    }
}

/// The one producer of a failed envelope.
///
/// `data` is therefore present on every reply the `--help` contract describes.
/// A configuration failure is a finding about a file.
///
/// So it reaches an agent as one, not as a multi-line blob it must read as prose.
pub fn config_diagnostic(path: &Path, message: &str) -> crate::Diagnostic {
    let first = message.lines().next().unwrap_or(message);
    crate::Diagnostic::new(
        "config",
        Level::Deny,
        path,
        first.to_string(),
        "Fix the configuration, or point --config at one that parses.",
    )
}

pub fn error_json(code: &str, message: &str) -> String {
    let envelope: Envelope<'_> = Envelope {
        status: "error",
        error: Some(ErrorBody {
            code: code.to_string(),
            message: message.to_string(),
        }),
        data: Data {
            files: &[],
            languages: Vec::new(),
            diagnostics: &[],
            pagination: Pagination {
                files: Page {
                    count: 0,
                    has_more: false,
                },
                diagnostics: Page {
                    count: 0,
                    has_more: false,
                },
            },
        },
    };
    // Every field is a string, number or bool, so this cannot fail.
    serde_json::to_string(&envelope).unwrap_or_default()
}

fn language_totals(report: &Report) -> Vec<LanguageTotal<'_>> {
    let mut names: Vec<&str> = report.files.iter().map(|f| f.language.as_str()).collect();
    names.sort_unstable();
    names.dedup();
    names
        .into_iter()
        .map(|language| {
            let (files, lines, comment_chars, code_chars) = report
                .files
                .iter()
                .filter(|f| f.language == language)
                .fold((0, 0, 0, 0), |(n, l, c, k), f| {
                    (n + 1, l + f.lines, c + f.comment_chars, k + f.code_chars)
                });
            LanguageTotal {
                language,
                files,
                lines,
                comment_chars,
                code_chars,
            }
        })
        .collect()
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
    println!(
        "\n{} files, {:.1}% comment ({comment}/{total} chars), {} findings",
        report.files.len(),
        percent(comment, total),
        report.diagnostics.len()
    );
}

fn print_stats(report: &Report) {
    println!(
        "{:<16} {:>6} {:>7} {:>10} {:>10} {:>7}",
        "language", "files", "lines", "comment", "code", "share"
    );
    for t in language_totals(report) {
        let prose = report
            .files
            .iter()
            .any(|f| f.language == t.language && f.prose);
        let share = if prose {
            "  prose".to_string()
        } else {
            format!(
                "{:>6.1}%",
                percent(t.comment_chars, t.comment_chars + t.code_chars)
            )
        };
        println!(
            "{:<16} {:>6} {:>7} {:>10} {:>10} {share}",
            t.language, t.files, t.lines, t.comment_chars, t.code_chars
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
