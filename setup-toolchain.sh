#!/usr/bin/env bash
set -euo pipefail

TOOLCHAIN="inkpaper-esp"
VERSION="1.95.0.0"

if ! command -v espup >/dev/null 2>&1; then
    echo "espup is not installed."
    echo "Install it with:"
    echo "  cargo install espup --locked"
    exit 1
fi

echo "Installing InkPaper ESP toolchain ${VERSION}..."

espup install \
    --name "${TOOLCHAIN}" \
    --toolchain-version "${VERSION}" \
    --targets esp32s3

echo
echo "Installed:"
rustc +"${TOOLCHAIN}" --version
cargo +"${TOOLCHAIN}" --version
