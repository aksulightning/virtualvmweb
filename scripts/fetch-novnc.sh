#!/bin/sh
# Vendor an immutable upstream noVNC release, including its license notices.
set -eu
revision=63107bd06d9e1f6136ff21aeda8cd62cbf0d433e
destination=vendor/novnc
if [ -f "$destination/REVISION" ] && [ "$(cat "$destination/REVISION")" = "$revision" ] && [ -f "$destination/core/rfb.js" ]; then exit 0; fi
stage=$(mktemp -d)
trap 'rm -rf "$stage"' EXIT INT TERM
git -C "$stage" init -q
git -C "$stage" fetch -q --depth 1 https://github.com/novnc/noVNC.git "$revision"
git -C "$stage" checkout -q --detach FETCH_HEAD
test "$(git -C "$stage" rev-parse HEAD)" = "$revision"
mkdir -p "$destination"
cp -R "$stage/core" "$stage/vendor" "$destination/"
cp "$stage/LICENSE.txt" "$destination/"
if [ -d "$stage/docs/licenses" ]; then cp -R "$stage/docs/licenses" "$destination/"; fi
printf '%s\n' "$revision" > "$destination/REVISION"

