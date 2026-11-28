# 0069 A missing DOI can be looked up at doi.org, once allowed

**Decided** 2026-11-28 by the maintainer, choosing the sharing and sources
round for the native Mac app. It is the first network use beyond updates and
feedback, so it is recorded here and in [PRIVACY.md](../../PRIVACY.md).

## The decision

1. A DOI or `doi.org` link pasted or typed into the picker is looked for in
   the library first.
2. When it is missing, one row offers "Look up 10.xxxx/…". The first lookup
   asks once whether Ibis may send DOIs to `doi.org`. Until that yes, nothing is
   sent and the row only says the DOI is not in the library.
3. Only the DOI is sent, by DOI content negotiation for CSL JSON. No library
   data, search text, document text or identifier of the person goes with it.
4. The result is added where Ibis can write locally: Ibis's own file, or the
   manager when it can take the item on this computer. A write to a manager's
   local port still needs the maintainer's yes before it is built.
5. The permission lives in the engine's settings, shows in Settings, and can be
   turned off at any time. The CLI and MCP use the same setting.

## Consequences

- PRIVACY.md lists the flow, its destination and its consent.
- Every other lookup (titles, ISBNs, other services) needs its own decision.
