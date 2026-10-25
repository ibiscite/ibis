# Contributing to Ibis

Keep every change simple, deterministic, safe and understandable to a non-technical user. Thank you for helping.

## How decisions are made

Ibis has one maintainer, who sets its direction. [ROADMAP.md](ROADMAP.md) is the scope, and its "Not planned" list is part of it. So that nobody's work is wasted:

- **Ideas and questions** start in [Discussions](https://github.com/ibiscite/ibis/discussions). When an idea fits, the maintainer opens an issue for it.
- **Issues** are for problems and agreed work.
- **Code pull requests** link an issue labelled `status:accepted`. Fixtures, typo fixes and small documentation corrections need no issue.

A pull request without an accepted issue may be closed with a pointer back here. That's not a judgement of the work, only of the timing.

## Ways to help, easiest first

1. **Report a problem.** Use an issue form. Say which app, operating system and output format you used. Share only invented examples, never real library records, document text or screenshots of private work.
2. **Report how pasting went in a writing app.** The "Writing app report" form asks exactly what we need, and reports that it worked help too.
3. **Add a fixture.** A tiny synthetic `.bib` (or other) file that reproduces a bug, in `fixtures/`, is often the most useful contribution of all. See `fixtures/README.md`.
4. **Change code.** Pick an issue labelled `status:accepted`, `help wanted` or `good first issue`, say in it that you're taking it, and send a small, focused pull request with tests.

## AI tools

AI-assisted work is welcome under these rules:

- **Disclose it.** Name the tool in the pull request's "AI assistance" line.
- **Understand it.** You can explain every line you submit and why it is needed. A change you can't explain will be closed.
- **Use your own words.** Write issues, pull request descriptions and review replies yourself.
- **Keep a person in charge.** No autonomous agents opening issues, pull requests or comments.
- **Check AI findings.** Reproduce an AI-found bug or security problem yourself before reporting it.
- **No AI on `good first issue`.** Those issues are for people learning the code.

The maintainer works with AI agents too, in the open. They follow [AGENTS.md](AGENTS.md) under the maintainer's standing authorization ([ADR 0015](docs/adr/0015-autonomous-agents.md)), and every pull request they write says so.

## Setup and pull requests

Setup, the checks and the [contribution standard](docs/development.md#contribution-standard) are in [docs/development.md](docs/development.md). The standard asks for one outcome per pull request, a Conventional Commit title, the template, and signed-off commits under the [Developer Certificate of Origin](https://developercertificate.org/). No contributor license agreement is required, or ever will be. The maintainer, or an agent acting for the maintainer, decides on each change.

Report security issues privately as described in [SECURITY.md](SECURITY.md). Everyone participating follows the [Code of Conduct](CODE_OF_CONDUCT.md).
