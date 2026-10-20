# Alpine Linux 3.23 x86_64 installation

Run installation commands as the host administrator. Enable the Alpine `community` repository. This guide assumes an OpenRC host with hardware virtualization enabled; use a real host/VM for production, not the CI container.

## Runtime and account

```sh
apk add libvirt-daemon libvirt-daemon-openrc libvirt-qemu libvirt-client \
  qemu-system-x86_64 qemu-img qemu-modules openrc polkit
addgroup -S virtualvmweb
adduser -S -D -H -s /sbin/nologin -G virtualvmweb virtualvmweb
rc-update add libvirtd default
rc-service libvirtd start
```

Inspect existing users/groups before creation if reinstalling. Verify `virsh -c qemu:///system list --all` and `/dev/kvm` on the host. Install the generated archive with `tar -xzf virtualvmweb-alpine3.23-x86_64.tar.gz -C /`; it contains `config.example.toml`, so an existing `config.toml` is preserved.

## Libvirt authorization

Keep libvirt's polkit authentication enabled. Install a root-owned `/etc/polkit-1/rules.d/50-virtualvmweb.rules`:

```javascript
// Grant local libvirt management only to this dedicated service account.
polkit.addRule(function(action, subject) {
    if (action.id == "org.libvirt.unix.manage" && subject.user == "virtualvmweb") {
        return polkit.Result.YES;
    }
});
```

Ensure D-Bus/polkit are running and the libvirt Unix socket is reachable by the account. Test with `su -s /bin/sh virtualvmweb -c 'virsh -c qemu:///system list --all'`. This grant gives broad virtualization management rights; the web application does not become a restricted tenant account. Do not make the libvirt socket world-writable with authentication disabled.

## Configuration and service

Copy the example to `/etc/virtualvmweb/config.toml`. Set its owner to `root:virtualvmweb`, mode `0640`, and the directory to `0750`. Generate the Argon2id hash without putting a plaintext password in shell history:

```sh
python3 -c 'import getpass, subprocess; subprocess.run(["/usr/bin/virtualvmweb", "--hash-password"], input=getpass.getpass("New administrator password: "), text=True, check=True)'
```

This optional helper requires `python3` only while setting the password. Copy the resulting hash into the quoted `password_hash` setting. Choose at least twelve bytes; no default credentials exist. Keep ISO files in the configured root-owned directory (default `/var/lib/libvirt/boot`), readable by both the service account and QEMU. Give neither browser users nor untrusted local accounts write access to the ISO directory. Configure an active libvirt directory pool and network using the host's normal libvirt tools before creating guests.

```sh
rc-update add virtualvmweb default
rc-service virtualvmweb start
```

Logs are in `/var/log/virtualvmweb/service.log`; arrange log rotation on the host. Restart the service after changing credentials/settings. Restarting invalidates sessions and closes consoles but does not shut down guests.

## HTTPS access

Keep `listen = "127.0.0.1:8080"`; set `public_origin = "https://vm.example.org"` to the exact browser origin, without trailing slash. Use a TLS reverse proxy that preserves `Host` and `Origin` and forwards WebSocket upgrades. An nginx location inside your existing TLS virtual host can use:

```nginx
location / {
    proxy_pass http://127.0.0.1:8080;
    proxy_http_version 1.1;
    proxy_set_header Host $http_host;
    proxy_set_header Upgrade $http_upgrade;
    proxy_set_header Connection "upgrade";
    proxy_read_timeout 3600s;
}
```

Restrict the TLS endpoint to your management network/VPN. There is no subpath deployment or forwarded-header authentication. For local evaluation use the exact default `http://127.0.0.1:8080` origin, including through an SSH local port forward.


## UEFI and forced power actions

Install `ovmf` (`apk add ovmf`) to enable UEFI for new x86_64 Q35 guests. The creation page enables UEFI only when libvirt advertises EFI autoselection for Q35/KVM; preview and final creation recheck this before allocating storage. Libvirt chooses firmware paths and creates per-guest NVRAM. Secure Boot is disabled. Existing guests are not converted between BIOS and UEFI.

Forced shutdown and forced restart require typing the guest name. Forced restart performs a hardware reset on a running guest; resume a paused guest first. Both can lose unsaved data. Removing a guest definition preserves its disks and UEFI NVRAM; leftover NVRAM must be retained for recovery or deliberately cleaned up by the host administrator.
