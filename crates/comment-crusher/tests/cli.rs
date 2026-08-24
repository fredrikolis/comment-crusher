// Concern: holds the binary to what a caller observes — exit codes, discovered config, allowances, JSON shape | Non-concern: how a file is scanned (src/ owns that) | IO: (temp trees) -> pass/fail

#![allow(
    clippy::expect_used,
    reason = "a failed expectation in a test is a failed test"
)]

use std::fmt::Write as _;
use std::path::Path;
use std::process::{Command, Output};

const BIN: &str = env!("CARGO_BIN_EXE_comment-crusher");

fn run(dir: &Path, args: &[&str]) -> Output {
    Command::new(BIN)
        .arg("--root")
        .arg(dir)
        .args(args)
        .current_dir(dir)
        .output()
        .expect("binary runs")
}

fn code(out: &Output) -> i32 {
    out.status.code().unwrap_or(-1)
}

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn write(dir: &Path, name: &str, body: &str) {
    let path = dir.join(name);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("mkdir");
    }
    std::fs::write(path, body).expect("write");
}

/// Comfortably over the 25% default. The prose sits below a line of code on purpose: a
/// leading comment is the header, and the header is exempt.
fn over_budget_rust() -> String {
    let prose = "// this file explains itself at considerable and unnecessary length\n".repeat(6);
    format!("fn head() -> u32 {{ 0 }}\n{prose}fn f() -> u32 {{ 1 }}\n")
}

fn lean_rust() -> String {
    (0..40).fold(String::new(), |mut s, i| {
        let _ = writeln!(s, "fn f{i}() -> u32 {{ {i} }}");
        s
    })
}

#[test]
fn a_file_within_budget_exits_zero_and_one_over_it_exits_one() {
    let dir = tempfile::tempdir().expect("tempdir");
    write(dir.path(), "lean.rs", &lean_rust());
    assert_eq!(code(&run(dir.path(), &["lean.rs"])), 0);

    write(dir.path(), "fat.rs", &over_budget_rust());
    let out = run(dir.path(), &["fat.rs"]);
    assert_eq!(code(&out), 1);
    assert!(stdout(&out).contains("comment-ratio"), "{}", stdout(&out));
}

#[test]
fn a_repo_config_found_by_walking_up_sets_the_budget() {
    let dir = tempfile::tempdir().expect("tempdir");
    write(dir.path(), "src/deep/fat.rs", &over_budget_rust());
    assert_eq!(code(&run(dir.path(), &["src/deep/fat.rs"])), 1);

    write(
        dir.path(),
        ".comment-crusher.toml",
        "[rules.comment-ratio]\nmax_ratio = 0.95\n[rules.comment-block]\nmax_lines = 50\n",
    );
    assert_eq!(code(&run(dir.path(), &["src/deep/fat.rs"])), 0);
}

#[test]
fn an_allowance_widens_the_budget_only_for_the_paths_it_names() {
    let dir = tempfile::tempdir().expect("tempdir");
    write(dir.path(), "docs/long.md", &"a line\n".repeat(500));
    write(dir.path(), "docs/short.md", &"a line\n".repeat(10));
    assert_eq!(code(&run(dir.path(), &["docs"])), 1);

    let out = run(
        dir.path(),
        &[
            "docs",
            "--allow",
            "docs/long.md",
            "doc-length.max_lines=1000",
        ],
    );
    assert_eq!(code(&out), 0, "{}", stdout(&out));

    // The same allowance pointed elsewhere leaves the finding standing.
    let out = run(
        dir.path(),
        &["docs", "--allow", "other/**", "doc-length.max_lines=1000"],
    );
    assert_eq!(code(&out), 1);
}

#[test]
fn an_allowance_records_the_reason_it_was_granted() {
    let dir = tempfile::tempdir().expect("tempdir");
    write(dir.path(), "docs/long.md", &"a line\n".repeat(500));
    write(
        dir.path(),
        ".comment-crusher.toml",
        "[[allow]]\npaths = [\"docs/**\"]\nreason = \"the spec is the product\"\nset = [\"doc-length.max_lines=450\"]\n",
    );
    let out = run(dir.path(), &["docs"]);
    assert_eq!(code(&out), 1);
    assert!(
        stdout(&out).contains("the spec is the product"),
        "{}",
        stdout(&out)
    );
}

#[test]
fn json_reports_every_file_measured_and_every_finding() {
    let dir = tempfile::tempdir().expect("tempdir");
    write(dir.path(), "fat.rs", &over_budget_rust());
    let out = run(dir.path(), &["fat.rs", "--format", "json"]);
    let v: serde_json::Value = serde_json::from_str(&stdout(&out)).expect("valid JSON");
    assert_eq!(v["files"][0]["language"], "rust");
    assert!(v["files"][0]["comment_chars"].as_u64().unwrap_or(0) > 0);
    assert_eq!(v["diagnostics"][0]["rule"], "comment-ratio");
    assert_eq!(v["diagnostics"][0]["level"], "deny");
}

#[test]
fn a_file_in_no_known_language_is_not_guessed_at() {
    let dir = tempfile::tempdir().expect("tempdir");
    write(dir.path(), "data.bin", "// this is not measured\n");
    let out = run(dir.path(), &["data.bin", "--format", "json"]);
    assert_eq!(code(&out), 0);
    let v: serde_json::Value = serde_json::from_str(&stdout(&out)).expect("valid JSON");
    assert_eq!(v["files"].as_array().map(Vec::len), Some(0));
}

#[test]
fn a_shebang_names_the_language_of_a_file_with_no_extension() {
    let dir = tempfile::tempdir().expect("tempdir");
    write(dir.path(), "hook", "#!/usr/bin/env bash\n# note\necho hi\n");
    let out = run(dir.path(), &["hook", "--format", "json"]);
    let v: serde_json::Value = serde_json::from_str(&stdout(&out)).expect("valid JSON");
    assert_eq!(v["files"][0]["language"], "shell");
}

#[test]
fn an_allowance_cannot_exempt_a_file_only_widen_its_bound() {
    let dir = tempfile::tempdir().expect("tempdir");
    write(dir.path(), "docs/long.md", &"a line\n".repeat(500));
    for setting in [
        "doc-length.level=allow",
        "doc-length.max_lines=0",
        "doc-length.max_lines=-1",
    ] {
        let out = run(dir.path(), &["docs", "--allow", "docs/**", setting]);
        assert_eq!(code(&out), 2, "{setting} should be refused");
    }
    // The same field, widened rather than removed, is accepted.
    assert_eq!(
        code(&run(
            dir.path(),
            &["docs", "--allow", "docs/**", "doc-length.max_lines=600"]
        )),
        0
    );
}

#[test]
fn a_malformed_allowance_is_a_configuration_error_not_a_finding() {
    let dir = tempfile::tempdir().expect("tempdir");
    write(dir.path(), "lean.rs", &lean_rust());
    assert_eq!(
        code(&run(
            dir.path(),
            &["lean.rs", "--allow", "*.rs", "max_ratio=0.9"]
        )),
        2
    );
    assert_eq!(code(&run(dir.path(), &["no-such-path"])), 2);
}
