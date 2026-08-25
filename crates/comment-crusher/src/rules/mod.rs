// Concern: the rule set — its thresholds, and running every rule a scanned file is subject to | Non-concern: what a rule measures, or the rules for a file that was never scanned | IO: (Scan) -> Vec<Diagnostic>

pub mod comment_block;
pub mod comment_ratio;
pub mod doc_length;
pub mod unreadable;

use anyhow::{Context, Result};
use std::path::Path;
use toml::Table;

use crate::diagnostic::Diagnostic;
use crate::scan::Scan;
use crate::syntax::Syntax;

#[derive(Debug, Clone)]
pub struct Rules {
    pub comment_ratio: comment_ratio::Config,
    pub comment_block: comment_block::Config,
    pub doc_length: doc_length::Config,
    pub unreadable: unreadable::Config,
}

impl Rules {
    pub fn from_table(table: &Table) -> Result<Self> {
        Ok(Self {
            comment_ratio: rule(table, comment_ratio::NAME)?,
            comment_block: rule(table, comment_block::NAME)?,
            doc_length: rule(table, doc_length::NAME)?,
            unreadable: rule(table, unreadable::NAME)?,
        })
    }

    /// Neither bound means anything applied to the other kind of file.
    pub fn check(&self, file: &Path, syn: &Syntax, scan: &Scan) -> Vec<Diagnostic> {
        if syn.prose {
            return doc_length::check(&self.doc_length, file, scan)
                .into_iter()
                .collect();
        }
        let mut out = comment_ratio::check(&self.comment_ratio, file, scan)
            .into_iter()
            .collect::<Vec<_>>();
        out.extend(comment_block::check(&self.comment_block, file, scan));
        out
    }
}

/// A missing section means the built-in defaults were lost, not overridden.
fn rule<T: for<'de> serde::Deserialize<'de>>(table: &Table, name: &str) -> Result<T> {
    table
        .get(name)
        .with_context(|| format!("[rules.{name}] is missing"))?
        .clone()
        .try_into()
        .with_context(|| format!("[rules.{name}] is malformed"))
}
