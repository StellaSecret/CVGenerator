#!/usr/bin/env bash
set -eo pipefail

TMP=$(mktemp /tmp/verify-wasm.XXXXXXXX)
trap 'rm -f "$TMP"' EXIT

echo "Downloading deployed WASM..."
JS_URL=$(curl -sL "https://stellasecret.github.io/CVGenerator/" \
  | grep -oE 'assets/cv-generator-[a-z0-9]+\.js' | head -1 || true)

if [ -z "$JS_URL" ]; then
  echo "FAIL: Could not find JS URL in index page"
  exit 1
fi

echo "Found JS: $JS_URL"
JS_FULL="https://stellasecret.github.io/CVGenerator/$JS_URL"

WASM_PATH=$(curl -sL "$JS_FULL" | grep -oE 'cv-generator_bg-[a-z0-9]+\.wasm' | head -1 || true)

if [ -z "$WASM_PATH" ]; then
  echo "FAIL: Could not find WASM path in JS file"
  exit 1
fi

FULL_URL="https://stellasecret.github.io/CVGenerator/assets/$WASM_PATH"
echo "Fetching $FULL_URL"
curl -sL "$FULL_URL" -o "$TMP"
SIZE=$(wc -c < "$TMP")
echo "WASM size: $SIZE bytes"

# Extract all strings to a text file to avoid pipefail issues with binary
STRINGS_FILE=$(mktemp /tmp/verify-strings.XXXXXXXX)
trap 'rm -f "$TMP" "$STRINGS_FILE"' EXIT
strings "$TMP" > "$STRINGS_FILE"

PASS=0
FAIL=0

check() {
  local label="$1"
  local pattern="$2"
  if grep -q "$pattern" "$STRINGS_FILE"; then
    echo "  OK   $label"
    PASS=$((PASS + 1))
  else
    echo "  MISS $label (pattern: $pattern)"
    FAIL=$((FAIL + 1))
  fi
}

echo ""
echo "Checking for diagnostic strings (must be present if WASM was rebuilt):"
check "RSX-TEST div"            "RSX-TEST"
check "RSX-END div"             "RSX-END"
check "JS-INJECTED div"         "JS-INJECTED"
check "NavLayout use_effect"    "NavLayout use_effect"
check "console.log running"     "running"
check "found header"            "found header"

echo ""
echo "Checking for existing content (must be present):"
check "CSS .nav-toggles"        ".nav-toggles"
check "Translation Accueil"     "Accueil"
check "Route /sync"             "/sync"

echo ""
echo "Result: $PASS passed, $FAIL failed"
if [ "$FAIL" -gt 0 ]; then
  echo "VERDICT: WASM binary may be stale"
  exit 1
else
  echo "VERDICT: WASM binary contains expected strings"
fi
