// Concern: holds the scanner to real code — the partition invariant and a pinned snapshot | Non-concern: the rules' thresholds, unit-tested in src/ | IO: (corpus) -> pass/fail

#![allow(
    clippy::expect_used,
    reason = "a failed expectation in a test is a failed test"
)]

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use comment_crusher::syntax::CommentKind;
use comment_crusher::{Config, Engine};

const SNAPSHOT: &str = "../../corpus-expected.toml";

/// No opt-out. `UPDATE_CORPUS_SNAPSHOT` re-records the totals, which still runs every walk;
/// nothing turns these into a pass without measuring.
fn corpus_root() -> PathBuf {
    let root = std::env::var_os("CORPUS_DIR").map_or_else(
        || Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/corpus"),
        PathBuf::from,
    );
    assert!(
        root.is_dir(),
        "no corpus at {}: run ./scripts/fetch-corpus.sh",
        root.display()
    );
    root
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

/// A file the tool declined to measure has no partition to check and no figure to feed.
fn declined(report: &comment_crusher::engine::Report) -> BTreeSet<PathBuf> {
    report
        .diagnostics
        .iter()
        .filter(|d| d.rule.starts_with("unreadable"))
        .filter_map(|d| d.file.clone())
        .collect()
}

/// Per-language totals for one repo, the shape the snapshot pins.
type Totals = BTreeMap<String, [usize; 4]>;

fn measure(dir: &Path) -> (Totals, Vec<String>) {
    let config = Config::defaults().expect("built-in defaults load");
    let report = Engine::new(&config, Some(dir)).run(std::slice::from_ref(&dir.to_path_buf()));

    let declined = declined(&report);

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

/// Comment plus code equals every corpus file's visible chars, and the totals hold.
#[test]
fn the_corpus_partitions_and_its_totals_hold() {
    let root = corpus_root();
    let snapshot_path = Path::new(env!("CARGO_MANIFEST_DIR")).join(SNAPSHOT);
    let mut measured: BTreeMap<String, Totals> = BTreeMap::new();
    let mut lost = Vec::new();
    for (name, dir) in repos(&root) {
        let (totals, off) = measure(&dir);
        measured.insert(name.clone(), totals);
        lost.extend(off.into_iter().map(|m| format!("{name}/{m}")));
    }
    assert!(
        lost.is_empty(),
        "{} files mis-counted:\n{}",
        lost.len(),
        lost.join("\n")
    );
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
#[test]
fn every_comment_marker_has_fired_on_real_source() {
    let root = corpus_root();
    let config = Config::defaults().expect("defaults");
    let (fired, strings_seen) = exercised(&root, &config);

    let mut unfired: Vec<String> = Vec::new();
    let mut unused_exemptions: Vec<&str> = snippet_only();
    for syn in config.languages() {
        for (token, opener) in &syn.openers {
            if let comment_crusher::syntax::Opener::Str(_) = opener {
                let key = format!("{} {token}", syn.name);
                if !strings_seen
                    .iter()
                    .any(|f| f.ends_with(&format!(" {token}")))
                {
                    unfired.push(format!("{key} (string delimiter)"));
                }
                continue;
            }
            let key = format!("{} {token}", syn.name);
            // The same token elsewhere is the same path, unless this language changes it.
            let anchored =
                syn.line_anchored && matches!(opener, comment_crusher::syntax::Opener::Line(_));
            // A doc marker is the language's own claim: `cpp ///` is not proved by Rust's.
            let own = anchored
                || matches!(
                    opener,
                    comment_crusher::syntax::Opener::Line(CommentKind::Doc)
                )
                || matches!(
                    opener,
                    comment_crusher::syntax::Opener::Block {
                        kind: CommentKind::Doc,
                        ..
                    }
                )
                || !syn.exceptions.is_empty()
                || !syn.cancel_after.is_empty();
            let elsewhere = !own && fired.iter().any(|f| f.ends_with(&format!(" {token}")));
            if fired.contains(&key) || elsewhere {
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

/// The markers that opened a comment and the string delimiters that appear, from the
/// scanner's own `opener` rather than a prefix match it never used.
fn exercised(root: &Path, config: &Config) -> (BTreeSet<String>, BTreeSet<String>) {
    let (mut markers, mut strings) = (BTreeSet::new(), BTreeSet::new());
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
                markers.insert(format!("{} {}", syn.name, region.opener));
            }
            for spec in &syn.strings {
                if text.contains(&spec.open) {
                    strings.insert(format!("{} {}", syn.name, spec.open));
                }
            }
        }
    }
    (markers, strings)
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

/// Snippets alone cannot prove depth-counting, so a pinned repository has to write one.
#[test]
fn a_pinned_repository_nests_a_block_comment() {
    let root = corpus_root();
    let config = Config::defaults().expect("defaults");
    let mut found: Vec<String> = Vec::new();
    for (name, dir) in repos(&root) {
        let report = Engine::new(&config, Some(&dir)).run(std::slice::from_ref(&dir));
        for f in &report.files {
            let path = dir.join(&f.path);
            let Some(syn) = config.language(&path) else {
                continue;
            };
            if !syn.nested_block {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            let mut flat = syn.clone();
            flat.nested_block = false;
            if comment_crusher::scan_in(&text, syn, &config).code_chars
                != comment_crusher::scan_in(&text, &flat, &config).code_chars
            {
                found.push(format!("{name}/{} ({})", f.path.display(), syn.name));
            }
        }
    }
    assert!(
        !found.is_empty(),
        "no pinned repository nests a block comment, so the 16 languages that declare it \
         rest on snippets alone. Pin one that does."
    );
}

const FIGURES: &str = "../../corpus-figures.toml";

/// What every threshold in `default_config.toml` is derived from, measured by the same walk
/// that checks the partition and read the way the engine reads a file.
#[derive(Default)]
struct Figures {
    shares: Vec<f64>,
    docs: Vec<usize>,
    over_1: usize,
    over_5: usize,
    lines: BTreeMap<&'static str, Vec<usize>>,
    chars: BTreeMap<&'static str, Vec<usize>>,
    header_by_language: BTreeMap<String, Vec<usize>>,
}

impl Figures {
    #[expect(
        clippy::cast_precision_loss,
        reason = "character counts are far below f64 precision"
    )]
    fn gather(&mut self, config: &Config, dir: &PathBuf) {
        let report = Engine::new(config, Some(dir)).run(std::slice::from_ref(dir));
        let declined = declined(&report);
        let (mut comment, mut code) = (0usize, 0usize);
        for f in report.files.iter().filter(|f| !declined.contains(&f.path)) {
            if f.prose {
                self.docs.push(f.lines);
                continue;
            }
            comment += f.comment_chars;
            code += f.code_chars;
            let path = dir.join(&f.path);
            let Ok(bytes) = std::fs::read(&path) else {
                continue;
            };
            // As the engine reads it, `#!` and legacy encodings included.
            let text = String::from_utf8_lossy(&bytes).into_owned();
            let Some(syn) = config
                .language(&path)
                .or_else(|| config.language_of_shebang(text.lines().next().unwrap_or_default()))
            else {
                continue;
            };
            for r in &comment_crusher::scan_in(&text, syn, config).regions {
                self.region(r, &syn.name);
            }
        }
        if comment + code > 0 {
            self.shares.push(comment as f64 / (comment + code) as f64);
        }
    }

    fn region(&mut self, r: &comment_crusher::scan::Region, language: &str) {
        let n = r.lines();
        self.over_1 += usize::from(n > 1);
        self.over_5 += usize::from(n > 5);
        let kind = if r.header {
            self.header_by_language
                .entry(language.to_string())
                .or_default()
                .push(r.chars);
            "header"
        } else if r.kind == comment_crusher::syntax::CommentKind::Doc {
            "doc"
        } else {
            "remark"
        };
        self.lines.entry(kind).or_default().push(n);
        self.chars.entry(kind).or_default().push(r.chars);
    }

    fn render(mut self) -> String {
        let mut medians: Vec<usize> = self
            .header_by_language
            .values()
            .map(|v| median(v.clone()))
            .collect();
        medians.sort_unstable();
        self.shares
            .sort_by(|a, b| a.partial_cmp(b).expect("finite"));
        self.docs.sort_unstable();
        let mut out = String::from(
            "# Concern: the corpus statistics every threshold in default_config.toml is \
derived from | Non-concern: which bound is chosen from them (default_config.toml argues that) \
| IO: none\n# Re-record with: UPDATE_CORPUS_SNAPSHOT=1 cargo test --test corpus\n\n",
        );
        let _ = write!(
            out,
            "repo_comment_share_median = {:.3}\nprose_lines_p75 = {}\n\
comments_over_1_line = {}\ncomments_over_5_lines = {}\n\
header_chars_language_median_p50 = {}\n\
header_chars_language_median_p90 = {}\n",
            self.shares[self.shares.len() / 2],
            pct(&self.docs, 0.75),
            self.over_1,
            self.over_5,
            pct(&medians, 0.50),
            pct(&medians, 0.90),
        );
        for kind in ["remark", "doc", "header"] {
            let mut l = self.lines.remove(kind).unwrap_or_default();
            let mut c = self.chars.remove(kind).unwrap_or_default();
            l.sort_unstable();
            c.sort_unstable();
            // Only what a threshold derives from; a remark's line bound is policy.
            let _ = write!(out, "\n[{kind}]\nchars_p90 = {}\n", pct(&c, 0.90));
            if kind != "remark" {
                let _ = writeln!(out, "lines_p75 = {}", pct(&l, 0.75));
            }
        }
        out
    }
}

/// Recording them stops a citation drifting when the corpus moves, and stops a hand-rolled
/// harness measuring the corpus a different way from the engine.
#[test]
fn the_cited_figures_reproduce() {
    let root = corpus_root();
    let config = Config::defaults().expect("defaults");
    let mut figures = Figures::default();
    for (_, dir) in repos(&root) {
        figures.gather(&config, &dir);
    }
    let out = figures.render();
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(FIGURES);
    if std::env::var_os("UPDATE_CORPUS_SNAPSHOT").is_some() {
        std::fs::write(&path, &out).expect("write figures");
        return;
    }
    assert_eq!(
        out.trim(),
        std::fs::read_to_string(&path).unwrap_or_default().trim(),
        "the corpus figures moved. Re-record, then re-read every threshold against them."
    );
}

fn median(mut v: Vec<usize>) -> usize {
    v.sort_unstable();
    pct(&v, 0.50)
}

#[expect(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "a percentile index over a corpus of thousands"
)]
fn pct(v: &[usize], p: f64) -> usize {
    assert!(!v.is_empty(), "no measurement to take a percentile of");
    v[((v.len() as f64 - 1.0) * p).round() as usize]
}
