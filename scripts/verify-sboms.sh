#!/usr/bin/env bash
# Preview's publishing jobs, before anything is signed or published, from the
# repository root: every package in dist/ must carry a GitHub SBOM attestation
# from this workflow at SOURCE_REF and SOURCE_SHA, and the SBOM published next
# to it in sbom/ must be the attested one. sbom/ holds nothing else.
# GH_TOKEN, REPO, SOURCE_REF, SOURCE_SHA come from the workflow.
set -euo pipefail
used=()
for file in dist/*; do
  name=$(basename "$file")
  case "$name" in
    SHA256SUMS-*) continue ;;
    Ibis-*) sbom="sbom/$(sed -E 's/^(Ibis-(macOS-Apple-silicon|macOS-Intel|Windows|Linux)).*/\1/' <<< "$name").cdx.json" ;;
    ibis-*) sbom="sbom/${name%%.*}-cli.cdx.json" ;;
    *) echo "::error::$name is not a known package"; exit 1 ;;
  esac
  [ -f "$sbom" ] || { echo "::error::$name has no SBOM ($sbom)"; exit 1; }
  attested=$(gh attestation verify "$file" --repo "$REPO" --source-ref "$SOURCE_REF" --source-digest "$SOURCE_SHA" \
    --signer-workflow "$REPO/.github/workflows/preview.yml" --predicate-type https://cyclonedx.org/bom \
    --format json --jq '[.[].verificationResult.statement.predicate] | unique | if length == 1 then .[0] else error("several SBOMs") end')
  if [ "$(jq -S . <<< "$attested")" != "$(jq -S . "$sbom")" ]; then
    echo "::error::$sbom is not the SBOM attested for $name"
    exit 1
  fi
  used+=("$sbom")
done
for sbom in sbom/*; do
  printf '%s\n' "${used[@]}" | grep -qxF "$sbom" || { echo "::error::$sbom describes no package"; exit 1; }
done
echo "Every package's SBOM is attested and matches."
