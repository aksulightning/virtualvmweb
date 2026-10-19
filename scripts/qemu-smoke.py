#!/usr/bin/env python3
"""Exercise real QEMU/KVM creation, disk safety and the authenticated VNC WebSocket bridge."""
import http.cookiejar
import json
import re
import secrets
import struct
import subprocess
import tempfile
import time
import urllib.error
import urllib.parse
import urllib.request
from pathlib import Path
import websocket

ORIGIN = 'http://127.0.0.1:18081'
VIRSH = ['virsh', '-c', 'qemu:///system']


def run():
    """Run against disposable CI resources, leaving diagnostics and a truthful acceleration report."""
    binary = Path('target/release/virtualvmweb').resolve()
    password = secrets.token_urlsafe(24)
    password_hash = subprocess.check_output([binary, '--hash-password'], input=password, text=True).strip()
    jar = http.cookiejar.CookieJar()
    client = urllib.request.build_opener(urllib.request.HTTPCookieProcessor(jar))

    def fetch(path, fields=None, expected=200):
        """Send an authenticated form or GET and enforce the expected final HTTP status."""
        body = None if fields is None else urllib.parse.urlencode(fields).encode()
        try:
            response = client.open(urllib.request.Request(ORIGIN + path, data=body, headers={'Origin': ORIGIN}), timeout=15)
        except urllib.error.HTTPError as error:
            response = error
        with response:
            text = response.read().decode()
            assert response.status == expected, (path, response.status, text)
            return text, response.url

    with tempfile.TemporaryDirectory(prefix='vvw-qemu-') as temporary:
        config = Path(temporary) / 'config.toml'
        config.write_text(f'''listen = "127.0.0.1:18081"
public_origin = "{ORIGIN}"
libvirt_uri = "qemu:///system"
admin_user = "admin"
password_hash = "{password_hash}"
iso_dir = "/tmp/vvw-isos"
novnc_dir = "{Path('vendor/novnc').resolve()}"
''')
        with Path('dist/qemu-server.log').open('w') as log:
            process = subprocess.Popen([binary, '--config', config], stdout=log, stderr=log)
            try:
                for _ in range(50):
                    try:
                        fetch('/healthz')
                        break
                    except urllib.error.URLError:
                        time.sleep(.1)
                text, _ = fetch('/login', {'username': 'admin', 'password': password})
                csrf = re.search(r'name="csrf" value="([a-f0-9]+)"', text).group(1)
                fields = {'csrf': csrf, 'name': 'vvw-ci-guest', 'cpus': 1, 'memory_mib': 256, 'disk_gib': 1, 'pool': 'vvw-ci', 'network': 'vvw-ci', 'iso': 'fixture.iso'}
                fetch('/machines/preview', fields)
                assert not subprocess.check_output(VIRSH + ['vol-list', 'vvw-ci', '--name'], text=True).strip()
                capabilities = subprocess.check_output(VIRSH + ['capabilities'], text=True)
                acceleration = 'kvm' if "type='kvm'" in capabilities or 'type="kvm"' in capabilities else 'tcg'
                if acceleration == 'kvm':
                    fields['confirm'] = 'yes'
                    _, url = fetch('/machines', fields)
                    guest_id = url.split('/machines/')[1].split('?')[0]
                    volume = f'virtualvmweb-{guest_id}.qcow2'
                    fetch(f'/machines/{guest_id}/edit', {'csrf': csrf, 'cpus': 1, 'memory_mib': 320, 'iso': '', 'change_iso': 'true'})
                else:
                    # Test the console on TCG, explicitly without claiming the KVM wizard was verified.
                    guest_id = '99f116cd-8cd7-40d1-8c80-fc847250400b'
                    volume = f'virtualvmweb-{guest_id}.qcow2'
                    subprocess.run(VIRSH + ['vol-create-as', 'vvw-ci', volume, '1G', '--format', 'qcow2'], check=True)
                    guest_xml = Path(temporary) / 'guest.xml'
                    guest_xml.write_text(f"<domain type='qemu'><name>vvw-ci-guest</name><uuid>{guest_id}</uuid><memory unit='MiB'>256</memory><vcpu>1</vcpu><os><type arch='x86_64' machine='q35'>hvm</type></os><devices><disk type='file' device='disk'><driver name='qemu' type='qcow2'/><source file='/var/lib/libvirt/images/vvw-ci/{volume}'/><target dev='vda' bus='virtio'/></disk><graphics type='vnc'><listen type='none'/></graphics><video><model type='vga'/></video></devices></domain>")
                    subprocess.run(VIRSH + ['define', str(guest_xml)], check=True)
                delete = {'csrf': csrf, 'pool': 'vvw-ci', 'name': volume, 'confirmation': volume}
                fetch('/volumes/delete', delete, expected=409)
                fetch(f'/machines/{guest_id}/action', {'csrf': csrf, 'operation': 'start'})
                fetch(f'/machines/{guest_id}/edit', {'csrf': csrf, 'cpus': 1, 'memory_mib': 256, 'iso': ''}, expected=409)
                fetch(f'/machines/{guest_id}/console')
                cookie = '; '.join(f'{cookie.name}={cookie.value}' for cookie in jar)
                ws_url = ORIGIN.replace('http:', 'ws:') + f'/machines/{guest_id}/ws'
                try:
                    websocket.create_connection(ws_url, origin='https://untrusted.invalid', cookie=cookie, timeout=5)
                    raise AssertionError('Hostile console origin was accepted')
                except websocket.WebSocketBadStatusException as error:
                    assert error.status_code == 403
                ws = websocket.create_connection(ws_url, origin=ORIGIN, cookie=cookie, timeout=10)
                received = bytearray()

                def read_bytes(count):
                    """Read a fixed-size RFB field across arbitrary WebSocket frame boundaries."""
                    while len(received) < count:
                        chunk = ws.recv()
                        assert isinstance(chunk, bytes) and chunk, 'VNC stream closed prematurely'
                        received.extend(chunk)
                    data = bytes(received[:count])
                    del received[:count]
                    return data

                assert read_bytes(12).startswith(b'RFB 003.')
                ws.send_binary(b'RFB 003.008\n')
                security_count = read_bytes(1)[0]
                assert 1 in read_bytes(security_count)
                ws.send_binary(b'\x01')
                assert read_bytes(4) == b'\0\0\0\0'
                ws.send_binary(b'\x01')
                header = read_bytes(24)
                width, height = struct.unpack('!HH', header[:4])
                read_bytes(struct.unpack('!I', header[20:24])[0])
                assert width > 0 and height > 0
                ws.close()
                fetch(f'/machines/{guest_id}/action', {'csrf': csrf, 'operation': 'pause'})
                fetch(f'/machines/{guest_id}/action', {'csrf': csrf, 'operation': 'resume'})
                fetch(f'/machines/{guest_id}/action', {'csrf': csrf, 'operation': 'force-off', 'confirmation': 'vvw-ci-guest'})
                fetch(f'/machines/{guest_id}/action', {'csrf': csrf, 'operation': 'undefine', 'confirmation': 'vvw-ci-guest'})
                assert Path(f'/var/lib/libvirt/images/vvw-ci/{volume}').is_file()
                fetch('/volumes/delete', delete)
                assert not Path(f'/var/lib/libvirt/images/vvw-ci/{volume}').exists()
                report = {'acceleration': acceleration, 'wizard_creation_and_offline_edit': acceleration == 'kvm', 'real_qemu_lifecycle': True, 'vnc_handshake': True, 'screen': [width, height], 'attached_disk_refused': True, 'separate_disk_deletion': True, 'guest_os_installation': 'not tested'}
                Path('dist/qemu-measurements.json').write_text(json.dumps(report, indent=2) + '\n')
                print(json.dumps(report))
            finally:
                process.terminate()
                try:
                    process.wait(timeout=10)
                except subprocess.TimeoutExpired:
                    process.kill()
                    process.wait()


if __name__ == '__main__':
    run()
