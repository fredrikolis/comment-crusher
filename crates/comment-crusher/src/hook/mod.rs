// Concern: the Claude Code hook — the event it answers and the entry that installs it | Non-concern: measuring a file (engine.rs) or wording a finding (diagnostic.rs) | IO: (event) -> context

//! `PostToolUse`, not Pre: a budget is a fact about the file written, not a veto over it.

mod settings;

use std::fmt::Write as _;
use std::io::Read;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use crate::config::{CONFIG_FILE, Config};
use crate::diagnostic::Level;
use crate::exit::{EXIT_BAD_ARGS, EXIT_VALIDATION, say};
use crate::{Diagnostic, Engine};

/// What one install did, for the caller that renders it.
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
    // Exit 0 from here: what it finds is about the file, not about the call.
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

/// The repo's own budget: a hook in every session must never answer from a budget above it.
fn repo_budget(file: &Path) -> Option<PathBuf> {
    let mut dir = file.parent()?;
    loop {
        if dir.join(".git").exists() {
            let budget = dir.join(CONFIG_FILE);
            return budget.is_file().then_some(budget);
        }
        dir = dir.parent()?;
    }
}

/// Nothing where the repo declared no budget, or where the walk never reaches the file.
fn findings(file: &Path) -> Option<(PathBuf, Vec<String>)> {
    let budget = repo_budget(file)?;
    let config = Config::load(file, Some(&budget), &[]).ok()?;
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
        "{headline}\n\n{body}\nThe bound is the detector. Cut what tripped it; raising a \
         threshold in .comment-crusher.toml is not a fix."
    );
    let mut message = headline.to_string();
    // A finding is two lines; the second is help the user need not read.
    for l in lines.iter().filter_map(|l| l.lines().next()).take(6) {
        let _ = write!(message, "\n{l}");
    }
    json!({
        "systemMessage": message,
        "hookSpecificOutput": { "hookEventName": "PostToolUse", "additionalContext": context },
    })
    .to_string()
}
