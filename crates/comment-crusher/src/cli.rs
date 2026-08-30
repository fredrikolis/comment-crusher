// Concern: declares the command-line surface and turns one invocation into a rendered report and an exit code | Non-concern: walking, scanning or judging | IO: (argv, stdout) -> exit code

use std::path::{Path, PathBuf};

use anyhow::{Result, bail};
use clap::{Parser, Subcommand, ValueEnum};
use serde::Serialize;

use crate::config::{CONFIG_FILE, Config, LoadFailure, Located};
use crate::diagnostic::Level;
use crate::engine::{Engine, FileStat, Report};
use crate::exit::{EXIT_BAD_ARGS, EXIT_INTERNAL, EXIT_NOT_FOUND, EXIT_VALIDATION, say};

/// Not TOML, and TOML the tool refuses, are different mistakes with different repairs.
const SYNTAX_RULE: &str = "config.syntax";
const REJECTED_RULE: &str = "config.rejected";

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
            Self::NotFound => EXIT_NOT_FOUND,
            Self::Internal => EXIT_INTERNAL,
        }
    }
}

const AFTER_HELP: &str = r##"EXAMPLES
  comment-crusher .                                  measure a tree
  comment-crusher src/parser.rs --format json        one file, for a machine
  comment-crusher src/parser.rs --format editor      one file, as an editor locates it
  comment-crusher . --stats                          per-language totals
  comment-crusher . --allow 'docs/**/*.md' doc-length.max_lines=2000

OUTPUT (--format editor)
  path:line:column: severity[rule]: message
    help: what to do about it
  One finding per pair of lines, in the shape a problem matcher, a language server client
  and an agent already parse. A finding about a whole file carries no line and column, one
  about the run no path either; the message ends in `(allowance: ...)` where a bound was
  widened for it. Nothing else is printed, so silence is a clean run.

OUTPUT (--format json)
  {"status":"success"|"error",
   "error":{"code":..., "message":...},
   "data":{"files":[{path,language,prose,lines,code_chars,comment_chars}],
           "languages":[{language,files,lines,comment_chars,code_chars}],
           "diagnostics":[{code,severity,message,location,help,docs_url,allowance}],
           "pagination":{"files":{count,has_more,next_cursor},"languages":{...},...}},
   "meta":{"request_id":..., "timestamp":...}}
  `--version` answers the same envelope with `data` of `{name, version}` and no report, and
  `--config-guide` with `data` of `{guide, shipped}`: the text, and the shipped table itself.

  `allowance` names the reason a bound was widened for the file, when one was.

  Three warnings are about the run, not about a file, and carry no `location` at all:
  `allowance.unused` for a --allow glob that matched none of the files measured,
  `target.outside_budget` for a target the budget's directory does not contain, and
  `target.excluded` for one [global] exclude names, which no walk would have reached.
  A budget file that is not TOML is `config.syntax`, carrying the byte range the parser
  pointed at; one the tool refuses afterwards is `config.rejected`.
  `error` appears only when status is error. A field with no value is omitted, never null,
  except `next_cursor`: the standard names it, and one reply leaves no page after it.
  `comment_chars` counts every comment in a file; a rule may judge a subset and says so.
  `location.span` is a byte offset and length; `location.start`/`end` are 1-based line and
  character column.

VERBS
  install-hook --claude [FILE]     add the PostToolUse entry to a settings file, ours only
  hook --claude                    what that entry runs, one event at a time
  Each verb's own --help carries its contract; both answer in the format the run asked for.

EXIT CODES
  0   nothing over budget
  2   bad_arguments: argv itself was rejected, a --allow value included
  3   validation_error: a file is over budget, or a budget or settings file was rejected
  24  not_found: a path does not exist
  1   internal error

LANGUAGE TABLE (crates/comment-crusher/src/default_config.toml)
  A file resolves by `filenames`, then `extensions`, then `interpreters` (the `#!`). Per entry:
    line / doc_line      line-comment markers; doc_line wins when both match
    exceptions           text that cancels a comment: `#[` is a PHP attribute, `{$` a
                         Pascal directive
    line_anchored        a line comment only opens where nothing but whitespace precedes it,
                         so a URL cannot open one where there are no strings to hide in
    open_after           text that opens one anyway, anchored or not: Tcl's `;`
    cancel_after         text before a line marker that cancels it: a CSS `//` after `:`
                         or `(` is an address, not a comment. Block markers are unaffected
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
  comment-crusher --config-guide   what a budget file may say, and what ships
  comment-crusher --version        the name and version, as an envelope
  comment-crusher --stats          per-language totals above the findings
  .comment-crusher.toml            the repo's budget and its allowances"##;

#[derive(Copy, Clone, PartialEq, Eq, ValueEnum)]
pub enum Format {
    /// Findings one per line, then a summary.
    Human,
    /// One envelope an agent can branch on.
    Json,
    /// `path:line:column: severity[rule]: message`, as an editor or an agent reads it.
    Editor,
}

#[derive(Parser)]
#[command(
    name = "comment-crusher",
    disable_version_flag = true,
    about = "Language-agnostic comment budget.",
    long_about = "Measures the comment characters, the longest single comment, and the length \
of every document under the paths given, and fails the ones over budget.\n\n\
The budget lives in .comment-crusher.toml, found by walking up from the target, so one repo \
answer holds whether the tool runs in CI, in a pre-commit hook, or against a single file. The \
`hook` verb reads the budget at the file's own git root instead, so a repo that declared none \
is never measured.",
    after_help = AFTER_HELP,
    after_long_help = AFTER_HELP
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,

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

    /// How to render the report. Also the shape a verb answers in, before or after it.
    #[arg(long, value_enum, default_value_t = Format::Human, global = true)]
    pub format: Format,

    /// Print per-language totals above the findings. Human format only; JSON always carries
    /// them under `data.languages`.
    #[arg(long)]
    pub stats: bool,

    /// Exit nonzero on a warning as well as an error.
    #[arg(long)]
    pub warnings_as_errors: bool,

    /// Print the version envelope and exit.
    #[arg(short = 'V', long)]
    pub version: bool,

    /// Print what a .comment-crusher.toml may say, every rule and global at its shipped value.
    #[arg(long)]
    pub config_guide: bool,
}

#[derive(Subcommand)]
pub enum Command {
    /// Add the `PostToolUse` entry to a settings file, or remove it.
    InstallHook(InstallHook),
    /// Hook entry point: a `PostToolUse` event on stdin.
    Hook(HookEntry),
}

const INSTALL_HELP: &str = r"EXAMPLES
  comment-crusher install-hook --claude                        ~/.claude/settings.json
  comment-crusher install-hook --claude .claude/settings.json  this repo alone
  comment-crusher install-hook --claude --uninstall            remove our entry, no other

OUTPUT
  One line, or an envelope under --format json: `data` of {outcome, path}, where outcome is
  added, already_present, removed or not_present. Running it twice changes nothing. Every
  other key in the file, and the order they sit in, is kept.

EXIT CODES
  0   the file now says what was asked
  2   bad_arguments: no HOME, and no FILE to fall back on
  3   validation_error: the settings file could not be read, parsed or replaced";

#[derive(clap::Args)]
#[command(after_help = INSTALL_HELP, after_long_help = INSTALL_HELP)]
pub struct InstallHook {
    /// Claude Code, the harness these verbs speak to.
    #[arg(long, required = true)]
    pub claude: bool,

    /// Settings file to edit [default: ~/.claude/settings.json].
    #[arg(value_name = "FILE")]
    pub file: Option<PathBuf>,

    /// Remove the entry this tool added, and nothing else.
    #[arg(long)]
    pub uninstall: bool,
}

const HOOK_HELP: &str = r#"WHAT IT ANSWERS
  One PostToolUse event on stdin, and on stdout the envelope Claude Code reads:
  {"systemMessage":..., "hookSpecificOutput":{"hookEventName":"PostToolUse",
   "additionalContext": the findings for the file the event names}}

SILENCE
  Nothing is printed unless the file's own git root holds a .comment-crusher.toml and a walk
  from that root reaches the file: what CI never measures, this never reports, and a budget
  above the repository is never borrowed.

EXIT CODES
  0   whatever it found, since a file over budget is not a failure of the call that wrote it
  2   bad_arguments: the event could not be read, or was not JSON"#;

#[derive(clap::Args)]
#[command(after_help = HOOK_HELP, after_long_help = HOOK_HELP)]
pub struct HookEntry {
    /// Read a Claude Code hook event.
    #[arg(long, required = true)]
    pub claude: bool,
}

impl Command {
    fn run(&self, format: Format) -> i32 {
        match self {
            Self::InstallHook(a) => install_hook(a, format),
            Self::Hook(_) => crate::hook::respond(),
        }
    }
}

/// Answered in the format the run asked for.
fn install_hook(args: &InstallHook, format: Format) -> i32 {
    match crate::hook::install(args.file.as_deref(), args.uninstall) {
        Ok(done) => {
            match format {
                Format::Json => say(&success_json(&serde_json::json!({
                    "outcome": done.outcome,
                    "path": done.path.to_string_lossy(),
                }))),
                Format::Human | Format::Editor => say(&done.message),
            }
            0
        }
        Err((code, message)) => {
            let wire = if code == EXIT_BAD_ARGS {
                "bad_arguments"
            } else {
                "validation_error"
            };
            match format {
                Format::Json => say(&error_json(wire, &message)),
                Format::Human | Format::Editor => say(&format!("comment-crusher: {message}")),
            }
            code
        }
    }
}

/// The guide `--config-guide` prints, in the shape of the file it describes. The rule table
/// is not written here: it is rendered from the shipped defaults, so it cannot drift.
const CONFIG_GUIDE: &str = r#"WRITING A BUDGET — comment-crusher

1. .comment-crusher.toml at the repository root, tracked:

     # Concern: the comment budget this repo holds itself to | Non-concern: what makes a
     # comment worth keeping | IO: none
     [rules.comment-ratio]
     header_free_chars = 200

     [[allow]]
     paths = ["docs/spec.md"]
     reason = "the specification is the product"
     set = ["doc-length.max_lines=2000"]

   - A run takes the nearest one, walking up from its target. The `hook` verb takes the one at
     the file's own git root and no other, so a repo that declared none is never measured.
   - Name only what differs from what ships. Every rule ships on and denying.
   - A key the defaults never declare is refused, `[[allow]]` excepted, so a typo fails the
     run instead of silently setting nothing.
   - [global] exclude adds gitignore patterns to the pruned list, never replaces it, and names
     what the repo does not author or nothing can be measured in: a generated tree, a vendored
     one, a binary fixture. A source file over a bound is widened by [[allow]], never excluded.
   - A pattern with no slash inside it prunes that name at any depth, whether or not it ends in
     one: `build/` prunes src/build too. A pattern with an inner slash is anchored to the root
     the run resolved, which --root moves. A trailing slash means directories only; `**` spans
     them. Nothing is named back in, because a pruned directory is never descended: a leading
     `!` is refused, and so is a `#` comment or a blank, which would exclude nothing.
   - [[allow]] paths and --allow take globs, not gitignore patterns: `vendor` there names a
     top-level vendor only, where the same word under exclude prunes every one.

2. Allowances widen a bound for the paths they name. They never unbind one:

   - a rule cannot be switched off, and a bound cannot be removed
   - a hundredfold of what ships is the ceiling, and a ratio never reaches 1
   - reason is required, and is printed beside every finding it covers
   - --allow GLOB RULE.FIELD=VALUE does the same for a single run

3. Every rule and global, at the value that ships. The language table is its own
   section of --help, and a budget file may override entries there too:

"#;

/// The one place a caller learns what a budget file may say, in both shapes: the text a person
/// reads, and the table an agent branches on.
fn config_guide() -> anyhow::Result<(String, serde_json::Value)> {
    let budget = crate::config::shipped_budget()?;
    let indented: String = toml::to_string_pretty(&budget)?
        .lines()
        .map(|l| {
            if l.is_empty() {
                String::from("\n")
            } else {
                format!("     {l}\n")
            }
        })
        .collect();
    Ok((
        format!("{CONFIG_GUIDE}{indented}"),
        serde_json::to_value(&budget)?,
    ))
}

#[derive(Serialize)]
struct ErrorBody {
    code: String,
    message: String,
}

/// Per collection: one `has_more` across three would say nothing about any of them.
#[derive(Serialize)]
struct Page {
    count: usize,
    has_more: bool,
    /// Null, not absent: the CLI standard names it in every collection.
    next_cursor: Option<String>,
}

impl Page {
    /// No cursor because there is no next page: a run answers about every file.
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
    languages: Page,
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
    /// The legend `--help` prints, for a test to hold to what the binary does.
    #[must_use]
    pub const fn after_help() -> &'static str {
        AFTER_HELP
    }

    /// Before clap: a rejected invocation still owes an answer in the shape it asked for.
    pub fn wants_json() -> bool {
        Self::scan().0
    }

    pub fn version_request() -> bool {
        Self::scan().1
    }

    /// A word where an option's value goes is that value, not a flag.
    fn scan() -> (bool, bool) {
        static ONCE: std::sync::OnceLock<(bool, bool)> = std::sync::OnceLock::new();
        *ONCE.get_or_init(Self::read_argv)
    }

    fn read_argv() -> (bool, bool) {
        // Clap owns each option's arity, so the pre-parse asks rather than copying it.
        let command = <Self as clap::CommandFactory>::command();
        // Spellings come from clap too, so a new alias cannot leave this blind.
        let named = |flag: &str| {
            command.get_arguments().find(|a| {
                a.get_long().is_some_and(|l| format!("--{l}") == flag)
                    || a.get_short().is_some_and(|c| format!("-{c}") == flag)
                    || a.get_all_aliases()
                        .into_iter()
                        .flatten()
                        .any(|l| format!("--{l}") == flag)
            })
        };
        let is = |flag: &str, id: &str| named(flag).is_some_and(|a| a.get_id() == id);
        let arity = |flag: &str| -> usize {
            named(flag)
                .and_then(clap::Arg::get_num_args)
                .map_or(0, |n| n.min_values())
        };
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
            let (flag, inline) = a
                .split_once('=')
                .map_or((a.as_str(), None), |(f, v)| (f, Some(v)));
            if is(flag, "version") && inline.is_none() {
                version = true;
            } else if is(flag, "format") {
                json = inline.map_or_else(
                    || value(&mut args).as_deref() == Some("json"),
                    |v| v == "json",
                );
            } else if inline.is_none() {
                for _ in 0..arity(flag) {
                    value(&mut args);
                }
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
        if self.config_guide {
            return match config_guide() {
                Ok((text, shipped)) => {
                    match self.format {
                        Format::Json => say(&success_json(
                            &serde_json::json!({ "guide": text, "shipped": shipped }),
                        )),
                        Format::Human | Format::Editor => say(&text),
                    }
                    0
                }
                Err(e) => self.print_error(Failure::Internal, &format!("{e:#}")),
            };
        }
        if let Some(command) = &self.command {
            return command.run(self.format);
        }
        match self.report() {
            Ok(report) => self.print(&report),
            Err((code, e)) => self.print_error(code, &format!("{e:#}")),
        }
    }

    /// An envelope whatever the format: the standard fixes this reply's shape.
    pub fn version_only() -> i32 {
        say(&success_json(&serde_json::json!({
            "name": "comment-crusher", "version": env!("CARGO_PKG_VERSION"),
        })));
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
        // Or every reported path is measured against something else, glob included.
        if let Some(root) = &self.root
            && !root.canonicalize().is_ok_and(|r| r.is_dir())
        {
            let (why, how) = if root.exists() {
                (Failure::BadArguments, "is not a directory")
            } else {
                (Failure::NotFound, "does not exist")
            };
            return Err((why, anyhow::anyhow!("--root {} {how}", root.display())));
        }
        // A directory is as much "not a config file" as a missing one.
        if let Some(path) = &self.config
            && !path.is_file()
        {
            let (why, how) = if path.exists() {
                (Failure::BadArguments, "is not a file")
            } else {
                (Failure::NotFound, "does not exist")
            };
            return Err((why, anyhow::anyhow!("--config {} {how}", path.display())));
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
                // Relative to the root; without --root, the budget file's own directory.
                let found = Config::source_path(&anchor, self.config.as_deref());
                let root = self.root.clone().or_else(|| {
                    found
                        .as_deref()
                        .and_then(Path::parent)
                        .map(Path::to_path_buf)
                });
                let path = found.map_or_else(
                    || PathBuf::from(CONFIG_FILE),
                    |p| root.map_or_else(|| p.clone(), |r| crate::config::relative_to(&r, &p)),
                );
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
                // A table alone drops whatever the run found, warnings included.
                if !report.diagnostics.is_empty() {
                    say("");
                    print_findings(report);
                }
                i32::from(rejected) * EXIT_VALIDATION
            }
            Format::Human => {
                print_findings(report);
                i32::from(rejected) * EXIT_VALIDATION
            }
            // Silence is a clean run: a hook can pass the output on whole.
            Format::Editor => {
                for d in &report.diagnostics {
                    say(&d.editor());
                }
                i32::from(rejected) * EXIT_VALIDATION
            }
        }
    }

    fn print_json(&self, report: &Report, rejected: bool) -> i32 {
        let languages = language_totals(report);
        let language_count = languages.len();
        let envelope = Envelope {
            status: if rejected { "error" } else { "success" },
            error: rejected.then(|| ErrorBody {
                code: "validation_error".to_string(),
                message: self.rejection(report),
            }),
            data: Data {
                files: &report.files,
                languages,
                diagnostics: &report.diagnostics,
                pagination: Pagination {
                    files: Page::whole(report.files.len()),
                    languages: Page::whole(language_count),
                    diagnostics: Page::whole(report.diagnostics.len()),
                },
            },
            meta: Meta::now(),
        };
        match serde_json::to_string_pretty(&envelope) {
            Ok(text) => {
                say(&text);
                i32::from(rejected) * EXIT_VALIDATION
            }
            Err(e) => self.print_error(Failure::Internal, &e.to_string()),
        }
    }

    /// What the run rejected, ahead of the diagnostics that say it in full.
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
            (n, 0) => format!("{n} findings over budget"),
            (n, _) if !self.warnings_as_errors => format!("{n} findings over budget"),
            (0, w) => format!("{w} warnings, and warnings are errors"),
            (n, w) => format!("{n} findings over budget, and {w} warnings are errors"),
        }
    }

    /// Every channel a caller reads is stdout, this one and clap's rejection alike.
    fn print_error(&self, failure: Failure, message: &str) -> i32 {
        match self.format {
            Format::Human => say(&format!("comment-crusher: {message}")),
            Format::Json => say(&error_json(failure.code(), message)),
            Format::Editor => say(&format!(
                "comment-crusher: error[{}]: {message}",
                failure.code()
            )),
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

/// The envelope every answer that is not a report wears, so one shape covers them all.
fn success_json(data: &serde_json::Value) -> String {
    let meta = Meta::now();
    serde_json::json!({
        "status": "success",
        "data": data,
        "meta": { "request_id": meta.request_id, "timestamp": meta.timestamp },
    })
    .to_string()
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
                languages: Page::whole(0),
                diagnostics: Page::whole(0),
            },
        },
        meta: Meta::now(),
    };
    serde_json::to_string(&envelope).unwrap_or_else(|e| {
        format!(r#"{{"status":"error","error":{{"code":"internal_error","message":"{e}"}}}}"#)
    })
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
        say(&format!("{}{}", d.human(), d.note()));
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
    say(&format!(
        "\n{} code files{also}, {:.1}% comment ({comment}/{total} chars), {} findings",
        report.files.len() - docs,
        percent(comment, total),
        report.diagnostics.len()
    ));
}

fn print_stats(report: &Report) {
    say(&format!(
        "{:<16} {:>6} {:>7} {:>10} {:>10} {:>7}",
        "language", "files", "lines", "comment", "code", "share"
    ));
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
        say(&format!(
            "{:<16} {:>6} {:>7} {:>10} {:>10} {share}",
            t.language, t.files, t.lines, t.comment_chars, t.code_chars
        ));
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
