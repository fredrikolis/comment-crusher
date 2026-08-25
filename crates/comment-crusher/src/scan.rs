// Concern: splits a file into comment regions and code, counting the visible characters of each | Non-concern: judging the result, or which language a file is | IO: (text, Syntax) -> Scan

use crate::embed::EmbedSpec;
use crate::syntax::{CommentKind, Opener, Resolve, StringSpec, Syntax};

/// A block comment, or a run of adjacent whole-line comments merged into one paragraph.
#[derive(Debug, Clone)]
pub struct Region {
    /// Markers included, so a merged run re-reads as the one comment it forms.
    pub start: usize,
    pub end: usize,
    pub start_line: usize,
    pub end_line: usize,
    pub chars: usize,
    /// 1-based; `end_column` is exclusive, as `end` is.
    pub start_column: usize,
    pub end_column: usize,
    pub kind: CommentKind,
    pub opener: String,
    own_line: bool,
    /// Only whole-line comments merge: one merging around trailing code would charge it prose.
    ends_line: bool,
    /// The leading comment above any code — a fixed per-file cost, budgeted apart.
    pub header: bool,
    /// Lifted from an embedded child scan, which already counted and merged it. Its
    /// `own_line`/`ends_line` were computed in the child's coordinates, so re-reading it
    /// against the parent source would bill the surrounding markup twice.
    nested: bool,
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
    /// `None` charges a header in full, not just its excess.
    pub fn charged_chars(&self, count_doc: bool, header_allowance: Option<usize>) -> usize {
        self.regions
            .iter()
            .filter(|r| count_doc || r.kind == CommentKind::Plain)
            .map(|r| match header_allowance {
                Some(allowed) if r.header => r.chars.saturating_sub(allowed),
                _ => r.chars,
            })
            .sum()
    }
}

/// A comment just read, before it becomes a `Region`.
struct Found {
    span: (usize, usize),
    start_line: usize,
    own_line: bool,
    kind: CommentKind,
    opener: String,
}

/// A component, its `<script>`, a template within that; and self-embedding terminates.
const MAX_EMBED_DEPTH: usize = 3;

/// Comment and code are an exact partition of the file's visible characters.
pub fn scan_in(src: &str, syn: &Syntax, resolve: &dyn Resolve) -> Scan {
    scan_at(src, syn, resolve, 0)
}

fn scan_at(src: &str, syn: &Syntax, resolve: &dyn Resolve, depth: usize) -> Scan {
    let mut s = Scanner {
        src,
        syn,
        resolve,
        depth,
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
        r.start_column = column_of(src, r.start);
        r.end_column = column_of(src, r.end);
        if r.nested {
            continue;
        }
        let (comment, example) = count_body(&src[r.start..r.end]);
        r.chars = comment;
        code_chars += example;
    }
    Scan {
        total_lines: src.lines().count(),
        code_chars,
        regions,
    }
}

/// Split into prose and fenced example: a doctest is code that happens to live in a comment.
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

/// The punctuation declared markers are made of; a word marker like `REM ` is not, since
/// stripping letters would eat prose.
const MARKER_CHARS: &str = r"!#%'(*+-/:;<=[{|";

fn is_fence(line: &str) -> bool {
    let body = line
        .trim_start()
        .trim_start_matches(|c: char| MARKER_CHARS.contains(c))
        .trim_start();
    body.starts_with("```") || body.starts_with("~~~")
}

/// Adjacent whole-line comments of one kind are one comment, bounded as one.
fn merge(raw: Vec<Region>) -> Vec<Region> {
    let mut out: Vec<Region> = Vec::with_capacity(raw.len());
    for r in raw {
        match out.last_mut() {
            Some(prev)
                if !prev.nested
                    && !r.nested
                    && prev.own_line
                    && prev.ends_line
                    && r.own_line
                    && r.ends_line
                    && prev.kind == r.kind
                    && r.start_line == prev.end_line + 1 =>
            {
                prev.end_line = r.end_line;
                prev.end = r.end;
            }
            _ => out.push(r),
        }
    }
    out
}

struct Scanner<'a> {
    src: &'a str,
    syn: &'a Syntax,
    resolve: &'a dyn Resolve,
    depth: usize,
    i: usize,
    line: usize,
    line_start: usize,
    code_chars: usize,
    /// Not `code_chars > 0`: a shebang is code, but the comment under it is still a header.
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

    fn opens_doc(&self) -> bool {
        let before = self.src[self.line_start..self.i].trim_start();
        before.is_empty() || self.syn.doc_prefixes.iter().any(|p| before.trim_end() == p)
    }

    fn own_line(&self) -> bool {
        self.src[self.line_start..self.i].trim().is_empty()
    }

    fn skip_shebang(&mut self) {
        if self.src.starts_with("#!") {
            self.advance_to(self.src.find('\n').map_or(self.src.len(), |n| n + 1));
            self.saw_code = false;
        }
    }

    fn advance_to(&mut self, end: usize) {
        let from = self.i;
        self.count_newlines(from, end);
        for ch in self.src[from..end.min(self.src.len())].chars() {
            if ch != '\n' && !ch.is_whitespace() {
                self.code_chars += 1;
                self.saw_code = true;
            }
        }
        self.i = end;
    }

    fn run(&mut self) {
        while self.i < self.src.len() {
            if self.take_embed()
                || self.take_raw_string()
                || self.take_heredoc()
                || self.take_opener()
            {
                continue;
            }
            let step = self.peek().map_or(1, char::len_utf8);
            self.advance_to(self.i + step);
        }
    }

    /// The markup is code; the body is the language the tag names, or code if unknown.
    fn take_embed(&mut self) -> bool {
        if self.depth >= MAX_EMBED_DEPTH {
            return false;
        }
        let Some((spec, body_start)) = self.match_embed() else {
            return false;
        };
        let attrs = &self.src[self.i..body_start];
        let end = self.embed_end(&spec, body_start);
        self.advance_to(body_start);

        let body = &self.src[body_start..end];
        match self.resolve.language_named(&spec.language_of(attrs)) {
            Some(child) => {
                let inner = scan_at(body, child, self.resolve, self.depth + 1);
                let (offset, line) = (body_start, self.line);
                for mut r in inner.regions {
                    r.start += offset;
                    r.end += offset;
                    r.start_line += line - 1;
                    r.end_line += line - 1;
                    r.header = false;
                    r.nested = true;
                    self.raw.push(r);
                }
                self.code_chars += inner.code_chars;
                self.count_newlines(body_start, end);
                self.saw_code = true;
                self.i = end;
            }
            None => self.advance_to(end),
        }
        true
    }

    fn embed_end(&self, spec: &EmbedSpec, body_start: usize) -> usize {
        if !spec.balanced {
            return find_ci(&self.src[body_start..], &spec.close)
                .map_or(self.src.len(), |n| body_start + n);
        }
        let (mut depth, mut j) = (1usize, body_start);
        while j < self.src.len() {
            let rest = &self.src[j..];
            if starts_with_ci(rest, &spec.close) {
                depth -= 1;
                if depth == 0 {
                    return j;
                }
                j += spec.close.len();
                continue;
            }
            if starts_with_ci(rest, &spec.open) {
                depth += 1;
                j += spec.open.len();
                continue;
            }
            j += rest.chars().next().map_or(1, char::len_utf8);
        }
        self.src.len()
    }

    fn terminates(&self, tok: &str, op: &Opener) -> bool {
        match op {
            Opener::Block { close, .. } => self.block_end(tok, close).is_some(),
            _ => true,
        }
    }

    /// The embed whose opening tag starts here, and the byte its body begins at.
    fn match_embed(&self) -> Option<(EmbedSpec, usize)> {
        let rest = self.rest();
        let spec = self.syn.embeds.iter().find(|e| {
            (!e.at_start || self.i == 0)
                && starts_with_ci(rest, &e.open)
                && !e
                    .skip
                    .iter()
                    .any(|k| starts_with_ci(&rest[e.open.len()..], k))
                && (!e.is_tag()
                    || rest[e.open.len()..]
                        .chars()
                        .next()
                        .is_some_and(|c| c.is_whitespace() || c == '>' || c == '/'))
        })?;
        let body_start = if spec.is_tag() {
            self.i + rest.find('>')? + 1
        } else {
            self.i + spec.open.len()
        };
        Some((spec.clone(), body_start))
    }

    fn take_opener(&mut self) -> bool {
        let candidates: Vec<(String, Opener)> = self
            .syn
            .matching_openers(
                self.rest(),
                &self.src[self.line_start..self.i],
                self.own_line(),
            )
            .map(|(t, o)| (t.to_owned(), o.clone()))
            .collect();
        // An unterminated block really does comment out the rest of the file; `/**/` is not one.
        let Some((tok, op)) = candidates
            .iter()
            .find(|(t, o)| self.terminates(t, o))
            .or_else(|| candidates.first())
            .cloned()
        else {
            return false;
        };
        match op {
            Opener::Line(kind) => self.consume_line_comment(&tok, kind),
            Opener::Block { close, kind } => self.consume_block(&tok, &close, kind),
            Opener::Str(spec) => return self.consume_string(&spec),
        }
        true
    }

    fn open_region(&self) -> (usize, bool) {
        (self.line, self.own_line())
    }

    fn push_region(&mut self, found: Found) {
        let Found {
            span,
            start_line: start,
            own_line,
            kind,
            opener,
        } = found;
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
            start_column: 1,
            end_column: 1,
            kind,
            opener,
            own_line,
            ends_line,
            header,
            nested: false,
        });
    }

    fn consume_line_comment(&mut self, tok: &str, kind: CommentKind) {
        let (start, own) = self.open_region();
        let end = self.src[self.i..]
            .find('\n')
            .map_or(self.src.len(), |n| self.i + n);
        let span = (self.i, end);
        self.i = end;
        self.push_region(Found {
            span,
            start_line: start,
            own_line: own,
            kind,
            opener: tok.to_string(),
        });
    }

    /// Byte past the closing delimiter, or `None` when the block never closes.
    fn block_end(&self, open: &str, close: &str) -> Option<usize> {
        let mut j = self.i + open.len();
        let mut depth = 1usize;
        while j < self.src.len() {
            let rest = &self.src[j..];
            if rest.starts_with(close) {
                depth -= 1;
                j += close.len();
                if depth == 0 {
                    return Some(j);
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
        None
    }

    fn consume_block(&mut self, open: &str, close: &str, kind: CommentKind) {
        let (start, own) = self.open_region();
        let j = self.block_end(open, close).unwrap_or(self.src.len());
        let end = j.min(self.src.len());
        self.count_newlines(self.i, j);
        let span = (self.i, end);
        self.i = end;
        self.push_region(Found {
            span,
            start_line: start,
            own_line: own,
            kind,
            opener: open.to_string(),
        });
    }

    /// A string is code; a docstring opening its own line is prose.
    fn consume_string(&mut self, spec: &StringSpec) -> bool {
        let (start, own) = self.open_region();
        let Some(end) = self.string_end(spec) else {
            return false;
        };
        let is_doc = spec.docstring && self.opens_doc();
        if is_doc {
            self.count_newlines(self.i, end);
            let span = (self.i, end);
            self.i = end;
            self.push_region(Found {
                span,
                start_line: start,
                own_line: own,
                kind: CommentKind::Doc,
                opener: spec.open.clone(),
            });
        } else {
            self.advance_to(end);
        }
        true
    }

    /// Past the closing delimiter, or `None` when this is not a string here.
    fn string_end(&self, spec: &StringSpec) -> Option<usize> {
        let mut j = self.i + spec.open.len();
        while j < self.src.len() {
            let rest = &self.src[j..];
            let Some(ch) = rest.chars().next() else { break };
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
                if spec.char_literal && self.src[self.i..end].chars().count() > 12 {
                    return None;
                }
                return Some(end);
            }
            j += ch.len_utf8();
        }
        None
    }

    /// `r#"…"#`: no escapes, so the close is the quote trailed by the opener's `#` count.
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

    /// `<<WORD`, `<<-'WORD'`, `<<~WORD`: code until a line equal to WORD.
    fn take_heredoc(&mut self) -> bool {
        if !self.syn.heredoc || !self.rest().starts_with("<<") {
            return false;
        }
        // `a<<item` is a shift or an append. A heredoc never opens against an identifier.
        if self.i > 0 && prev_is_ident(&self.src[..self.i]) {
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

    /// The one place a line number moves, so its callers cannot drift.
    fn count_newlines(&mut self, from: usize, to: usize) {
        for (off, ch) in self.src[from..to.min(self.src.len())].char_indices() {
            if ch == '\n' {
                self.line += 1;
                self.line_start = from + off + 1;
            }
        }
    }
}

fn starts_with_ci(haystack: &str, needle: &str) -> bool {
    haystack
        .as_bytes()
        .get(..needle.len())
        .is_some_and(|head| head.eq_ignore_ascii_case(needle.as_bytes()))
}

fn find_ci(haystack: &str, needle: &str) -> Option<usize> {
    if needle.is_empty() {
        return None;
    }
    haystack
        .as_bytes()
        .windows(needle.len())
        .position(|w| w.eq_ignore_ascii_case(needle.as_bytes()))
}

fn column_of(src: &str, offset: usize) -> usize {
    crate::text::place(src, offset).1
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

/// A heredoc's terminator, and whether its closing line may be indented.
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
