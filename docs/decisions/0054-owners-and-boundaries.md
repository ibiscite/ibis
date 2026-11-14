# 0054 Owners accept intents and publish observations; boundaries are typed

Decided 2026-11-14 by the orchestrator under the maintainer's direction ("the best
rule is one the code makes unnecessary"), from four audits (history, Rust, front
end, tooling) and three architecture passes (system and flows, primitives,
extensibility), each checked against the code and reconciled by one synthesis.
It extends [0052](0052-engine-owns-library-and-citations.md); nothing here
reverses it.

## Shape

- **System.** The command line, JSON-RPC/MCP and the desktop app each enclose
  one engine. The desktop also encloses one native delivery owner and the React
  presentation.
- **Subsystems.** The engine encloses three exclusive owners: settings, library,
  and citation sessions with recents. Sources and CSL sit beneath them. Native
  delivery owns capture, clipboard, hide, paste and receipts.
- **Primitives.** An owner accepts intents and publishes immutable observations
  (whole snapshots with a revision). Work it starts carries its captured context,
  cancellation and one absolute deadline. Record, snapshot, session and
  presentation identities stay distinct types.
- **Boundaries.** `CitableFields` feeds checked artifacts and artifacts feed
  destinations. A typed `Problem` survives to rendering. A retained encoded
  reply feeds RPC framing. Executable actions feed menus, controls and keys.

## Narrow choices

- Citations and exports share preparation but are distinct artifacts: a citation
  needs the whole selection; an export names what it included and refused.
- Native effects deepen the existing delivery seam; no universal host type.
- Async work shares one operation-scope pattern inside owners; OS proofs,
  worker capacity and cancellation domains stay separate.
- Catalogues are local declarations with exhaustive matches, not a framework
  that generates every handler.

## Mistakes these remove

Each class below happened, and the boundary named makes it unwritable:

| Mistake | Boundary |
| --- | --- |
| A clean textual merge breaks a consumer or leaves bindings stale | The gate accepts only the integrated tree it tested, with every blocking suite |
| A late load, preview, hide or copy wins | Owner acceptance plus captured operation context |
| Recents bypass limits or revisions | Private recents state behind revision-checked operations |
| An export forgets privacy filtering | Mandatory `CitableFields`; checked artifact variants |
| A new command silently becomes Quit | Actions that cannot be built without their handler |
| A controller runs before its ports exist | Complete construction; no partial `connect` |
| Recovery is lost between engine and interface | `Problem` carried through events to rendering |

## Stages

Each is one pull request that deletes the path it replaces:

1. The gate's acceptance is authoritative: the tested tree is the merged tree.
2. Typed egress artifacts finish (after the egress and engine slices).
3. Attachment probing is owned by sources with one cumulative deadline.
4. One RPC method declaration and an immutable encoded reply.
5. One published settings view (after slice 5).
6. Citations capture their context; recents become private.
7. The file library's lifecycle moves into the engine.
8. Zotero's lifecycle moves through the same owner.
9. Native delivery is one operation from content to receipt.
10. Picker actions are executable declarations; accelerators are parsed types.

Two small deletion passes (front end; Rust and tooling) run once their paths
are free.

## Not doing

Universal filesystem or host frameworks, a universal scenario simulator,
workspace-wide type-branding sweeps, folding crates, and broad scanner rewrites
without evidence. Online lookup, writes and attachment sharing wait for their own
slices of [0051](0051-references-in-and-out.md).
