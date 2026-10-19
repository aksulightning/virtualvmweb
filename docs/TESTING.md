# Verification

## Automated checks

GitHub Actions invokes `scripts/ci.sh` inside Alpine 3.23 x86_64. It checks Rust formatting, repository-required comments, Clippy warnings, unit/HTTP tests, actual libvirt test-driver lifecycle calls, JavaScript syntax, the release binary and installation packaging. `scripts/http-smoke.py` logs into the running release server, fetches pages/local noVNC assets, logs out, and writes RSS and twenty-request latency measurements to `dist/measurements.json`.

The test driver verifies libvirt integration but does **not** emulate QEMU, storage allocation, graphical consoles or guest operating systems. Test-only credentials/configuration are generated in a temporary directory and are not included in release archives. Any unrun checks must be reported as unverified.

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

Record the Alpine/libvirt/QEMU/browser versions and outcomes. Real KVM boot, OS installation, OpenRC reboot ordering and HTTPS proxy behavior remain unverified until this host test is performed; CI test-driver success alone must never be presented as proof of them.

