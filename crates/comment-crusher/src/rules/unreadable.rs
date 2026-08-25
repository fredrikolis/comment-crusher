// Concern: fails a resolved file nothing can be measured in, whether it is binary or unreadable | Non-concern: reading it or deciding it is binary (engine.rs) | IO: (path, error) -> Diagnostic

use serde::Deserialize;
use std::path::Path;

use crate::diagnostic::{Diagnostic, Level};

pub const NAME: &str = "unreadable";
const BINARY: &str = "unreadable.binary";
const IO: &str = "unreadable.io";
const HELP: &str = "Exclude it, or give its extension a language that fits.";

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub level: Level,
}

pub fn binary(cfg: &Config, file: &Path) -> Option<Diagnostic> {
    (cfg.level != Level::Allow).then(|| {
        Diagnostic::new(
            BINARY,
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
            IO,
            cfg.level,
            file,
            format!("could not be read, so nothing in it is measured: {error}"),
            HELP,
        )
    })
}
