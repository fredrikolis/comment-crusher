// Concern: adds or removes our hook entry in a settings file, keeping every other key | Non-concern: what the hook answers, or which event carries it | IO: (settings path) -> edited file, outcome

use std::path::{Path, PathBuf};

use serde_json::{Map, Value, json};

const EVENT: &str = "PostToolUse";

/// The agent a settings file belongs to: each reads the same entry shape from its own file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Harness {
    Claude,
    Codex,
}

impl Harness {
    const fn verb(self) -> &'static str {
        match self {
            Self::Claude => "hook --claude",
            Self::Codex => "hook --codex",
        }
    }

    const fn matcher(self) -> &'static str {
        match self {
            Self::Claude => "Write|Edit|MultiEdit|NotebookEdit",
            Self::Codex => "apply_patch|Edit|Write",
        }
    }

    /// A bare name, so the entry survives a reinstall elsewhere.
    fn command(self) -> String {
        format!("comment-crusher {}", self.verb())
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum Outcome {
    Added,
    AlreadyPresent,
    Removed,
    NotPresent,
}

impl Outcome {
    /// The same four answers a machine reads, in the shape the wire spells them.
    pub const fn wire(&self) -> &'static str {
        match self {
            Self::Added => "added",
            Self::AlreadyPresent => "already_present",
            Self::Removed => "removed",
            Self::NotPresent => "not_present",
        }
    }

    pub fn describe(&self, path: &Path) -> String {
        let p = path.display();
        match self {
            Self::Added => format!("hook installed in {p}"),
            Self::AlreadyPresent => format!("hook already in {p}; nothing changed"),
            Self::Removed => format!("hook removed from {p}"),
            Self::NotPresent => format!("no hook of ours in {p}; nothing changed"),
        }
    }
}

/// What the agent reads for every project; one repo is `.claude/settings.json` or
/// `.codex/hooks.json`.
pub fn default_path(harness: Harness) -> Option<PathBuf> {
    let home = PathBuf::from(std::env::var_os("HOME")?);
    Some(match harness {
        Harness::Claude => home.join(".claude").join("settings.json"),
        Harness::Codex => home.join(".codex").join("hooks.json"),
    })
}

fn is_ours(entry: &Value, harness: Harness) -> bool {
    entry
        .get("hooks")
        .and_then(Value::as_array)
        .is_some_and(|hooks| {
            hooks.iter().any(|h| {
                h.get("command")
                    .and_then(Value::as_str)
                    .is_some_and(|c| c.contains(harness.verb()))
            })
        })
}

/// Empty where there is no file, but never for one that fails to parse: it holds the
/// permissions a user accepted over time, and starting fresh would discard them.
fn read(path: &Path) -> Result<Map<String, Value>, String> {
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Map::new()),
        Err(e) => return Err(format!("cannot read {}: {e}", path.display())),
    };
    if text.trim().is_empty() {
        return Ok(Map::new());
    }
    match serde_json::from_str(&text) {
        Ok(Value::Object(m)) => Ok(m),
        Ok(_) => Err(format!(
            "{} is JSON but not an object; refusing to touch it",
            path.display()
        )),
        Err(e) => Err(format!(
            "{} is not valid JSON ({e}); fix it first — it holds the permissions you accepted \
             and this will not overwrite them",
            path.display()
        )),
    }
}

/// Rename, so an interrupted write cannot truncate what it holds.
fn write(path: &Path, root: &Map<String, Value>) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)
            .map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
    }
    let mut text = serde_json::to_string_pretty(root).map_err(|e| e.to_string())?;
    text.push('\n');
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, text).map_err(|e| format!("cannot write {}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, path).map_err(|e| format!("cannot replace {}: {e}", path.display()))
}

/// Keeps every other key, and running it again changes nothing.
pub fn install(path: &Path, harness: Harness) -> Result<Outcome, String> {
    let mut root = read(path)?;
    let hooks = root
        .entry("hooks")
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .ok_or_else(|| format!("`hooks` in {} is not an object", path.display()))?;
    let list = hooks
        .entry(EVENT)
        .or_insert_with(|| json!([]))
        .as_array_mut()
        .ok_or_else(|| format!("`hooks.{EVENT}` in {} is not an array", path.display()))?;
    if list.iter().any(|e| is_ours(e, harness)) {
        return Ok(Outcome::AlreadyPresent);
    }
    list.push(json!({
        "matcher": harness.matcher(),
        "hooks": [{ "type": "command", "command": harness.command() }],
    }));
    write(path, &root)?;
    Ok(Outcome::Added)
}

/// Ours and nothing else, pruning what we created so the file returns to its shape before.
pub fn uninstall(path: &Path, harness: Harness) -> Result<Outcome, String> {
    let mut root = read(path)?;
    let mut removed = false;
    let hooks_empty = {
        let Some(hooks) = root.get_mut("hooks").and_then(Value::as_object_mut) else {
            return Ok(Outcome::NotPresent);
        };
        if let Some(list) = hooks.get_mut(EVENT).and_then(Value::as_array_mut) {
            let before = list.len();
            list.retain(|e| !is_ours(e, harness));
            removed = list.len() != before;
            if list.is_empty() {
                hooks.remove(EVENT);
            }
        }
        hooks.is_empty()
    };
    if !removed {
        return Ok(Outcome::NotPresent);
    }
    if hooks_empty {
        root.remove("hooks");
    }
    write(path, &root)?;
    Ok(Outcome::Removed)
}
