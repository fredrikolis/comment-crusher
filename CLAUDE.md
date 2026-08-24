<!-- Concern: what an agent must know to change this repo — its constitution, layout and gates | Non-concern: using comment-crusher (README.md) or the review criteria (docs/) | IO: none -->
# CLAUDE.md — comment-crusher

## Constitution

1. **The bound is the detector.** Never raise a threshold to make something pass. A tripped
   bound is a question about the shape of the code, not about the number.
2. **Measure size, claim nothing about quality.** The tool cannot tell a good comment from a
   bad one. No message, doc, or README line may suggest otherwise.
3. **Skip, never guess.** A file in no known language is not measured. A wrong number is
   worse than no number.
4. **No fixtures.** Real code, pinned by SHA, or an inline snippet. Nothing invented.
5. **One repo answer.** The same budget must hold in CI, in a hook, and against a single file.
6. **Simplicity is king.** Solve it with the least complexity. Adding a language is
   configuration, not code.

## Layout

| Path | Role |
|---|---|
| `crates/comment-crusher/src/default_config.toml` | the language table and the shipped thresholds |
| `src/syntax.rs` | the resolved token table a scan matches against |
| `src/scan.rs` | the scanner: text + syntax -> comment regions and code |
| `src/config.rs` | layered config, allowances, language resolution |
| `src/rules/` | one module per rule; each owns its `Config` and its check |
| `src/engine.rs` | walking, parallelism, per-file dispatch |
| `src/cli.rs` | the command-line surface and the printed report |
| `corpus.toml` | the pinned third-party repos the tests measure |
| `corpus-expected.toml` | their per-language totals; a diff here is a scanner change |
| `docs/budget-policy.md` | the criteria the `budget` review gate applies |

## Before every change

```sh
cargo fmt --all
cargo clippy --all-targets -- -D warnings
cargo test
cargo run -- .
```

All four, clean. The corpus must be present: `./scripts/fetch-corpus.sh`.

## Invariants worth knowing

- **Partition.** Comment chars plus code chars equal a file's visible chars, always. Any
  change to the scanner is checked against this over the whole corpus. A merged run of
  whole-line comments must never span code — that double-counts, and the invariant catches it.
- **Longest opener wins.** `///` before `//`, `"""` before `"`. Order is by token length,
  set once in `resolve_syntax`.
- **A bad string open self-heals at end of line** unless the spec says `multiline`, so a
  mis-read quote costs one line, not the rest of the file.

## Conventions

- Commits: [Conventional Commits](https://www.conventionalcommits.org/). One logical change each.
- Rule names are kebab-case; config fields are snake_case.
- Every file carries a first-line annotation: `Concern: … | Non-concern: … | IO: …`, under
  200 characters. Run `annotated-tree --annotation-guide` before writing one.
- No `.unwrap()` or `.expect()` outside tests; clippy denies them.
