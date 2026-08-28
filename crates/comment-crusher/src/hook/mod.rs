// Concern: the Claude Code hook — the event it answers and the entry that installs it | Non-concern: measuring a file (engine.rs) or wording a finding (diagnostic.rs) | IO: (event) -> context

//! `PostToolUse`, not Pre: a budget is a fact about the file written, not a veto over it.

mod settings;

use std::fmt::Write as _;
use std::io::Read;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use crate::config::Config;
use crate::diagnostic::Level;
use crate::exit::{EXIT_BAD_ARGS, EXIT_VALIDATION, say};
use crate::{Diagnostic, Engine};

/// What one install did, for a caller that renders it in the format the run asked for.
pub struct Installed {
    pub path: PathBuf,
    pub outcome: &'static str,
    pub message: String,
}

pub fn install(file: Option<&Path>, uninstall: bool) -> Result<Installed, (i32, String)> {
    let Some(path) = file.map(Path::to_path_buf).or_else(settings::default_path) else {
        return Err((
            EXIT_BAD_ARGS,
            "no HOME, so no default settings file: name one".to_string(),
        ));
    };
    let done = if uninstall {
        settings::uninstall(&path)
    } else {
        settings::install(&path)
    };
    match done {
        Ok(outcome) => Ok(Installed {
            message: outcome.describe(&path),
            outcome: outcome.wire(),
            path,
        }),
        Err(e) => Err((EXIT_VALIDATION, e)),
    }
}

/// Answers only what the event names: nothing else in it names a file this call wrote.
pub fn respond() -> i32 {
    let mut raw = String::new();
    if std::io::stdin().read_to_string(&mut raw).is_err() {
        say("comment-crusher: the hook event could not be read");
        return EXIT_BAD_ARGS;
    }
    let Ok(event) = serde_json::from_str::<Value>(&raw) else {
        say("comment-crusher: the hook event is not JSON");
        return EXIT_BAD_ARGS;
    };
    // Exit 0 from here on: an over-budget file is a fact about the file, not a failed call.
    let Some(file) = written(&event) else {
        return 0;
    };
    if let Some((root, lines)) = findings(&file) {
        say(&envelope(&root, &lines));
    }
    0
}

fn written(event: &Value) -> Option<PathBuf> {
    let cwd = event
        .pointer("/cwd")
        .and_then(Value::as_str)
        .map_or_else(|| PathBuf::from("."), PathBuf::from);
    ["/tool_input/file_path", "/tool_response/filePath"]
        .iter()
        .filter_map(|p| event.pointer(p).and_then(Value::as_str))
        .map(|p| cwd.join(p))
        .find(|p| p.is_file())
}

/// The same answer CI gives, or none: no budget above the path, or no walk that reaches it.
fn findings(file: &Path) -> Option<(PathBuf, Vec<String>)> {
    Config::source_path(file, None)?;
    let config = Config::load(file, None, &[]).ok()?;
    let engine = Engine::new(&config, None);
    if !engine.reaches(file) {
        return None;
    }
    let report = engine.run(std::slice::from_ref(&file.to_path_buf()));
    let lines: Vec<String> = report
        .diagnostics
        .iter()
        .filter(|d| d.level == Level::Deny)
        .map(Diagnostic::editor)
        .collect();
    (!lines.is_empty()).then(|| (config.root().to_path_buf(), lines))
}

fn envelope(root: &Path, lines: &[String]) -> String {
    let headline = "comment-crusher: over budget. CI runs comment-crusher, so this fails it.";
    let mut body = String::new();
    let _ = writeln!(body, "# paths relative to {}", root.display());
    for l in lines {
        let _ = writeln!(body, "{l}");
    }
    let context = format!(
        "{headline}\n\n{body}\nThe bound is the detector: cut or restructure what tripped it. \
         Raising a threshold in .comment-crusher.toml is not a fix."
    );
    let mut message = headline.to_string();
    // One finding is two lines, and the second is the help the agent gets and the user need not.
    for l in lines.iter().filter_map(|l| l.lines().next()).take(6) {
        let _ = write!(message, "\n{l}");
    }
    json!({
        "systemMessage": message,
        "hookSpecificOutput": { "hookEventName": "PostToolUse", "additionalContext": context },
    })
    .to_string()
}
