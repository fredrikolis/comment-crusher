// Concern: where a byte offset sits in a text, in lines and characters | Non-concern: what the text means — the scanner reads comments, config.rs reads TOML | IO: (text, offset) -> (line, column)

pub fn place(src: &str, offset: usize) -> (usize, usize) {
    let before = &src[..offset.min(src.len())];
    let line_start = before.rfind('\n').map_or(0, |n| n + 1);
    (
        before.matches('\n').count() + 1,
        before[line_start..].chars().count() + 1,
    )
}
