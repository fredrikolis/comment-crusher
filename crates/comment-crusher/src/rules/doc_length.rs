// Concern: fails a prose document longer than its budget | Non-concern: anything about a code file, or what a document says | IO: (Scan) -> Option<Diagnostic>

use serde::Deserialize;
use std::path::Path;

use crate::diagnostic::{Diagnostic, Level};
use crate::scan::Scan;

pub const NAME: &str = "doc-length";

#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub level: Level,
    pub max_lines: usize,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            level: Level::Deny,
            max_lines: 400,
        }
    }
}

pub fn check(cfg: &Config, file: &Path, scan: &Scan) -> Option<Diagnostic> {
    if cfg.level == Level::Allow || cfg.max_lines == 0 || scan.total_lines <= cfg.max_lines {
        return None;
    }
    Some(Diagnostic::new(
        NAME,
        cfg.level,
        file,
        format!(
            "document is {} lines, budget is {}",
            scan.total_lines, cfg.max_lines
        ),
    ))
}
