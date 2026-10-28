#!/usr/bin/env bash
# One-time maintainer setup for macOS signing (#1242). Run once on a Mac with
# the GitHub CLI signed in as a repository admin:
#
#   bash scripts/macos-signing-setup.sh
#
# It makes Ibis's self-signed code signing certificate, stores it as secrets of
# the "macos-signing" environment (usable only from main and v* tags), and pins
# its SHA-1 in the MACOS_SIGNING_LEAF variable. Keep the backup it writes: the
# certificate must never change, or every user grants paste permission again.
set -euo pipefail

REPO=${REPO:-ibiscite/ibis}
ENVIRONMENT=macos-signing
NAME="Ibis Self-Signed"
BACKUP=${BACKUP:-"$HOME/Documents/Ibis macOS signing"}

if gh secret list --env "$ENVIRONMENT" --repo "$REPO" 2>/dev/null | grep -qw MACOS_SIGNING_P12; then
  echo "$REPO already has a signing certificate; replacing it would make every user grant paste permission again." >&2
  exit 1
fi
if [ -e "$BACKUP" ]; then
  echo "$BACKUP already exists; move it away first." >&2
  exit 1
fi

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
# The system LibreSSL, whose PKCS#12 files the macOS keychain imports.
openssl=/usr/bin/openssl
cat > "$work/req.cnf" <<CNF
[req]
distinguished_name = dn
x509_extensions = ext
prompt = no
[dn]
CN = $NAME
[ext]
basicConstraints = critical,CA:FALSE
keyUsage = critical,digitalSignature
extendedKeyUsage = critical,codeSigning
CNF
"$openssl" req -x509 -newkey rsa:2048 -nodes -days 7300 -config "$work/req.cnf" \
  -keyout "$work/key.pem" -out "$work/cert.pem" 2> /dev/null
password=$("$openssl" rand -hex 24)
"$openssl" pkcs12 -export -name "$NAME" -inkey "$work/key.pem" -in "$work/cert.pem" \
  -out "$work/identity.p12" -passout "pass:$password"
leaf=$("$openssl" x509 -in "$work/cert.pem" -noout -fingerprint -sha1 | sed 's/.*=//; s/://g' | tr 'A-F' 'a-f')

# Deployments only from main and release tags.
gh api --method PUT "repos/$REPO/environments/$ENVIRONMENT" --input - > /dev/null <<JSON
{"deployment_branch_policy": {"protected_branches": false, "custom_branch_policies": true}}
JSON
gh api --method POST "repos/$REPO/environments/$ENVIRONMENT/deployment-branch-policies" -f name=main -f type=branch > /dev/null 2>&1 || true
gh api --method POST "repos/$REPO/environments/$ENVIRONMENT/deployment-branch-policies" -f name='v*' -f type=tag > /dev/null 2>&1 || true

base64 < "$work/identity.p12" | tr -d '\n' | gh secret set MACOS_SIGNING_P12 --env "$ENVIRONMENT" --repo "$REPO"
printf '%s' "$password" | gh secret set MACOS_SIGNING_P12_PASSWORD --env "$ENVIRONMENT" --repo "$REPO"
gh variable set MACOS_SIGNING_LEAF --body "$leaf" --repo "$REPO"

mkdir -p "$BACKUP"
cp "$work/identity.p12" "$BACKUP/ibis-macos-signing.p12"
printf '%s\n' "$password" > "$BACKUP/password.txt"
chmod -R go-rwx "$BACKUP"
echo "Done. Certificate SHA-1: $leaf"
echo "Backup in $BACKUP: move it into your password manager, then delete the folder."
