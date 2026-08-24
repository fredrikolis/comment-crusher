// Concern: a region of one language written inside another — where it ends, and which language its opening tag names | Non-concern: scanning either language (scan.rs) | IO: (tag text) -> language name

use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct EmbedSpec {
    pub open: String,
    pub close: String,
    pub default: String,
    /// In priority order.
    pub attrs: Vec<String>,
    pub map: HashMap<String, String>,
    pub at_start: bool,
    pub balanced: bool,
    pub skip: Vec<String>,
}

impl EmbedSpec {
    pub fn is_tag(&self) -> bool {
        self.open.starts_with('<')
    }

    /// An attribute that is present but unmapped names a language this table does not have,
    /// so the body stays code. Falling back to `default` there would guess.
    pub fn language_of(&self, attrs: &str) -> String {
        for name in &self.attrs {
            if let Some(value) = attr_value(attrs, name) {
                return self.map.get(&value).cloned().unwrap_or(value);
            }
        }
        self.default.clone()
    }
}

fn attr_value(attrs: &str, name: &str) -> Option<String> {
    let mut rest = attrs;
    while let Some(at) = rest.to_ascii_lowercase().find(name) {
        let after = &rest[at + name.len()..];
        let before_ok = at == 0
            || rest[..at]
                .chars()
                .next_back()
                .is_some_and(char::is_whitespace);
        let value = after.trim_start().strip_prefix('=').map(str::trim_start);
        if let (true, Some(value)) = (before_ok, value) {
            let quote = value.chars().next().filter(|c| *c == '"' || *c == '\'');
            let body = match quote {
                Some(q) => value[1..].split(q).next()?,
                None => value.split_whitespace().next()?,
            };
            return Some(body.trim().to_ascii_lowercase());
        }
        rest = after;
    }
    None
}
