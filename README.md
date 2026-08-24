<!-- Concern: what comment-crusher is, what it measures, and how to adopt it | Non-concern: the exhaustive flag reference (--help owns it) or how this repo is developed (CONTRIBUTING.md) | IO: none -->
# comment-crusher

A language-agnostic comment budget. It fails a file whose **comment-to-code ratio** is over
budget, whose **single comment** runs too long, or whose **document** is longer than allowed —
in any of 43 languages, from one binary, with no per-language toolchain.

```
$ comment-crusher .
error: src/parser.rs [comment-ratio] 41% comment (2104/5117 chars), budget is 25%
error: src/parser.rs:88 [comment-block] comment spans 19 lines, budget is 5
error: docs/design.md [doc-length] document is 812 lines, budget is 400

312 files, 14.2% comment (48219/339104 chars), 3 findings
```

## Why

An LLM is a text generator and will fill whatever space you leave it. A limit you can always
meet gets spent on words; one you cannot meet gets spent on thinking.

Past that, the bound stops being editorial and becomes a design detector:

- A function that needed a paragraph of explanation needed extracting, not better comments.
- A document that will not fit is two documents, or one nobody will read.

**Never raise a threshold to make a file pass.** The bound is the detector, and a bigger
number only hides what it found. Where an exception is genuinely right, grant an
[allowance](#allowances) — which records *why*, in one place, for a reviewer to see.

## Install

```sh
cargo install comment-crusher
```

## Use

```sh
comment-crusher .                      # a tree
comment-crusher src/parser.rs          # one file — the pre-commit and agent-hook path
comment-crusher . --format json        # for a machine
comment-crusher . --stats              # per-language totals, no findings
```

Exit codes: `0` nothing over budget, `1` something is, `2` the configuration or a path is wrong.

The budget is found by walking up from the target, so **one repo answer holds everywhere** —
CI, a pre-commit hook, or a single file an agent just edited.

## What it measures

Every visible (non-whitespace) character of a file is either **comment** or **code**, and the
two always sum to the whole file. Characters rather than lines, so a trailing `// why` costs
what it actually occupies and no language's line shape distorts the comparison.

| Counted as comment | Counted as code |
|---|---|
| Line comments, including the marker | Everything outside a comment |
| Block comments, including delimiters | String and raw-string contents |
| Doc comments — `///`, `/** */`, docstrings | **Fenced examples inside a doc comment** |
| | Heredoc bodies |
| | The shebang |

A fenced ` ``` ` block inside a doc comment is a doctest: code that happens to live in a
comment. Pricing it as prose would tax the one thing a doc comment is for.

### The rules

| Rule | Bounds | Default |
|---|---|---|
| `comment-ratio` | comment chars as a share of a code file | 25%, files under 200 chars skipped |
| `comment-block` | one block comment, or one run of adjacent whole-line comments | 5 lines, 400 chars |
| `doc-length` | a prose document — `.md`, `.rst`, `.adoc`, `.txt` | 400 lines |

`comment-block` gives a doc comment more room than a remark (`doc_max_lines`, 10) and the
leading banner more still (`header_max_lines`, 30). The **header** — the first comment in a
file, above any code: a licence banner, an SPDX line, a file annotation — is a fixed per-file
cost, exempt from the ratio by default.

Every rule ships **on and denying**. A budget nobody enables measures nothing.

## Configuring

`.comment-crusher.toml`, found by walking up from the target. Layers, low to high: built-in
defaults, `~/.config/comment-crusher/config.toml`, the repo file, then `--allow`.

```toml
[global]
exclude = ["generated"]          # added to the built-in list, not replacing it

[rules.comment-ratio]
max_ratio = 0.20
count_doc_comments = true
min_chars = 200
skip_header = true

[rules.comment-block]
max_lines = 5
doc_max_lines = 10
header_max_lines = 30
max_chars = 400

[rules.doc-length]
max_lines = 400
```

Any rule takes `level = "allow" | "warn" | "deny"`. `allow` switches it off.

### Allowances

An exception names the paths it covers, the reason it exists, and exactly what it changes:

```toml
[[allow]]
paths = ["docs/spec.md", "docs/reference/**/*.md"]
reason = "the specification is the product"
set = ["doc-length.max_lines=2000"]
```

Every finding a widened budget still produces reports the reason beside it, so a reader sees
that a threshold was *widened* rather than met. The same thing from the command line, for a
CI step that owns its own exceptions:

```sh
comment-crusher . --allow 'docs/**/*.md' doc-length.max_lines=2000
```

## Languages

43 out of the box, resolved by extension, then exact filename, then the interpreter on a
`#!` line — so an extensionless git hook or `bin/` script is measured like anything else. A
file in no known language is **skipped, never guessed at**.

Rust, C, C++, Go, Java, Kotlin, Swift, C#, JavaScript, TypeScript, Python, Ruby, PHP, shell,
PowerShell, Lua, Perl, SQL, Haskell, Elixir, Erlang, Clojure, OCaml, Zig, Dart, Scala, R,
Julia, CSS, HTML/XML/Vue/Svelte, TOML, YAML, INI, Make, Dockerfile, Terraform, Protobuf,
GraphQL, Vim, Markdown, reStructuredText, AsciiDoc, plain text.

Each is a table entry, so adding one is configuration, not code:

```toml
[languages.nim]
extensions = [".nim"]
line = ["#"]
block = [["#[", "]#"]]
nested_block = true
strings = [{ open = '"', close = '"' }]
```

The scanner handles nested block comments, raw strings, heredocs, char literals against
lifetimes, docstrings, and markers inside strings. It is held to a **partition invariant** —
comment chars plus code chars equal the file's visible chars — over a corpus of twelve
pinned real-world repositories, one per comment-syntax family. See [CONTRIBUTING.md](CONTRIBUTING.md).

## In CI and in hooks

```yaml
- run: cargo install comment-crusher
- run: comment-crusher .
```

```sh
# .githooks/pre-commit
comment-crusher . || exit 1
```

Because the budget lives in the repo and a single file can be measured on its own, the same
answer is available to an editor, a hook, or an agent the moment it writes a file. A second
crate wiring that into an agent's `PreToolUse` hook is planned; this one is the measurement.

## Licence

MIT.
