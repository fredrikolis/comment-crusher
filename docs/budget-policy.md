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

## An allowance widens, it never exempts

`level` and a zero bound are refused inside an `[[allow]]`, and the refusal is the point: a
per-path exemption nobody reads is the failure this tool exists to prevent. A change that
reopens either route is a finding, whatever it is called.

## A language entry is proved against real source

The scanner is what is being proved, not the language. An entry that only recombines
constructs already proved — `//` beside `/* */` — needs a unit snippet. One that introduces a
new construct, a new nesting form or a new string shape, needs a corpus repository pinned by
SHA in `corpus.toml`. A snapshot diff that moves without an explanation of which construct
re-classified is a finding.

## The tool is honest about what it cannot see

It measures size, never quality. No message, doc line, or README sentence may claim it
distinguishes a good comment from a bad one.
