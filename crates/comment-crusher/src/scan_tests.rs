// Concern: holds the scanner to what each language construct must count as | Non-concern: the rules' thresholds, or real-world code (tests/corpus.rs) | IO: (snippets) -> pass/fail

use super::*;
use crate::config::Config;
use crate::syntax::Opener;
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
    assert!(s.regions[0].header);
    assert_eq!(s.regions[0].start_line, 1);
    assert!(!s.regions[1].header);

    let s = run("rs", "fn f() {}\n// note\n");
    assert!(!s.regions[0].header);
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

    // `lang="ts"` picks TypeScript rather than the default JavaScript.
    let s = run("vue", "<script lang=\"ts\">\n/* note */\n</script>\n");
    assert_eq!(s.regions.len(), 1);
    assert_eq!(s.regions[0].chars, visible("/* note */"));

    let s = run("html", "<style>\n/* note */\n</style>\n");
    assert_eq!(s.regions.len(), 1);
}

/// A child region is counted and merged in the child's own coordinates, so the parent must
/// neither re-read it nor join it to a neighbour across the markup between them.
#[test]
fn an_embedded_child_is_counted_exactly_once() {
    for (ext, src) in [
        // A parent comment adjacent to the child's would merge across the `<script>` markup.
        ("html", "<!-- a -->\n<script>// x\n</script>\n"),
        // A fence inside an embedded doc comment: its body is code, and only once.
        (
            "html",
            "<script>\n/**\n * ```\n * let x = 1;\n * ```\n */\nlet y = 2;\n</script>\n",
        ),
        ("vue", "<!-- a -->\n<script lang=\"ts\">\n// x\n</script>\n"),
        (
            "svelte",
            "<!-- a -->\n<script>// x</script>\n<p>{y /* c */}</p>\n",
        ),
        (
            "astro",
            "---\n// fm\n---\n<!-- a -->\n<script>// x</script>\n",
        ),
    ] {
        let scan = run(ext, src);
        assert_eq!(
            scan.comment_chars(true, false) + scan.code_chars,
            visible(src),
            "{ext}: {src:?}"
        );
    }
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
fn an_unmapped_tag_attribute_names_a_language_rather_than_falling_back() {
    // `default` applies only when no attribute names anything at all.
    let named = run(
        "html",
        "<script type=\"text/x-handlebars\">\n// not js\n</script>\n",
    );
    assert_eq!(
        named.regions.len(),
        0,
        "an unmapped type must not read as the default"
    );
    let bare = run("html", "<script>\n// js\n</script>\n");
    assert_eq!(bare.regions.len(), 1);
    let mapped = run("html", "<script lang=\"ts\">\n// ts\n</script>\n");
    assert_eq!(mapped.regions.len(), 1);
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

/// `/**` outranks `/*`, so an empty block looks unterminated and would bill the rest of the
/// file as comment. Every language declaring a `/** */` doc block shares the shape.
#[test]
fn an_empty_block_comment_does_not_swallow_the_file() {
    for ext in [
        "rs", "c", "cc", "java", "js", "ts", "php", "kt", "swift", "scala",
    ] {
        let src = "/**/\nlet a = 1;\nlet b = 2;\n";
        let scan = run(ext, src);
        assert_eq!(scan.regions.len(), 1, "{ext}");
        assert_eq!(scan.regions[0].chars, 4, "{ext}: {:?}", scan.regions[0]);
        assert_eq!(
            scan.comment_chars(true, false) + scan.code_chars,
            visible(src),
            "{ext}"
        );
    }
    // A block that genuinely never closes still comments out the rest, as the language says.
    let scan = run("rs", "/* open\nlet a = 1;\n");
    assert_eq!(scan.regions.len(), 1);
    assert_eq!(scan.code_chars, 0);
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

/// A language with markers but no string literals has nowhere for data to hide, so a URL
/// would otherwise open a comment and bill a file with none as mostly prose.
#[test]
fn a_url_does_not_open_a_comment_where_the_marker_is_line_anchored() {
    for (ext, src) in [
        ("pug", "a(href='https://x/y') Link\n"),
        ("styl", "body\n  background url(https://x/y.png)\n"),
        ("adoc", "See https://x/y for more.\n"),
        ("dockerfile", "RUN curl https://x/y | sh\n"),
        ("ini", "color = #ffffff\n"),
        ("s", "mov r0, #1\nmov r1, #0x20\n"),
        ("css", "a { background: url(http://x/b.png); }\n"),
        ("scss", "a { background: url(http://x/b.png); }\n"),
        ("tcl", "set url http://x/p#frag\n"),
        ("awk", "$0 ~ /#/ { n++ }\n"),
        ("bat", "set P=%PATH::=%\n"),
        ("env", "URL=https://x/y#frag\n"),
    ] {
        let scan = run(ext, src);
        assert_eq!(scan.regions.len(), 0, "{ext}: {src:?}");
        assert_eq!(scan.code_chars, visible(src), "{ext}");
    }
    // A marker that does start its line still opens a comment.
    assert_eq!(run("pug", "//- a note\np Hi\n").regions.len(), 1);
    assert_eq!(run("ini", "; a note\nk = 1\n").regions.len(), 1);
}

#[test]
fn a_php_attribute_is_not_a_comment() {
    let s = run("php", "#[Attribute]\nclass A {}\n# real comment\n");
    assert_eq!(s.regions.len(), 1);
    assert_eq!(s.regions[0].start_line, 3);
}

/// A marker inside a string is data. Proved for every language that declares both.
#[test]
fn no_language_reads_a_marker_inside_its_own_string_as_a_comment() {
    let cfg = Config::defaults().unwrap();
    let mut checked = 0usize;
    for syn in cfg.languages() {
        let Some(spec) = syn.strings.iter().find(|s| !s.docstring) else {
            continue;
        };
        let Some((token, _)) = syn
            .openers
            .iter()
            .find(|(_, o)| matches!(o, Opener::Line(_) | Opener::Block { .. }))
        else {
            continue;
        };
        let src = format!("{}{token} n{}\n", spec.open, spec.close);
        let scan = crate::scan::scan_in(&src, syn, &cfg);
        assert_eq!(scan.regions.len(), 0, "{}: {src:?}", syn.name);
        checked += 1;
    }
    assert!(checked > 40, "only {checked} languages exercised");
}

/// One case per language in the shipped table, in that language's real syntax.
const CASES: &[(&str, &str, usize)] = &[
    ("adb", "-- n\nX : Integer := 1;\n", 1),
    ("cls", "// n\nInteger x = 1;\n/* b */\nString s = 0;\n", 2),
    ("adoc", "// n\nSome text\n", 1),
    ("asm", "; n\nmov eax, 1\n", 1),
    ("astro", "---\n// fm\n---\n<!-- b -->\n<p>x</p>\n", 2),
    ("awk", "# n\n{ print $1 }\n", 1),
    (
        "bat",
        ":: n\necho a\nREM b\necho c\nrem d\necho e\nRem f\n",
        4,
    ),
    ("bicep", "// n\nparam x string\n", 1),
    ("c", "// n\nint x = 1;\n/* b */\nchar *s = \"// no\";\n", 2),
    ("clj", "; n\n(def x 1)\n", 1),
    ("cmake", "# n\nset(X 1)\n#[[ b ]]\nset(Y 2)\n", 2),
    ("coffee", "# n\nx = 1\n### b ###\ny = 2\n", 2),
    ("cc", "// n\nint x = 1;\n/* b */\nauto s = \"// no\";\n", 2),
    ("cr", "# n\nx = 1\n", 1),
    ("cs", "// n\nint x = 1;\n/* b */\nvar s = \"// no\";\n", 2),
    ("css", "/* b */\na { color: red; }\n", 1),
    ("scss", "// n\n$x: 1;\n/* b */\na { color: $x; }\n", 2),
    ("cue", "// n\nx: 1\n", 1),
    ("d", "// n\nint x;\n/+ b +/\nint y;\n", 2),
    ("dart", "// n\nvar x = 1;\n/* b */\nvar y = 2;\n", 2),
    ("dockerfile", "# n\nFROM alpine\n", 1),
    ("env", "# n\nKEY=value\n", 1),
    ("ex", "# n\nx = 1\n", 1),
    ("elm", "-- n\nx = 1\n{- b -}\ny = 2\n", 2),
    ("erl", "% n\nf() -> ok.\n", 1),
    ("fish", "# n\nset x 1\n", 1),
    ("f90", "! n\nprint *, 1\n", 1),
    ("fs", "// n\nlet x = 1\n(* b *)\nlet y = 2\n", 2),
    ("s", "# n\nmovq $1, %rax\n/* b */\nnop\n", 2),
    ("gleam", "// n\nfn f() { 1 }\n", 1),
    ("go", "// n\nvar x = 1\n/* b */\nvar s = `// no`\n", 2),
    ("graphql", "# n\ntype Q { a: Int }\n", 1),
    ("groovy", "// n\ndef x = 1\n/* b */\ndef y = 2\n", 2),
    ("hbs", "{{!-- b --}}\n<p>{{x}}</p>\n", 1),
    ("hs", "-- n\nx = 1\n{- b -}\ny = 2\n", 2),
    ("hx", "// n\nvar x = 1;\n/* b */\nvar y = 2;\n", 2),
    ("html", "<!-- b -->\n<p>x</p>\n", 1),
    ("ini", "# n\nkey = 1\n; also\nother = 2\n", 2),
    (
        "java",
        "// n\nint x = 1;\n/* b */\nString s = \"// no\";\n",
        2,
    ),
    ("js", "// n\nlet x = 1;\n/* b */\nlet s = \"// no\";\n", 2),
    ("jsonc", "// n\n{ \"a\": 1 }\n", 1),
    ("jsonnet", "// n\n{ a: 1 }\n# also\n{ b: 2 }\n", 2),
    ("jl", "# n\nx = 1\n#= b =#\ny = 2\n", 2),
    ("just", "# n\nbuild:\n", 1),
    ("kt", "// n\nval x = 1\n/* b */\nval y = 2\n", 2),
    ("lisp", "; n\n(defun f () 1)\n#| b |#\n(defun g () 2)\n", 2),
    ("lua", "-- n\nlocal x = 1\n--[[ b ]]\nlocal y = 2\n", 2),
    ("mk", "# n\nall:\n", 1),
    ("md", "<!-- b -->\ntext\n", 1),
    ("nim", "# n\nlet x = 1\n#[ b ]#\nlet y = 2\n", 2),
    ("nix", "# n\nx = 1;\n/* b */\ny = 2;\n", 2),
    ("m", "// n\nint x = 1;\n/* b */\nid s = 0;\n", 2),
    ("ml", "(* b *)\nlet x = 1\n", 1),
    ("odin", "// n\nx := 1\n/* b */\ny := 2\n", 2),
    (
        "pas",
        "// n\nx := 1;\n{ b }\ny := 2;\n(* c *)\nz := 3;\n",
        3,
    ),
    ("pl", "# n\nmy $x = 1;\n=pod\nb\n=cut\nmy $y = 2;\n", 2),
    (
        "php",
        "// n\n$x = 1;\n# also\n$y = 2;\n/* b */\n$s = \"// no\";\n",
        3,
    ),
    ("txt", "plain text\n", 0),
    ("ps1", "# n\n$x = 1\n<# b #>\n$y = 2\n", 2),
    ("proto", "// n\nmessage M {}\n/* b */\nmessage N {}\n", 2),
    ("pug", "//- n\np Hello\n", 1),
    ("purs", "-- n\nx = 1\n{- b -}\ny = 2\n", 2),
    ("py", "# n\nx = 1\n", 1),
    ("r", "# n\nx <- 1\n", 1),
    ("rego", "# n\nallow { true }\n", 1),
    ("res", "// n\nlet x = 1\n/* b */\nlet y = 2\n", 2),
    ("rst", "plain text\n", 0),
    ("rb", "# n\nx = 1\n=begin\nb\n=end\ny = 2\n", 2),
    ("rs", "// n\nfn f() {}\n/*! b */\nlet s = \"// no\";\n", 2),
    ("scala", "// n\nval x = 1\n/* b */\nval y = 2\n", 2),
    ("glsl", "// n\nfloat x = 1.0;\n/* b */\nfloat y = 2.0;\n", 2),
    ("sh", "# n\nx=1\n", 1),
    ("sml", "(* b *)\nval x = 1\n", 1),
    ("sol", "// n\nuint x = 1;\n/* b */\nuint y = 2;\n", 2),
    ("sql", "-- n\nSELECT 1;\n/* b */\nSELECT 2;\n", 2),
    ("bzl", "# n\nx = 1\n", 1),
    (
        "styl",
        "// n\na\n  color red\n/* b */\nc\n  color blue\n",
        2,
    ),
    ("svelte", "<!-- b -->\n<p>x</p>\n", 1),
    ("swift", "// n\nlet x = 1\n/* b */\nlet y = 2\n", 2),
    ("tcl", "# n\nset x 1\n", 1),
    ("tf", "# n\nx = 1\n// also\ny = 2\n/* b */\nz = 3\n", 3),
    (
        "thrift",
        "// n\nstruct S {}\n# also\nstruct T {}\n/* b */\nstruct U {}\n",
        3,
    ),
    ("toml", "# n\nkey = 1\n", 1),
    ("ts", "// n\nlet x: number = 1;\n/* b */\nlet y = 2;\n", 2),
    ("vb", "' n\nDim x = 1\n", 1),
    ("v", "// n\nwire a;\n/* b */\nwire b;\n", 2),
    ("vhd", "-- n\nsignal a : bit;\n", 1),
    ("vue", "<!-- b -->\n<p>x</p>\n", 1),
    ("yml", "# n\nkey: 1\n", 1),
    ("zig", "// n\nconst x = 1;\n", 1),
];

/// One case per language in the shipped table, written in that language's real syntax rather
/// than generated from the table, and asserting how many comments it holds. A wrong marker
/// fails here; a partition-only assertion would not, because an unrecognized marker still
/// partitions with everything counted as code.
#[test]
fn real_source_in_every_language_yields_the_comments_it_contains() {
    let cfg = Config::defaults().unwrap();
    let mut seen = std::collections::BTreeSet::new();
    for (ext, src, want) in CASES {
        let syn = cfg.language(Path::new(&format!("x.{ext}")));
        let Some(syn) = syn else {
            unreachable!("no language resolves .{ext}")
        };
        seen.insert(syn.name.clone());
        let scan = crate::scan::scan_in(src, syn, &cfg);
        assert_eq!(
            scan.regions.len(),
            *want,
            "{} (.{ext}): {src:?} -> {:?}",
            syn.name,
            scan.regions.iter().map(|r| r.chars).collect::<Vec<_>>()
        );
        assert_eq!(
            scan.comment_chars(true, false) + scan.code_chars,
            visible(src),
            "{}: partition",
            syn.name
        );
    }
    let all: std::collections::BTreeSet<String> = cfg.languages().map(|s| s.name.clone()).collect();
    let missing: Vec<_> = all.difference(&seen).collect();
    assert!(
        missing.is_empty(),
        "languages with no real-source case: {missing:?}"
    );
}

/// Each marker exempted from the corpus proof must be exercised by a case above, or the
/// exemption rests on nothing at all.
#[test]
fn every_corpus_exemption_is_covered_by_a_snippet() {
    let cfg = Config::defaults().unwrap();
    let mut fired: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for (ext, src, _) in CASES {
        let Some(syn) = cfg.language(Path::new(&format!("x.{ext}"))) else {
            continue;
        };
        for region in crate::scan::scan_in(src, syn, &cfg).regions {
            let Some(rest) = src.get(region.start..) else {
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
    let missing: Vec<&str> = include_str!("../snippet-only.txt")
        .lines()
        .filter(|l| !l.is_empty())
        .filter(|l| !fired.contains(*l))
        .collect();
    assert!(
        missing.is_empty(),
        "snippet-only.txt exempts markers no case opens a comment with: {missing:?}"
    );
}
