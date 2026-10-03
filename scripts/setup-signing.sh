#!/bin/bash
# Creates a permanent, self-signed code-signing identity for PastePilot in your
# login Keychain (one time per Mac). Every build signed with it keeps the same
# identity, so macOS remembers Accessibility, Microphone and Keychain
# permissions across updates. The private key never leaves your Keychain.
#
#   npm run setup-signing
set -euo pipefail

NAME="PastePilot Local Signing"
KEYCHAIN="$HOME/Library/Keychains/login.keychain-db"

if security find-certificate -c "$NAME" "$KEYCHAIN" >/dev/null 2>&1; then
  echo "✔ \"$NAME\" already exists. Nothing to do."
  exit 0
fi

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
PASS="$(openssl rand -hex 16)"

cat > "$TMP/cert.cnf" <<CNF
[req]
distinguished_name = dn
x509_extensions = ext
prompt = no
[dn]
CN = $NAME
[ext]
basicConstraints = critical, CA:false
keyUsage = critical, digitalSignature
extendedKeyUsage = critical, codeSigning
CNF

openssl req -x509 -newkey rsa:2048 -nodes -days 7300 \
  -keyout "$TMP/key.pem" -out "$TMP/cert.pem" -config "$TMP/cert.cnf" 2>/dev/null
# Legacy PKCS#12 algorithms so macOS `security import` accepts the file.
openssl pkcs12 -export -inkey "$TMP/key.pem" -in "$TMP/cert.pem" -name "$NAME" \
  -out "$TMP/identity.p12" -passout "pass:$PASS" \
  -keypbe PBE-SHA1-3DES -certpbe PBE-SHA1-3DES -macalg sha1 2>/dev/null \
  || openssl pkcs12 -export -legacy -inkey "$TMP/key.pem" -in "$TMP/cert.pem" -name "$NAME" \
       -out "$TMP/identity.p12" -passout "pass:$PASS"

# -T lets codesign use the key without asking each time.
security import "$TMP/identity.p12" -k "$KEYCHAIN" -P "$PASS" -T /usr/bin/codesign >/dev/null

echo "✔ Created \"$NAME\" in your login Keychain."
echo "  Builds are now signed with it, so permissions survive updates."
echo "  If macOS asks whether codesign may use the key, click Always Allow."
