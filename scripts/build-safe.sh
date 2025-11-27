#!/usr/bin/env bash
# ~/.GH/pass-secret-service/scripts/build-safe.sh
# -----------------------------------------------
# Copyright (C) 2025 Qompass AI, All rights reserved

set -euo pipefail
IS_ROOT=0
if [[ "$EUID" -eq 0 ]]; then
    IS_ROOT=1
fi
TARGET="${1:-x86_64-unknown-linux-gnu}"
shift || true
CARGO_ARGS=("$@")
if command -v cargo-zigbuild >/dev/null 2>&1; then
    CARGO_CMD="cargo zigbuild"
else
    echo "⚠️ cargo-zigbuild not found; falling back to cargo"
    CARGO_CMD="cargo"
fi
export CARGO_INCREMENTAL=0
export SCCACHE_IDLE_TIMEOUT=1200
if [[ "$IS_ROOT" -eq 0 ]]; then
    export RUSTC_WRAPPER="${RUSTC_WRAPPER:-sccache}"

    if [[ -n "${SCCACHE_DIST_SERVER:-}" ]]; then
        echo "🔄 Using sccache-dist server: $SCCACHE_DIST_SERVER"
        export SCCACHE_START_SERVER=1
        export SCCACHE_DIST_IDLE_TIMEOUT=600
    fi
else
    echo "🚫 Running as root — disabling sccache to prevent permission issues"
    unset RUSTC_WRAPPER
fi
echo "🚀 Building with target: $TARGET"
exec $CARGO_CMD --release --target "$TARGET" "${CARGO_ARGS[@]}"
