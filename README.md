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

An allowance widens a bound for the paths it names, up to a hundredfold. It cannot remove one
or switch a rule off, and there is no `allowed` list. A gas expands to occupy whatever volume
it is given; an LLM fills whatever space is available with words.

```toml
[[allow]]
paths = ["docs/spec.md"]
reason = "the specification is the product"   # required, and printed beside every finding
set = ["doc-length.max_lines=2000"]           # only genuine upper bounds may be set
```

Generated trees are not measured: `.gitignore` applies, plus `target`, `node_modules`,
`vendor`, `dist`, `build` and `.venv`.

## Use

```sh
cargo install --git https://github.com/fredrikolis/comment-crusher
comment-crusher src/parser.rs  # one file, the agent-hook path; add --format json
```

The budget lives in `.comment-crusher.toml`, found by walking up from the target, so one
answer holds in CI, in a hook, and against a file an agent just wrote. Exit codes:

| 0 | 1 | 2 | 3 | 24 |
|---|---|---|---|---|
| nothing over budget | internal error | argv rejected, a bad `--allow` value included | over budget, or the budget file rejected | no such path |

## What it measures

| Rule | Bounds | Default |
|---|---|---|
| `comment-ratio` | comment chars as a share of comment plus code | 15%, under 200 chars skipped |
| `comment-block` | one block comment, or one run of whole-line comments | 1 line, 10 for a doc comment, 16 for a banner, 400 chars |
| `doc-length` | a prose document (`.md`, `.rst`, `.adoc`, `.txt`, and kin) | 77 lines |
| `unreadable` | a resolved file that is binary or cannot be read | deny |

Across the 42 repositories the tests measure, the median comment share is 15.6%, and 77 lines
is under their documents' p75 of 89. `comment-block` is policy, not a measurement.

**Comment** is markers, their delimiters, doc comments and docstrings. **Code** is strings,
heredoc bodies, the shebang, and fenced examples in a doc comment. Counted in characters, the
two sum to every non-whitespace character, so a trailing `// why` costs what it occupies.

**A banner is exempt** from the ratio up to `header_max_chars`, so a file whose only comment
is its banner measures 0%, and every character below it is still charged; `skip_header =
false` charges the banner too. A doc comment gets more room than a remark.

## Languages

- Resolved by exact filename, then extension, then the `#!` interpreter, so `CMakeLists.txt`
  is CMake and a git hook is measured like anything else. An unknown language is skipped, and
  adding one is a row in `default_config.toml`.
- Nested blocks in 16 languages, heredocs in 6, docstrings in 4, raw strings, char literals.
- `<script>` and `<style>` scanned as the language their tag names, across HTML, Vue, Svelte
  and Astro; named, never guessed, so a `type=` the table does not map leaves its body code.
- Comment plus code equals the file's visible chars over 42 pinned repositories, and every
  declared marker has opened a real comment in one, bar the rare forms `snippet-only.txt`
  names and snippets cover.

MIT licence.
