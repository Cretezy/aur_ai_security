#!/usr/bin/env bash

set -euo pipefail

if ! command -v cargo >/dev/null 2>&1; then
    curl --proto '=https' --tlsv1.2 --fail --silent --show-error \
        https://sh.rustup.rs | sh -s -- \
        -y \
        --profile minimal \
        --default-toolchain stable \
        --target wasm32-unknown-unknown
    export PATH="${CARGO_HOME:-${HOME}/.cargo}/bin:${PATH}"
fi

rustup target add wasm32-unknown-unknown
cargo install -q "worker-build@^0.8"
cargo install -q topcoat-cli --locked

topcoat asset bundle \
    --release \
    --package aur_security_web \
    --bin aur_security_web \
    --out web/static/_topcoat/assets
worker-build --release --panic-unwind web
