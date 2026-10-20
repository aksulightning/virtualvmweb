# Research and third-party record

Reviewed 2026-10-19 while implementing the first version. Official documentation and upstream source were used to check APIs; the application's Rust, HTML, CSS and integration JavaScript were written for this repository.

| Source actually reviewed | Finding and implementation impact |
| --- | --- |
| [Axum API](https://docs.rs/axum/latest/axum/) — upstream crate documentation | Tokio integration, request extractors, routing and WebSocket support. Used for the HTTP service. |
| [virt 0.4.3 API](https://docs.rs/virt/0.4.3/virt/) and [upstream domain source](https://github.com/libvirt/libvirt-rust/blob/v0.4.3/src/domain.rs) | Direct libvirt bindings; domain state/XML operations and graphics socket APIs. Isolated blocking calls and used `open_graphics` with a Unix socket pair. The upstream main-branch storage source was also inspected; compilation/tests are the compatibility check against the locked crate. |
| [Connect API](https://docs.rs/virt/latest/virt/connect/struct.Connect.html), [DomainInfo](https://docs.rs/virt/latest/virt/domain/struct.DomainInfo.html), [StoragePool](https://docs.rs/virt/latest/virt/storage_pool/struct.StoragePool.html) | Host/guest inventory, RAM units, cumulative CPU time and pool enumeration. Assigned memory is not labeled as guest RAM consumption. |
| [Alpine 3.23 libvirt APKBUILD](https://github.com/alpinelinux/aports/blob/3.23-stable/community/libvirt/APKBUILD) and [OpenRC script](https://github.com/alpinelinux/aports/blob/3.23-stable/community/libvirt/libvirt.initd) | Verified community package split, QEMU/common drivers and OpenRC subpackage. Alpine 3.23 x86_64 is the initial target, not a claim that it is the newest Alpine release. |
| [libvirt authentication](https://libvirt.org/auth.html), [domain XML](https://libvirt.org/formatdomain.html#graphical-framebuffers) and [graphics API](https://libvirt.org/html/libvirt-libvirt-domain.html#virDomainOpenGraphics) | Verified Unix socket/polkit access model and local graphics handoff. Production instructions keep polkit; disposable CI uses a root-only Unix socket. |
| [noVNC overview](https://novnc.com/info.html), [v1.7.0 tag](https://github.com/novnc/noVNC/releases/tag/v1.7.0) and GitHub release/tag objects; [RFB module imports](https://github.com/novnc/noVNC/blob/63107bd06d9e1f6136ff21aeda8cd62cbf0d433e/core/rfb.js) | Browser VNC client; local modules need a WebSocket transport. Tag resolves to commit `63107bd06d9e1f6136ff21aeda8cd62cbf0d433e`, pinned in the fetch script. |

## License and design decisions

### Native login form Origin regression — 2026-10-19

Reviewed the WHATWG [Fetch standard, appending Origin](https://fetch.spec.whatwg.org/#append-a-request-origin-header). For non-CORS POST requests, `no-referrer` serializes the Origin as `null`. Our response policy therefore conflicted with our strict Origin check on native HTML form submissions. Changed the policy to `same-origin`, retained strict rejection of missing/null/foreign origins, and added a Chromium form-submission regression using response headers captured from the actual release server. The previous HTTP smoke supplied Origin manually and the browser preview omitted production headers, so neither exposed this interaction.

### Debian compatibility follow-up — 2026-10-19

- Reviewed Debian's [trixie libvirt-daemon-system package](https://packages.debian.org/trixie/libvirt-daemon-system): metapackage for the local QEMU deployment. Used the Debian package names in the separate installation guide.
- Reviewed Debian's [trixie rustc package](https://packages.debian.org/trixie/rustc): base version 1.85.1 is below this project's Rust 1.91 requirement. Documented the need for a newer toolchain rather than lowering the requirement.
- Reviewed the official [rustup installer page](https://rustup.rs/): linked it as an optional source for a newer native toolchain; no remote installer was executed.
- Reviewed upstream [systemd.exec](https://www.freedesktop.org/software/systemd/man/latest/systemd.exec.html): dedicated User/Group, UMask and filesystem protection settings inform the new unit. No upstream code copied. These documentation checks do not establish Debian runtime compatibility; host acceptance remains unverified.

The repository already contained the GPLv3 license text; retained unchanged and declared GPL-3.0-only in Cargo metadata. virt/libvirt use LGPL terms; noVNC identifies MPL-2.0 and includes additional third-party notices. The packaging script preserves noVNC's license files and bundled vendor notices. Dependency resolution is recorded in Cargo.lock. No virt-manager code or artwork was copied.

The local `qemu:///system` scope, single administrator, conservative offline edits and separate disk deletion are project design decisions. Runtime libvirt, polkit and ISO filesystem permissions remain host configuration, requiring real-host verification. Alpine wiki KVM/OpenRC search results were consulted during planning, but a direct KVM page fetch returned 403; installation details were subsequently checked against the upstream Alpine package source rather than treating that failed fetch as reviewed documentation.
