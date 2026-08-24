// Concern: one finding — its rule, severity, location and message — and the two shapes it prints in | Non-concern: deciding that a finding exists (rules/ owns that) | IO: (finding) -> text or JSON

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
    /// The wire name. `deny`/`warn`/`allow` say what the budget does about a finding; an agent
    /// branches on how bad it is, which is what the CLI contract calls severity.
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

#[derive(Debug, Clone, Serialize)]
struct Location {
    file: PathBuf,
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
    pub file: PathBuf,
    pub line: Option<usize>,
    pub end_line: Option<usize>,
    /// Byte offset and length of the region that tripped the rule, machine-exact.
    pub span: Option<(usize, usize)>,
    pub message: String,
    pub help: &'static str,
    /// The allowance that widened this file's budget, when one did, so a report shows that a
    /// bound was widened rather than met.
    pub allowance: Option<String>,
}

#[derive(Serialize)]
struct Wire<'a> {
    code: &'a str,
    severity: &'a str,
    message: &'a str,
    location: Location,
    help: &'a str,
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
                start: self.line.map(|line| Position { line, column: 1 }),
                end: self.end_line.map(|line| Position { line, column: 1 }),
            },
            help: self.help,
            allowance: self.allowance.as_deref(),
        }
        .serialize(s)
    }
}

impl Diagnostic {
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
            file: file.to_path_buf(),
            line: None,
            end_line: None,
            span: None,
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

    pub fn human(&self) -> String {
        let at = self.line.map_or_else(String::new, |l| format!(":{l}"));
        format!(
            "{}: {}{} [{}] {}",
            self.level,
            self.file.display(),
            at,
            self.rule,
            self.message
        )
    }
}
