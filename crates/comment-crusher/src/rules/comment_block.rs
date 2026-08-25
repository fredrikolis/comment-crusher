// Concern: fails any single comment longer than its kind is allowed | Non-concern: a file's total comment share (comment-ratio) | IO: (Scan) -> Vec<Diagnostic>

use serde::Deserialize;
use std::path::Path;

use crate::diagnostic::{Diagnostic, Level};
use crate::scan::{Region, Scan};
use crate::syntax::CommentKind;

pub const NAME: &str = "comment-block";
const LINES: &str = "comment-block.lines";
const CHARS: &str = "comment-block.chars";
const HELP: &str = "One line, or change the shape of what needed a paragraph.";

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub level: Level,
    pub max_lines: usize,
    pub doc_max_lines: usize,
    pub header_max_lines: usize,
    pub max_chars: usize,
    pub doc_max_chars: usize,
    pub header_max_chars: usize,
}

pub fn check(cfg: &Config, file: &Path, scan: &Scan) -> Vec<Diagnostic> {
    if cfg.level == Level::Allow {
        return Vec::new();
    }
    let mut out = Vec::new();
    for r in &scan.regions {
        let (limit, chars, what) = bound(cfg, r);
        if limit > 0 && r.lines() > limit {
            out.push(
                Diagnostic::new(
                    LINES,
                    cfg.level,
                    file,
                    format!("{what} spans {} lines, budget is {limit}", r.lines()),
                    HELP,
                )
                .at(r.start_line)
                .spanning(r.start, r.end, r.end_line)
                .columns(r.start_column, r.end_column),
            );
        } else if chars > 0 && r.chars > chars {
            out.push(
                Diagnostic::new(
                    CHARS,
                    cfg.level,
                    file,
                    format!("{what} is {} chars, budget is {chars}", r.chars),
                    HELP,
                )
                .at(r.start_line)
                .spanning(r.start, r.end, r.end_line)
                .columns(r.start_column, r.end_column),
            );
        }
    }
    out
}

const fn bound(cfg: &Config, r: &Region) -> (usize, usize, &'static str) {
    if r.header {
        (cfg.header_max_lines, cfg.header_max_chars, "file header")
    } else if matches!(r.kind, CommentKind::Doc) {
        (cfg.doc_max_lines, cfg.doc_max_chars, "doc comment")
    } else {
        (cfg.max_lines, cfg.max_chars, "comment")
    }
}
