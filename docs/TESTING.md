# Verification

## Automated checks

GitHub Actions invokes `scripts/ci.sh` inside Alpine 3.23 x86_64. It checks Rust formatting, repository-required comments, Clippy warnings, unit/HTTP tests, actual libvirt test-driver lifecycle calls, JavaScript syntax, the release binary and installation packaging. `scripts/http-smoke.py` logs into the running release server, fetches pages/local noVNC assets, logs out, and writes RSS and twenty-request latency measurements to `dist/measurements.json`.

The test driver verifies libvirt integration but does **not** emulate QEMU, storage allocation, graphical consoles or guest operating systems. A second disposable privileged CI container runs real libvirt/QEMU: it tests lifecycle operations, a complete RFB initialization over the authenticated WebSocket, hostile-origin rejection, attached-disk deletion refusal and separate detached-disk removal. When the runner provides KVM, it also exercises wizard creation and offline editing; otherwise it records TCG and explicitly marks those KVM checks unverified. No guest OS installation is claimed.

Test-only credentials/configuration are generated in a temporary directory and are not included in release archives. Redacted HTML previews are saved for visual inspection. Any unrun checks must be reported as unverified.

## Recorded CI evidence

On 2026-10-19, [run 39279261962](https://github.com/aksulightning/virtualvmweb/actions/runs/39279261962) passed formatting, Clippy, all 12 Rust tests, the release HTTP smoke test and real QEMU/KVM integration. The latter exercised wizard creation, offline edits, lifecycle operations, the VNC handshake, logout revocation and disk deletion safeguards. Its separate Chromium screenshot step failed; this run is not an overall green CI result.

The one-guest libvirt test-driver measurement was 20,624 KiB RSS, a 3,787,760-byte release binary and 0.57 ms median / 0.89 ms maximum for twenty local HTTP requests. These are small synthetic smoke measurements, not production capacity or guest workload benchmarks. Each subsequent CI run publishes its own measurements and browser screenshots in the `alpine-build` artifact.

## Real-host acceptance (Alpine + KVM)

Use a disposable guest and empty test directory pool; never reuse production disks. Confirm the dedicated service account can manage `qemu:///system` and `/dev/kvm` is available. Place a bootable x86_64 ISO in the configured ISO directory and prepare an active network.

1. Log in over HTTPS. Create a guest from the wizard and confirm a stopped domain plus one reserved-name qcow2 volume exist.
2. Start it, open the browser console and complete an OS installation. Verify keyboard, pointer, scaling, full-screen and Ctrl-Alt-Del. Confirm no TCP VNC listener was added.
3. Pause/resume, reboot and gracefully shut down. Confirm unavailable guest actions are rejected server-side even from a stale form.
4. Edit CPU/RAM and eject the ISO while stopped; start the installed system again. Verify unrelated devices and UUID are unchanged.
5. Enable autostart, restart the host and confirm expected behavior. Restart only virtualvmweb and confirm guest continuity plus session revocation.
6. Confirm deleting an attached disk fails. Remove the guest definition, verify the disk remains, then separately delete the detached test disk. Repeat refusal checks with a backing chain or snapshot.
7. Open a console, log out in another tab and confirm access closes. Test missing/incorrect Origin and expired sessions.
8. Temporarily stop libvirtd and verify a clear outage/stale-state indication, then restore it and confirm reconnection.

Record the Alpine/libvirt/QEMU/browser versions and outcomes. Full guest OS boot and installation, OpenRC reboot ordering and HTTPS proxy behavior remain unverified until this host test is performed; CI success alone must never be presented as proof of them.

