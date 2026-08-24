// Concern: the library surface — the config, scanner, rules and engine a caller composes | Non-concern: argv parsing or process exit (main.rs owns those) | IO: none

pub mod cli;
pub mod config;
pub mod diagnostic;
pub mod engine;
pub mod rules;
pub mod scan;
pub mod syntax;

pub use config::Config;
pub use diagnostic::{Diagnostic, Level};
pub use engine::{Engine, FileStat, Report};
pub use scan::{Scan, scan};
pub use syntax::Syntax;
