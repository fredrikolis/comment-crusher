// Concern: resolves the layered configuration and answers which language a path is | Non-concern: walking, scanning or judging | IO: (defaults, config files, overrides) -> Config

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use globset::{Glob, GlobSet, GlobSetBuilder};
use serde::Deserialize;
use toml::{Table, Value};

use crate::rules::Rules;
use crate::syntax::{CommentKind, Opener, StringSpec, Syntax};

pub const CONFIG_FILE: &str = ".comment-crusher.toml";
const DEFAULTS: &str = include_str!("default_config.toml");

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawGlobal {
    #[serde(default)]
    exclude: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawAllow {
    paths: Vec<String>,
    #[serde(default)]
    set: Vec<String>,
    #[serde(default)]
    reason: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawLanguage {
    #[serde(default)]
    extensions: Vec<String>,
    #[serde(default)]
    filenames: Vec<String>,
    #[serde(default)]
    interpreters: Vec<String>,
    #[serde(default)]
    prose: bool,
    #[serde(default)]
    line: Vec<String>,
    #[serde(default)]
    doc_line: Vec<String>,
    #[serde(default)]
    block: Vec<[String; 2]>,
    #[serde(default)]
    doc_block: Vec<[String; 2]>,
    #[serde(default)]
    nested_block: bool,
    #[serde(default)]
    hash_raw_strings: bool,
    #[serde(default)]
    heredoc: bool,
    #[serde(default)]
    line_exceptions: Vec<String>,
    #[serde(default)]
    strings: Vec<RawString>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawString {
    open: String,
    close: String,
    #[serde(default)]
    multiline: bool,
    #[serde(default = "default_escape")]
    escape: String,
    #[serde(default)]
    char_literal: bool,
    #[serde(default)]
    docstring: bool,
}

fn default_escape() -> String {
    "\\".to_string()
}

/// A widened budget for the paths it names, and the reason it was widened. Every finding it
/// suppresses is a decision someone recorded, not a threshold quietly raised for everyone.
pub struct Allowance {
    pub reason: Option<String>,
    globs: GlobSet,
    set: Vec<(String, Value)>,
}

pub struct Config {
    pub exclude: Vec<String>,
    pub base: Rules,
    allowances: Vec<Allowance>,
    rules_table: Table,
    langs: Vec<Syntax>,
    by_ext: HashMap<String, usize>,
    by_filename: HashMap<String, usize>,
    by_interpreter: HashMap<String, usize>,
}

impl Config {
    /// The built-in defaults alone, with no file layer consulted. What a caller measuring a
    /// tree that is not its own — a test corpus, another repo — should use.
    pub fn defaults() -> Result<Self> {
        let table: Table =
            toml::from_str(DEFAULTS).context("built-in default config is invalid")?;
        Self::build(&table, &[])
    }

    /// Layer the built-in defaults, the user file, the nearest `.comment-crusher.toml` above
    /// `root`, and any `--allow` given on the command line.
    pub fn load(
        root: &Path,
        explicit: Option<&Path>,
        cli_allow: &[(String, String)],
    ) -> Result<Self> {
        let mut table: Table =
            toml::from_str(DEFAULTS).context("built-in default config is invalid")?;
        if let Some(user) = user_config_path() {
            overlay(&mut table, &user)?;
        }
        match explicit {
            Some(p) => overlay(&mut table, p)?,
            None => {
                if let Some(found) = find_upward(root) {
                    overlay(&mut table, &found)?;
                }
            }
        }
        Self::build(&table, cli_allow)
    }

    fn build(table: &Table, cli_allow: &[(String, String)]) -> Result<Self> {
        let global: RawGlobal = section(table, "global")?;
        let rules_table = table
            .get("rules")
            .and_then(Value::as_table)
            .cloned()
            .unwrap_or_default();
        let base = Rules::from_table(&rules_table)?;

        let mut allowances = Vec::new();
        for raw in table
            .get("allow")
            .cloned()
            .map(Value::try_into::<Vec<RawAllow>>)
            .transpose()
            .context("[[allow]] is malformed")?
            .unwrap_or_default()
        {
            allowances.push(Allowance {
                reason: raw.reason,
                globs: build_globs(&raw.paths)?,
                set: raw
                    .set
                    .iter()
                    .map(|s| parse_setting(s))
                    .collect::<Result<_>>()?,
            });
        }
        for (glob, setting) in cli_allow {
            allowances.push(Allowance {
                reason: Some("--allow".to_string()),
                globs: build_globs(std::slice::from_ref(glob))?,
                set: vec![parse_setting(setting)?],
            });
        }

        let raw_langs: HashMap<String, RawLanguage> = section(table, "languages")?;
        let mut cfg = Self {
            exclude: global.exclude,
            base,
            allowances,
            rules_table,
            langs: Vec::with_capacity(raw_langs.len()),
            by_ext: HashMap::new(),
            by_filename: HashMap::new(),
            by_interpreter: HashMap::new(),
        };
        let mut names: Vec<_> = raw_langs.into_iter().collect();
        names.sort_by(|a, b| a.0.cmp(&b.0));
        for (name, raw) in names {
            cfg.push_language(&name, &raw);
        }
        Ok(cfg)
    }

    fn push_language(&mut self, name: &str, raw: &RawLanguage) {
        let idx = self.langs.len();
        for e in &raw.extensions {
            self.by_ext
                .insert(e.trim_start_matches('.').to_lowercase(), idx);
        }
        for f in &raw.filenames {
            self.by_filename.insert(f.clone(), idx);
        }
        for i in &raw.interpreters {
            self.by_interpreter.insert(i.clone(), idx);
        }
        self.langs.push(resolve_syntax(name, raw));
    }

    pub fn language(&self, path: &Path) -> Option<&Syntax> {
        let idx = path
            .file_name()
            .and_then(|n| n.to_str())
            .and_then(|n| self.by_filename.get(n))
            .or_else(|| {
                path.extension()
                    .and_then(|e| e.to_str())
                    .and_then(|e| self.by_ext.get(&e.to_lowercase()))
            })?;
        self.langs.get(*idx)
    }

    /// The language a `#!` line names, the only way an extensionless file resolves.
    pub fn language_of_shebang(&self, first_line: &str) -> Option<&Syntax> {
        let rest = first_line.strip_prefix("#!")?;
        let idx = rest
            .split_whitespace()
            .filter_map(|w| Path::new(w).file_name()?.to_str())
            .find_map(|w| self.by_interpreter.get(w))?;
        self.langs.get(*idx)
    }

    /// Every allowance whose globs match `rel`, in declaration order.
    pub fn matching(&self, rel: &Path) -> Vec<&Allowance> {
        self.allowances
            .iter()
            .filter(|a| a.globs.is_match(rel))
            .collect()
    }

    /// The base rules with every matching allowance applied, and the reasons that widened them.
    pub fn rules_for(&self, rel: &Path) -> Result<(Rules, Vec<String>)> {
        let matched = self.matching(rel);
        if matched.is_empty() {
            return Ok((self.base.clone(), Vec::new()));
        }
        let mut table = self.rules_table.clone();
        let mut reasons = Vec::new();
        for a in matched {
            for (path, value) in &a.set {
                set_dotted(&mut table, path, value.clone());
            }
            if let Some(r) = &a.reason {
                reasons.push(r.clone());
            }
        }
        Ok((Rules::from_table(&table)?, reasons))
    }
}

fn section<T: for<'de> Deserialize<'de> + Default>(table: &Table, key: &str) -> Result<T> {
    table.get(key).map_or_else(
        || Ok(T::default()),
        |v| {
            v.clone()
                .try_into()
                .with_context(|| format!("[{key}] is malformed"))
        },
    )
}

fn resolve_syntax(name: &str, raw: &RawLanguage) -> Syntax {
    let mut openers: Vec<(String, Opener)> = Vec::new();
    for (toks, kind) in [
        (&raw.doc_line, CommentKind::Doc),
        (&raw.line, CommentKind::Plain),
    ] {
        for t in toks {
            openers.push((t.clone(), Opener::Line(kind)));
        }
    }
    for (pairs, kind) in [
        (&raw.doc_block, CommentKind::Doc),
        (&raw.block, CommentKind::Plain),
    ] {
        for p in pairs {
            openers.push((
                p[0].clone(),
                Opener::Block {
                    close: p[1].clone(),
                    kind,
                },
            ));
        }
    }
    let strings: Vec<StringSpec> = raw
        .strings
        .iter()
        .map(|s| StringSpec {
            open: s.open.clone(),
            close: s.close.clone(),
            multiline: s.multiline,
            escape: s.escape.chars().next(),
            char_literal: s.char_literal,
            docstring: s.docstring,
        })
        .collect();
    for (i, s) in strings.iter().enumerate() {
        openers.push((s.open.clone(), Opener::Str(i)));
    }
    // Longest first, so `///` never loses to `//` and `"""` never loses to `"`.
    openers.sort_by_key(|o| std::cmp::Reverse(o.0.len()));
    Syntax {
        name: name.to_string(),
        prose: raw.prose,
        nested_block: raw.nested_block,
        hash_raw_strings: raw.hash_raw_strings,
        heredoc: raw.heredoc,
        strings,
        openers,
        line_exceptions: raw.line_exceptions.clone(),
    }
}

fn build_globs(patterns: &[String]) -> Result<GlobSet> {
    let mut b = GlobSetBuilder::new();
    for p in patterns {
        b.add(Glob::new(p).with_context(|| format!("bad glob `{p}`"))?);
    }
    b.build().context("glob set")
}

/// `comment-ratio.max_ratio=0.4` -> the dotted path and its typed value.
fn parse_setting(s: &str) -> Result<(String, Value)> {
    let Some((path, raw)) = s.split_once('=') else {
        bail!("`{s}` is not <rule>.<field>=<value>");
    };
    let path = path.trim().to_string();
    if !path.contains('.') {
        bail!("`{s}` is missing the rule: write <rule>.<field>=<value>");
    }
    let raw = raw.trim();
    let value = raw.parse::<i64>().map_or_else(
        |_| {
            raw.parse::<f64>().map_or_else(
                |_| {
                    raw.parse::<bool>()
                        .map_or_else(|_| Value::String(raw.to_string()), Value::Boolean)
                },
                Value::Float,
            )
        },
        Value::Integer,
    );
    Ok((path, value))
}

fn set_dotted(table: &mut Table, path: &str, value: Value) {
    let mut parts = path.split('.').peekable();
    let mut cursor = table;
    while let Some(part) = parts.next() {
        if parts.peek().is_none() {
            cursor.insert(part.to_string(), value);
            return;
        }
        let entry = cursor
            .entry(part.to_string())
            .or_insert_with(|| Value::Table(Table::new()));
        if !entry.is_table() {
            *entry = Value::Table(Table::new());
        }
        match entry.as_table_mut() {
            Some(t) => cursor = t,
            None => return,
        }
    }
}

fn overlay(base: &mut Table, path: &Path) -> Result<()> {
    let text =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let layer: Table =
        toml::from_str(&text).with_context(|| format!("parsing {}", path.display()))?;
    merge(base, layer);
    Ok(())
}

/// Later layers overlay earlier ones key by key. `global.exclude` is the one additive key:
/// naming a directory of your own should not silently drop `target` and `node_modules`.
fn merge(base: &mut Table, layer: Table) {
    for (k, v) in layer {
        match (base.get_mut(&k), v) {
            (Some(Value::Table(bt)), Value::Table(lt)) => merge(bt, lt),
            (Some(Value::Array(ba)), Value::Array(la)) if k == "exclude" => ba.extend(la),
            (_, v) => {
                base.insert(k, v);
            }
        }
    }
}

fn user_config_path() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
    let p = base.join("comment-crusher").join("config.toml");
    p.is_file().then_some(p)
}

fn find_upward(from: &Path) -> Option<PathBuf> {
    let start = if from.is_dir() {
        from.to_path_buf()
    } else {
        from.parent()?.to_path_buf()
    };
    let mut dir = start.canonicalize().unwrap_or(start);
    loop {
        let candidate = dir.join(CONFIG_FILE);
        if candidate.is_file() {
            return Some(candidate);
        }
        if !dir.pop() {
            return None;
        }
    }
}
