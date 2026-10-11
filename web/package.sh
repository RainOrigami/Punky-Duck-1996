#!/usr/bin/env bash
# Pack the built page (web/build.sh first) into a zip for a static file host.
# Usage: web/package.sh <out.zip>
# Every "?v=dev" in the page is replaced with a hash of the build, so browsers
# and hosts cannot mix files of two uploads.
set -euo pipefail
out=$(realpath -m "${1:?usage: web/package.sh <out.zip>}")
cd "$(dirname "$0")"
files=(index.html style.css main.js pkg/pd_web.js pkg/pd_web_bg.wasm)
stage=$(mktemp -d)
trap 'rm -rf "$stage"' EXIT
mkdir "$stage/pkg"
for f in "${files[@]}"; do cp "$f" "$stage/$f"; done
v=$(cat "${files[@]}" | sha256sum | cut -c1-10)
sed -i "s/?v=dev/?v=$v/g" "$stage/index.html" "$stage/main.js"
mkdir -p "$(dirname "$out")"
rm -f "$out"
(cd "$stage" && zip -q -9 -X "$out" "${files[@]}")
echo "$out (build $v)"
