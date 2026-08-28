// Concern: one finding — its rule, severity, location and message — and the three shapes it prints in | Non-concern: deciding that a finding exists (rules/ owns that) | IO: (finding) -> text or JSON

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Level {
    Allow,
    Warn,
    Deny,
}

impl Level {
    /// What the budget does is `deny`/`warn`; how bad it is, is what the wire calls severity.
    pub const fn severity(self) -> &'static str {
        match self {
            Self::Allow => "advice",
            Self::Warn => "warning",
            Self::Deny => "error",
        }
    }
}

impl std::fmt::Display for Level {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Self::Allow => "allow",
            Self::Warn => "warning",
            Self::Deny => "error",
        };
        f.write_str(s)
    }
}

#[derive(Debug, Clone, Serialize)]
struct Position {
    line: usize,
    column: usize,
}

#[derive(Debug, Clone, Serialize)]
struct Span {
    offset: usize,
    length: usize,
}

/// A path that is not UTF-8 is still a path: it goes out lossily rather than costing the
/// caller every other finding in the tree.
pub fn wire_path<S: serde::Serializer>(path: &Path, s: S) -> Result<S::Ok, S::Error> {
    s.serialize_str(&path.to_string_lossy())
}

#[expect(clippy::ref_option, reason = "serde hands the field by reference")]
fn wire_path_opt<S: serde::Serializer>(path: &Option<PathBuf>, s: S) -> Result<S::Ok, S::Error> {
    match path {
        Some(p) => wire_path(p, s),
        None => s.serialize_none(),
    }
}

impl Location {
    /// Nothing locates a finding about the run itself, and an empty object says less than
    /// no key at all.
    const fn is_empty(&self) -> bool {
        self.file.is_none() && self.span.is_none() && self.start.is_none() && self.end.is_none()
    }
}

#[derive(Debug, Clone, Serialize)]
struct Location {
    #[serde(
        skip_serializing_if = "Option::is_none",
        serialize_with = "wire_path_opt"
    )]
    file: Option<PathBuf>,
    #[serde(skip_serializing_if = "Option::is_none")]
    span: Option<Span>,
    #[serde(skip_serializing_if = "Option::is_none")]
    start: Option<Position>,
    #[serde(skip_serializing_if = "Option::is_none")]
    end: Option<Position>,
}

#[derive(Debug, Clone)]
pub struct Diagnostic {
    pub rule: &'static str,
    pub level: Level,
    /// Absent when the finding is about the invocation rather than about a file.
    pub file: Option<PathBuf>,
    pub line: Option<usize>,
    pub end_line: Option<usize>,
    pub span: Option<(usize, usize)>,
    pub start_column: Option<usize>,
    pub end_column: Option<usize>,
    pub message: String,
    pub help: &'static str,
    /// Present when a bound was widened rather than met.
    pub allowance: Option<String>,
}

#[derive(Serialize)]
struct Wire<'a> {
    code: &'a str,
    severity: &'a str,
    message: &'a str,
    #[serde(skip_serializing_if = "Location::is_empty")]
    location: Location,
    help: &'a str,
    docs_url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    allowance: Option<&'a str>,
}

impl Serialize for Diagnostic {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        Wire {
            code: self.rule,
            severity: self.level.severity(),
            message: &self.message,
            location: Location {
                file: self.file.clone(),
                span: self.span.map(|(offset, length)| Span { offset, length }),
                start: self.line.map(|line| Position {
                    line,
                    column: self.start_column.unwrap_or(1),
                }),
                end: self.end_line.map(|line| Position {
                    line,
                    column: self.end_column.unwrap_or(1),
                }),
            },
            help: self.help,
            docs_url: format!("{DOCS}#{}", self.section()),
            allowance: self.allowance.as_deref(),
        }
        .serialize(s)
    }
}

/// The README is the documentation, so a code points at the section that defines it.
const DOCS: &str = "https://github.com/fredrikolis/comment-crusher";

impl Diagnostic {
    fn section(&self) -> &'static str {
        match self.rule.split('.').next().unwrap_or_default() {
            "config" | "target" => "use",
            "allowance" => "no-file-is-exempt",
            _ => "what-it-measures",
        }
    }

    pub fn new(
        rule: &'static str,
        level: Level,
        file: &Path,
        message: String,
        help: &'static str,
    ) -> Self {
        Self {
            rule,
            level,
            file: Some(file.to_path_buf()),
            line: None,
            end_line: None,
            span: None,
            start_column: None,
            end_column: None,
            message,
            help,
            allowance: None,
        }
    }

    #[must_use]
    pub const fn at(mut self, line: usize) -> Self {
        self.line = Some(line);
        self
    }

    /// The exact region a rule judged, so an agent can act on it without re-finding it.
    #[must_use]
    pub const fn spanning(mut self, start: usize, end: usize, end_line: usize) -> Self {
        self.span = Some((start, end.saturating_sub(start)));
        self.end_line = Some(end_line);
        self
    }

    /// So slicing `start`..`end` reaches the region's real end, not its last line's start.
    #[must_use]
    pub const fn columns(mut self, start: usize, end: usize) -> Self {
        self.start_column = Some(start);
        self.end_column = Some(end);
        self
    }

    fn where_(&self) -> String {
        self.file.as_ref().map_or_else(
            || "comment-crusher".to_string(),
            |f| f.display().to_string(),
        )
    }

    pub fn note(&self) -> String {
        self.allowance
            .as_ref()
            .map_or_else(String::new, |a| format!(" (allowance: {a})"))
    }

    pub fn human(&self) -> String {
        let at = self.line.map_or_else(String::new, |l| format!(":{l}"));
        format!(
            "{}: {}{at} [{}] {}",
            self.level,
            self.where_(),
            self.rule,
            self.message
        )
    }

    /// `path:line:column: severity[rule]: message`, then the rule's help beneath it.
    pub fn editor(&self) -> String {
        let at = self.line.map_or_else(String::new, |l| {
            format!(":{l}:{}", self.start_column.unwrap_or(1))
        });
        format!(
            "{}{at}: {}[{}]: {}{}\n  help: {}",
            self.where_(),
            self.level.severity(),
            self.rule,
            self.message,
            self.note(),
            self.help
        )
    }

    /// A finding about the invocation, which no path locates.
    pub fn about_the_run(
        rule: &'static str,
        level: Level,
        message: String,
        help: &'static str,
    ) -> Self {
        let mut d = Self::new(rule, level, Path::new(""), message, help);
        d.file = None;
        d
    }
}
