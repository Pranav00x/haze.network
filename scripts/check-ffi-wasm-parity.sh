#!/bin/sh
# Fails if src/wasm.rs (the web wallet's browser FFI surface) and
# src/ffi.rs (the mobile UniFFI surface) have drifted - i.e. either one
# exports a top-level pub fn the other doesn't. Run this after adding any
# new function to either file. See the commit that introduced this script
# for the 16-function gap it would have caught (marketplace mint/list/
# transfer, collection launches, allowlist signing, and
# sign_identity_message - missing from ffi.rs for an unknown but real
# stretch of time before anyone noticed).
set -e
cd "$(dirname "$0")/.."

# plan_send (wasm.rs) / plan_send_ffi (ffi.rs) are the same function under a
# deliberately different name on the mobile side - not a gap, normalize it
# away before comparing.
WASM_FNS=$(grep -oE '^pub fn [a-z_0-9]+' src/wasm.rs | sed 's/pub fn //' | sort)
FFI_FNS=$(grep -oE '^pub fn [a-z_0-9]+' src/ffi.rs | sed 's/pub fn //' | sed 's/^plan_send_ffi$/plan_send/' | sort)

MISSING_FROM_FFI=$(comm -23 <(echo "$WASM_FNS") <(echo "$FFI_FNS"))
MISSING_FROM_WASM=$(comm -13 <(echo "$WASM_FNS") <(echo "$FFI_FNS"))

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

COUNT=$(echo "$WASM_FNS" | wc -l)
echo "FFI/wasm parity check passed - $COUNT functions match on both sides."
