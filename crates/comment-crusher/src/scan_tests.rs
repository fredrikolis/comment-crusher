// Concern: holds the scanner to what each language construct must count as | Non-concern: the rules' thresholds, or real-world code (tests/corpus.rs) | IO: (snippets) -> pass/fail

use super::*;
use crate::config::Config;
use std::path::Path;

fn run(ext: &str, src: &str) -> Scan {
    let cfg = Config::defaults().unwrap();
    let syn = cfg
        .language(Path::new(&format!("x.{ext}")))
        .unwrap()
        .clone();
    crate::scan::scan_in(src, &syn, &cfg)
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
        (
            "el",
            ";; note\n#| block\nstill block |#\n(f \"; not a comment\")\n",
        ),
        ("nim", "## doc\n#[ block\nstill ]#\nlet s = \"# no\"\n"),
        ("elm", "-- note\n{- block\nstill -}\nx = 1\n"),
        ("fs", "// note\n(* block\nstill *)\nlet x = 1\n"),
        ("f90", "! note\nprint *, 'hi'\n"),
        ("vb", "' note\nDim x = 1\n"),
        ("bat", ":: note\nREM also a note\necho hi\n"),
        ("j2", "{# note #}\n<p>{{ x }}</p>\n"),
        ("hbs", "{{!-- note --}}\n{{! short }}\n<p>{{x}}</p>\n"),
        ("cmake", "# note\n#[[ block\nstill ]]\nset(X 1)\n"),
        ("coffee", "# note\n### block\nstill ###\nx = 1\n"),
        (
            "nix",
            "# note\n/* block */\nx = \'\'raw # not a comment\'\';\n",
        ),
        ("fish", "# note\nset x 1\n"),
        (
            "gleam",
            "//// module doc\n/// item doc\n// note\nfn f() { 1 }\n",
        ),
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
fn an_embedded_script_is_scanned_as_the_language_its_tag_names() {
    let s = run("html", "<!-- page -->\n<script>\n// a note\n</script>\n");
    assert_eq!(s.regions.len(), 2);
    assert_eq!(s.regions[1].start_line, 3);
    assert_eq!(s.regions[1].chars, visible("// a note"));

    // `lang="ts"` picks TypeScript, whose `///` is a doc comment JavaScript has no notion of.
    let s = run("vue", "<script lang=\"ts\">\n/* note */\n</script>\n");
    assert_eq!(s.regions.len(), 1);
    assert_eq!(s.regions[0].chars, visible("/* note */"));

    let s = run("html", "<style>\n/* note */\n</style>\n");
    assert_eq!(s.regions.len(), 1);
}

#[test]
fn a_markup_expression_is_scanned_as_code_and_its_braces_are_balanced() {
    // Most of a Svelte component's logic is here, not in <script>.
    let s = run(
        "svelte",
        "<p>x</p>\n<form use:go={() => {\n// a note\n}}>y</form>\n",
    );
    assert_eq!(s.regions.len(), 1);
    assert_eq!(s.regions[0].chars, visible("// a note"));

    // A block directive is control flow, not an expression, and its body keeps being markup.
    let s = run("svelte", "{#if user}\n<!-- m -->\n{/if}\n");
    assert_eq!(s.regions.len(), 1);
    assert_eq!(s.regions[0].chars, visible("<!-- m -->"));

    let s = run("vue", "<p>{{ x /* note */ }}</p>\n");
    assert_eq!(s.regions.len(), 1);
    assert_eq!(s.regions[0].chars, visible("/* note */"));
}

#[test]
fn an_embedded_region_in_no_known_language_stays_code() {
    // JSON has no comments, so `//` inside one is data, not prose.
    let src = "<script type=\"application/json\">\n{\"a\": \"//b\"}\n</script>\n";
    let s = run("html", src);
    assert_eq!(s.regions.len(), 0);
    assert_eq!(s.code_chars, visible(src));
}

#[test]
fn an_astro_frontmatter_fence_is_matched_only_at_the_top() {
    let s = run("astro", "---\n// a note\n---\n<p>x --- y</p>\n<!-- m -->\n");
    assert_eq!(s.regions.len(), 2);
    assert_eq!(s.regions[0].chars, visible("// a note"));
    assert_eq!(s.regions[1].start_line, 5);
}

#[test]
fn block_comments_nest_in_every_family_that_nests_them() {
    // Lisp `#| |#`, Nim `#[ ]#`, Haskell/Elm `{- -}`, ML `(* *)`, D `/+ +/`.
    for (ext, src, tail) in [
        ("el", "#| a #| b |# c |# (f)", "(f)"),
        ("nim", "#[ a #[ b ]# c ]# f()", "f()"),
        ("elm", "{- a {- b -} c -} f", "f"),
        ("fs", "(* a (* b *) c *) f", "f"),
        ("d", "/+ a /+ b +/ c +/ f", "f"),
    ] {
        let s = run(ext, src);
        assert_eq!(s.regions.len(), 1, "{ext}");
        assert_eq!(s.code_chars, visible(tail), "{ext}");
    }
}

#[test]
fn a_shell_heredoc_survives_every_opener_form() {
    for src in [
        "cat <<EOF\n# no\nEOF\n",
        "cat <<-'EOF'\n# no\n\tEOF\n",
        "cat <<\"EOF\"\n# no\nEOF\n",
    ] {
        assert_eq!(run("sh", src).regions.len(), 0, "{src:?}");
    }
    // `<<` that opens no heredoc must not swallow the rest of the file.
    let s = run("sh", "x=$((1 << 3))\n# note\n");
    assert_eq!(s.regions.len(), 1);
}

#[test]
fn a_php_attribute_is_not_a_comment() {
    let s = run("php", "#[Attribute]\nclass A {}\n# real comment\n");
    assert_eq!(s.regions.len(), 1);
    assert_eq!(s.regions[0].start_line, 3);
}
