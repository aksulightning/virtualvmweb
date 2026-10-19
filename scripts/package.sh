#!/bin/sh
# Assemble a relocatable Alpine installation tree without changing the host.
set -eu
stage=dist/virtualvmweb
mkdir -p "$stage/usr/bin" "$stage/usr/share/virtualvmweb" "$stage/etc/init.d" "$stage/etc/virtualvmweb" "$stage/usr/share/doc/virtualvmweb"
install -m755 target/release/virtualvmweb "$stage/usr/bin/virtualvmweb"
cp -R vendor/novnc "$stage/usr/share/virtualvmweb/"
install -m755 packaging/virtualvmweb.initd "$stage/etc/init.d/virtualvmweb"
install -m640 packaging/config.example.toml "$stage/etc/virtualvmweb/config.example.toml"
cp README.md RESEARCH.md LICENSE "$stage/usr/share/doc/virtualvmweb/"
cp -R docs "$stage/usr/share/doc/virtualvmweb/"
tar -czf dist/virtualvmweb-alpine3.23-x86_64.tar.gz -C "$stage" .
sha256sum dist/virtualvmweb-alpine3.23-x86_64.tar.gz > dist/SHA256SUMS

