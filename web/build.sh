#!/usr/bin/env bash
# Build the web frontend into web/pkg/. Usage: web/build.sh [--dev]
# Then serve this directory with any static file server, e.g.
#   python3 -m http.server -d web 8080
set -euo pipefail
cd "$(dirname "$0")/.."

export PATH="${CARGO_HOME:-$HOME/.cargo}/bin:$PATH"

profile=release flag=--release
if [[ ${1:-} == --dev ]]; then profile=debug flag=; fi

# The CLI must be the same version as the wasm-bindgen crate in Cargo.lock.
want=$(cargo metadata --format-version 1 --locked 2>/dev/null \
    | grep -o '"name":"wasm-bindgen","version":"[^"]*"' | head -1 | cut -d'"' -f8)
have=$(wasm-bindgen --version 2>/dev/null | awk '{print $2}' || true)
if [[ -z $want ]]; then
    echo "cannot determine the wasm-bindgen version from Cargo.lock" >&2
    exit 1
fi
if [[ $have != "$want" ]]; then
    echo "installing wasm-bindgen-cli $want (found: ${have:-none})"
    cargo install wasm-bindgen-cli --version "$want" --locked
fi
if command -v rustup >/dev/null 2>&1; then
    rustup target list --installed | grep -q wasm32-unknown-unknown || rustup target add wasm32-unknown-unknown
fi

cargo build -p pd-web --target wasm32-unknown-unknown $flag
wasm-bindgen --target web --no-typescript --out-dir web/pkg \
    "target/wasm32-unknown-unknown/$profile/pd_web.wasm"
ls -l web/pkg
echo "built. serve with: python3 -m http.server -d web 8080"
