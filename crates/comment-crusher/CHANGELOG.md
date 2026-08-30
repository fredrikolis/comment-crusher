<!-- Concern: version history and notable changes | Non-concern: usage or roadmap | IO: none -->
# Changelog

## [0.2.0] - 2026-08-30

First published release.

- Four rules across 91 languages from one binary: `comment-ratio`, `comment-block`,
  `doc-length` and `unreadable`, at thresholds derived from 43 pinned repositories.
- `.comment-crusher.toml` declares one budget per repository, found by walking up from the
  target, so CI, a pre-commit hook and a single file give the same answer.
- `[[allow]]` widens a bound for the paths it names and never unbinds one. `reason` is
  required, and prints beside every finding it covers.
- `[global] exclude` takes gitignore patterns, and never names a file back in.
- `install-hook --claude` writes a PostToolUse entry; `hook --claude` answers it with the
  findings for the file the event names, and nothing where that git root declared no budget.
- `--format editor` and `--format json`; `--config-guide` prints what a budget may say.
