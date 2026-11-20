#!/bin/bash
#
# Creates a self-signed code-signing certificate in your login Keychain, used by
# scripts/build-release.sh to sign DB Refresh.
#
# Why: macOS remembers which app may read a Keychain item by its code signature.
# Ad-hoc signatures change on every build, so macOS asks for your Mac password again
# after each update. A stable certificate keeps the signature's identity the same,
# so "Always Allow" sticks across builds.
#
# This does NOT replace an Apple Developer ID: other Macs still need "Open Anyway"
# the first time.
#
# Usage:
#   scripts/create-signing-certificate.sh
# macOS asks once for your login password to trust the certificate for code signing.

set -Eeuo pipefail

CERTIFICATE_NAME="DB Refresh Local Signing"
KEYCHAIN="$HOME/Library/Keychains/login.keychain-db"
VALID_DAYS=3650
OPENSSL="$(command -v openssl)"

if security find-identity -v -p codesigning | grep -q "\"$CERTIFICATE_NAME\""; then
    echo "\"$CERTIFICATE_NAME\" already exists and is valid. Nothing to do."
    exit 0
fi

WORK_DIR="$(mktemp -d)"
# The private key must not outlive this script outside the Keychain.
trap 'rm -rf "$WORK_DIR"' EXIT
EXPORT_PASSWORD="$("$OPENSSL" rand -hex 16)"

echo "==> Creating certificate \"$CERTIFICATE_NAME\" (valid $VALID_DAYS days)"
"$OPENSSL" req -x509 -newkey rsa:2048 -nodes \
    -days "$VALID_DAYS" \
    -subj "/CN=$CERTIFICATE_NAME" \
    -addext "basicConstraints=critical,CA:false" \
    -addext "keyUsage=critical,digitalSignature" \
    -addext "extendedKeyUsage=critical,codeSigning" \
    -keyout "$WORK_DIR/key.pem" \
    -out "$WORK_DIR/certificate.pem" 2>/dev/null

# macOS only imports PKCS#12 files that use the older SHA1/3DES algorithms.
"$OPENSSL" pkcs12 -export \
    -inkey "$WORK_DIR/key.pem" \
    -in "$WORK_DIR/certificate.pem" \
    -name "$CERTIFICATE_NAME" \
    -keypbe PBE-SHA1-3DES -certpbe PBE-SHA1-3DES -macalg sha1 \
    -passout "pass:$EXPORT_PASSWORD" \
    -out "$WORK_DIR/identity.p12"

echo "==> Importing into your login Keychain"
# -T lets codesign use the private key without asking every time.
security import "$WORK_DIR/identity.p12" \
    -k "$KEYCHAIN" \
    -P "$EXPORT_PASSWORD" \
    -T /usr/bin/codesign >/dev/null

echo "==> Trusting it for code signing (macOS asks for your login password)"
security add-trusted-cert -r trustRoot -p codeSign -k "$KEYCHAIN" "$WORK_DIR/certificate.pem"

if ! security find-identity -v -p codesigning | grep -q "\"$CERTIFICATE_NAME\""; then
    echo "ERROR: the certificate was imported but is not valid for code signing." >&2
    exit 1
fi
echo
echo "Done. scripts/build-release.sh now signs with \"$CERTIFICATE_NAME\"."
