// Concern: holds the README, --help and every bound to the figures and defaults that ship | Non-concern: measuring the corpus (tests/corpus.rs) | IO: (docs, config, figures) -> pass/fail

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

fn claims(
    t: &toml::Table,
    languages: usize,
    repos: usize,
    ratio: f64,
) -> Vec<(String, &'static str)> {
    vec![
        (format!("{languages} languages"), "the language count"),
        (format!("{repos} repositor"), "the corpus size"),
        (
            format!("{repos} pinned repositor"),
            "the corpus size, again",
        ),
        (
            format!("{} lines", bound(t, "doc-length", "max_lines")),
            "doc-length",
        ),
        (
            format!(
                "{} line and {} chars",
                bound(t, "comment-block", "max_lines"),
                bound(t, "comment-block", "max_chars")
            ),
            "comment-block remark bounds",
        ),
        (
            format!(
                "{} and {} for a doc comment",
                bound(t, "comment-block", "doc_max_lines"),
                bound(t, "comment-block", "doc_max_chars")
            ),
            "comment-block doc bounds",
        ),
        (
            format!(
                "{} and {} for a banner",
                bound(t, "comment-block", "header_max_lines"),
                bound(t, "comment-block", "header_max_chars")
            ),
            "comment-block header bounds",
        ),
        (
            format!(
                "under {} chars skipped",
                bound(t, "comment-ratio", "min_chars")
            ),
            "comment-ratio.min_chars",
        ),
        (format!("{:.0}%,", ratio * 100.0), "comment-ratio.max_ratio"),
    ]
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
    let ratio = t["rules"]["comment-ratio"]["max_ratio"]
        .as_float()
        .expect("max_ratio is a float");
    let claims = claims(&t, languages, repos, ratio);
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
            help.lines()
                .any(|l| l.split_whitespace().next() == Some(code)),
            "--help omits {code}"
        );
    }
}

/// Every numeric bound a rule ships must be one an allowance can widen, or adding a field to
/// a rule leaves it silently unreachable from `[[allow]]`.
#[test]
fn every_shipped_bound_is_widenable() {
    let t = defaults();
    let rules = t["rules"].as_table().expect("rules");
    let mut missing = Vec::new();
    for (rule, body) in rules {
        for (field, value) in body.as_table().expect("rule table") {
            // `level` is not a bound, and a floor decides whether a rule applies at all.
            if value.as_integer().is_none() && value.as_float().is_none() {
                continue;
            }
            if matches!(field.as_str(), "min_chars" | "header_free_chars") {
                continue;
            }
            let path = format!("{rule}.{field}");
            if !comment_crusher::config::widenable(&path) {
                missing.push(path);
            }
        }
    }
    assert!(
        missing.is_empty(),
        "shipped bounds no allowance can widen: {missing:?}"
    );
}

/// Each derived bound equals the corpus figure its `# Measured:` line names, so a bound and
/// its basis cannot drift apart.
#[test]
fn every_derived_bound_matches_its_figure() {
    let t = defaults();
    let f: toml::Table = toml::from_str(
        &std::fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus-figures.toml"),
        )
        .expect("corpus-figures.toml"),
    )
    .expect("figures parse");
    let derived = [
        ("doc-length.max_lines", f["prose_lines_p75"].clone()),
        (
            "comment-ratio.header_free_chars",
            f["header_chars_language_median_p90"].clone(),
        ),
        ("comment-block.max_chars", f["remark"]["chars_p90"].clone()),
        ("comment-block.doc_max_lines", f["doc"]["lines_p75"].clone()),
        ("comment-block.doc_max_chars", f["doc"]["chars_p90"].clone()),
        (
            "comment-block.header_max_lines",
            f["header"]["lines_p75"].clone(),
        ),
        (
            "comment-block.header_max_chars",
            f["header"]["chars_p90"].clone(),
        ),
    ];
    for (path, figure) in derived {
        let (rule, field) = path.split_once('.').expect("dotted");
        assert_eq!(
            t["rules"][rule][field].to_string(),
            figure.to_string(),
            "{path} no longer equals the figure it cites"
        );
    }
}
