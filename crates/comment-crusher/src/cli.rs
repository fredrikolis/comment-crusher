// Concern: declares the command-line surface and turns one invocation into a rendered report and an exit code | Non-concern: walking, scanning or judging | IO: (argv, stdout) -> exit code

use std::path::{Path, PathBuf};

use anyhow::{Result, bail};
use clap::{Parser, ValueEnum};
use serde::Serialize;

use crate::config::{CONFIG_FILE, Config, LoadFailure, Located};
use crate::diagnostic::Level;
use crate::engine::{Engine, FileStat, Report};

/// Not TOML, and TOML the tool refuses, are different mistakes with different repairs.
const SYNTAX_RULE: &str = "config.syntax";
const REJECTED_RULE: &str = "config.rejected";

/// Exit codes an agent branches on, from the CLI standard.
pub const EXIT_BAD_ARGS: i32 = 2;
const EXIT_VALIDATION: i32 = 3;

/// The wire code and the exit code are one decision, so they are one value.
#[derive(Debug, Clone, Copy)]
pub enum Failure {
    BadArguments,
    NotFound,
    Internal,
}

impl Failure {
    const fn code(self) -> &'static str {
        match self {
            Self::BadArguments => "bad_arguments",
            Self::NotFound => "not_found",
            Self::Internal => "internal_error",
        }
    }

    const fn exit(self) -> i32 {
        match self {
            Self::BadArguments => EXIT_BAD_ARGS,
            Self::NotFound => 24,
            Self::Internal => 1,
        }
    }
}

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
           "diagnostics":[{code,severity,message,location,help,docs_url,allowance}],
           "pagination":{"files":{count,has_more,next_cursor},"diagnostics":{...}}},
   "meta":{"request_id":..., "timestamp":...}}

  `allowance` names the reason a bound was widened for the file, when one was.

  Two warnings are about the run, not about a file, and carry no `location.file`:
  `allowance.unused` for a --allow glob that matched none of the files measured, and
  `target.outside_budget` for a target the budget's directory does not contain. A budget file that is not TOML is `config.syntax`, with
  the byte range the parser pointed at; one the tool refuses is `config.rejected`.
  `error` appears only when status is error. A field with no value is omitted, never null.
  `comment_chars` counts every comment in a file; a rule may judge a subset and says so.
  `location.span` is a byte offset and length; `location.start`/`end` are 1-based line and
  character column.

EXIT CODES
  0   nothing over budget
  2   bad_arguments: argv itself was rejected, a --allow value included
  3   validation_error: a file is over budget, or the budget file was rejected
  24  not_found: a path does not exist
  1   internal error

LANGUAGE TABLE (crates/comment-crusher/src/default_config.toml)
  A file resolves by `filenames`, then `extensions`, then `interpreters` (the `#!`). Per entry:
    line / doc_line      line-comment markers; doc_line wins when both match
    exceptions           text that cancels a comment: `#[` is a PHP attribute, `{$` a
                         Pascal directive
    line_anchored        a line comment only opens where nothing but whitespace precedes it,
                         so a URL cannot open one where there are no strings to hide in
    block / doc_block    [open, close] pairs
    nested_block         `/* /* */ */` closes once, not twice
    strings              [open, close] regions whose contents are CODE, not comment
      multiline          may cross a newline; without it a bad open self-heals at end of line
      escape             what quotes a delimiter inside; a backslash unless set to ""
      docstring          opening a line makes it a doc comment (Python, Elixir)
      char_literal       only a string if it closes within a few chars on the same line
    doc_prefixes         what may precede a docstring and still leave it one: Elixir
                         writes `@moduledoc """`, never `"""` at the margin
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

    /// Widen a budget for the paths a glob matches, up to a hundredfold, e.g.
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

    /// Print the version envelope and exit.
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
    /// Absent while `has_more` is false, which is always.
    #[serde(skip_serializing_if = "Option::is_none")]
    next_cursor: Option<String>,
}

impl Page {
    const fn whole(count: usize) -> Self {
        Self {
            count,
            has_more: false,
            next_cursor: None,
        }
    }
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
    meta: Meta,
}

/// What a caller keys a run on in a log it reads later.
#[derive(Serialize)]
struct Meta {
    request_id: String,
    timestamp: u64,
}

impl Meta {
    /// The pid and the nanosecond: unique across runs, and no RNG to get there.
    fn now() -> Self {
        let since = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default();
        Self {
            request_id: format!("req_{:x}{:08x}", std::process::id(), since.subsec_nanos()),
            timestamp: since.as_secs(),
        }
    }
}

impl Cli {
    /// Read straight off argv, before clap: a rejected invocation still owes the caller an
    /// answer in the shape it asked for, and a version request outranks everything.
    pub fn wants_json() -> bool {
        Self::scan().0
    }

    pub fn version_request() -> bool {
        Self::scan().1
    }

    /// A word where an option's value goes is that value, not a flag of its own.
    fn scan() -> (bool, bool) {
        let (mut json, mut version) = (false, false);
        let mut args = std::env::args_os()
            .skip(1)
            .take_while(|a| a != "--")
            .map(|a| a.to_string_lossy().into_owned())
            .peekable();
        // Clap refuses a hyphen-led value, so one here is still the flag it looks like.
        let value = |it: &mut std::iter::Peekable<_>| -> Option<String> {
            it.next_if(|a: &String| !a.starts_with('-'))
        };
        while let Some(a) = args.next() {
            match a.as_str() {
                "--version" | "-V" => version = true,
                "--format=json" => json = true,
                "--format" => json = value(&mut args).as_deref() == Some("json"),
                "--allow" => {
                    value(&mut args);
                    value(&mut args);
                }
                "--config" | "--root" => {
                    value(&mut args);
                }
                _ => {}
            }
        }
        (json, version)
    }

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

    /// Renders its own failures: a JSON caller gets an envelope, never prose it cannot parse.
    pub fn run(&self) -> i32 {
        match self.report() {
            Ok(report) => self.print(&report),
            Err((code, e)) => self.print_error(code, &format!("{e:#}")),
        }
    }

    /// An envelope whatever the format: the standard fixes this reply's shape.
    pub fn version_only() -> i32 {
        let meta = Meta::now();
        println!(
            "{}",
            serde_json::json!({
                "status": "success",
                "data": { "name": "comment-crusher", "version": env!("CARGO_PKG_VERSION") },
                "meta": { "request_id": meta.request_id, "timestamp": meta.timestamp },
            })
        );
        0
    }

    fn report(&self) -> Result<Report, (Failure, anyhow::Error)> {
        for p in &self.paths {
            if !p.exists() {
                return Err((
                    Failure::NotFound,
                    anyhow::anyhow!("no such path: {}", p.display()),
                ));
            }
        }
        // On argv, so argv is what was wrong: an agent branching on 2 re-reads --help.
        let allow = self.allowances().map_err(|e| (Failure::BadArguments, e))?;
        let anchor = self
            .paths
            .first()
            .cloned()
            .unwrap_or_else(|| PathBuf::from("."));
        if let Some(path) = &self.config
            && !path.exists()
        {
            return Err((
                Failure::NotFound,
                anyhow::anyhow!("no such config: {}", path.display()),
            ));
        }
        let config = match Config::load(&anchor, self.config.as_deref(), &allow) {
            Ok(config) => config,
            // The caller's own invocation, not a fact about any file in the repo.
            Err(LoadFailure::Argv(e)) => return Err((Failure::BadArguments, e)),
            Err(other) => {
                let (code, e, span) = match other {
                    LoadFailure::Syntax(e, s) => (SYNTAX_RULE, e, Some(s)),
                    other => (REJECTED_RULE, other.into_error(), None),
                };
                let path = Config::source_path(&anchor, self.config.as_deref())
                    .unwrap_or_else(|| PathBuf::from(CONFIG_FILE));
                return Ok(Report {
                    files: Vec::new(),
                    diagnostics: vec![config_diagnostic(code, &path, &format!("{e:#}"), span)],
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
            error: rejected.then(|| ErrorBody {
                code: "validation_error".to_string(),
                message: self.rejection(report),
            }),
            data: Data {
                files: &report.files,
                languages: language_totals(report),
                diagnostics: &report.diagnostics,
                pagination: Pagination {
                    files: Page::whole(report.files.len()),
                    diagnostics: Page::whole(report.diagnostics.len()),
                },
            },
            meta: Meta::now(),
        };
        match serde_json::to_string_pretty(&envelope) {
            Ok(text) => {
                println!("{text}");
                i32::from(rejected) * EXIT_VALIDATION
            }
            Err(e) => self.print_error(Failure::Internal, &e.to_string()),
        }
    }

    /// Every channel a caller reads is stdout, this one and clap's rejection alike.
    /// Nothing was measured when the configuration was rejected.
    fn rejection(&self, report: &Report) -> String {
        if report
            .diagnostics
            .iter()
            .any(|d| d.rule.starts_with("config."))
        {
            return "the configuration was rejected".to_string();
        }
        let n = report
            .diagnostics
            .iter()
            .filter(|d| d.level == Level::Deny)
            .count();
        let warnings = report.diagnostics.len() - n;
        match (n, warnings) {
            (n, _) if !self.warnings_as_errors => format!("{n} findings over budget"),
            (0, w) => format!("{w} warnings, and warnings are errors"),
            (n, w) => format!("{n} findings over budget, and {w} warnings are errors"),
        }
    }

    fn print_error(&self, failure: Failure, message: &str) -> i32 {
        match self.format {
            Format::Human => println!("comment-crusher: {message}"),
            Format::Json => println!("{}", error_json(failure.code(), message)),
        }
        failure.exit()
    }
}

/// A configuration failure is a finding about a file.
///
/// So it reaches an agent as one, not as a multi-line blob it must read as prose.
fn config_diagnostic(
    rule: &'static str,
    path: &Path,
    message: &str,
    at: Option<Located>,
) -> crate::Diagnostic {
    let first = message.lines().next().unwrap_or(message);
    let d = crate::Diagnostic::new(
        rule,
        Level::Deny,
        path,
        first.to_string(),
        "Fix the configuration, or point --config at one that parses.",
    );
    let Some(at) = at else {
        return d;
    };
    d.at(at.start.0)
        .spanning(at.offset, at.offset + at.length, at.end.0)
        .columns(at.start.1, at.end.1)
}

/// The envelope for a run that produced no report at all, so `data` is empty rather than
/// absent: every reply carries the shape the `--help` contract describes.
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
                files: Page::whole(0),
                diagnostics: Page::whole(0),
            },
        },
        meta: Meta::now(),
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
    // The share is over code files, so that is what the count beside it names.
    let docs = report.files.iter().filter(|f| f.prose).count();
    let also = if docs == 0 {
        String::new()
    } else {
        format!(" and {docs} documents")
    };
    println!(
        "\n{} code files{also}, {:.1}% comment ({comment}/{total} chars), {} findings",
        report.files.len() - docs,
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
