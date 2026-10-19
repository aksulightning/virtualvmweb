# virtualvmweb

A small, server-rendered Rust web interface for **Alpine Linux 3.23 x86_64** and local libvirt/QEMU/KVM. The interface is in Finnish. Inspired by virt-manager's familiar guest-management workflow; independently implemented with its own interface.

## Included in 0.1

- Host overview, searchable guest inventory and five-second status updates.
- Start, graceful shutdown, reboot, pause/resume and autostart.
- Offline CPU/RAM/media editing; advanced CPU/NUMA layouts are explicitly refused.
- Reviewed creation from a host ISO, an active directory pool and an existing network.
- noVNC browser console through an authenticated libvirt Unix socket; newly created guests have no VNC network listener.
- Pool/volume/network inventory; separate, confirmed removal of guest definitions and detached application disks.
- One administrator, Argon2id passwords, bounded sessions, CSRF/origin checks and action logging.

Guest configuration belongs to libvirt. There is no application database or Node runtime. The Rust executable uses Alpine's shared libvirt library; noVNC files are served locally. New guests use x86_64 Q35/BIOS, virtio disk/network and VGA. Multi-host management, snapshots, migration, PCI passthrough and user roles are outside this release.

## Build and install

For the additional Debian 13/systemd installation path, see [Debian compatibility instructions](docs/DEBIAN.md). It requires a native Debian build; Alpine remains the primary verified target.

Enable Alpine's `community` repository, then:

```sh
apk add cargo rust build-base pkgconf libvirt-dev git
export RUSTFLAGS='-C target-feature=-crt-static'
sh scripts/fetch-novnc.sh
cargo build --release --locked
sh scripts/package.sh
```

The install tree and archive are under `dist/`. Follow [the Alpine installation guide](docs/INSTALL.md) for runtime packages, the dedicated account, libvirt authorization, ISO ownership, OpenRC and HTTPS. Configuration is in `/etc/virtualvmweb/config.toml`; the service defaults to `127.0.0.1:8080`. Generate a hash with `virtualvmweb --hash-password` using standard input, not a command-line password.

## Verification and boundaries

GitHub Actions runs formatting, documentation checks, Clippy, Rust tests and release HTTP smoke checks inside Alpine. [Testing](docs/TESTING.md) distinguishes the libvirt test driver from real QEMU/KVM and documents the host acceptance test. CI artifacts contain the release archive and measured RSS/HTTP timings; these measurements do not include guest RAM.

Use this application for a trusted host administrator: libvirt management rights are powerful. Do not concurrently attach/delete disks with another manager during destructive operations. Read [the security model](docs/SECURITY.md) before exposing the service.

The repository's existing GPLv3 license is retained. noVNC and bundled dependencies keep their original notices; see [research and third-party records](RESEARCH.md).

