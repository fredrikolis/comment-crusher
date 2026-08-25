// Concern: holds the README and --help to the numbers that ship | Non-concern: whether a bound is right (default_config.toml argues that) | IO: (docs, config) -> pass/fail

#![allow(
    clippy::expect_used,
    reason = "a failed expectation in a test is a failed test"
)]

use comment_crusher::cli::Cli;
use std::path::Path;

fn readme() -> String {
    std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../README.md"))
        .expect("README.md")
}

fn defaults() -> toml::Table {
    toml::from_str(include_str!("../src/default_config.toml")).expect("defaults parse")
}

fn bound(table: &toml::Table, rule: &str, field: &str) -> String {
    table["rules"][rule][field].to_string()
}

/// Every shipped bound the README quotes, so a threshold cannot move without the sentence
/// that names it moving too.
#[test]
fn the_readme_quotes_the_bounds_that_ship() {
    let t = defaults();
    let text = readme();
    let languages = t["languages"].as_table().expect("languages").len();
    let repos =
        std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus.toml"))
            .expect("corpus.toml")
            .matches("[[repo]]")
            .count();
    let claims = [
        (format!("{languages} languages"), "the language count"),
        (format!("{repos} repositories"), "the corpus size"),
        (format!("{repos} pinned repositories"), "the corpus size"),
        (
            format!("{} lines", bound(&t, "doc-length", "max_lines")),
            "doc-length",
        ),
        (
            format!("{} chars", bound(&t, "comment-block", "max_chars")),
            "comment-block.max_chars",
        ),
        (
            format!(
                "{} for a doc comment",
                bound(&t, "comment-block", "doc_max_lines")
            ),
            "comment-block.doc_max_lines",
        ),
        (
            format!(
                "{} for a banner",
                bound(&t, "comment-block", "header_max_lines")
            ),
            "comment-block.header_max_lines",
        ),
        (
            format!(
                "under {} chars skipped",
                bound(&t, "comment-ratio", "min_chars")
            ),
            "comment-ratio.min_chars",
        ),
    ];
    let missing: Vec<&str> = claims
        .iter()
        .filter(|(needle, _)| !text.contains(needle.as_str()))
        .map(|(_, what)| *what)
        .collect();
    assert!(
        missing.is_empty(),
        "README no longer states what ships for: {missing:?}"
    );
}

/// The exit codes are written in three places, and an agent branches on them.
#[test]
fn the_readme_and_help_list_the_exit_codes_the_binary_returns() {
    let text = readme();
    let help = Cli::after_help();
    for code in ["0", "1", "2", "3", "24"] {
        assert!(text.contains(&format!("| {code} |")), "README omits {code}");
        assert!(
            help.lines().any(|l| l.trim_start().starts_with(code)),
            "--help omits {code}"
        );
    }
}
