// Concern: fails any single comment longer than its kind is allowed | Non-concern: a file's total comment share (comment-ratio) | IO: (Scan) -> Vec<Diagnostic>

use serde::Deserialize;
use std::path::Path;

use crate::diagnostic::{Diagnostic, Level};
use crate::scan::{Region, Scan};
use crate::syntax::CommentKind;

pub const NAME: &str = "comment-block";

#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub level: Level,
    pub max_lines: usize,
    /// A doc comment on a public item earns more room than a remark inside a body does.
    pub doc_max_lines: usize,
    /// A licence banner or file annotation is a fixed per-file cost, bounded on its own.
    pub header_max_lines: usize,
    pub max_chars: usize,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            level: Level::Deny,
            max_lines: 5,
            doc_max_lines: 10,
            header_max_lines: 30,
            max_chars: 400,
        }
    }
}

pub fn check(cfg: &Config, file: &Path, scan: &Scan) -> Vec<Diagnostic> {
    if cfg.level == Level::Allow {
        return Vec::new();
    }
    let mut out = Vec::new();
    for r in &scan.regions {
        let (limit, what) = bound(cfg, r);
        if limit > 0 && r.lines() > limit {
            out.push(
                Diagnostic::new(
                    NAME,
                    cfg.level,
                    file,
                    format!("{what} spans {} lines, budget is {limit}", r.lines()),
                )
                .at(r.start_line),
            );
        } else if cfg.max_chars > 0 && r.chars > cfg.max_chars {
            out.push(
                Diagnostic::new(
                    NAME,
                    cfg.level,
                    file,
                    format!("{what} is {} chars, budget is {}", r.chars, cfg.max_chars),
                )
                .at(r.start_line),
            );
        }
    }
    out
}

const fn bound(cfg: &Config, r: &Region) -> (usize, &'static str) {
    if r.header {
        (cfg.header_max_lines, "file header")
    } else if matches!(r.kind, CommentKind::Doc) {
        (cfg.doc_max_lines, "doc comment")
    } else {
        (cfg.max_lines, "comment")
    }
}
