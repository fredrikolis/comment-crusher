// Concern: fails a prose document longer than its budget | Non-concern: anything about a code file, or what a document says | IO: (Scan) -> Option<Diagnostic>

use serde::Deserialize;
use std::path::Path;

use crate::diagnostic::{Diagnostic, Level};
use crate::scan::Scan;

pub const NAME: &str = "doc-length";
const HELP: &str = "Split it, or grant an allowance in .comment-crusher.toml with a reason.";

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub level: Level,
    pub max_lines: usize,
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
        HELP,
    ))
}
