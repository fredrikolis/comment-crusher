// Concern: holds the scanner to real code — the partition invariant and a pinned snapshot | Non-concern: the rules' thresholds, unit-tested in src/ | IO: (corpus) -> pass/fail

#![allow(
    clippy::expect_used,
    reason = "a failed expectation in a test is a failed test"
)]

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use comment_crusher::{Config, Engine};

const SNAPSHOT: &str = "../../corpus-expected.toml";

fn corpus_root() -> Option<PathBuf> {
    let root = std::env::var_os("CORPUS_DIR").map_or_else(
        || Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/corpus"),
        PathBuf::from,
    );
    root.is_dir().then_some(root)
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
    let report = Engine::new(&config, dir)
        .run(std::slice::from_ref(&dir.to_path_buf()))
        .expect("engine runs");

    let mut totals = Totals::new();
    let mut mismatches = Vec::new();
    for f in &report.files {
        let e = totals.entry(f.language.clone()).or_insert([0; 4]);
        e[0] += 1;
        e[1] += f.lines;
        e[2] += f.comment_chars;
        e[3] += f.code_chars;

        // Every visible character is either comment or code. A scanner that loses one has
        // silently dropped a region; one that double-counts has left a state it entered.
        let text = std::fs::read_to_string(dir.join(&f.path)).unwrap_or_default();
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
        eprintln!("skipping: run scripts/fetch-corpus.sh first");
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
        eprintln!("skipping: run scripts/fetch-corpus.sh first");
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
