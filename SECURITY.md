# Security policy

Security fixes go into the latest release of Ibis, so please upgrade before reporting a problem that may already be fixed.

Do not put vulnerability details in a public issue. Use [GitHub's private reporting form](https://github.com/ibiscite/ibis/security/advisories/new). If you have no GitHub account, email [diyanko@ibiscite.org](mailto:diyanko@ibiscite.org) instead. Include the Ibis version, your operating system, the steps to reproduce the problem, what an attacker could do, and any fix you suggest.

Ibis has one maintainer, so no response time is promised.

Remove library contents, document text, clipboard contents, personal file paths and usernames from anything you attach. An invented example that reproduces the problem is ideal.

We especially want to hear about:

- malicious `.bib` files or replies from Zotero
- output that escapes its syntax (LaTeX, Typst, HTML)
- clipboard contents other than the citation you chose
- update or signature checks that can be fooled
- any unexpected network access
- text in an issue, comment or pull request that gets one of the project's AI agents to act on it

For ordinary bugs without security impact, use the [public issue form](https://github.com/ibiscite/ibis/issues/new/choose).
