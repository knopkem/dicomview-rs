#!/usr/bin/env bash
set -euo pipefail

# publish.sh — Build and publish dicomview-rs to crates.io and npm.
#
# Usage:
#   ./publish.sh [--dry-run] [--skip-crates] [--skip-npm]
#
# Prerequisites:
#   - Rust toolchain with wasm32-unknown-unknown target
#   - wasm-pack (cargo install wasm-pack)
#   - Node.js + npm
#   - Logged in to crates.io (cargo login) and npm (npm login)

DRY_RUN=false
SKIP_CRATES=false
SKIP_NPM=false

for arg in "$@"; do
  case "$arg" in
    --dry-run)    DRY_RUN=true ;;
    --skip-crates) SKIP_CRATES=true ;;
    --skip-npm)   SKIP_NPM=true ;;
    -h|--help)
      echo "Usage: ./publish.sh [--dry-run] [--skip-crates] [--skip-npm]"
      exit 0
      ;;
    *)
      echo "Unknown option: $arg" >&2
      exit 1
      ;;
  esac
done

ROOT_DIR="$(cd "$(dirname "$0")" && pwd)"
JS_DIR="$ROOT_DIR/js"

# Read versions
CARGO_VERSION=$(grep '^version' "$ROOT_DIR/Cargo.toml" | head -1 | sed 's/.*"\(.*\)"/\1/')
NPM_VERSION=$(node -p "require('$JS_DIR/package.json').version")

echo "==> dicomview-rs publish pipeline"
echo "    Cargo workspace version: $CARGO_VERSION"
echo "    npm package version:     $NPM_VERSION"
echo ""

if [ "$CARGO_VERSION" != "$NPM_VERSION" ]; then
  echo "ERROR: Version mismatch between Cargo.toml ($CARGO_VERSION) and package.json ($NPM_VERSION)" >&2
  exit 1
fi

VERSION="$CARGO_VERSION"
echo "==> Publishing version $VERSION"
if $DRY_RUN; then
  echo "    (dry-run mode — nothing will be published)"
fi
echo ""

# Step 1: Run Rust tests
echo "==> Step 1/6: Running Rust tests..."
cargo test --workspace --quiet
echo "    ✓ All Rust tests passed"
echo ""

# Step 2: Check WASM target compiles
echo "==> Step 2/6: Checking wasm32 target..."
cargo check --workspace --target wasm32-unknown-unknown --quiet
echo "    ✓ wasm32-unknown-unknown check passed"
echo ""

# Step 3: Build WASM with wasm-pack
echo "==> Step 3/6: Building WASM binary..."
wasm-pack build "$ROOT_DIR/crates/dicomview-wasm" \
  --target web \
  --out-dir "$JS_DIR/wasm" \
  --out-name dicomview_wasm
rm -f "$JS_DIR/wasm/.gitignore" "$JS_DIR/wasm/package.json"
echo "    ✓ WASM build complete"
echo ""

# Step 4: Build TypeScript
echo "==> Step 4/6: Building TypeScript..."
cd "$JS_DIR"
npm install --quiet 2>/dev/null
npx tsc -p tsconfig.json
echo "    ✓ TypeScript build complete"
echo ""

# Step 5: Publish to crates.io
if ! $SKIP_CRATES; then
  echo "==> Step 5/6: Publishing to crates.io..."
  cd "$ROOT_DIR"
  if $DRY_RUN; then
    cargo publish -p dicomview-core --dry-run --quiet 2>/dev/null || true
    cargo publish -p dicomview-gpu --dry-run --quiet 2>/dev/null || true
    echo "    ✓ crates.io dry-run complete"
  else
    cargo publish -p dicomview-core --quiet
    echo "    Waiting for crates.io index to update..."
    sleep 15
    cargo publish -p dicomview-gpu --quiet
    sleep 15
    cargo publish -p dicomview-wasm --quiet
    echo "    ✓ Published to crates.io"
  fi
else
  echo "==> Step 5/6: Skipping crates.io (--skip-crates)"
fi
echo ""

# Step 6: Publish to npm
if ! $SKIP_NPM; then
  echo "==> Step 6/6: Publishing to npm..."
  cd "$JS_DIR"
  if $DRY_RUN; then
    npm pack --dry-run
    echo "    ✓ npm dry-run complete"
  else
    npm publish --access public
    echo "    ✓ Published @knopkem/dicomview@$VERSION to npm"
  fi
else
  echo "==> Step 6/6: Skipping npm (--skip-npm)"
fi
echo ""

echo "==> Done! Published dicomview-rs v$VERSION"
