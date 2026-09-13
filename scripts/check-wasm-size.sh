#!/bin/sh
# Fails when the shipped engine grows past `limit` bytes. The limit is the
# first real build (666,637 bytes, 12 Sep 2026) plus 25%, rounded up to a
# KiB; every later increase is a written decision.
set -eu

file="web/src/engine/generated/skyspace_wasm_bg.wasm"
limit=833536

if [ ! -f "$file" ]; then
    echo "check-wasm-size: $file is missing; run 'wasm-pack build crates/skyspace-wasm --target web --release --out-dir ../../web/src/engine/generated' first" >&2
    exit 1
fi

size=$(wc -c < "$file" | tr -d ' ')
echo "check-wasm-size: $file is $size bytes (limit $limit)"
if [ "$size" -gt "$limit" ]; then
    echo "check-wasm-size: over the limit by $((size - limit)) bytes" >&2
    exit 1
fi
