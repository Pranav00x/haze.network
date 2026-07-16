#!/bin/sh
# Fails if src/wasm.rs (the web wallet's browser FFI surface) and
# src/ffi.rs (the mobile UniFFI surface) have drifted - i.e. either one
# exports a top-level pub fn the other doesn't. Run this after adding any
# new function to either file. See the commit that introduced this script
# for the 16-function gap it would have caught (marketplace mint/list/
# transfer, collection launches, allowlist signing, and
# sign_identity_message - missing from ffi.rs for an unknown but real
# stretch of time before anyone noticed).
#
# Deliberately POSIX sh, not bash - no process substitution (<(...)), since
# CI runs this via `sh` and Ubuntu's /bin/sh is dash, not bash. Uses real
# temp files instead.
set -e
cd "$(dirname "$0")/.."

WASM_TMP=$(mktemp)
FFI_TMP=$(mktemp)
trap 'rm -f "$WASM_TMP" "$FFI_TMP"' EXIT

grep -oE '^pub fn [a-z_0-9]+' src/wasm.rs | sed 's/pub fn //' | sort > "$WASM_TMP"
# plan_send (wasm.rs) / plan_send_ffi (ffi.rs) are the same function under a
# deliberately different name on the mobile side - not a gap, normalize it
# away before comparing.
grep -oE '^pub fn [a-z_0-9]+' src/ffi.rs | sed 's/pub fn //' | sed 's/^plan_send_ffi$/plan_send/' | sort > "$FFI_TMP"

MISSING_FROM_FFI=$(comm -23 "$WASM_TMP" "$FFI_TMP")
MISSING_FROM_WASM=$(comm -13 "$WASM_TMP" "$FFI_TMP")

if [ -n "$MISSING_FROM_FFI" ] || [ -n "$MISSING_FROM_WASM" ]; then
  echo "FFI/wasm parity check failed:"
  if [ -n "$MISSING_FROM_FFI" ]; then
    echo "  in src/wasm.rs but missing from src/ffi.rs:"
    echo "$MISSING_FROM_FFI" | sed 's/^/    - /'
  fi
  if [ -n "$MISSING_FROM_WASM" ]; then
    echo "  in src/ffi.rs but missing from src/wasm.rs:"
    echo "$MISSING_FROM_WASM" | sed 's/^/    - /'
  fi
  exit 1
fi

COUNT=$(wc -l < "$WASM_TMP")
echo "FFI/wasm parity check passed - $COUNT functions match on both sides."
