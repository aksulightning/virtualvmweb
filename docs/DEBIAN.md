# Debian 13 (trixie), amd64

This small compatibility addition supplies a systemd unit and native build instructions. Alpine remains the primary CI target. Debian compilation and real-host operation are not yet verified; this is not a `.deb` package. Do not install the Alpine/musl archive on Debian/glibc.

## Build on Debian

Install host dependencies as root:

```sh
apt-get update
apt-get install build-essential pkg-config libvirt-dev git ca-certificates \
  libvirt-daemon-system libvirt-clients qemu-system-x86 qemu-utils polkitd
```

Use Rust/Cargo **1.91 or newer**, for example through [rustup](https://rustup.rs/). Debian 13's base Rust 1.85 package is too old for this project. In the repository, as an ordinary build user:

```sh
unset RUSTFLAGS
sh scripts/fetch-novnc.sh
cargo build --release --locked
```

## Install and configure

Run from the repository as root. Create the account only on first installation; these commands preserve an existing `config.toml`:

```sh
adduser --system --group --no-create-home virtualvmweb
install -m755 target/release/virtualvmweb /usr/bin/virtualvmweb
install -d -m755 /usr/share/virtualvmweb
cp -R vendor/novnc /usr/share/virtualvmweb/
install -d -o root -g virtualvmweb -m750 /etc/virtualvmweb
install -m640 -o root -g virtualvmweb packaging/config.example.toml /etc/virtualvmweb/config.example.toml
if [ ! -e /etc/virtualvmweb/config.toml ]; then
  install -m640 -o root -g virtualvmweb packaging/config.example.toml /etc/virtualvmweb/config.toml
fi
install -m644 packaging/virtualvmweb.service /etc/systemd/system/virtualvmweb.service
systemctl enable --now libvirtd.service
```

Follow the shared [libvirt authorization and configuration instructions](INSTALL.md#libvirt-authorization): install the dedicated-account polkit rule, generate an administrator password hash, configure the ISO directory and prepare an active directory pool/network. Verify `su -s /bin/sh virtualvmweb -c 'virsh -c qemu:///system list --all'` succeeds. Keep Debian's libvirt/QEMU AppArmor protections enabled and use host-approved ISO/storage locations.

Once the configuration and permissions are ready, use systemd instead of the Alpine OpenRC commands:

```sh
systemctl daemon-reload
systemctl enable --now virtualvmweb.service
journalctl -u virtualvmweb.service
```

Logs go to the journal. The shared HTTPS instructions and [real-host acceptance checklist](TESTING.md#real-host-acceptance-alpine--kvm) apply with Debian/systemd substituted for Alpine/OpenRC. Restart the service after changing configuration; sessions are invalidated and guests continue running.

For UEFI creation, install `ovmf` with `apt-get install ovmf`. Reload the creation page afterward; if UEFI remains unavailable, check `virsh -c qemu:///system domcapabilities --arch x86_64 --machine q35 --virttype kvm` for the OS firmware enum's `efi` value. The application uses libvirt firmware autoselection, without hard-coded distribution paths or Secure Boot. Guest definition removal preserves NVRAM as well as disks.
