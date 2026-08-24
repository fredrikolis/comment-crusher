// Concern: resolves the layered configuration and answers which language a path is | Non-concern: walking, scanning or judging | IO: (defaults, config files, overrides) -> Config

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use globset::{Glob, GlobSet, GlobSetBuilder};
use serde::Deserialize;
use toml::{Table, Value};

use crate::embed::EmbedSpec;
use crate::rules::Rules;
use crate::syntax::{CommentKind, Opener, Resolve, StringSpec, Syntax};

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
    exceptions: Vec<String>,
    #[serde(default)]
    line_anchored: bool,
    #[serde(default)]
    strings: Vec<RawString>,
    #[serde(default)]
    embed: Vec<RawEmbed>,
    /// Named entries from `[embed_sets]`, applied before this language's own `embed`.
    #[serde(default)]
    embed_use: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawEmbed {
    open: String,
    close: String,
    default: String,
    #[serde(default)]
    attrs: Vec<String>,
    #[serde(default)]
    map: HashMap<String, String>,
    #[serde(default)]
    at_start: bool,
    #[serde(default)]
    balanced: bool,
    #[serde(default)]
    skip: Vec<String>,
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
    /// The directory the budget was found in. Allowance globs and reported paths are relative
    /// to it, so one repo answer holds from wherever the tool is invoked.
    root: PathBuf,
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
        Self::build(&table, &[], PathBuf::from("."))
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
        let found = explicit.map_or_else(|| find_upward(root), |p| Some(p.to_path_buf()));
        let base = match &found {
            Some(p) => {
                overlay(&mut table, p)?;
                p.parent()
                    .map_or_else(|| PathBuf::from("."), Path::to_path_buf)
            }
            None => directory_of(root),
        };
        Self::build(&table, cli_allow, base)
    }

    fn build(table: &Table, cli_allow: &[(String, String)], root: PathBuf) -> Result<Self> {
        let global: RawGlobal = section(table, "global")?;
        let rules_table = table
            .get("rules")
            .and_then(Value::as_table)
            .cloned()
            .unwrap_or_default();
        let base = Rules::from_table(&rules_table)?;

        let allowances = build_allowances(table, cli_allow, &rules_table)?;

        let sets: HashMap<String, Vec<RawEmbed>> = section(table, "embed_sets")?;
        let raw_langs: HashMap<String, RawLanguage> = section(table, "languages")?;
        let mut cfg = Self {
            root,
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
            cfg.push_language(&name, &raw, &sets)?;
        }
        Ok(cfg)
    }

    fn push_language(
        &mut self,
        name: &str,
        raw: &RawLanguage,
        sets: &HashMap<String, Vec<RawEmbed>>,
    ) -> Result<()> {
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
        self.langs
            .push(resolve_syntax(name, raw, sets).with_context(|| format!("[languages.{name}]"))?);
        Ok(())
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

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn languages(&self) -> impl Iterator<Item = &Syntax> {
        self.langs.iter()
    }

    fn matching(&self, rel: &Path) -> Vec<&Allowance> {
        self.allowances
            .iter()
            .filter(|a| a.globs.is_match(rel))
            .collect()
    }

    /// The base rules with every matching allowance applied, and the reasons that widened
    /// them. If a combination ever failed to deserialize, the unwidened base applies, which
    /// is stricter than any allowance could make it and so can never under-report.
    pub fn rules_for(&self, rel: &Path) -> (Rules, Vec<String>) {
        let matched = self.matching(rel);
        if matched.is_empty() {
            return (self.base.clone(), Vec::new());
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
        let rules = Rules::from_table(&table).unwrap_or_else(|_| self.base.clone());
        (rules, reasons)
    }
}

impl Resolve for Config {
    fn language_named(&self, name: &str) -> Option<&Syntax> {
        self.langs.iter().find(|l| l.name == name)
    }
}

fn build_allowances(
    table: &Table,
    cli_allow: &[(String, String)],
    rules_table: &Table,
) -> Result<Vec<Allowance>> {
    let declared: Vec<RawAllow> = table
        .get("allow")
        .cloned()
        .map(Value::try_into::<Vec<RawAllow>>)
        .transpose()
        .context("[[allow]] is malformed")?
        .unwrap_or_default();

    let mut allowances = Vec::with_capacity(declared.len() + cli_allow.len());
    for raw in declared {
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

    // Deserialized here, not mid-walk, where the file it covers would leave the report.
    for a in &allowances {
        let mut probe = rules_table.clone();
        for (path, value) in &a.set {
            set_dotted(&mut probe, path, value.clone());
        }
        Rules::from_table(&probe).with_context(|| {
            let what = a
                .set
                .iter()
                .map(|(p, v)| format!("{p}={v}"))
                .collect::<Vec<_>>()
                .join(", ");
            format!("allowance `{what}` is not a value its rule can hold")
        })?;
    }
    Ok(allowances)
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

fn resolve_syntax(
    name: &str,
    raw: &RawLanguage,
    sets: &HashMap<String, Vec<RawEmbed>>,
) -> Result<Syntax> {
    let embed_specs = gather_embeds(raw, sets)?;
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
    if openers.iter().any(|(tok, _)| tok.is_empty()) {
        bail!("`{name}` declares an empty comment or string marker");
    }
    // Longest first, so `///` never loses to `//` and `"""` never loses to `"`.
    openers.sort_by_key(|o| std::cmp::Reverse(o.0.len()));
    Ok(Syntax {
        name: name.to_string(),
        prose: raw.prose,
        nested_block: raw.nested_block,
        hash_raw_strings: raw.hash_raw_strings,
        heredoc: raw.heredoc,
        strings,
        openers,
        exceptions: raw.exceptions.clone(),
        line_anchored: raw.line_anchored,
        embeds: embed_specs
            .into_iter()
            .map(resolve_embed)
            .collect::<Result<_>>()?,
    })
}

/// A language's `embed_use` sets come first, then its own `embed`, because order decides
/// which opener wins and a `{` expression must lose to a `<script` tag.
fn gather_embeds<'a>(
    raw: &'a RawLanguage,
    sets: &'a HashMap<String, Vec<RawEmbed>>,
) -> Result<Vec<&'a RawEmbed>> {
    let mut out: Vec<&RawEmbed> = Vec::new();
    for set in &raw.embed_use {
        let named = sets
            .get(set)
            .with_context(|| format!("no `[embed_sets]` entry named `{set}`"))?;
        out.extend(named);
    }
    out.extend(&raw.embed);
    Ok(out)
}

fn resolve_embed(e: &RawEmbed) -> Result<EmbedSpec> {
    if e.open.is_empty() || e.close.is_empty() {
        bail!("an embed needs a non-empty `open` and `close`");
    }
    Ok(EmbedSpec {
        open: e.open.clone(),
        close: e.close.clone(),
        default: e.default.clone(),
        attrs: e.attrs.clone(),
        map: e
            .map
            .iter()
            .map(|(k, v)| (k.to_ascii_lowercase(), v.clone()))
            .collect(),
        at_start: e.at_start,
        balanced: e.balanced,
        skip: e.skip.clone(),
    })
}

fn build_globs(patterns: &[String]) -> Result<GlobSet> {
    let mut b = GlobSetBuilder::new();
    for p in patterns {
        b.add(Glob::new(p).with_context(|| format!("bad glob `{p}`"))?);
    }
    b.build().context("glob set")
}

/// The fields an allowance may set, and the open upper limit each must stay under. Only
/// upper bounds appear here: `min_chars` decides whether a rule applies at all and `level`
/// whether it runs, so setting either would exempt a path rather than widen it, and a ratio
/// of 1 can never be exceeded. This list is what makes "no file is exempt" true.
const WIDENABLE: &[(&str, f64)] = &[
    ("comment-ratio.max_ratio", 1.0),
    ("comment-block.max_lines", f64::INFINITY),
    ("comment-block.doc_max_lines", f64::INFINITY),
    ("comment-block.header_max_lines", f64::INFINITY),
    ("comment-block.max_chars", f64::INFINITY),
    ("doc-length.max_lines", f64::INFINITY),
];

/// `comment-ratio.max_ratio=0.4` -> the dotted path and its typed value.
fn parse_setting(s: &str) -> Result<(String, Value)> {
    let Some((path, raw)) = s.split_once('=') else {
        bail!("`{s}` is not <rule>.<field>=<value>");
    };
    let path = path.trim().to_string();
    let Some((_, limit)) = WIDENABLE.iter().find(|(p, _)| *p == path) else {
        let names = WIDENABLE
            .iter()
            .map(|(p, _)| *p)
            .collect::<Vec<_>>()
            .join(", ");
        bail!("`{path}` is not a bound an allowance may widen. One of: {names}");
    };
    let raw = raw.trim();
    let value = raw.parse::<i64>().map_or_else(
        |_| raw.parse::<f64>().ok().map(Value::Float),
        |i| Some(Value::Integer(i)),
    );
    let Some(value) = value else {
        bail!("`{s}` is not a number");
    };
    let n = as_f64(&value);
    if n <= 0.0 || n >= *limit {
        bail!("`{s}` would leave the path exempt; an allowance only ever widens a bound");
    }
    Ok((path, value))
}

#[expect(
    clippy::cast_precision_loss,
    reason = "bounds are far below f64 precision"
)]
const fn as_f64(value: &Value) -> f64 {
    match value {
        Value::Integer(i) => *i as f64,
        Value::Float(f) => *f,
        _ => 0.0,
    }
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
        *entry = Value::Table(entry.as_table().cloned().unwrap_or_default());
        let Some(table) = entry.as_table_mut() else {
            return;
        };
        cursor = table;
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

fn directory_of(path: &Path) -> PathBuf {
    let abs = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    if abs.is_dir() {
        return abs;
    }
    abs.parent()
        .map_or_else(|| PathBuf::from("."), Path::to_path_buf)
}

fn find_upward(from: &Path) -> Option<PathBuf> {
    let mut dir = directory_of(from);
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
