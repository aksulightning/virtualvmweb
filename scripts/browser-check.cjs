// Inspect actual server-rendered, redacted HTML in Chromium without shipping browser tooling.
const http = require('node:http');
const fs = require('node:fs/promises');
const path = require('node:path');
const assert = require('node:assert/strict');
const { chromium } = require(process.env.VIRTUALVMWEB_PLAYWRIGHT);
let browserHeaders;

const routes = new Map([
  ['/', 'dashboard.html'], ['/login', 'login.html'], ['/machines', 'machines.html'],
  ['/storage', 'storage.html'], ['/networks', 'networks.html'], ['/settings', 'settings.html'],
  ['/machines/new', 'machines-new.html'],
]);

/** Serves only known redacted previews and application-owned static assets. */
async function serve(request, response) {
  const pathname = new URL(request.url, 'http://localhost').pathname;
  if (request.method === 'POST' && pathname === '/login') {
    request.resume();
    response.setHeader('Content-Type', 'text/plain');
    response.end(request.headers.origin || 'missing');
    return;
  }
  let filename;
  if (routes.has(pathname)) filename = path.join('dist/previews', routes.get(pathname));
  else if (/^\/machines\/[a-f0-9-]{36}$/.test(pathname)) filename = 'dist/previews/guest.html';
  else if (['/assets/app.js', '/assets/style.css', '/assets/audio.js', '/assets/audio-rfb.js'].includes(pathname)) filename = pathname.slice(1);
  else { response.writeHead(404); response.end(); return; }
  try {
    const data = await fs.readFile(filename);
    if (filename.endsWith('.html')) {
      for (const [name, value] of Object.entries(browserHeaders)) response.setHeader(name, value);
      // Negative control reproduces the previous production policy only in this test server.
      if (request.url.includes('?legacy')) response.setHeader('Referrer-Policy', 'no-referrer');
    }
    response.setHeader('Content-Type', filename.endsWith('.css') ? 'text/css' : filename.endsWith('.js') ? 'text/javascript' : 'text/html; charset=utf-8');
    response.end(data);
  } catch { response.writeHead(500); response.end(); }
}

/** Checks desktop/mobile layout, navigation reachability and local search behavior. */
async function run() {
  await fs.mkdir('dist/screenshots', { recursive: true });
  browserHeaders = JSON.parse(await fs.readFile('dist/previews/headers.json', 'utf8'));
  const server = http.createServer(serve);
  await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
  const origin = `http://127.0.0.1:${server.address().port}`;
  const browser = await chromium.launch({ channel: 'chromium', headless: true });
  try {
    for (const [name, viewport] of [['desktop', { width: 1440, height: 1000 }], ['mobile', { width: 390, height: 844 }]]) {
      const page = await browser.newPage({ viewport });
      const errors = [];
      page.on('pageerror', error => errors.push(error.message));
      for (const [route, screen] of [['/login', 'login'], ['/', 'dashboard'], ['/machines', 'machines'], ['/machines/new', 'create']]) {
        await page.goto(origin + route);
        console.log(`Inspecting ${name} ${route}`, await page.evaluate(() => ({ width: document.documentElement.scrollWidth, height: document.documentElement.scrollHeight })));
        await page.screenshot({ path: `dist/screenshots/${name}-${screen}.png`, fullPage: true });
        const overflow = await page.evaluate(() => document.documentElement.scrollWidth > innerWidth + 1);
        assert.equal(overflow, false, `${name} ${route} overflows the viewport`);
        if (route !== '/login') await page.getByRole('button', { name: 'Kirjaudu ulos' }).waitFor({ state: 'visible' });
      }
      await page.goto(origin + '/machines');
      await page.locator('#machine-search').fill('definitely-no-such-guest');
      await page.locator('.empty-filter').waitFor({ state: 'visible' });
      await page.locator('#machine-search').fill('');
      assert.equal(await page.locator('tr[data-name]:visible').count(), 1);
      await page.locator('.machine-name').click();
      await page.getByRole('button', { name: 'Pakotettu sammutus', exact: true }).waitFor({ state: 'visible' });
      await page.getByRole('button', { name: 'Pakotettu uudelleen käynnistys', exact: true }).waitFor({ state: 'visible' });
      await page.screenshot({ path: `dist/screenshots/${name}-guest.png`, fullPage: true });
      assert.equal(await page.evaluate(() => document.documentElement.scrollWidth > innerWidth + 1), false);
      for (const [suffix, expected] of [['?legacy', 'null'], ['', origin]]) {
        await page.goto(origin + '/login' + suffix);
        await page.locator('[name="username"]').fill('admin');
        await page.locator('[name="password"]').fill('browser-origin-test');
        const submitted = page.waitForResponse(response => response.request().method() === 'POST');
        await page.getByRole('button', { name: 'Kirjaudu sisään' }).click();
        assert.equal(await (await submitted).text(), expected, `${name}: native login form Origin`);
      }
      assert.deepEqual(errors, [], 'Unexpected browser JavaScript errors');
      await page.close();
    }
    const audioPage = await browser.newPage();
    await audioPage.goto(origin + '/login');
    const rendered = await audioPage.evaluate(async () => {
      const { decodePcm } = await import('/assets/audio-rfb.js');
      const channels = decodePcm(new Uint8Array([0, 64, 0, 192, 0, 32, 0, 224]));
      const context = new OfflineAudioContext(2, 2, 44100);
      const buffer = context.createBuffer(2, 2, 44100);
      channels.forEach((data, index) => buffer.copyToChannel(data, index));
      const source = context.createBufferSource();
      source.buffer = buffer;
      source.connect(context.destination);
      source.start();
      const output = await context.startRendering();
      return [[...output.getChannelData(0)], [...output.getChannelData(1)]];
    });
    assert.deepEqual(rendered, [[0.5, 0.25], [-0.5, -0.25]], 'PCM reaches the browser audio output graph');
    let audioClosed = false;
    await audioPage.routeWebSocket('**/audio-test', socket => {
      let step = 0;
      socket.send(Buffer.from('RFB 003.008\n'));
      socket.onClose(() => { audioClosed = true; });
      socket.onMessage(message => {
        const bytes = Buffer.from(message);
        if (step === 0) socket.send(Buffer.from([1, 1]));
        if (step === 1) socket.send(Buffer.alloc(4));
        if (step === 2) socket.send(Buffer.alloc(24));
        if (step === 3) {
          const ack = Buffer.alloc(16);
          ack.writeUInt16BE(1, 2);
          ack.writeInt32BE(-259, 12);
          socket.send(ack);
        }
        if (step === 5) {
          assert.deepEqual([...bytes], [255, 1, 0, 0]);
          socket.send(Buffer.from([255, 1, 0, 2, 0, 0, 0, 8, 0, 64, 0, 192, 0, 32, 0, 224]));
        }
        step++;
      });
    });
    await audioPage.evaluate(async () => {
      document.body.innerHTML = '<button id="audio-toggle">Ääni päälle</button><input id="audio-volume" type="range" min="0" max="1" step="0.01" value="0.8"><span id="audio-status"></span>';
      const { initAudio } = await import('/assets/audio.js');
      initAudio(`ws://${location.host}/audio-test`);
    });
    await audioPage.locator('#audio-toggle').click();
    await audioPage.locator('#audio-status').filter({ hasText: 'Ääni käytössä' }).waitFor();
    assert.equal(await audioPage.locator('#audio-toggle').getAttribute('aria-pressed'), 'true');
    await audioPage.locator('#audio-volume').evaluate(input => { input.value = '0.2'; input.dispatchEvent(new Event('input')); });
    await audioPage.locator('#audio-toggle').click();
    assert.equal(await audioPage.locator('#audio-status').textContent(), 'Ääni pois päältä.');
    assert.equal(await audioPage.locator('#audio-toggle').getAttribute('aria-pressed'), 'false');
    await audioPage.waitForTimeout(100);
    assert.equal(audioClosed, true, 'Mute closes the audio transport');
    await audioPage.close();
    console.log('Browser stereo PCM audio rendering passed.');
    console.log('Desktop/mobile rendering, logout reachability and guest filtering passed.');
    console.log('Native form Origin regression passed: old policy null, production policy same-origin.');
  } finally {
    await browser.close();
    await new Promise(resolve => server.close(resolve));
  }
}

run().catch(error => { console.error(error); process.exitCode = 1; });
