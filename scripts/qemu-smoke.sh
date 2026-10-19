#!/bin/sh
# Disposable privileged CI container only: exercise real libvirt/QEMU without touching a user host.
set -eu
test "${VIRTUALVMWEB_DISPOSABLE_CI:-}" = yes
apk add --no-cache libvirt-qemu qemu-system-x86_64 qemu-img qemu-modules dbus py3-websocket-client
mkdir -p /run/libvirt /var/lib/libvirt/images/vvw-ci /tmp/vvw-isos
# Root-only socket authentication is the boundary in this disposable container.
# Production uses the dedicated account and polkit described in docs/INSTALL.md.
printf 'unix_sock_rw_perms = "0700"\nauth_unix_rw = "none"\n' > /tmp/vvw-libvirtd.conf
printf 'user = "root"\ngroup = "root"\nsecurity_driver = "none"\ncgroup_controllers = []\n' > /etc/libvirt/qemu.conf
virtlogd -d
virtlockd -d
libvirtd -d -f /tmp/vvw-libvirtd.conf
attempt=0
until virsh -c qemu:///system list >/dev/null 2>&1; do
    attempt=$((attempt + 1))
    test "$attempt" -lt 30
    sleep 1
done
virsh -c qemu:///system pool-define-as vvw-ci dir --target /var/lib/libvirt/images/vvw-ci
virsh -c qemu:///system pool-start vvw-ci
printf '<network><name>vvw-ci</name><bridge name="virbr-vvw"/></network>\n' > /tmp/vvw-network.xml
virsh -c qemu:///system net-define /tmp/vvw-network.xml
virsh -c qemu:///system net-start vvw-ci
truncate -s 1M /tmp/vvw-isos/fixture.iso
python3 scripts/qemu-smoke.py

