# ~/.GH/Qompass/Rust/qompass-hold/scripts/xbuild.sh
# -------------------------------------------------
# Copyright (C) 2025 Qompass AI, All rights reserved

#!/bin/bash
set -euxo pipefail

targets=(
    x86_64-unknown-linux-gnu
    aarch64-unknown-linux-gnu
    x86_64-pc-windows-msvc
    x86_64-apple-darwin
    aarch64-apple-darwin
)

for target in "${targets[@]}"; do
    cargo zigbuild --release --target "$target"
done
