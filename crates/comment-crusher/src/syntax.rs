// Concern: the resolved per-language token table a scan matches against, longest token first | Non-concern: reading it from TOML (config.rs) or using it (scan.rs) | IO: none

use crate::embed::EmbedSpec;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommentKind {
    Plain,
    Doc,
}

#[derive(Debug, Clone)]
pub struct StringSpec {
    pub open: String,
    pub close: String,
    pub multiline: bool,
    pub escape: Option<char>,
    pub char_literal: bool,
    pub docstring: bool,
}

#[derive(Debug, Clone)]
pub enum Opener {
    Line(CommentKind),
    Block { close: String, kind: CommentKind },
    Str(StringSpec),
}

pub trait Resolve {
    fn language_named(&self, name: &str) -> Option<&Syntax>;
}

#[derive(Debug, Clone)]
pub struct Syntax {
    pub name: String,
    pub prose: bool,
    pub nested_block: bool,
    pub hash_raw_strings: bool,
    pub heredoc: bool,
    pub strings: Vec<StringSpec>,
    pub openers: Vec<(String, Opener)>,
    pub exceptions: Vec<String>,
    pub cancel_after: Vec<String>,
    pub open_after: Vec<String>,
    pub line_anchored: bool,
    pub doc_prefixes: Vec<String>,
    pub embeds: Vec<EmbedSpec>,
}

impl Syntax {
    /// More than one can match: `/**/` is `/*` `*/`, not an unterminated `/**`.
    pub fn matching_openers<'a>(
        &'a self,
        rest: &'a str,
        before: &'a str,
        own_line: bool,
    ) -> impl Iterator<Item = (&'a str, &'a Opener)> {
        self.openers
            .iter()
            .filter(move |(tok, op)| {
                let comment = matches!(op, Opener::Line(_) | Opener::Block { .. });
                let anchored = self.line_anchored && matches!(op, Opener::Line(_));
                rest.starts_with(tok.as_str())
                    && !(comment && self.exceptions.iter().any(|e| rest.starts_with(e)))
                    && !(comment && self.cancel_after.iter().any(|e| before.ends_with(e)))
                    && (own_line
                        || !anchored
                        || self
                            .open_after
                            .iter()
                            .any(|e| before.trim_end().ends_with(e)))
            })
            .map(|(tok, op)| (tok.as_str(), op))
    }

    pub const fn measurable(&self) -> bool {
        self.prose || !self.openers.is_empty() || !self.embeds.is_empty()
    }
}
