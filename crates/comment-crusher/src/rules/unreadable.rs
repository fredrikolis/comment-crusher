// Concern: fails a file whose language resolved but which could not be read at all | Non-concern: reading it (engine.rs), or decoding one that is not UTF-8 | IO: (path, error) -> Diagnostic

use serde::Deserialize;
use std::path::Path;

use crate::diagnostic::{Diagnostic, Level};

pub const NAME: &str = "unreadable";
const HELP: &str = "Exclude it, or give its extension a language that fits.";

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub level: Level,
}

/// A file whose extension named a language but whose bytes are not text at all.
pub fn binary(cfg: &Config, file: &Path) -> Option<Diagnostic> {
    (cfg.level != Level::Allow).then(|| {
        Diagnostic::new(
            NAME,
            cfg.level,
            file,
            "is not UTF-8 or Latin-1 text, so nothing in it is measured".to_string(),
            HELP,
        )
    })
}

pub fn check(cfg: &Config, file: &Path, error: &std::io::Error) -> Option<Diagnostic> {
    (cfg.level != Level::Allow).then(|| {
        Diagnostic::new(
            NAME,
            cfg.level,
            file,
            format!("could not be read, so nothing in it is measured: {error}"),
            HELP,
        )
    })
}
