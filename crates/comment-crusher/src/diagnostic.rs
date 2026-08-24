// Concern: one finding — its rule, severity, location and message — and the two shapes it prints in | Non-concern: deciding that a finding exists (rules/ owns that) | IO: (finding) -> text or JSON

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Level {
    Allow,
    Warn,
    Deny,
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
pub struct Diagnostic {
    pub rule: &'static str,
    pub level: Level,
    pub file: PathBuf,
    pub line: Option<usize>,
    pub message: String,
    /// The allowance that raised this file's budget, when one did. Present so a report can
    /// show that a threshold was widened rather than met.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowance: Option<String>,
}

impl Diagnostic {
    pub fn new(rule: &'static str, level: Level, file: &Path, message: String) -> Self {
        Self {
            rule,
            level,
            file: file.to_path_buf(),
            line: None,
            message,
            allowance: None,
        }
    }

    #[must_use]
    pub const fn at(mut self, line: usize) -> Self {
        self.line = Some(line);
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
