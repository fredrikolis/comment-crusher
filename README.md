<!-- Concern: what comment-crusher is, what it measures, and how to configure it | Non-concern: the exhaustive flag reference (--help owns it) or how this repo is developed (CLAUDE.md) | IO: none -->
# comment-crusher

A gas expands to occupy whatever volume it is given. An LLM fills whatever space is available
with words. comment-crusher gives it less space: across 90 languages from one binary, it fails
a file whose comment-to-code ratio is over budget, whose single comment runs too long, or whose
document is longer than allowed.

```
$ comment-crusher .
error: src/parser.rs [comment-ratio] 41% comment (2104/5117 chars), budget is 15%
312 files, 14.2% comment (48219/339104 chars), 1 findings
```

## No file is exempt

A limit you can always meet gets spent on words; one you cannot meet gets spent on thinking,
so never raise a threshold to make a file pass. There is no `allowed` list and no unlimited
budget: an allowance widens a bound for the paths it names, never removing one or switching a
rule off.

```toml
[[allow]]
paths = ["docs/spec.md"]
reason = "the specification is the product"   # required, and printed beside every finding
set = ["doc-length.max_lines=2000"]           # only genuine upper bounds may be set
```

## Use

```sh
comment-crusher src/parser.rs  # one file, the agent-hook path; add --format json
```

Install with `cargo install --git https://github.com/fredrikolis/comment-crusher`. The budget
lives in `.comment-crusher.toml`, found by walking up from the target, so one answer holds in
CI, in a hook, and against a file an agent just wrote. Exit codes:

| 0 | 2 | 3 | 24 |
|---|---|---|---|
| nothing over budget | argv rejected | something is over, or the config was rejected | no such path |

## What it measures

| Rule | Bounds | Default |
|---|---|---|
| `comment-ratio` | comment chars as a share of a code file | 15%, under 200 chars skipped |
| `comment-block` | one block comment, or one run of whole-line comments | 1 line, 400 chars |
| `doc-length` | a prose document (`.md`, `.rst`, `.adoc`, `.txt`, and kin) | 77 lines |
| `unreadable` | a resolved file that is binary or cannot be read | deny |

15% is the median comment share of the 38 real repositories the tests measure, over the files
this rule judges; 77 lines is their 75th-percentile document. `comment-block` is policy. Every
visible character is
either comment or code, and the two sum to the whole file: characters, not lines, so a trailing
`// why` costs what it occupies. **Comment** is markers, their delimiters, doc comments and
docstrings; **code** is strings, heredoc bodies, the shebang, and fenced examples in a doc
comment, because a doctest is code living in one.

A doc comment gets more room than a remark, the banner more still, and that banner is exempt
from the ratio so a small file can carry a mandated licence line and still have room for a
comment. Bounded, though: past `header_max_chars` it is charged like anything else, because
corpus headers run to a median of 137 characters, not to an essay.

## Languages

Resolved by exact filename, then extension, then the `#!` interpreter, so `CMakeLists.txt`
is CMake and a git hook is measured like anything else. Unknown languages are skipped;
adding one is a row in `default_config.toml`.

- Nested blocks in 16 languages, heredocs in 6, docstrings in 4, raw strings, char literals.
- `<script>` and `<style>` scanned as the language their tag names, across HTML, Vue, Svelte
  and Astro; the child is named, never guessed, so `type="application/json"` names a language
  the table lacks and that body stays code.
- Two invariants over 38 pinned repositories: comment plus code equals the file's visible
  chars, and every declared marker has opened a real comment in one, bar the rare forms
  `snippet-only.txt` names and snippets cover. MIT licence.
