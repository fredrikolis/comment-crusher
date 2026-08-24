<!-- Concern: the invariants comment-crusher must always satisfy, and the vocabulary they quantify over | Non-concern: how any is implemented, or what threshold is right | IO: none -->
# SPEC — comment-crusher

The register of decisions comment-crusher is willing to freeze. How any of them is enforced —
the rules, the gates, the tests — lives with the enforcement, not here.

**Intentionally under-specified.** Anything no clause forbids is admissible. A new capability
needs no clause admitting it, only the absence of one it breaks.

## Vocabulary

- **run** — one execution against one set of Targets, from the arguments it is given to the
  Report it completes.
- **Target** — a path a run is pointed at, and everything beneath it.
- **Measured file** — a file within a Target whose Language a run resolved.
- **Language** — the token table by which a Measured file is split into Comment and Code.
- **Comment / Code** — the two classes every visible character of a Measured file falls into.
- **Budget** — the thresholds in force for one Measured file, after every Allowance is applied.
- **Allowance** — a declared widening of a Budget for the paths it names, carrying its reason.
  It is only ever a widening: nothing in this vocabulary exempts a file.
- **Report** — everything a run emits on any channel a caller can observe, its exit status
  included.

## MEASURE

**M1** — Every visible character of a Measured file is either Comment or Code, and no
character is both.

**M2** — Every character of a comment marker or delimiter is Comment, and every character
within a string, a heredoc body, or a fenced example inside a comment is Code.

## SCOPE

**S1** — Every file within a Target is either Measured or falls under a criterion the Report
states, and no file is Measured under a Language its path did not resolve.

## BUDGET

**B1** — Every finding a Report raises names the rule that raised it, the file, the measured
value, and the Budget it exceeded.

**B2** — Every finding produced under a widened Budget carries the Allowance's reason.

**B3** — Every Measured file has a finite Budget under every rule in force, and no Allowance
removes a rule, disables one, or makes a bound unlimited.

## CORE

**C1** — Every Report is determined by the run's arguments, the bytes at the configuration
paths it resolves, and the bytes within its Targets, and by nothing else.

**C2** — A run creates, changes, and removes no artifact anywhere.
