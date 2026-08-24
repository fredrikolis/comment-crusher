// Concern: the resolved per-language token table a scan matches against, longest token first | Non-concern: reading it from TOML (config.rs) or using it (scan.rs) | IO: none

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
    Str(usize),
}

/// Resolves a language name, so an embedded region can be scanned.
pub trait Resolve {
    fn language_named(&self, name: &str) -> Option<&Syntax>;
}

/// For callers with no language table: every embedded region stays code.
pub struct NoEmbeds;

impl Resolve for NoEmbeds {
    fn language_named(&self, _name: &str) -> Option<&Syntax> {
        None
    }
}

use crate::embed::EmbedSpec;

#[derive(Debug, Clone)]
pub struct Syntax {
    pub name: String,
    pub prose: bool,
    pub nested_block: bool,
    pub hash_raw_strings: bool,
    pub heredoc: bool,
    pub strings: Vec<StringSpec>,
    /// Longest token first, so `///` is matched before `//`.
    pub openers: Vec<(String, Opener)>,
    /// Cancels a line comment when the marker is followed by it: `#[` is a PHP attribute.
    pub line_exceptions: Vec<String>,
    pub embeds: Vec<EmbedSpec>,
}

impl Syntax {
    pub fn match_opener(&self, rest: &str) -> Option<(&str, &Opener)> {
        self.openers
            .iter()
            .find(|(tok, op)| {
                rest.starts_with(tok.as_str())
                    && !(matches!(op, Opener::Line(_))
                        && self.line_exceptions.iter().any(|e| rest.starts_with(e)))
            })
            .map(|(tok, op)| (tok.as_str(), op))
    }

    pub const fn measurable(&self) -> bool {
        self.prose || !self.openers.is_empty() || !self.embeds.is_empty()
    }
}
