<!-- Concern: the criteria a reviewer applies to any change touching a threshold, an allowance, or the language table | Non-concern: the general code standards git-agent-verdict ships | IO: none -->
# Budget policy

Criteria for the `budget` gate. Applies to `.comment-crusher.toml`, the defaults in
`default_config.toml`, and any `[[allow]]` entry.

## A threshold is never raised to make a file pass

The bound is the detector. A change that widens a default, or an allowance whose reason is
that a file currently fails, is a finding — the file is the problem the number found.

A default may move only for a reason about the *measurement*: it mis-priced a construct, or
the evidence in the corpus says the shipped number never fired.

## An allowance states a reason a reader can check

`reason` names why the exception is right, not that it is convenient. "the specification is
the product" is a reason. "too long otherwise" restates the finding.

An allowance covers the narrowest path set that works, and changes only the fields it must.

## A language entry is proved against real source

A new or changed `[languages.*]` entry needs a corpus repository whose comment syntax it
covers, pinned by SHA in `corpus.toml`. A snapshot diff that moves without an explanation of
which construct re-classified is a finding.

## The tool is honest about what it cannot see

It measures size, never quality. No message, doc line, or README sentence may claim it
distinguishes a good comment from a bad one.
