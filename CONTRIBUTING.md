# Contributing to Ibis

Keep every change simple, deterministic, safe and understandable to a non-technical user. Thank you for helping.

## Ways to help, easiest first

1. **Report a problem.** Use an issue template. Say which app, operating system and output format you used. Share only invented examples, never real library records, document text or screenshots of private work.
2. **Report an app where the shortcut or pasting fails.** The "App compatibility" template asks exactly what we need.
3. **Add a fixture.** A tiny synthetic `.bib` (or other) file that reproduces a bug, in `fixtures/`, is often the most useful contribution of all. See `fixtures/README.md`.
4. **Change code.** Small, focused pull requests with tests.

## AI-assisted contributions

Welcome, under the same bar as any other change: say which tool helped, keep it to one outcome, include a test or fixture that fails without the change, and pass every automated check. You must be able to say what the change does and why, and never paste private data into AI tools. Agents follow [AGENTS.md](AGENTS.md). A good start: *"Read AGENTS.md. I want Ibis to <outcome>. Propose the smallest change with a test, then implement it."*

## Setup and pull requests

Setup, the checks and the [contribution standard](docs/development.md#contribution-standard) (one outcome per pull request, a Conventional Commit title, the template, signed-off commits under the [Developer Certificate of Origin](https://developercertificate.org/)) are in [docs/development.md](docs/development.md). No contributor license agreement is required, or ever will be. The maintainer, or an agent acting for the maintainer (ADR 0015), decides on each change.

Report security issues privately as described in [SECURITY.md](SECURITY.md). Everyone participating follows the [Code of Conduct](CODE_OF_CONDUCT.md).
