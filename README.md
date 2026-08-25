<!-- Concern: what comment-crusher is, what it measures, and how to configure it | Non-concern: the exhaustive flag reference (--help owns it) or how this repo is developed (CLAUDE.md) | IO: none -->
# comment-crusher

A gas expands to occupy whatever volume it is given. An LLM fills whatever space is available
with words. comment-crusher gives it less space: across 89 languages from one binary, it fails
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
reason = "the specification is the product"
set = ["doc-length.max_lines=2000"]
```

`reason` is required, so every finding under a widened budget prints one; `--allow` does the
same on the command line, and only genuine upper bounds may be set.

## Use

```sh
comment-crusher src/parser.rs  # one file, the agent-hook path; add --format json
```

Install with `cargo install --git https://github.com/fredrikolis/comment-crusher`. Exit `0`
nothing over budget, `3` something is or the config was rejected, `2` argv rejected, `24` no
such path. The budget lives in `.comment-crusher.toml`, found by walking up from the target,
so one answer holds in CI, in a hook, and against a file an agent just wrote.

## What it measures

| Rule | Bounds | Default |
|---|---|---|
| `comment-ratio` | comment chars as a share of a code file | 15%, under 200 chars skipped |
| `comment-block` | one block comment, or one run of whole-line comments | 1 line, 400 chars |
| `doc-length` | a prose document (`.md`, `.rst`, `.adoc`, `.txt`, and kin) | 77 lines |
| `unreadable` | a resolved file that is binary or cannot be read | deny |

A file over 15% comment is unusual: that is roughly the median of the 38 real repositories the
tests measure, and 77 lines is their 75th-percentile document. `comment-block` is a policy, and
says so. Every visible character is
either comment or code, and the two sum to the whole file: characters, not lines, so a trailing
`// why` costs what it occupies. **Comment** is markers, their delimiters, doc comments and
docstrings; **code** is strings, heredoc bodies, the shebang, and fenced examples in a doc
comment, because a doctest is code living in one.

A doc comment gets more room than a remark, the banner more still, and that banner is exempt
from the ratio so a small file can carry a mandated licence line and still have room for a
comment. `header_max_lines` keeps the banner itself bounded.

## Languages

Resolved by exact filename, then extension, then the `#!` interpreter, so `CMakeLists.txt` is
CMake and a git hook is measured like anything else. A file in no known language is skipped;
adding one is a row in `crates/comment-crusher/src/default_config.toml`, never code.

- Nested block comments in 16 languages, heredocs in 6, docstrings in 4, raw strings, char
  literals held apart from lifetimes.
- `<script>` and `<style>` scanned as the language their tag names, across HTML, Vue, Svelte
  and Astro; the child is named, never guessed, so `type="application/json"` names a language
  the table lacks and that body stays code.
- Two invariants over 38 pinned repositories: comment plus code equals the file's visible
  chars, and every declared marker has opened a real comment in one, bar the rare forms
  `crates/comment-crusher/snippet-only.txt` names and snippets cover. MIT licence.
