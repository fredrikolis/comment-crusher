// Concern: the rule set — its thresholds, and running every rule a file is subject to | Non-concern: what any one rule measures, or how a file is scanned | IO: (Scan) -> Vec<Diagnostic>

pub mod comment_block;
pub mod comment_ratio;
pub mod doc_length;

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
}

impl Rules {
    pub fn from_table(table: &Table) -> Result<Self> {
        Ok(Self {
            comment_ratio: rule(table, comment_ratio::NAME)?,
            comment_block: rule(table, comment_block::NAME)?,
            doc_length: rule(table, doc_length::NAME)?,
        })
    }

    /// A document is bounded by its length and a code file by its comment budget. Neither
    /// bound means anything applied to the other.
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

fn rule<T: for<'de> serde::Deserialize<'de> + Default>(table: &Table, name: &str) -> Result<T> {
    table.get(name).map_or_else(
        || Ok(T::default()),
        |v| {
            v.clone()
                .try_into()
                .with_context(|| format!("[rules.{name}] is malformed"))
        },
    )
}
