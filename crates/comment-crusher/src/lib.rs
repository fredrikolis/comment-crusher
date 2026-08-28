// Concern: the library surface — the config, scanner, rules, engine, hook and CLI a caller composes | Non-concern: running one (main.rs owns argv and the process exit) | IO: none

pub mod cli;
pub mod config;
pub mod diagnostic;
pub mod embed;
pub mod engine;
pub mod exit;
mod hook;
pub mod rules;
pub mod scan;
pub mod syntax;
mod text;

pub use config::Config;
pub use diagnostic::Diagnostic;
pub use engine::Engine;
pub use scan::scan_in;
