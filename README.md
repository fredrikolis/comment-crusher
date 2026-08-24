<!-- Concern: what comment-crusher is, what it measures, and how to configure it | Non-concern: the exhaustive flag reference (--help owns it) or how this repo is developed (CLAUDE.md) | IO: none -->
# comment-crusher

A language-agnostic comment budget. It fails a file whose **comment-to-code ratio** is over
budget, whose **single comment** runs too long, or whose **document** is longer than allowed —
in 88 languages, from one binary, with no per-language toolchain.

```
$ comment-crusher .
error: src/parser.rs [comment-ratio] 41% comment (2104/5117 chars), budget is 25%
error: src/parser.rs:88 [comment-block] comment spans 19 lines, budget is 5
error: docs/design.md [doc-length] document is 812 lines, budget is 400

312 files, 14.2% comment (48219/339104 chars), 3 findings
```

## Why

A gas expands to occupy whatever volume it is given. An LLM fills whatever space is available
with words. Neither is a flaw to be argued with — it is what the thing does, and the only
answer is a smaller container.

A limit you can always meet gets spent on words; one you cannot meet gets spent on thinking.
Past that, the bound stops being editorial and becomes a design detector:

- A function that needed a paragraph of explanation needed extracting, not better comments.
- A document that will not fit is two documents, or one nobody will read.

**Never raise a threshold to make a file pass.** The bound is the detector, and a bigger
number only hides what it found.

## No file is exempt

There is no `allowed` list, and no unlimited budget. Every file a run measures is measured
against a finite bound. An **allowance** widens a bound for the paths it names — it can never
remove one, switch a rule off, or set a limit to zero; the tool refuses all three. A file is
over budget or under it, never outside.

```toml
[[allow]]
paths = ["docs/spec.md", "docs/reference/**/*.md"]
reason = "the specification is the product"
set = ["doc-length.max_lines=2000"]
```

Every finding a widened budget still produces prints the reason beside it, so a reader sees
that a threshold was *widened* rather than met. Same thing from the command line:

```sh
comment-crusher . --allow 'docs/**/*.md' doc-length.max_lines=2000
```

## Use

```sh
comment-crusher .                  # a tree
comment-crusher src/parser.rs      # one file — the pre-commit and agent-hook path
comment-crusher . --format json    # for a machine
comment-crusher . --stats          # per-language totals
```

Exit `0` nothing over budget, `1` something is, `2` bad configuration or path. Install with
`cargo install comment-crusher`.

The budget lives in `.comment-crusher.toml`, found by walking up from the target, so **one
repo answer holds everywhere** — CI, a pre-commit hook, or a single file an agent just wrote.

## What it measures

| Rule | Bounds | Default |
|---|---|---|
| `comment-ratio` | comment chars as a share of a code file | 25%, files under 200 chars skipped |
| `comment-block` | one block comment, or one run of adjacent whole-line comments | 5 lines / 400 chars |
| `doc-length` | a prose document — `.md`, `.rst`, `.adoc`, `.txt` | 400 lines |

Every visible character is either **comment** or **code**, and the two sum to the whole file.
Characters, not lines, so a trailing `// why` costs what it occupies. Markers and delimiters
are comment; strings, heredoc bodies, the shebang, and **fenced examples inside a doc
comment** are code — a doctest is code that happens to live in a comment.

A doc comment gets more room than a remark (`doc_max_lines`), and the leading banner more
still (`header_max_lines`); that header — licence, SPDX, file annotation — is a fixed
per-file cost, exempt from the ratio. Every rule ships **on and denying**. See
`crates/comment-crusher/src/default_config.toml` for every knob, and `--help` for every flag.

## Languages

Resolved by extension, then exact filename, then the `#!` interpreter — so an extensionless
git hook or `bin/` script is measured like anything else. A file in no known language is
**skipped, never guessed at**. Adding one is configuration, not code:

```toml
[languages.nim]
extensions = [".nim"]
line = ["#"]
doc_line = ["##"]
block = [["#[", "]#"]]
nested_block = true
```

The scanner handles nested block comments in five families, raw strings, heredocs, char
literals against lifetimes, docstrings, and markers inside strings. It is held to a
**partition invariant** — comment chars plus code chars equal the file's visible chars — over
a corpus of twelve pinned real-world repositories.

## Licence

MIT.
