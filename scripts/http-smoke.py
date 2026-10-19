#!/usr/bin/env python3
"""Run the release binary against libvirt's test driver and record HTTP/RSS measurements."""
import http.cookiejar
import json
import re
import statistics
import subprocess
import tempfile
import time
import urllib.error
import urllib.parse
import urllib.request
from pathlib import Path


def run():
    """Start an isolated test instance, exercise authenticated pages and always stop it."""
    root = Path.cwd()
    binary = root / 'target/release/virtualvmweb'
    password = 'ci-only-randomized-' + __import__('secrets').token_hex(16)
    password_hash = subprocess.check_output([binary, '--hash-password'], input=password, text=True).strip()
    origin = 'http://127.0.0.1:18080'
    jar = http.cookiejar.CookieJar()
    client = urllib.request.build_opener(urllib.request.HTTPCookieProcessor(jar))
    with tempfile.TemporaryDirectory(prefix='virtualvmweb-ci-') as temporary:
        config = Path(temporary) / 'config.toml'
        config.write_text(f'''listen = "127.0.0.1:18080"
public_origin = "{origin}"
libvirt_uri = "test:///default"
admin_user = "admin"
password_hash = "{password_hash}"
iso_dir = "{temporary}"
novnc_dir = "{root / 'vendor/novnc'}"
''')
        with (root / 'dist/http-smoke.log').open('w') as log:
            process = subprocess.Popen([binary, '--config', config], stdout=log, stderr=log)
            try:
                for _ in range(100):
                    try:
                        with client.open(origin + '/healthz', timeout=1) as response:
                            assert response.read() == b'ok\n'
                        break
                    except (urllib.error.URLError, ConnectionError):
                        if process.poll() is not None:
                            raise RuntimeError('Server exited before becoming ready')
                        time.sleep(.1)
                else:
                    raise RuntimeError('Server did not become ready')
                previews = root / 'dist/previews'
                previews.mkdir(exist_ok=True)
                with client.open(origin + '/login') as response:
                    (previews / 'login.html').write_text(response.read().decode())
                body = urllib.parse.urlencode({'username': 'admin', 'password': password}).encode()
                req = urllib.request.Request(origin + '/login', data=body, headers={'Origin': origin})
                with client.open(req) as response:
                    text = response.read().decode()
                    assert 'PALVELIN' in text
                    assert response.headers['X-Content-Type-Options'] == 'nosniff'
                csrf = re.search(r'name="csrf" value="([a-f0-9]+)"', text).group(1)
                (previews / 'dashboard.html').write_text(text.replace(csrf, 'preview-only'))
                guest_id = re.search(r'/machines/([a-f0-9-]{36})', text).group(1)
                with client.open(origin + '/machines/' + guest_id) as response:
                    (previews / 'guest.html').write_text(response.read().decode().replace(csrf, 'preview-only'))
                for path in ['/machines', '/storage', '/networks', '/settings', '/machines/new', '/novnc/core/rfb.js']:
                    with client.open(origin + path) as response:
                        assert response.status == 200
                        if not path.startswith('/novnc'):
                            (previews / (path.strip('/').replace('/', '-') + '.html')).write_text(response.read().decode().replace(csrf, 'preview-only'))
                durations = []
                for _ in range(20):
                    start = time.perf_counter()
                    with client.open(origin + '/fragments/machines') as response:
                        assert response.status == 200
                        response.read()
                    durations.append((time.perf_counter() - start) * 1000)
                status = Path(f'/proc/{process.pid}/status').read_text()
                rss = int(re.search(r'^VmRSS:\s+(\d+)', status, re.M).group(1))
                report = {'environment': 'Alpine 3.23 x86_64; libvirt test driver; release build; 1 guest', 'rss_kib': rss, 'requests': len(durations), 'median_ms': round(statistics.median(durations), 2), 'max_ms': round(max(durations), 2), 'binary_bytes': binary.stat().st_size}
                (root / 'dist/measurements.json').write_text(json.dumps(report, indent=2) + '\n')
                print(json.dumps(report))
                body = urllib.parse.urlencode({'csrf': csrf}).encode()
                with client.open(urllib.request.Request(origin + '/logout', data=body, headers={'Origin': origin})) as response:
                    assert response.url.endswith('/login')
                print('Release HTTP smoke test passed.')
            finally:
                process.terminate()
                try:
                    process.wait(timeout=10)
                except subprocess.TimeoutExpired:
                    process.kill()
                    process.wait()


if __name__ == '__main__':
    run()

