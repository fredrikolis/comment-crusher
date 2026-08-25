// Concern: holds the scanner to real code — the partition invariant and a pinned snapshot | Non-concern: the rules' thresholds, unit-tested in src/ | IO: (corpus) -> pass/fail

#![allow(
    clippy::expect_used,
    reason = "a failed expectation in a test is a failed test"
)]

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use comment_crusher::{Config, Engine};

const SNAPSHOT: &str = "../../corpus-expected.toml";

/// Absent corpus is a failure, not a skip. `CORPUS_OPTIONAL=1` opts out.
fn corpus_root() -> Option<PathBuf> {
    let root = std::env::var_os("CORPUS_DIR").map_or_else(
        || Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/corpus"),
        PathBuf::from,
    );
    if root.is_dir() {
        return Some(root);
    }
    assert!(
        std::env::var_os("CORPUS_OPTIONAL").is_some(),
        "no corpus at {}: run ./scripts/fetch-corpus.sh, or set CORPUS_OPTIONAL=1 to skip",
        root.display()
    );
    None
}

fn repos(root: &Path) -> Vec<(String, PathBuf)> {
    let mut out: Vec<_> = std::fs::read_dir(root)
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .filter(|e| e.path().is_dir())
        .filter_map(|e| Some((e.file_name().to_str()?.to_string(), e.path())))
        .collect();
    out.sort();
    out
}

/// Per-language totals for one repo, the shape the snapshot pins.
type Totals = BTreeMap<String, [usize; 4]>;

fn measure(dir: &Path) -> (Totals, Vec<String>) {
    let config = Config::defaults().expect("built-in defaults load");
    let report = Engine::new(&config, Some(dir)).run(std::slice::from_ref(&dir.to_path_buf()));

    // A file the tool declined to measure has no partition to check.
    let declined: BTreeSet<PathBuf> = report
        .diagnostics
        .iter()
        .filter(|d| d.rule == "unreadable")
        .map(|d| d.file.clone())
        .collect();

    let mut totals = Totals::new();
    let mut mismatches = Vec::new();
    for f in &report.files {
        if declined.contains(&f.path) {
            continue;
        }
        let e = totals.entry(f.language.clone()).or_insert([0; 4]);
        e[0] += 1;
        e[1] += f.lines;
        e[2] += f.comment_chars;
        e[3] += f.code_chars;

        // Lose a character and a region was dropped; gain one and a state was left entered.
        let Ok(bytes) = std::fs::read(dir.join(&f.path)) else {
            // An IO failure is not a scanner defect; the engine reports it separately.
            continue;
        };
        let text = String::from_utf8_lossy(&bytes);
        let visible = text.chars().filter(|c| !c.is_whitespace()).count();
        if f.comment_chars + f.code_chars != visible {
            mismatches.push(format!(
                "{}: comment {} + code {} != {visible} visible",
                f.path.display(),
                f.comment_chars,
                f.code_chars
            ));
        }
    }
    (totals, mismatches)
}

#[test]
fn corpus_partitions_every_file_into_comment_and_code() {
    let Some(root) = corpus_root() else {
        return;
    };
    let mut all = Vec::new();
    for (name, dir) in repos(&root) {
        let (_, mismatches) = measure(&dir);
        all.extend(mismatches.into_iter().map(|m| format!("{name}/{m}")));
    }
    assert!(
        all.is_empty(),
        "{} files mis-counted:\n{}",
        all.len(),
        all.join("\n")
    );
}

#[test]
fn corpus_totals_match_the_pinned_snapshot() {
    let Some(root) = corpus_root() else {
        return;
    };
    let snapshot_path = Path::new(env!("CARGO_MANIFEST_DIR")).join(SNAPSHOT);
    let measured: BTreeMap<String, Totals> = repos(&root)
        .into_iter()
        .map(|(name, dir)| (name, measure(&dir).0))
        .collect();
    let rendered = render(&measured);

    if std::env::var_os("UPDATE_CORPUS_SNAPSHOT").is_some() {
        std::fs::write(&snapshot_path, &rendered).expect("write snapshot");
        return;
    }
    let expected = std::fs::read_to_string(&snapshot_path).unwrap_or_default();
    assert_eq!(
        rendered.trim(),
        expected.trim(),
        "corpus totals moved. If the change is intended, re-record with \
         UPDATE_CORPUS_SNAPSHOT=1 cargo test --test corpus and review the diff."
    );
}

fn render(measured: &BTreeMap<String, Totals>) -> String {
    let mut out = String::from(
        "# Concern: the per-language totals the pinned corpus must keep producing | \
Non-concern: choosing the corpus (corpus.toml) or asserting over it (tests/corpus.rs) | IO: none\n\
# Re-record with: UPDATE_CORPUS_SNAPSHOT=1 cargo test --test corpus\n",
    );
    for (repo, totals) in measured {
        for (lang, v) in totals {
            let _ = write!(
                out,
                "\n[\"{repo}\".{lang}]\nfiles = {}\nlines = {}\ncomment_chars = {}\ncode_chars = {}\n",
                v[0], v[1], v[2], v[3]
            );
        }
    }
    out
}

/// Not "its language appears in the corpus": the token itself must have fired on real source.
/// Only real code shows whether a marker set is complete, which is how a Pascal compiler
/// directive and a fixed-form Fortran comment were both found mis-declared.
#[test]
fn every_comment_marker_has_fired_on_real_source() {
    let Some(root) = corpus_root() else {
        return;
    };
    let config = Config::defaults().expect("defaults");
    let fired = markers_that_opened_a_comment(&root, &config);
    let strings_seen = string_delimiters_seen(&root, &config);

    let mut unfired: Vec<String> = Vec::new();
    let mut unused_exemptions: Vec<&str> = snippet_only();
    for syn in config.languages() {
        for (token, opener) in &syn.openers {
            if let comment_crusher::syntax::Opener::Str(_) = opener {
                let key = format!("{} {token}", syn.name);
                if !strings_seen.contains(&key)
                    && !strings_seen
                        .iter()
                        .any(|f| f.ends_with(&format!(" {token}")))
                {
                    unfired.push(format!("{key} (string delimiter)"));
                }
                continue;
            }
            let key = format!("{} {token}", syn.name);
            // The same token in another language is the same scanner path.
            if fired.contains(&key) || fired.iter().any(|f| f.ends_with(&format!(" {token}"))) {
                continue;
            }
            if let Some(i) = unused_exemptions.iter().position(|e| *e == key) {
                unused_exemptions.remove(i);
                continue;
            }
            unfired.push(key);
        }
    }
    assert!(
        unfired.is_empty(),
        "markers no pinned repository ever opened a comment with. Pin a repository that uses \
         one, drop the marker, or add it to snippet-only.txt:\n{}",
        unfired.join("\n")
    );
    assert!(
        unused_exemptions.is_empty(),
        "snippet-only.txt names markers that are now proved or no longer declared; remove \
         them:\n{}",
        unused_exemptions.join("\n")
    );
}

/// Which declared markers actually opened a comment in real source.
fn markers_that_opened_a_comment(root: &Path, config: &Config) -> BTreeSet<String> {
    let mut fired = BTreeSet::new();
    for (_, dir) in repos(root) {
        let report = Engine::new(config, Some(&dir)).run(std::slice::from_ref(&dir));
        for f in &report.files {
            let Some(syn) = config.language(&dir.join(&f.path)) else {
                continue;
            };
            let Ok(text) = std::fs::read_to_string(dir.join(&f.path)) else {
                continue;
            };
            for region in comment_crusher::scan_in(&text, syn, config).regions {
                let Some(rest) = text.get(region.start..) else {
                    continue;
                };
                if let Some((token, _)) = syn
                    .openers
                    .iter()
                    .find(|(t, _)| rest.starts_with(t.as_str()))
                {
                    fired.insert(format!("{} {token}", syn.name));
                }
            }
        }
    }
    fired
}

/// Which declared string delimiters actually appear in real source of their own language.
fn string_delimiters_seen(root: &Path, config: &Config) -> BTreeSet<String> {
    let mut seen = BTreeSet::new();
    for (_, dir) in repos(root) {
        let report = Engine::new(config, Some(&dir)).run(std::slice::from_ref(&dir));
        for f in &report.files {
            let Some(syn) = config.language(&dir.join(&f.path)) else {
                continue;
            };
            let Ok(text) = std::fs::read_to_string(dir.join(&f.path)) else {
                continue;
            };
            for spec in &syn.strings {
                if text.contains(&spec.open) {
                    seen.insert(format!("{} {}", syn.name, spec.open));
                }
            }
        }
    }
    seen
}

/// Correct but rare forms no pinned repository happens to use. One home, read by this test
/// and by `scan_tests.rs`, which asserts a snippet exercises each of them.
const SNIPPET_ONLY_LIST: &str = include_str!("../snippet-only.txt");

fn snippet_only() -> Vec<&'static str> {
    SNIPPET_ONLY_LIST
        .lines()
        .filter(|l| !l.is_empty())
        .collect()
}
