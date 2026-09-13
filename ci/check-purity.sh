#!/bin/sh
# The purity gates for skyspace-core and skyspace-parse (01-crates.md).
# Run from the workspace root. Exits non-zero on the first failure.
set -eu

fail() { echo "purity: $1" >&2; exit 1; }

# 1. Frozen dependency tree: a new dependency forces an edit to ci/core-deps.txt.
cargo tree -p skyspace-core --edges normal --prefix none | sed 's/ (\*)$//' | sort -u \
  | diff -u ci/core-deps.txt - || fail "skyspace-core dependency tree changed; update ci/core-deps.txt on purpose"

# 2. No async in core's own code.
if grep -rnE '^[[:space:]]*(pub )?async fn|\.await' crates/skyspace-core/src; then
  fail "async found in skyspace-core"
fi

# 3. Core builds for the browser. A tripwire, not a proof.
cargo check -p skyspace-core --target wasm32-unknown-unknown

# 4. The same freeze for skyspace-parse, because `skyspace replay` only works while it is pure.
cargo tree -p skyspace-parse --edges normal --prefix none | sed 's/ (\*)$//' | sort -u \
  | diff -u ci/parse-deps.txt - || fail "skyspace-parse dependency tree changed; update ci/parse-deps.txt on purpose"

# 5. Clock and filesystem use inside core is caught by crates/skyspace-core/clippy.toml.
cargo clippy -p skyspace-core --all-targets -- -D warnings

echo "purity: ok"
