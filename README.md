<!-- Concern: what comment-crusher is, what it measures, and how to configure it | Non-concern: the exhaustive flag reference (--help owns it) or how this repo is developed (CLAUDE.md) | IO: none -->
# comment-crusher

[![crates.io](https://img.shields.io/crates/v/comment-crusher.svg)](https://crates.io/crates/comment-crusher)

A gas expands to fill its container. Similarly, an AI agent will litter code with comments/notes/reminders... Unless we fight back 🥷🏻


`comment-crusher` lets you put hard limits on comment-to-code ratios and line-budgets for docs. Every time a file is modified using a tool call, `comment-crusher` will check these ratios and warn the agent if they are exceeded (so we can avoid expensive review churn, or worse end up with bloated files. 91 programming languages supported with minimal deps.

**What your agent should leave behind:**

```rust
counter += 1;
```

**What it leaves instead:**

```rust
// Increment the counter by one. We do this because the counter needs to go
// up by one at this point. Note that this is the increment step, which is
// important for the loop above to eventually terminate.
counter += 1;
```

**What the budget says:**

```
$ comment-crusher .
error: src/counter.rs [comment-ratio] 41% comment (2104/5117 chars), budget is 15%

298 code files and 14 documents, 14.2% comment (48219/339104 chars), 1 findings
```

## Use

```bash
cargo install comment-crusher

comment-crusher .
comment-crusher src/counter.rs --format editor  # path:line:column: severity[rule]: message
comment-crusher src/counter.rs --format json    # one envelope to branch on
comment-crusher --config-guide                  # what a budget may say, and every default
comment-crusher --help                          # the whole surface
```

The budget lives in `.comment-crusher.toml`, found by walking up from the target, so one answer
holds in CI, in a pre-commit hook and against a single file.

Install Claude hooks (modifies ~/.claude/settings.json)

```bash
comment-crusher install-hook --claude
```

## What the installed hook does

1. Measures the file the agent just wrote, at PostToolUse
2. Hands the findings back with the help under each, so the agent fixes it in the session
3. Says nothing where the file's own git root declared no budget

Nothing else. `install-hook --claude --uninstall` takes the hooks back out, and leaves the
budget file in place.

## What it measures

| Rule | Bounds |
|---|---|
| `comment-ratio` | comment chars as a share of comment plus code |
| `comment-block` | one block comment, or one run of whole-line comments |
| `doc-length` | a prose document (`.md`, `.rst`, `.adoc`, `.txt`, and kin) |
| `unreadable` | a resolved file that is binary or cannot be read |

**Comment** is markers, their delimiters, doc comments and docstrings. **Code** is strings,
heredoc bodies, the shebang, and fenced examples inside any comment. The two sum to every
non-whitespace character, so a trailing `// why` costs what it occupies.

## No file is exempt

An allowance widens a bound for the paths it names, and never unbinds one: it cannot remove a
bound or switch a rule off, it stops at a hundredfold of what ships, and a ratio never reaches
1. `reason` is required, and prints beside every finding it covers.

```toml
[[allow]]
paths = ["docs/spec.md"]
reason = "the specification is the product"
set = ["doc-length.max_lines=2000"]
```

MIT licence.
