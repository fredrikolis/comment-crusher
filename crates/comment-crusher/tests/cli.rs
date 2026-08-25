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

/// Without `--root`, which every other case pins: the default anchor is what argv globs are
/// read against, and it is the one thing `--root` hides.
fn run_bare(dir: &Path, args: &[&str]) -> Output {
    Command::new(BIN)
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

/// Well over budget. The prose sits below a line of code on purpose: a leading comment is
/// the header, and the header is exempt.
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
    assert_eq!(code(&out), 3, "a file over budget is a validation_error");
    assert!(stdout(&out).contains("comment-ratio"), "{}", stdout(&out));
}

#[test]
fn a_repo_config_found_by_walking_up_sets_the_budget() {
    let dir = tempfile::tempdir().expect("tempdir");
    write(dir.path(), "src/deep/fat.rs", &over_budget_rust());
    assert_eq!(code(&run(dir.path(), &["src/deep/fat.rs"])), 3);

    write(
        dir.path(),
        ".comment-crusher.toml",
        "[rules.comment-ratio]\nmax_ratio = 0.95\n[rules.comment-block]\nmax_lines = 50\n\
         max_chars = 5000\n",
    );
    assert_eq!(code(&run(dir.path(), &["src/deep/fat.rs"])), 0);
}

#[test]
fn an_allowance_widens_the_budget_only_for_the_paths_it_names() {
    let dir = tempfile::tempdir().expect("tempdir");
    write(dir.path(), "docs/long.md", &"a line\n".repeat(500));
    write(dir.path(), "docs/short.md", &"a line\n".repeat(10));
    assert_eq!(code(&run(dir.path(), &["docs"])), 3);

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
    assert_eq!(code(&out), 3);
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
    assert_eq!(code(&out), 3);
    assert!(
        stdout(&out).contains("the spec is the product"),
        "{}",
        stdout(&out)
    );
}

/// The budget's own directory is the base for every path, so the same file gets the same
/// answer from the repo root, from beside it, and by absolute path.
#[test]
fn one_repo_answer_holds_from_any_working_directory() {
    let dir = tempfile::tempdir().expect("tempdir");
    write(dir.path(), "docs/long.md", &"a line\n".repeat(500));
    write(
        dir.path(),
        ".comment-crusher.toml",
        "[[allow]]\npaths = [\"docs/**\"]\nreason = \"the spec is the product\"\nset = [\"doc-length.max_lines=600\"]\n",
    );
    let abs = dir.path().join("docs/long.md");
    let from_root = Command::new(BIN)
        .arg(".")
        .current_dir(dir.path())
        .output()
        .expect("runs");
    let from_docs = Command::new(BIN)
        .arg("long.md")
        .current_dir(dir.path().join("docs"))
        .output()
        .expect("runs");
    let by_abs = Command::new(BIN)
        .arg(&abs)
        .current_dir(dir.path())
        .output()
        .expect("runs");
    assert_eq!(code(&from_root), 0);
    assert_eq!(code(&from_docs), 0, "{}", stdout(&from_docs));
    assert_eq!(code(&by_abs), 0, "{}", stdout(&by_abs));
}

#[test]
fn json_reports_every_file_measured_and_every_finding() {
    let dir = tempfile::tempdir().expect("tempdir");
    write(dir.path(), "fat.rs", &over_budget_rust());
    let out = run(dir.path(), &["fat.rs", "--format", "json"]);
    let v: serde_json::Value = serde_json::from_str(&stdout(&out)).expect("valid JSON");
    assert_eq!(v["status"], "error");
    assert_eq!(v["error"]["code"], "validation_error");
    assert_eq!(v["data"]["files"][0]["language"], "rust");
    assert!(v["data"]["files"][0]["comment_chars"].as_u64().unwrap_or(0) > 0);
    assert_eq!(v["data"]["languages"][0]["language"], "rust");
    let d = &v["data"]["diagnostics"][0];
    assert_eq!(d["code"], "comment-ratio");
    assert_eq!(d["severity"], "error");
    assert_eq!(d["location"]["file"], "fat.rs");
    let block = v["data"]["diagnostics"]
        .as_array()
        .expect("array")
        .iter()
        .find(|d| d["code"] == "comment-block.lines")
        .expect("comment-block finding");
    assert!(block["location"]["span"]["length"].as_u64().unwrap_or(0) > 0);
    assert!(block["location"]["end"]["line"].as_u64().unwrap_or(0) > 0);
    assert!(d["help"].as_str().is_some_and(|h| !h.is_empty()));
    assert_eq!(
        v["data"]["pagination"]["diagnostics"]["count"],
        v["data"]["diagnostics"].as_array().map_or(0, Vec::len),
        "the count is of what was sent, not of what the thresholds happen to find"
    );
    assert_eq!(v["data"]["pagination"]["files"]["count"], 1);
}

#[test]
fn a_file_in_no_known_language_is_not_guessed_at() {
    let dir = tempfile::tempdir().expect("tempdir");
    write(dir.path(), "data.bin", "// this is not measured\n");
    let out = run(dir.path(), &["data.bin", "--format", "json"]);
    assert_eq!(code(&out), 0);
    let v: serde_json::Value = serde_json::from_str(&stdout(&out)).expect("valid JSON");
    assert_eq!(v["status"], "success");
    assert_eq!(v["data"]["files"].as_array().map(Vec::len), Some(0));
}

#[test]
fn a_shebang_names_the_language_of_a_file_with_no_extension() {
    let dir = tempfile::tempdir().expect("tempdir");
    write(dir.path(), "hook", "#!/usr/bin/env bash\n# note\necho hi\n");
    let out = run(dir.path(), &["hook", "--format", "json"]);
    let v: serde_json::Value = serde_json::from_str(&stdout(&out)).expect("valid JSON");
    assert_eq!(v["data"]["files"][0]["language"], "shell");
}

/// A widened bound with no stated reason is a threshold raised in silence, which is the thing
/// the allowance mechanism exists to prevent.
#[test]
fn an_allowance_without_a_reason_is_refused() {
    let dir = tempfile::tempdir().expect("tempdir");
    write(dir.path(), "docs/long.md", &"a line\n".repeat(500));
    write(
        dir.path(),
        ".comment-crusher.toml",
        "[[allow]]\npaths = [\"docs/**\"]\nset = [\"doc-length.max_lines=600\"]\n",
    );
    let out = run(dir.path(), &["docs"]);
    assert_eq!(code(&out), 3);
    assert!(stdout(&out).contains("reason"), "{}", stdout(&out));
}

#[test]
fn an_allowance_cannot_exempt_a_file_only_widen_its_bound() {
    let dir = tempfile::tempdir().expect("tempdir");
    write(dir.path(), "docs/long.md", &"a line\n".repeat(500));
    for setting in [
        // Switches the rule off.
        "doc-length.level=allow",
        // Zero is how every rule spells "no limit".
        "doc-length.max_lines=0",
        "doc-length.max_lines=-1",
        // A floor decides whether the rule applies at all, so raising it exempts.
        "comment-ratio.min_chars=999999",
        // A ratio can never exceed 1, so the rule could never fire.
        "comment-ratio.max_ratio=1",
        "comment-ratio.max_ratio=2",
        // Past a hundredfold a bound is not widened, it is gone.
        "doc-length.max_lines=1e30",
        "doc-length.max_lines=99999999999",
        "doc-length.max_lines=9001",
        // Not a bound at all.
        "comment-ratio.count_doc_comments=false",
        "comment-ratio.skip_header=true",
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

/// argv rejected, configuration rejected and a file over budget are three different things,
/// and an agent branches on the code, never the message.
#[test]
fn each_kind_of_failure_has_its_own_code_and_exit() {
    let dir = tempfile::tempdir().expect("tempdir");
    write(dir.path(), "fat.rs", &over_budget_rust());
    for (args, want_code, want_exit) in [
        (vec!["--bogus", "--format", "json"], "bad_arguments", 2),
        (vec!["nope", "--format", "json"], "not_found", 24),
        (vec!["fat.rs", "--format", "json"], "validation_error", 3),
    ] {
        let out = run(dir.path(), &args);
        let v: serde_json::Value = serde_json::from_str(&stdout(&out)).expect("valid JSON");
        assert_eq!(v["error"]["code"], want_code, "{args:?}");
        assert_eq!(code(&out), want_exit, "{args:?}");
        assert!(v["data"].is_object(), "every reply carries data: {args:?}");
    }
}

/// The same bad setting is the caller's mistake on argv and the repo's in a file, and an
/// agent branches on 2 to re-read `--help`, on 3 to go read the file.
#[test]
fn a_malformed_allowance_names_which_input_was_wrong() {
    let dir = tempfile::tempdir().expect("tempdir");
    write(dir.path(), "lean.rs", &lean_rust());
    for setting in ["max_ratio=0.9", "comment-block.max_lines=2.9"] {
        let argv = run(
            dir.path(),
            &["lean.rs", "--allow", "*.rs", setting, "--format", "json"],
        );
        assert_eq!(code(&argv), 2, "{setting}: {}", stdout(&argv));
        assert!(stdout(&argv).contains("bad_arguments"), "{setting}");
    }
    write(
        dir.path(),
        ".comment-crusher.toml",
        "[[allow]]\npaths = [\"*.rs\"]\nreason = \"x\"\nset = [\"max_ratio=0.9\"]\n",
    );
    let file = run(dir.path(), &["lean.rs", "--format", "json"]);
    assert_eq!(code(&file), 3, "{}", stdout(&file));
    assert!(
        stdout(&file).contains("validation_error"),
        "{}",
        stdout(&file)
    );
}

#[test]
fn stats_reports_per_language_totals_in_both_formats() {
    let dir = tempfile::tempdir().expect("tempdir");
    write(dir.path(), "a.rs", &lean_rust());
    let human = run(dir.path(), &["a.rs", "--stats"]);
    assert_eq!(code(&human), 0);
    let text = stdout(&human);
    assert!(text.contains("language"), "{text}");
    assert!(text.contains("rust"), "{text}");

    // JSON carries the same totals whether or not --stats was asked for.
    let json = run(dir.path(), &["a.rs", "--format", "json"]);
    let v: serde_json::Value = serde_json::from_str(&stdout(&json)).expect("valid JSON");
    assert_eq!(v["data"]["languages"][0]["language"], "rust");
    assert_eq!(v["data"]["languages"][0]["files"], 1);
}

#[test]
fn warnings_as_errors_escalates_a_warning_and_nothing_else() {
    let dir = tempfile::tempdir().expect("tempdir");
    write(dir.path(), "docs/long.md", &"a line\n".repeat(500));
    write(
        dir.path(),
        ".comment-crusher.toml",
        "[rules.doc-length]\nlevel = \"warn\"\n",
    );
    assert_eq!(
        code(&run(dir.path(), &["docs"])),
        0,
        "a warning alone exits 0"
    );
    assert_eq!(code(&run(dir.path(), &["docs", "--warnings-as-errors"])), 3);

    // With nothing to report, the flag changes nothing.
    write(dir.path(), "clean/short.md", "a line\n");
    assert_eq!(
        code(&run(dir.path(), &["clean", "--warnings-as-errors"])),
        0
    );
}

#[test]
fn version_answers_in_the_requested_shape_whatever_else_is_wrong() {
    let dir = tempfile::tempdir().expect("tempdir");
    // The standard makes it an envelope whether or not --format json was asked for.
    for args in [vec!["--version"], vec!["--format", "json", "-V"]] {
        let out = run(dir.path(), &args);
        assert_eq!(code(&out), 0);
        let v: serde_json::Value = serde_json::from_str(&stdout(&out)).expect("valid JSON");
        assert_eq!(v["status"], "success");
        assert_eq!(v["data"]["name"], "comment-crusher");
        assert!(v["data"]["version"].as_str().is_some(), "{}", stdout(&out));
    }

    // A version request outranks an argument clap would otherwise reject.
    assert_eq!(
        code(&run(dir.path(), &["--version", "--format", "bogus"])),
        0
    );
}

/// A legacy-encoded source file is still source: every comment marker is ASCII, so it is
/// decoded lossily and measured rather than refused.
#[test]
fn a_file_that_is_not_utf8_is_still_measured() {
    let dir = tempfile::tempdir().expect("tempdir");
    // `// caf<0xe9>` — a comment in Latin-1, which is not valid UTF-8.
    let mut latin1 = b"// caf\xe9\n".to_vec();
    latin1.extend_from_slice(lean_rust().as_bytes());
    std::fs::write(dir.path().join("legacy.rs"), &latin1).expect("write");
    let out = run(dir.path(), &["legacy.rs", "--format", "json"]);
    let v: serde_json::Value = serde_json::from_str(&stdout(&out)).expect("valid JSON");
    assert_eq!(v["data"]["files"][0]["language"], "rust");
    assert!(v["data"]["files"][0]["comment_chars"].as_u64().unwrap_or(0) > 0);
    assert_eq!(code(&out), 0);
}

/// A configuration that does not parse is a finding about a file, not a traceback in a
/// string, so an agent branches on it the way it branches on any other.
#[test]
fn a_broken_config_arrives_as_a_located_diagnostic() {
    let dir = tempfile::tempdir().expect("tempdir");
    write(dir.path(), "a.rs", &lean_rust());
    write(
        dir.path(),
        ".comment-crusher.toml",
        "[rules.comment-ratio]\nmax_ratio = \"not a number\"\n",
    );
    let out = run(dir.path(), &["a.rs", "--format", "json"]);
    let v: serde_json::Value = serde_json::from_str(&stdout(&out)).expect("valid JSON");
    let d = &v["data"]["diagnostics"][0];
    assert_eq!(
        d["code"], "config.rejected",
        "a value the tool refuses, not bad TOML"
    );
    assert_eq!(d["severity"], "error");
    assert!(
        d["location"]["file"]
            .as_str()
            .is_some_and(|f| f.ends_with(".comment-crusher.toml")),
        "{d}"
    );
    assert!(!d["message"].as_str().unwrap_or_default().contains('\n'));
    assert_eq!(code(&out), 3);

    // A syntax error carries the place the parser pointed at, not only prose about it.
    write(
        dir.path(),
        ".comment-crusher.toml",
        "this is = = not toml\n",
    );
    let out = run(dir.path(), &["a.rs", "--format", "json"]);
    let v: serde_json::Value = serde_json::from_str(&stdout(&out)).expect("valid JSON");
    assert_eq!(v["data"]["diagnostics"][0]["code"], "config.syntax");
    let loc = &v["data"]["diagnostics"][0]["location"];
    assert_eq!(loc["start"]["line"], 1, "{loc}");
    assert_eq!(loc["start"]["column"], 6, "{loc}");
    assert_eq!(loc["span"]["offset"], 5, "{loc}");
}

/// A file the scanner cannot read is reported as that, under its own code — never counted
/// as if it were empty.
#[test]
fn a_file_that_cannot_be_read_says_so_under_its_own_code() {
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::write(dir.path().join("blob.rs"), b"fn f() {}\0\0\0").expect("write");
    let out = run(dir.path(), &["blob.rs", "--format", "json"]);
    assert_eq!(code(&out), 3);
    assert!(
        stdout(&out).contains("unreadable.binary"),
        "{}",
        stdout(&out)
    );

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        let locked = dir.path().join("locked.rs");
        std::fs::write(&locked, "fn f() {}\n").expect("write");
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000)).expect("chmod");
        // Root reads it anyway. Say which of the two was asserted rather than passing mute.
        let out = run(dir.path(), &["locked.rs", "--format", "json"]);
        if std::fs::read(&locked).is_ok() {
            println!("running as root: the io path was not exercised");
            assert_eq!(code(&out), 0, "{}", stdout(&out));
        } else {
            assert_eq!(code(&out), 3);
            assert!(stdout(&out).contains("unreadable.io"), "{}", stdout(&out));
        }
    }
}

/// With no budget file to anchor to, a glob is read from where it was typed, and one that
/// matched nothing says so rather than widening in silence.
#[test]
fn an_allow_glob_is_read_from_the_working_directory_and_reports_matching_nothing() {
    let dir = tempfile::tempdir().expect("tempdir");
    write(dir.path(), "docs/long.md", &"a line\n".repeat(500));

    let widened = run_bare(
        dir.path(),
        &["docs", "--allow", "docs/**", "doc-length.max_lines=600"],
    );
    assert_eq!(code(&widened), 0, "{}", stdout(&widened));

    let missed = run_bare(
        dir.path(),
        &["docs", "--allow", "nothing/**", "doc-length.max_lines=600"],
    );
    assert_eq!(code(&missed), 3, "{}", stdout(&missed));
    assert!(
        stdout(&missed).contains("allowance.unused"),
        "{}",
        stdout(&missed)
    );
}

/// --config overrides the walk-up, and naming one that is not there is a missing path.
#[test]
fn an_explicit_config_replaces_the_one_the_walk_would_have_found() {
    let dir = tempfile::tempdir().expect("tempdir");
    write(dir.path(), "docs/long.md", &"a line\n".repeat(500));
    write(
        dir.path(),
        ".comment-crusher.toml",
        "[rules.doc-length]\nmax_lines = 600\n",
    );
    assert_eq!(code(&run(dir.path(), &["docs"])), 0);

    write(
        dir.path(),
        "strict.toml",
        "[rules.doc-length]\nmax_lines = 10\n",
    );
    let out = run(dir.path(), &["docs", "--config", "strict.toml"]);
    assert_eq!(code(&out), 3, "{}", stdout(&out));

    let missing = run(
        dir.path(),
        &["docs", "--config", "nope.toml", "--format", "json"],
    );
    assert_eq!(code(&missing), 24, "{}", stdout(&missing));
    assert!(
        stdout(&missing).contains("not_found"),
        "{}",
        stdout(&missing)
    );
}

/// The message an agent reads before the diagnostics has to describe the run it summarises.
#[test]
fn the_rejection_message_counts_only_what_it_names() {
    let dir = tempfile::tempdir().expect("tempdir");
    write(dir.path(), "fat.rs", &over_budget_rust());
    write(dir.path(), "docs/long.md", &"a line\n".repeat(500));

    let message = |args: &[&str]| -> String {
        let out = run(dir.path(), args);
        let v: serde_json::Value = serde_json::from_str(&stdout(&out)).expect("valid JSON");
        v["error"]["message"]
            .as_str()
            .unwrap_or_default()
            .to_string()
    };

    // No warning was raised, so none is announced, flag or no flag.
    for args in [
        vec!["fat.rs", "--format", "json"],
        vec!["fat.rs", "--warnings-as-errors", "--format", "json"],
    ] {
        let m = message(&args);
        assert!(!m.contains("warning"), "{args:?} -> {m}");
    }

    // A glob matching nothing is a warning, and only --warnings-as-errors makes it an error.
    let allow = [
        "docs",
        "--allow",
        "zzz/**",
        "doc-length.max_lines=600",
        "--format",
        "json",
    ];
    let plain = message(&allow);
    assert!(!plain.contains("warning"), "{plain}");
    let strict = message(&[allow.as_slice(), &["--warnings-as-errors"]].concat());
    assert!(strict.contains("1 warnings are errors"), "{strict}");
}

/// A filename the platform allows but UTF-8 does not must cost that one path, not the report.
#[cfg(unix)]
#[test]
fn a_filename_that_is_not_utf8_does_not_discard_the_other_findings() {
    use std::os::unix::ffi::OsStrExt as _;
    let dir = tempfile::tempdir().expect("tempdir");
    write(dir.path(), "lean.rs", &lean_rust());
    let bad = dir
        .path()
        .join(std::ffi::OsStr::from_bytes(b"\xff\xfebad.rs"));
    std::fs::write(&bad, lean_rust()).expect("write");

    let out = run(dir.path(), &[".", "--format", "json"]);
    assert_eq!(code(&out), 0, "{}", stdout(&out));
    let v: serde_json::Value = serde_json::from_str(&stdout(&out)).expect("valid JSON");
    assert_eq!(
        v["data"]["files"].as_array().map_or(0, Vec::len),
        2,
        "{}",
        stdout(&out)
    );
}

/// A reader that stops reading is not an error, and a linter that panics on one is useless
/// in a pipeline.
#[cfg(unix)]
#[test]
fn a_closed_reader_is_not_a_crash() {
    use std::process::{Command, Stdio};
    let dir = tempfile::tempdir().expect("tempdir");
    write(dir.path(), "a.rs", &lean_rust());
    for args in [vec!["--help"], vec![".", "--format", "json"]] {
        let mut child = Command::new(BIN)
            .args(&args)
            .current_dir(dir.path())
            .stdout(Stdio::piped())
            .spawn()
            .expect("spawn");
        drop(child.stdout.take());
        let status = child.wait().expect("wait");
        assert_ne!(status.code(), Some(101), "{args:?} panicked");
    }
}

/// The hundredfold is against what ships, not against whatever the repo already granted
/// itself, or a widened bound compounds every time it is widened again.
#[test]
fn the_allowance_ceiling_does_not_compound_with_the_repo_budget() {
    let dir = tempfile::tempdir().expect("tempdir");
    write(dir.path(), "docs/long.md", &"a line\n".repeat(500));
    write(
        dir.path(),
        ".comment-crusher.toml",
        "[rules.doc-length]\nmax_lines = 5000\n\n[[allow]]\npaths = [\"docs/**\"]\n\
         reason = \"x\"\nset = [\"doc-length.max_lines=400000\"]\n",
    );
    let out = run(dir.path(), &["docs"]);
    assert_eq!(code(&out), 3, "{}", stdout(&out));
    assert!(stdout(&out).contains("9000"), "{}", stdout(&out));
}
