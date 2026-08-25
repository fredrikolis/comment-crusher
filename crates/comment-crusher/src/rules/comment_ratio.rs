// Concern: fails a code file whose comment characters exceed their allowed share | Non-concern: the size of any one comment, or of a document | IO: (Scan) -> Option<Diagnostic>

use serde::Deserialize;
use std::path::Path;

use crate::diagnostic::{Diagnostic, Level};
use crate::scan::Scan;

pub const NAME: &str = "comment-ratio";
const HELP: &str = "Extract what needed explaining, or delete what the code already says.";

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub level: Level,
    pub max_ratio: f64,
    pub count_doc_comments: bool,
    pub min_chars: usize,
    pub skip_header: bool,
    /// How much of the header the exemption covers. Beyond it the header counts like any
    /// other comment, so an essay cannot shelter under a banner's name.
    pub header_max_chars: usize,
}

#[expect(
    clippy::cast_precision_loss,
    reason = "character counts are far below f64 precision"
)]
pub fn check(cfg: &Config, file: &Path, scan: &Scan) -> Option<Diagnostic> {
    if cfg.level == Level::Allow || cfg.max_ratio <= 0.0 {
        return None;
    }
    let comment = scan.comment_chars(cfg.count_doc_comments, cfg.skip_header)
        + scan.header_excess(cfg.skip_header, cfg.header_max_chars);
    let total = comment + scan.code_chars;
    if total < cfg.min_chars {
        return None;
    }
    let ratio = comment as f64 / total as f64;
    if ratio <= cfg.max_ratio {
        return None;
    }
    Some(Diagnostic::new(
        NAME,
        cfg.level,
        file,
        format!(
            "{:.0}% comment ({comment}/{total} chars), budget is {:.0}%",
            ratio * 100.0,
            cfg.max_ratio * 100.0
        ),
        HELP,
    ))
}
