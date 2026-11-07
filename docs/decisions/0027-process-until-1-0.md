# 0027 Process until 1.0 (with 0011, 0015 and 0017)

Agents have standing engineering and merge authority over the maintainer's own
work and work by [AGENTS.md](../../AGENTS.md): no compatibility code, required "CI result" before
every merge. Stable releases wait for the maintainer's one-click approval. A
plan or answer the maintainer approves is recorded the same day in these files,
the issues and the tracking issue; nothing lives only in a chat, a tool's memory
or a local file (maintainer, 2026-10-28). The website demo is critical and stays
functionally aligned with the released app (2026-10-29).

2026-11-02 (maintainer): the path from a finished change to a nightly stays
efficient, quick and smart, so changes ship directly and are tested in batches on
the nightly. So pull requests merge themselves on a green "CI result"; related
changes go together in one pull request; the limit on active pull requests is
gone; and a separate review of the final head is needed only for the updater,
release signing and publishing, CI permissions and security boundaries. Pull
requests run only what their paths can affect, main runs everything, and a
nightly never waits for CI or the release checks. A numbered release does.

The testing rhythm (maintainer, 2026-11-02): agents merge to main on green, a
testing session checks the real app on the Mac, and the maintainer tries one
nightly a day with a short list of what changed.
