<!-- Concern: what an agent must know to change this repo — its constitution, layout and gates | Non-concern: using comment-crusher (README.md) or the review criteria (.githooks/commit-msg) | IO: none -->
# CLAUDE.md — comment-crusher

## Constitution

1. **The bound is the detector.** Never raise a threshold to make something pass. A tripped
   bound asks about the shape of the code, not the number.
2. **Measure size, claim nothing about quality.** No message, doc or README line may suggest
   the tool can tell a good comment from a bad one.
3. **Skip, never guess.** A wrong number is worse than no number.
4. **No fixtures.** Real code pinned by SHA, or an inline snippet. Nothing invented.
5. **One repo answer.** The same budget holds in CI, in a hook, and against a single file.
6. **Simplicity is king.** Least complexity. Adding a language is configuration, not code.

## Layout, relative to `crates/comment-crusher/src/`

| Path | Role |
|---|---|
| `default_config.toml` | the language table and the shipped thresholds |
| `syntax.rs` | the resolved token table a scan matches against |
| `embed.rs` | where one language inside another ends, and which it is |
| `scan.rs` | the scanner: text + syntax -> comment regions and code |
| `config.rs` | layered config, allowances, language resolution |
| `rules/` | one module per rule, each owning its `Config` and check |
| `engine.rs` | walking, parallelism, per-file dispatch |
| `cli.rs` | the CLI surface, the report, and the `--help` legend |
| `diagnostic.rs`, `lib.rs`, `main.rs` | one finding and its two shapes; the crate; argv |
| `../../../corpus.toml`, `corpus-expected.toml`, `corpus-figures.toml` | the pinned repos, their totals, and the statistics every threshold derives from |

## Before every change

`./scripts/fetch-corpus.sh` once, then `cargo fmt --all`, `cargo clippy --all-targets -- -D
warnings`, `cargo test` and `cargo run -- .`, all clean. The last holds the repo to its own
budget: a change that cannot live inside `.comment-crusher.toml` needs a better shape.

## No fixtures

`corpus.toml` pins real repositories by SHA; `scripts/fetch-corpus.sh` clones them into
gitignored `target/corpus/`. Five assertions over them:

- **The partition invariant**: comment plus code equals a file's visible chars. Lose one and
  a region was dropped; gain one and a state was left entered.
- **Every declared marker has opened a real comment** in a pinned repo. The same token in
  another language is the same scanner path, unless this one anchors or cancels it; rare
  forms no repo uses are in `snippet-only.txt`, asserted both ways.
- **A pinned repository nests a block comment**, so depth counting rests on real source.
- **`corpus-expected.toml`** per-language totals, and **`corpus-figures.toml`** the statistics
  the thresholds derive from; movement in either is a scanner change. Re-record both with
  `UPDATE_CORPUS_SNAPSHOT=1 cargo test --test corpus`, then read the diff.

## Adding a language

`default_config.toml` is the table; knobs live under LANGUAGE TABLE in `--help` (`cli.rs`). A
recombination of proved constructs needs a real-syntax snippet in `scan_tests.rs`; a new
construct needs a corpus repo.

## What this repo sets for itself

`.comment-crusher.toml` grants no allowance and overrides one rule field:
`comment-ratio.header_free_chars = 200`, because `annotated-tree --max-length` defaults to 200
and every file here carries an annotation. The shipped default is 90, the corpus median.

## Gates

Annotations are checked by [annotated-tree](https://github.com/fredrikolis/annotated-tree);
commits go through [git-agent-verdict](https://github.com/fredrikolis/git-agent-verdict).
`.githooks/commit-msg` declares which reviews run and in what order; `git agent-verdict
--reviewer-prompt <gate>` prints a gate's live brief. Per clone, once: `git config
core.hooksPath .githooks` and `git config --global agent-verdict.runner claude`.

## Invariants worth knowing

- **A merged run of whole-line comments never spans code.**
- **Named, never guessed**: an embedded region, and a tag attribute. An unresolved target is
  refused at load; nesting stops at depth 3.
- **A marker with no strings to hide data in is line-anchored**, or a URL opens a comment.
  `make` is the one exception: it has no string literals, and its trailing `#` is a comment.
- **Every knob has one home**: the LANGUAGE TABLE in `--help` (`cli.rs`).

## Conventions

- Commits: [Conventional Commits](https://www.conventionalcommits.org/), one change each.
- Rule names are kebab-case; config fields are snake_case.
- Every file carries a first-line annotation under 200 chars; see `--annotation-guide`.
- No `.unwrap()` or `.expect()` outside tests; clippy denies them.
