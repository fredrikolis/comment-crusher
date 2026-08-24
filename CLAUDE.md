<!-- Concern: what an agent must know to change this repo — its constitution, layout and gates | Non-concern: using comment-crusher (README.md) or the review criteria (.githooks/commit-msg) | IO: none -->
# CLAUDE.md — comment-crusher

## Constitution

1. **The bound is the detector.** Never raise a threshold to make something pass. A tripped
   bound is a question about the shape of the code, not the number.
2. **Measure size, claim nothing about quality.** The tool cannot tell a good comment from a
   bad one, and no message, doc or README line may suggest otherwise.
3. **Skip, never guess.** A wrong number is worse than no number.
4. **No fixtures.** Real code, pinned by SHA, or an inline snippet. Nothing invented.
5. **One repo answer.** The same budget must hold in CI, in a hook, and against a single file.
6. **Simplicity is king.** Least complexity. Adding a language is configuration, not code.

## Layout

| Path | Role |
|---|---|
| `src/default_config.toml` | the language table and the shipped thresholds |
| `src/syntax.rs` | the resolved token table a scan matches against |
| `src/embed.rs` | where one language inside another ends, and which it is |
| `src/scan.rs` | the scanner: text + syntax -> comment regions and code |
| `src/config.rs` | layered config, allowances, language resolution |
| `src/rules/` | one module per rule, each owning its `Config` and check |
| `src/engine.rs` | walking, parallelism, per-file dispatch |
| `src/cli.rs` | the CLI surface, the report, and the `--help` legend |
| `corpus.toml` / `corpus-expected.toml` | the pinned repos, and their totals |

## Before every change

`./scripts/fetch-corpus.sh` once, then `cargo fmt --all`, `cargo clippy --all-targets -- -D
warnings`, `cargo test`, and `cargo run -- .`, all clean. The last is the repo holding itself
to its own budget: a change that cannot live inside `.comment-crusher.toml` needs a better
shape.

## No fixtures

`corpus.toml` pins real repositories by SHA; `scripts/fetch-corpus.sh` clones them into
gitignored `target/corpus/`. Three assertions over them:

- **The partition invariant** over every corpus file. Lose a character and a region was
  dropped; gain one and a state was left entered.
- **Every declared marker has opened a real comment** in a pinned repo. Rare forms no repo
  uses are listed in `snippet-only.txt`, asserted both ways so it cannot grow or go stale.
- **`corpus-expected.toml`**, per-language totals. Revs are SHAs, so movement is a scanner
  change. Re-record with `UPDATE_CORPUS_SNAPSHOT=1 cargo test --test corpus`; read the diff.

## Adding a language

`src/default_config.toml` is the table; knobs are documented under LANGUAGE TABLE in `--help`
(`src/cli.rs`). A recombination of proved constructs needs a snippet in `src/scan_tests.rs`,
written in the language's real syntax; a new construct needs a corpus repo.

## Gates

Annotations are checked by [annotated-tree](https://github.com/fredrikolis/annotated-tree);
commits go through [git-agent-verdict](https://github.com/fredrikolis/git-agent-verdict).
`.githooks/commit-msg` declares which reviews run and in what order; run
`git agent-verdict --reviewer-prompt <gate>` for a gate's live brief. Per clone, once:
`git config core.hooksPath .githooks` and `git config --global agent-verdict.runner claude`.

## Invariants worth knowing

- **Partition.** Comment chars plus code chars equal a file's visible chars, always, checked
  over the whole corpus. A merged run of whole-line comments must never span code.
- **An embedded region is named, never guessed.** An unresolved child leaves the body code,
  which under-reports rather than inventing comments. Nesting stops at depth 3.
- **Every knob has one home**: the LANGUAGE TABLE in `--help` (`src/cli.rs`). The table and
  the structs mirroring it carry no prose.

## Conventions

- Commits: [Conventional Commits](https://www.conventionalcommits.org/), one change each.
- Rule names are kebab-case; config fields are snake_case.
- Every file carries a first-line annotation under 200 characters. Run
  `annotated-tree --annotation-guide` before writing one.
- No `.unwrap()` or `.expect()` outside tests; clippy denies them.
