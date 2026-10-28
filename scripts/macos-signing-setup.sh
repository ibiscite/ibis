#!/usr/bin/env bash
# Local preparation only. Does not call GitHub, import a keychain identity or
# change trust settings. Read docs/code-signing.md before configuring GitHub.
set -euo pipefail
set +x
umask 077

if [ "$#" -ne 1 ] || [ -e "$1" ]; then
  echo "Usage: $0 <new-private-backup-directory>" >&2
  echo "Never regenerate an existing signing identity; preserve its backup." >&2
  exit 2
fi
backup=$1
# mkdir fails atomically if another process already created the destination.
mkdir -m 700 "$backup"
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
openssl=/usr/bin/openssl
cat > "$work/req.cnf" <<'CNF'
[req]
distinguished_name = dn
x509_extensions = ext
prompt = no
[dn]
CN = Ibis Self-Signed
[ext]
basicConstraints = critical,CA:FALSE
keyUsage = critical,digitalSignature
extendedKeyUsage = critical,codeSigning
CNF
"$openssl" req -x509 -newkey rsa:2048 -nodes -days 7300 -config "$work/req.cnf" \
  -keyout "$work/key.pem" -out "$work/cert.pem" 2> /dev/null
"$openssl" rand -hex 24 > "$backup/password.txt"
"$openssl" pkcs12 -export -name 'Ibis Self-Signed' -inkey "$work/key.pem" -in "$work/cert.pem" \
  -out "$backup/ibis-macos-signing.p12" -passout "file:$backup/password.txt"
"$openssl" x509 -in "$work/cert.pem" -noout -fingerprint -sha1 \
  | sed 's/.*=//; s/://g' | tr 'A-F' 'a-f' > "$backup/leaf.txt"
cp "$work/cert.pem" "$backup/certificate.pem"
echo "Local identity prepared. No GitHub or system settings changed."
echo "Keep this directory private and back it up before configuring GitHub."
echo "The configuration steps and exact effects are in docs/code-signing.md."
