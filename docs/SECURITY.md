# Security and operational boundaries

virtualvmweb is a single-host administrator interface, not a multi-tenant isolation boundary. The service account needs libvirt management access. A compromised web service can therefore control guests and attached host resources; Rust memory safety does not eliminate authorization or FFI risks.

- Authentication uses Argon2id, OS-random 256-bit session IDs, HttpOnly/SameSite=Strict cookies and an eight-hour absolute lifetime. HTTPS origins set Secure cookies. Restart revokes every session. At most sixteen sessions exist.
- All mutations require an exact configured Origin and a server-side CSRF token. All requests require the configured Host. Untrusted forwarded headers are ignored. Password checks are limited to five/minute globally and two concurrent checks; an attacker can temporarily deny login, which is a deliberate single-account tradeoff.
- Console upgrades require authentication and an exact Origin. The UUID selects a VNC graphics device through libvirt's `open_graphics`; no client-supplied destinations or public VNC listener are used. Four consoles maximum; logout/expiry closes them within fifteen seconds plus a bounded in-flight write. Clipboard integration is not provided.
- Libvirt calls run on blocking workers with serialized access and an eight-request admission limit. There is no shell execution. Slow daemon calls can still delay subsequent operations: a request timeout cannot safely cancel a mutation already handed to libvirt.
- ISO selection canonicalizes a single filename under an administrator-controlled root. Keep that directory and its parents non-writable by untrusted accounts. XML uses escaped scalar values and structured edits; raw XML editing is not exposed.
- New volume names contain a reserved prefix and UUID. Failed guest definition triggers volume rollback; failed cleanup is explicitly reported. Guests are left stopped until the administrator starts them.
- Removing a guest never removes its disks. A separate typed-confirmation flow deletes only reserved-name volumes after scanning active/inactive guest references, named pool/volume sources and backing chains in active pools. Saved states, snapshots, inactive pools and ambiguous relative backing paths block deletion. Names are an eligibility convention, not a cryptographic proof of ownership.

Reference checks cannot see arbitrary disk-image chains outside libvirt's managed pools, or make an atomic lock against other administrators/tools. **Do not delete application disks used outside this libvirt inventory; do not mutate storage concurrently through other tools.** Retain backups. Do not rename unrelated volumes to the application's reserved namespace.

Lifecycle audit events and detailed infrastructure errors go to the service log; secrets, password hashes and raw XML are not returned in normal views. Logs must be protected and rotated. `/healthz` is liveness only. The browser marks polling failures as stale, and missing guest metrics are shown as unavailable rather than zero.

