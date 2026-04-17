#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DEMO_DIR="$ROOT_DIR/demo/web"
PKG_DIR="$DEMO_DIR/pkg"
TARGET_DIR="$ROOT_DIR/target/wasm-demo"
NIGHTLY_CARGO="$(rustup which --toolchain nightly cargo)"
NIGHTLY_RUSTC="$(rustup which --toolchain nightly rustc)"
WASM_BINDGEN_BIN="${WASM_BINDGEN_BIN:-$HOME/.cargo/bin/wasm-bindgen}"

if [[ ! -x "$WASM_BINDGEN_BIN" ]]; then
  echo "error: wasm-bindgen CLI not found at $WASM_BINDGEN_BIN" >&2
  echo "install it with: cargo install wasm-bindgen-cli --version 0.2.118" >&2
  exit 1
fi

mkdir -p "$PKG_DIR"

(
  export PATH="$(dirname "$NIGHTLY_CARGO"):$PATH"
  export RUSTC="$NIGHTLY_RUSTC"
  export CARGO_TARGET_DIR="$TARGET_DIR"

  "$NIGHTLY_CARGO" build \
    --release \
    -p xifty-wasm \
    --target wasm32-unknown-unknown
)

"$WASM_BINDGEN_BIN" \
  --target web \
  --out-dir "$PKG_DIR" \
  "$TARGET_DIR/wasm32-unknown-unknown/release/xifty_wasm.wasm"

echo "Built browser demo assets into $PKG_DIR"
