#!/bin/sh
# Reproducible Alpine build checks; all dependencies stay inside the CI container.
set -eu
apk add --no-cache cargo rust rustfmt rust-clippy build-base pkgconf libvirt-dev libvirt-client libvirt-common-drivers git python3 nodejs
export RUSTFLAGS='-C target-feature=-crt-static'
mkdir -p dist
cargo fmt --all --check
python3 scripts/check-docs.py
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
node --check assets/app.js
node --check assets/console.js
node --check assets/audio.js
node scripts/audio-check.mjs
sh scripts/fetch-novnc.sh
cargo build --locked --release
sh scripts/package.sh
python3 scripts/http-smoke.py

