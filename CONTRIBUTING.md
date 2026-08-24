<!-- Concern: how this repo is developed and gated, and how to work on the scanner | Non-concern: using comment-crusher (README.md) or the annotation format (`annotated-tree --annotation-guide`) | IO: none -->
# Contributing

Contributions are welcome, and issues and ideas more so.

## The loop

```sh
cargo fmt --all
cargo clippy --all-targets -- -D warnings
./scripts/fetch-corpus.sh          # once; idempotent afterwards
cargo test
cargo run -- .                     # the repo holds itself to its own budget
```

All four must pass clean. The last one is not ceremony: `.comment-crusher.toml` is this
repo's budget, and a change that cannot live inside it needs a better shape, not a bigger
number.

## No fixtures

There are no test fixture files. `corpus.toml` pins twelve real repositories by commit SHA —
one per comment-syntax family — and `scripts/fetch-corpus.sh` clones them into gitignored
`target/corpus/`. Nothing third-party is vendored, and nobody maintains a fake source tree.

Two things are asserted over that corpus:

- **The partition invariant.** For every file, comment chars plus code chars equal the file's
  visible chars. A scanner that loses a character has dropped a region; one that gains a
  character has left a state it entered. This holds across ~1,800 real files.
- **A pinned snapshot.** `corpus-expected.toml` records per-language totals. Because the revs
  are SHAs, any movement is a scanner change. Re-record with
  `UPDATE_CORPUS_SNAPSHOT=1 cargo test --test corpus` and **review the diff** — it is the
  regression report.

Construct-level cases live as inline snippets in `crates/comment-crusher/src/scan_tests.rs`.
A new language belongs in the corpus if it brings a comment syntax no pinned repo covers.

## Adding a language

`crates/comment-crusher/src/default_config.toml` is the table. An entry is configuration, not
code — see the legend at the top of that file for every knob. Prove it against real source by
adding a repo to `corpus.toml`, not by writing a fixture.

## Gates

Every file carries a first-line annotation — `Concern: … | Non-concern: … | IO: …` — checked
by [annotated-tree](https://github.com/fredrikolis/annotated-tree). Commits go through
[git-agent-verdict](https://github.com/fredrikolis/git-agent-verdict), which dispatches the
reviewers and records their verdicts in the commit message.

Per clone, once:

```sh
git config core.hooksPath .githooks
git config --global agent-verdict.runner claude
```

`.githooks/pre-commit` runs the mechanical gates — annotations, then this repo's own comment
budget. `.githooks/commit-msg` declares which reviews run and in what order; run
`git agent-verdict --reviewer-prompt <gate>` for a gate's live brief. A second copy of either
here would drift.
