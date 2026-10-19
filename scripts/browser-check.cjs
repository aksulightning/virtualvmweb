// Inspect actual server-rendered, redacted HTML in Chromium without shipping browser tooling.
const http = require('node:http');
const fs = require('node:fs/promises');
const path = require('node:path');
const assert = require('node:assert/strict');
const { chromium } = require(process.env.VIRTUALVMWEB_PLAYWRIGHT);

const routes = new Map([
  ['/', 'dashboard.html'], ['/login', 'login.html'], ['/machines', 'machines.html'],
  ['/storage', 'storage.html'], ['/networks', 'networks.html'], ['/settings', 'settings.html'],
  ['/machines/new', 'machines-new.html'],
]);

/** Serves only known redacted previews and application-owned static assets. */
async function serve(request, response) {
  const pathname = new URL(request.url, 'http://localhost').pathname;
  let filename;
  if (routes.has(pathname)) filename = path.join('dist/previews', routes.get(pathname));
  else if (/^\/machines\/[a-f0-9-]{36}$/.test(pathname)) filename = 'dist/previews/guest.html';
  else if (['/assets/app.js', '/assets/style.css'].includes(pathname)) filename = pathname.slice(1);
  else { response.writeHead(404); response.end(); return; }
  try {
    const data = await fs.readFile(filename);
    response.setHeader('Content-Type', filename.endsWith('.css') ? 'text/css' : filename.endsWith('.js') ? 'text/javascript' : 'text/html; charset=utf-8');
    response.end(data);
  } catch { response.writeHead(500); response.end(); }
}

/** Checks desktop/mobile layout, navigation reachability and local search behavior. */
async function run() {
  await fs.mkdir('dist/screenshots', { recursive: true });
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
      assert.deepEqual(errors, [], 'Unexpected browser JavaScript errors');
      await page.close();
    }
    console.log('Desktop/mobile rendering, logout reachability and guest filtering passed.');
  } finally {
    await browser.close();
    await new Promise(resolve => server.close(resolve));
  }
}

run().catch(error => { console.error(error); process.exitCode = 1; });
