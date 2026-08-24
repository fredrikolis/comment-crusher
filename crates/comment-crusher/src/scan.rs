// Concern: splits a file into comment regions and code, counting the visible characters of each | Non-concern: judging the result, or which language a file is | IO: (text, Syntax) -> Scan

use crate::syntax::{CommentKind, Opener, StringSpec, Syntax};

/// One comment as a reader sees it: a block comment, or a run of adjacent whole-line
/// comments merged into the paragraph they form.
#[derive(Debug, Clone)]
pub struct Region {
    /// Byte span of the whole comment, markers included, so its body can be re-read once
    /// adjacent line comments have been merged into the paragraph they form.
    pub start: usize,
    pub end: usize,
    pub start_line: usize,
    pub end_line: usize,
    pub chars: usize,
    pub kind: CommentKind,
    /// Nothing but whitespace precedes it on its first line.
    pub own_line: bool,
    /// Nothing but whitespace follows it on its last line. With `own_line`, this is what makes
    /// a comment whole-line, and only whole-line comments merge — a run that merged around the
    /// code sitting after each one would charge that code as prose.
    pub ends_line: bool,
    /// The leading comment of a file, above any code: a licence banner, an SPDX line, a file
    /// annotation. A fixed per-file cost, budgeted apart from what the body spends.
    pub header: bool,
}

impl Region {
    pub const fn lines(&self) -> usize {
        self.end_line - self.start_line + 1
    }
}

#[derive(Debug, Clone, Default)]
pub struct Scan {
    pub total_lines: usize,
    pub code_chars: usize,
    pub regions: Vec<Region>,
}

impl Scan {
    /// Comment characters charged against the budget: doc comments only when `count_doc`,
    /// and never the header when `skip_header`.
    pub fn comment_chars(&self, count_doc: bool, skip_header: bool) -> usize {
        self.regions
            .iter()
            .filter(|r| count_doc || r.kind == CommentKind::Plain)
            .filter(|r| !(skip_header && r.header))
            .map(|r| r.chars)
            .sum()
    }

    pub fn header(&self) -> Option<&Region> {
        self.regions.first().filter(|r| r.header)
    }
}

/// Every visible character of a file is either comment or code, markers and delimiters
/// included, so `comment_chars(true, false) + code_chars` is the file's whole visible weight.
pub fn scan(src: &str, syn: &Syntax) -> Scan {
    let mut s = Scanner {
        src,
        syn,
        i: 0,
        line: 1,
        line_start: 0,
        code_chars: 0,
        saw_code: false,
        raw: Vec::new(),
    };
    s.skip_shebang();
    s.run();
    let mut regions = merge(s.raw);
    let mut code_chars = s.code_chars;
    for r in &mut regions {
        let (comment, example) = count_body(&src[r.start..r.end.min(src.len())]);
        r.chars = comment;
        code_chars += example;
    }
    Scan {
        total_lines: src.lines().count(),
        code_chars,
        regions,
    }
}

/// A comment's visible characters, split into prose and the fenced examples inside it. A
/// doctest is code that happens to live in a comment; pricing it as an essay would tax the
/// one thing a doc comment is for.
fn count_body(text: &str) -> (usize, usize) {
    let (mut prose, mut example, mut fenced) = (0, 0, false);
    for line in text.lines() {
        let visible = count_visible(line);
        if is_fence(line) {
            fenced = !fenced;
            prose += visible;
        } else if fenced {
            example += visible;
        } else {
            prose += visible;
        }
    }
    (prose, example)
}

/// Markdown fences are the one convention every doc-comment dialect shares — rustdoc,
/// `JSDoc`, Javadoc, docstrings — so the marker is stripped and the fence read beneath it.
fn is_fence(line: &str) -> bool {
    let body = line
        .trim_start()
        .trim_start_matches(|c: char| "/*#-;%!<>=".contains(c))
        .trim_start();
    body.starts_with("```") || body.starts_with("~~~")
}

/// Adjacent whole-line comments of the same kind read as one comment, so they are bounded
/// as one. A run broken by code or a blank line is two.
fn merge(raw: Vec<Region>) -> Vec<Region> {
    let mut out: Vec<Region> = Vec::with_capacity(raw.len());
    for r in raw {
        match out.last_mut() {
            Some(prev)
                if prev.own_line
                    && prev.ends_line
                    && r.own_line
                    && r.ends_line
                    && prev.kind == r.kind
                    && r.start_line == prev.end_line + 1 =>
            {
                prev.end_line = r.end_line;
                prev.end = r.end;
                prev.chars += r.chars;
            }
            _ => out.push(r),
        }
    }
    out
}

struct Scanner<'a> {
    src: &'a str,
    syn: &'a Syntax,
    i: usize,
    line: usize,
    line_start: usize,
    code_chars: usize,
    /// Whether any code has been seen yet. Not `code_chars > 0`: a shebang is code, and a
    /// file whose first line is one still has its next comment as its header.
    saw_code: bool,
    raw: Vec<Region>,
}

impl<'a> Scanner<'a> {
    fn rest(&self) -> &'a str {
        &self.src[self.i..]
    }

    fn peek(&self) -> Option<char> {
        self.rest().chars().next()
    }

    /// Only whitespace precedes the cursor on the current line.
    fn own_line(&self) -> bool {
        self.src[self.line_start..self.i].trim().is_empty()
    }

    fn skip_shebang(&mut self) {
        if self.src.starts_with("#!") {
            self.advance_to(self.src.find('\n').map_or(self.src.len(), |n| n + 1));
            self.saw_code = false;
        }
    }

    /// Move to `end`, counting newlines and non-whitespace characters as code.
    fn advance_to(&mut self, end: usize) {
        for (off, ch) in self.src[self.i..end].char_indices() {
            if ch == '\n' {
                self.line += 1;
                self.line_start = self.i + off + 1;
            } else if !ch.is_whitespace() {
                self.code_chars += 1;
                self.saw_code = true;
            }
        }
        self.i = end;
    }

    fn run(&mut self) {
        while self.i < self.src.len() {
            if self.take_raw_string() || self.take_heredoc() || self.take_opener() {
                continue;
            }
            let step = self.peek().map_or(1, char::len_utf8);
            self.advance_to(self.i + step);
        }
    }

    fn take_opener(&mut self) -> bool {
        let Some((tok, op)) = self.syn.match_opener(self.rest()) else {
            return false;
        };
        let (tok, op) = (tok.to_owned(), op.clone());
        match op {
            Opener::Line(kind) => self.consume_line_comment(kind),
            Opener::Block { close, kind } => self.consume_block(&tok, &close, kind),
            Opener::Str(idx) => {
                let Some(spec) = self.syn.strings.get(idx).cloned() else {
                    return false;
                };
                return self.consume_string(&spec);
            }
        }
        true
    }

    fn open_region(&self) -> (usize, bool) {
        (self.line, self.own_line())
    }

    fn push_region(
        &mut self,
        span: (usize, usize),
        start: usize,
        own_line: bool,
        kind: CommentKind,
    ) {
        let header = own_line && !self.saw_code && self.raw.is_empty();
        let ends_line = self.src[span.1.min(self.src.len())..]
            .chars()
            .take_while(|c| *c != '\n')
            .all(char::is_whitespace);
        self.raw.push(Region {
            start: span.0,
            end: span.1,
            start_line: start,
            end_line: self.line,
            chars: 0,
            kind,
            own_line,
            ends_line,
            header,
        });
    }

    fn consume_line_comment(&mut self, kind: CommentKind) {
        let (start, own) = self.open_region();
        let end = self.src[self.i..]
            .find('\n')
            .map_or(self.src.len(), |n| self.i + n);
        let span = (self.i, end);
        self.i = end;
        self.push_region(span, start, own, kind);
    }

    fn consume_block(&mut self, open: &str, close: &str, kind: CommentKind) {
        let (start, own) = self.open_region();
        let mut j = self.i + open.len();
        let mut depth = 1usize;
        while j < self.src.len() {
            let rest = &self.src[j..];
            if rest.starts_with(close) {
                depth -= 1;
                j += close.len();
                if depth == 0 {
                    break;
                }
                continue;
            }
            if self.syn.nested_block && rest.starts_with(open) {
                depth += 1;
                j += open.len();
                continue;
            }
            j += rest.chars().next().map_or(1, char::len_utf8);
        }
        let end = j.min(self.src.len());
        self.count_newlines(self.i, j);
        let span = (self.i, end);
        self.i = end;
        self.push_region(span, start, own, kind);
    }

    /// A string's contents are code. A docstring opening its own line is prose, and counted.
    fn consume_string(&mut self, spec: &StringSpec) -> bool {
        let (start, own) = self.open_region();
        let Some(end) = self.string_end(spec) else {
            return false;
        };
        let is_doc = spec.docstring && own;
        if is_doc {
            self.count_newlines(self.i, end);
            let span = (self.i, end);
            self.i = end;
            self.push_region(span, start, own, CommentKind::Doc);
        } else {
            self.advance_to(end);
        }
        true
    }

    /// Byte index just past the closing delimiter, or `None` when this is not a string here.
    fn string_end(&self, spec: &StringSpec) -> Option<usize> {
        let mut j = self.i + spec.open.len();
        while j < self.src.len() {
            let rest = &self.src[j..];
            let ch = rest.chars().next()?;
            if ch == '\n' && !spec.multiline {
                return None;
            }
            if spec.escape.is_some_and(|e| ch == e) {
                j += ch.len_utf8();
                j += rest[ch.len_utf8()..]
                    .chars()
                    .next()
                    .map_or(0, char::len_utf8);
                continue;
            }
            if rest.starts_with(spec.close.as_str()) {
                let end = j + spec.close.len();
                // A char literal that ran long is a lifetime or a quoted word, not a string.
                if spec.char_literal && end - self.i > 12 {
                    return None;
                }
                return Some(end);
            }
            j += ch.len_utf8();
        }
        None
    }

    /// `r"..."` and `r#"..."#`: no escape processing, so the closing quote is the one
    /// followed by as many `#` as the opener carried.
    fn take_raw_string(&mut self) -> bool {
        if !self.syn.hash_raw_strings || self.peek() != Some('r') {
            return false;
        }
        if self.i > 0 && prev_is_ident(&self.src[..self.i]) {
            return false;
        }
        let after_r = &self.src[self.i + 1..];
        let hashes = after_r.bytes().take_while(|b| *b == b'#').count();
        if !after_r[hashes..].starts_with('"') {
            return false;
        }
        let terminator = format!("\"{}", "#".repeat(hashes));
        let body = self.i + 1 + hashes + 1;
        let end = self.src[body..]
            .find(&terminator)
            .map_or(self.src.len(), |n| body + n + terminator.len());
        self.advance_to(end);
        true
    }

    /// `<<WORD`, `<<-'WORD'`, `<<~WORD`: the body is code until a line equal to WORD.
    fn take_heredoc(&mut self) -> bool {
        if !self.syn.heredoc || !self.rest().starts_with("<<") {
            return false;
        }
        let Some((word, indented)) = heredoc_word(&self.src[self.i + 2..]) else {
            return false;
        };
        let Some(nl) = self.src[self.i..].find('\n') else {
            return false;
        };
        let mut j = self.i + nl + 1;
        while j < self.src.len() {
            let end = self.src[j..].find('\n').map_or(self.src.len(), |n| j + n);
            let text = &self.src[j..end];
            let matches = if indented {
                text.trim() == word
            } else {
                text.trim_end() == word
            };
            j = (end + 1).min(self.src.len());
            if matches {
                break;
            }
        }
        self.advance_to(j);
        true
    }

    fn count_newlines(&mut self, from: usize, to: usize) {
        for (off, ch) in self.src[from..to.min(self.src.len())].char_indices() {
            if ch == '\n' {
                self.line += 1;
                self.line_start = from + off + 1;
            }
        }
    }
}

fn count_visible(s: &str) -> usize {
    s.chars().filter(|c| !c.is_whitespace()).count()
}

fn prev_is_ident(before: &str) -> bool {
    before
        .chars()
        .next_back()
        .is_some_and(|c| c.is_alphanumeric() || c == '_')
}

/// The terminator of a heredoc opener, and whether its closing line may be indented.
fn heredoc_word(after: &str) -> Option<(String, bool)> {
    let mut rest = after;
    let indented = rest.starts_with('-') || rest.starts_with('~');
    if indented {
        rest = &rest[1..];
    }
    let quote = rest.chars().next().filter(|c| *c == '\'' || *c == '"');
    if quote.is_some() {
        rest = &rest[1..];
    }
    let word: String = rest
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_')
        .collect();
    if word.len() < 2 || word.chars().next().is_some_and(char::is_numeric) {
        return None;
    }
    Some((word, indented))
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    reason = "a failed lookup in a test is a failed test"
)]
#[path = "scan_tests.rs"]
mod tests;
