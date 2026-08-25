<!-- Concern: what comment-crusher is, what it measures, and how to configure it | Non-concern: the exhaustive flag reference (--help owns it) or how this repo is developed (CLAUDE.md) | IO: none -->
# comment-crusher

Across 91 languages from one binary, comment-crusher fails a file whose comment share
is over budget, whose single comment runs too long, or whose document is longer than allowed.

```
$ comment-crusher .
error: src/parser.rs [comment-ratio] 41% comment (2104/5117 chars), budget is 15%

298 code files and 14 documents, 14.2% comment (48219/339104 chars), 1 findings
```

## No file is exempt

An allowance widens a bound for the paths it names. It cannot remove one or switch a rule off,
and it stops at a hundredfold, or at whatever the field itself allows: a ratio never reaches 1.
A gas expands to occupy whatever volume it is given; an LLM fills whatever space is available
with words.

```toml
[[allow]]
paths = ["docs/spec.md"]
reason = "the specification is the product"   # required, and printed beside every finding
set = ["doc-length.max_lines=2000"]           # only genuine upper bounds may be set
```

Generated trees are not measured: `.gitignore` applies, plus `target`, `node_modules`,
`vendor`, `dist`, `build` and `.venv`. `[global] exclude` adds to that list, never replaces it.

## Use

```sh
cargo install --git https://github.com/fredrikolis/comment-crusher
comment-crusher src/parser.rs  # one file, the agent-hook path; add --format json
```

The budget lives in `.comment-crusher.toml`, found by walking up from the target, so one
answer holds in CI, in a hook, and against a file an agent just wrote. Exit codes:

| Code | Meaning |
|---|---|
| 0 | nothing over budget |
| 1 | internal error |
| 2 | argv rejected, including a bad `--allow` value |
| 3 | a file over budget, or the budget file rejected |
| 24 | no such path |

## What it measures

| Rule | Bounds | Default |
|---|---|---|
| `comment-ratio` | comment chars as a share of comment plus code | 15%, under 200 chars skipped |
| `comment-block` | one block comment, or one run of whole-line comments | 1 line and 163 chars; 8 and 314 for a doc comment; 13 and 971 for a banner |
| `doc-length` | a prose document (`.md`, `.rst`, `.adoc`, `.txt`, and kin) | 90 lines |
| `unreadable` | a resolved file that is binary or cannot be read | deny |

The median comment share is 18% and the 75th-percentile document is 90 lines, across the
43 repositories the tests measure. `comment-block` is policy, not a measurement.

**Comment** is markers, their delimiters, doc comments and docstrings. **Code** is strings,
heredoc bodies, the shebang, and fenced examples inside any comment. Counted in characters, the
two sum to every non-whitespace character, so a trailing `// why` costs what it occupies.

**A banner is discounted**, not exempt: the ratio ignores its first `header_free_chars` and
charges the rest like any comment, so a file carrying only a short banner measures 0% while a
long one still fails. `skip_header = false` charges it whole. Under `comment-block`, a doc
comment gets more lines and more characters than a remark, and a banner more still.

## Languages

- Resolved by exact filename, then extension, then the `#!` interpreter, so `CMakeLists.txt`
  is CMake and a git hook is measured like anything else. An unknown language is skipped, and
  adding one is a row in `default_config.toml`.
- Nested blocks in 16 languages, heredocs in 6, docstrings in 4, raw strings, char literals.
- `<script>` and `<style>` scanned as the language their tag names, across HTML, Vue, Svelte
  and Astro; named, never guessed, so a `type=` the table does not map leaves its body code.
- Comment plus code equals the file's visible chars over 43 pinned repositories, and every
  declared marker has opened a real comment in one.

MIT licence.
