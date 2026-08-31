// Concern: holds the README, --help and every bound to the figures and defaults that ship | Non-concern: measuring the corpus (tests/corpus.rs) | IO: (docs, config, figures) -> pass/fail

#![allow(
    clippy::expect_used,
    reason = "a failed expectation in a test is a failed test"
)]

use comment_crusher::cli::Cli;
use comment_crusher::diagnostic::{Diagnostic, Level};
use std::path::Path;

fn defaults() -> toml::Table {
    toml::from_str(include_str!("../src/default_config.toml")).expect("defaults parse")
}

/// The README contracts a finding relies on: every `docs_url` anchor, and the allowance it
/// prints, which the hundredfold ceiling has to still admit.
#[test]
fn the_readme_carries_every_anchor_a_finding_links_to() {
    let text =
        std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../README.md"))
            .expect("README.md");
    let headings: Vec<String> = text
        .lines()
        .filter_map(|l| l.strip_prefix("## "))
        .map(|h| h.to_lowercase().replace(' ', "-"))
        .collect();
    for rule in [
        "config.rejected",
        "target.excluded",
        "allowance.unused",
        "comment-ratio",
    ] {
        let d = Diagnostic::about_the_run(rule, Level::Warn, String::new(), "");
        let wire = serde_json::to_value(&d).expect("a finding serializes");
        let url = wire["docs_url"].as_str().expect("docs_url");
        let Some(anchor) = url.split('#').nth(1) else {
            continue;
        };
        assert!(
            headings.iter().any(|h| h == anchor),
            "{rule} links to #{anchor}, which the README lost"
        );
    }
    let shipped = defaults()["rules"]["doc-length"]["max_lines"]
        .as_integer()
        .expect("max_lines is an integer");
    let printed: i64 = text
        .split("doc-length.max_lines=")
        .nth(1)
        .and_then(|rest| rest.split(['"', ' ']).next())
        .and_then(|n| n.parse().ok())
        .expect("the README prints an allowance");
    assert!(printed <= shipped * 100, "the ceiling rejects {printed}");
}

/// An agent branches on the exit codes, so `--help` names every one.
#[test]
fn the_help_lists_the_exit_codes_the_binary_returns() {
    let help = Cli::after_help();
    for code in ["0", "1", "2", "3", "24"] {
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
