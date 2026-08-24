// Concern: holds the scanner to what each language construct must count as | Non-concern: the rules' thresholds, or real-world code (tests/corpus.rs) | IO: (snippets) -> pass/fail

use super::*;
use crate::config::Config;
use std::path::Path;

fn syn_for(name: &str) -> Syntax {
    let cfg = Config::defaults().unwrap();
    cfg.language(Path::new(&format!("x.{name}")))
        .unwrap()
        .clone()
}

fn run(ext: &str, src: &str) -> Scan {
    scan(src, &syn_for(ext))
}

fn split(ext: &str, src: &str) -> (usize, usize) {
    let s = run(ext, src);
    (s.comment_chars(true, false), s.code_chars)
}

fn visible(src: &str) -> usize {
    src.chars().filter(|c| !c.is_whitespace()).count()
}

#[test]
fn every_construct_partitions_into_comment_plus_code() {
    let cases = [
        (
            "rs",
            "let x = 1; // note\n/* block */\n/// doc\nfn f() {}\n",
        ),
        (
            "rs",
            "let s = \"// not a comment\";\nlet r = r#\"/* nor this */\"#;\n",
        ),
        (
            "py",
            "#!/usr/bin/env python\n\"\"\"Doc.\"\"\"\nx = '# not a comment'\n",
        ),
        (
            "sh",
            "#!/bin/bash\n# note\ncat <<'EOF'\n# inside a heredoc\nEOF\n",
        ),
        (
            "lua",
            "-- note\n--[[ block\nstill block ]]\nlocal s = [[ -- not a comment ]]\n",
        ),
        ("rb", "# note\n=begin\nblock\n=end\nputs 'x'\n"),
    ];
    for (ext, src) in cases {
        let (comment, code) = split(ext, src);
        assert_eq!(comment + code, visible(src), "{ext}: {src:?}");
    }
}

#[test]
fn a_comment_marker_belongs_to_the_comment() {
    let (comment, code) = split("rs", "// ab\n");
    assert_eq!((comment, code), (4, 0));
}

#[test]
fn markers_inside_strings_are_code() {
    assert_eq!(run("rs", "let s = \"// x\";\n").regions.len(), 0);
    assert_eq!(run("rs", "let r = r#\"// x\"#;\n").regions.len(), 0);
    assert_eq!(run("py", "s = '# x'\n").regions.len(), 0);
}

#[test]
fn block_comments_nest_where_the_language_says_so() {
    let s = run("rs", "/* a /* b */ c */ let x = 1;\n");
    assert_eq!(s.regions.len(), 1);
    assert_eq!(s.regions[0].chars, visible("/* a /* b */ c */"));

    let s = run("c", "/* a /* b */ let x = 1;\n");
    assert_eq!(s.regions[0].chars, visible("/* a /* b */"));
}

#[test]
fn a_docstring_is_prose_only_when_it_opens_a_line() {
    let s = run("py", "\"\"\"Module doc.\"\"\"\n");
    assert_eq!(s.regions.len(), 1);
    assert_eq!(s.regions[0].kind, CommentKind::Doc);
    assert_eq!(run("py", "x = \"\"\"data\"\"\"\n").regions.len(), 0);
}

#[test]
fn a_heredoc_body_is_code() {
    let s = run("sh", "cat <<EOF\n# not a comment\nEOF\n# a comment\n");
    assert_eq!(s.regions.len(), 1);
    assert_eq!(s.regions[0].start_line, 4);
}

#[test]
fn a_fenced_example_inside_a_doc_comment_is_code() {
    let src = "/// Doc.\n/// ```\n/// let x = 1;\n/// ```\nfn f() {}\n";
    let (comment, code) = split("rs", src);
    assert_eq!(comment + code, visible(src));
    assert_eq!(code, visible("/// let x = 1;fn f() {}"));
}

#[test]
fn adjacent_whole_line_comments_merge_and_trailing_ones_do_not() {
    let s = run("rs", "// a\n// b\n// c\nfn f() {}\n");
    assert_eq!(s.regions.len(), 1);
    assert_eq!(s.regions[0].lines(), 3);

    let s = run("rs", "let a = 1; // a\nlet b = 2; // b\n");
    assert_eq!(s.regions.len(), 2);
}

#[test]
fn a_lifetime_is_not_an_unterminated_string() {
    let s = run("rs", "impl<'a> Foo<'a> {\n    // note\n}\n");
    assert_eq!(s.regions.len(), 1);
    assert_eq!(s.regions[0].start_line, 2);
}

#[test]
fn the_leading_comment_is_the_header_and_nothing_else_is() {
    let s = run("rs", "// banner\nfn f() {}\n// body note\n");
    assert!(s.header().is_some());
    assert_eq!(s.header().map(|r| r.start_line), Some(1));
    assert!(!s.regions[1].header);

    let s = run("rs", "fn f() {}\n// note\n");
    assert!(s.header().is_none());
}

#[test]
fn a_shebang_is_neither_comment_nor_a_header() {
    let s = run("sh", "#!/bin/sh\n# note\n");
    assert_eq!(s.regions.len(), 1);
    assert_eq!(s.regions[0].start_line, 2);
    assert!(s.regions[0].header);
}

#[test]
fn a_php_attribute_is_not_a_comment() {
    let s = run("php", "#[Attribute]\nclass A {}\n# real comment\n");
    assert_eq!(s.regions.len(), 1);
    assert_eq!(s.regions[0].start_line, 3);
}
