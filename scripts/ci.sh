#!/bin/sh
# Reproducible Alpine build checks; all dependencies stay inside the CI container.
set -eu
apk add --no-cache cargo rust rustfmt rust-clippy build-base pkgconf libvirt-dev libvirt-client libvirt-common-drivers git python3 nodejs
export RUSTFLAGS='-C target-feature=-crt-static'
mkdir -p dist
# The first bootstrap run exports the resolver result for committing Cargo.lock.
if [ ! -f Cargo.lock ]; then cargo generate-lockfile; fi
format_status=0
cargo fmt --all --check || format_status=$?
if [ "$format_status" -ne 0 ]; then cargo fmt --all; fi
git diff -- src > dist/formatted.patch
python3 scripts/check-docs.py
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
node --check assets/app.js
node --check assets/console.js
sh scripts/fetch-novnc.sh
cargo build --locked --release
sh scripts/package.sh
python3 scripts/http-smoke.py
# The formatting gate remains failing until formatted sources are committed.
exit "$format_status"

