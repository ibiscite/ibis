# 0049 Local machines gate merges

2026-11-13 (maintainer): this is a solo project; the maintainer and his agents
are the only writers. **The local machines are the gate; GitHub CI confirms
after merge.** Waiting for a hosted runner before every merge costs time
without establishing a new trust boundary.

`scripts/land.sh` owns the complete gate, with no changed-path plan. It pushes
the clean branch, runs the Mac and Ubuntu in parallel, and adds Windows
PowerShell 5.1 tests when packaging or PowerShell paths change. Each result
belongs to the exact head. Ubuntu runs detached with a log/end marker and an
exclusive checkout/build lock. Warm target directories are kept; no sccache.
Unreachable machines fail unless explicitly skipped; the PR and status record
those exceptions. [Testing](../testing.md#where-checks-run) documents commands,
machines, logs and invocation.

After passing, preserve existing PR text, append one gate line, set the commit
status **Local gate** to success on that SHA and squash-merge immediately.
The release app's PR uses this same gate. Delete the path classifier, local
check-plan runner, PR placeholders and Release Please CI-dispatch refusal.
Keep title feedback for outside PRs: it is read-only and uses no credentials.
Nix and syntax-tool builds move to main pushes. Release workflows retain full
build, signing, provenance, install/start and publication checks.

Main CI always confirms the full three-system matrix. Daily/manual runs add
heavy qualification. **CI result** still protects nightly publication through
`scripts/ci-main.mjs`; it no longer authorizes merges. A red main preserves the
preceding nightly. This updates the merge/check portions of decisions 0027,
0034 and 0037. Number 0048 was already assigned to Nix.

With warm dependencies and targets, budget about 4–8 minutes from push to merge:
`max(Mac full gate, Ubuntu Rust + Vitest, optional Windows tests) + API calls`.
A cold Rust target can take 15–30 minutes. These are estimates, pending one
real run; hosted confirmation and release builds are outside merge latency.

## Applying

The orchestrator applies the repository ruleset change **before this change
lands**, once no pull request is waiting on `CI result`, then lands this change
with its own `land.sh`: pull requests no longer run CI, so nothing would post
`CI result` for it. The commands below preserve other rules, replace `CI result` with
`Local gate`, remove its Actions integration binding (a user posts the commit
status), and remove any `merge_queue` rule. They back up each changed ruleset
under ignored `.local/`; review that backup before restoring any settings.
No settings were changed while implementing this decision.

```sh
repo=$(gh repo view --json nameWithOwner --jq .nameWithOwner)
mkdir -p .local/rulesets
while IFS= read -r id; do
  gh api "repos/$repo/rulesets/$id" > ".local/rulesets/$id.before.json"
  if ! jq -e '.rules | any(.type == "merge_queue" or
    (.type == "required_status_checks" and
     any(.parameters.required_status_checks[]; .context == "CI result")))' \
    ".local/rulesets/$id.before.json" > /dev/null; then
    continue
  fi
  jq '{name, target, enforcement, bypass_actors, conditions,
    rules: [.rules[] | select(.type != "merge_queue") |
      if .type == "required_status_checks" then
        .parameters.required_status_checks |= map(
          if .context == "CI result" then
            .context = "Local gate" | del(.integration_id)
          else . end)
      else . end]}' ".local/rulesets/$id.before.json" \
    > ".local/rulesets/$id.local-gate.json"
  gh api --method PUT "repos/$repo/rulesets/$id" \
    --input ".local/rulesets/$id.local-gate.json"
done < <(gh api --paginate "repos/$repo/rulesets" \
  --jq '.[] | select(.source_type == "Repository") | .id')
gh api "repos/$repo/rules/branches/main" \
  --jq '.[] | select(.type == "required_status_checks" or .type == "merge_queue")'
```

Then run `land.sh` on one small branch and verify the SHA-bound status and
immediate squash merge, a detached Ubuntu run through an SSH interruption,
and the Windows 5.1 tests. Check that main push CI confirms all three systems
and that Preview still waits for passing main CI before publishing a nightly.
