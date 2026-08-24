// Concern: a region of one language written inside another — where it ends, and which language its opening tag names | Non-concern: scanning either language (scan.rs) | IO: (tag text) -> language name

use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct EmbedSpec {
    pub open: String,
    pub close: String,
    pub default: String,
    /// In priority order. Every field below is documented for an author in the `[languages]`
    /// legend of `default_config.toml`, which is where they are written.
    pub attrs: Vec<String>,
    pub map: HashMap<String, String>,
    pub at_start: bool,
    pub balanced: bool,
    pub skip: Vec<String>,
}

impl EmbedSpec {
    /// An opener beginning `<` is a tag, so its body starts past the `>`.
    pub fn is_tag(&self) -> bool {
        self.open.starts_with('<')
    }

    pub fn language_of(&self, attrs: &str) -> &str {
        for name in &self.attrs {
            if let Some(value) = attr_value(attrs, name)
                && let Some(lang) = self.map.get(&value)
            {
                return lang;
            }
        }
        &self.default
    }
}

/// The value of `name="…"`, `name='…'` or unquoted `name=…` in a tag's attribute text.
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
