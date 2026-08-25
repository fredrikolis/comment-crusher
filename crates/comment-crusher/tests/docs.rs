// Concern: holds the README, --help and every bound to the figures and defaults that ship | Non-concern: measuring the corpus (tests/corpus.rs) | IO: (docs, config, figures) -> pass/fail

#![allow(
    clippy::expect_used,
    reason = "a failed expectation in a test is a failed test"
)]

use comment_crusher::cli::Cli;
use std::path::Path;

/// Whitespace-flattened, so rewrapping a paragraph cannot fail a claim that still holds.
fn readme() -> String {
    let text =
        std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../README.md"))
            .expect("README.md");
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn defaults() -> toml::Table {
    toml::from_str(include_str!("../src/default_config.toml")).expect("defaults parse")
}

fn bound(table: &toml::Table, rule: &str, field: &str) -> String {
    table["rules"][rule][field].to_string()
}

/// How many languages declare a construct, so the README cannot hand-count it wrong.
fn declaring(t: &toml::Table) -> (usize, usize, usize, usize) {
    let langs = t["languages"].as_table().expect("languages");
    let count = |f: &dyn Fn(&toml::Value) -> bool| langs.values().filter(|l| f(l)).count();
    (
        langs.len(),
        count(&|l| l.get("nested_block").is_some()),
        count(&|l| l.get("heredoc").is_some()),
        count(&|l| {
            l.get("strings")
                .and_then(toml::Value::as_array)
                .is_some_and(|v| {
                    v.iter()
                        .any(|s| s.get("docstring").and_then(toml::Value::as_bool) == Some(true))
                })
        }),
    )
}

fn claims(t: &toml::Table, repos: usize, ratio: f64) -> Vec<(String, &'static str)> {
    let (languages, nested, heredoc, docstring) = declaring(t);
    vec![
        (format!("{languages} languages"), "the language count"),
        (format!("{repos} repositor"), "the corpus size"),
        (
            format!("Nested blocks in {nested} languages"),
            "the nesting count",
        ),
        (format!("heredocs in {heredoc}"), "the heredoc count"),
        (format!("docstrings in {docstring}"), "the docstring count"),
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
    let repos =
        std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus.toml"))
            .expect("corpus.toml")
            .matches("[[repo]]")
            .count();
    let ratio = t["rules"]["comment-ratio"]["max_ratio"]
        .as_float()
        .expect("max_ratio is a float");
    let claims = claims(&t, repos, ratio);
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
            f["header_chars_language_median_p50"].clone(),
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

/// Every language knob the table uses is documented, or `--help` goes silently incomplete
/// when a twentieth is added.
#[test]
fn every_language_knob_is_in_the_help_table() {
    let t = defaults();
    let table = Cli::after_help()
        .split("LANGUAGE TABLE")
        .nth(1)
        .expect("LANGUAGE TABLE section")
        .to_string();
    let mut keys = std::collections::BTreeSet::new();
    let collect = |v: &toml::Value, keys: &mut std::collections::BTreeSet<String>| {
        if let Some(m) = v.as_table() {
            for (k, inner) in m {
                keys.insert(k.clone());
                if let Some(list) = inner.as_array() {
                    for item in list {
                        if let Some(sub) = item.as_table() {
                            keys.extend(sub.keys().cloned());
                        }
                    }
                }
            }
        }
    };
    for lang in t["languages"].as_table().expect("languages").values() {
        collect(lang, &mut keys);
    }
    for set in t["embed_sets"].as_table().expect("embed_sets").values() {
        for item in set.as_array().into_iter().flatten() {
            collect(item, &mut keys);
        }
    }
    // Resolution keys and the attribute values a tag maps are named in prose, not as knobs.
    let described = ["extensions", "filenames", "interpreters", "map"];
    let missing: Vec<&String> = keys
        .iter()
        .filter(|k| !described.contains(&k.as_str()) && !table.contains(k.as_str()))
        .collect();
    assert!(missing.is_empty(), "knobs --help never names: {missing:?}");
}

/// What this repo sets for itself, and why, stays checkable: no allowance, and one rule field
/// whose value is the annotation cap CLAUDE.md names.
#[test]
fn this_repo_grants_no_allowance() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let own: toml::Table = toml::from_str(
        &std::fs::read_to_string(root.join(".comment-crusher.toml")).expect("own budget"),
    )
    .expect("own budget parses");
    assert!(own.get("allow").is_none(), "this repo grants no allowance");
    let set: Vec<String> = own
        .get("rules")
        .and_then(toml::Value::as_table)
        .map(|rules| {
            rules
                .iter()
                .flat_map(|(rule, fields)| {
                    fields
                        .as_table()
                        .into_iter()
                        .flat_map(|f| f.keys())
                        .map(move |k| format!("{rule}.{k}"))
                })
                .collect()
        })
        .unwrap_or_default();
    assert_eq!(set, vec!["comment-ratio.header_free_chars"], "{set:?}");
    let claim = std::fs::read_to_string(root.join("CLAUDE.md")).expect("CLAUDE.md");
    let value = own["rules"]["comment-ratio"]["header_free_chars"].to_string();
    assert!(
        claim.contains(&format!("header_free_chars = {value}")),
        "CLAUDE.md no longer says why this repo sets {value}"
    );
}
