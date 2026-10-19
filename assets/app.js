// Progressive enhancement for server-rendered pages; mutations remain ordinary forms.

const search = document.querySelector('#machine-search');
const stateFilter = document.querySelector('#state-filter');
const notice = document.querySelector('#notice');

/** Filters existing table rows without changing server-side inventory or keyboard focus. */
function filterMachines() {
  const query = (search?.value || '').toLocaleLowerCase('fi');
  const state = stateFilter?.value || '';
  const rows = [...document.querySelectorAll('tr[data-name]')];
  for (const row of rows) row.hidden = !row.dataset.name.includes(query) || (state && row.dataset.state !== state);
  const empty = document.querySelector('.empty-filter');
  if (empty) empty.hidden = !rows.length || rows.some(row => !row.hidden);
}

/** Shows fixed application feedback; no URL or backend text is inserted as HTML. */
function showNotice(message) {
  if (notice) { notice.textContent = message; notice.hidden = false; }
}

/** Marks stale data explicitly when the daemon or HTTP connection becomes unavailable. */
function refreshStatus(ok) {
  const target = document.querySelector('#refresh-status');
  if (target) {
    target.textContent = ok ? `Päivitetty ${new Date().toLocaleTimeString('fi-FI')}` : 'Yhteys katkesi · tiedot voivat olla vanhentuneita';
    target.classList.toggle('error', !ok);
  }
}

/** Fetches one same-origin resource and redirects expired sessions to login. */
async function request(path) {
  const response = await fetch(path, { credentials: 'same-origin', signal: AbortSignal.timeout(10000) });
  if (response.redirected || response.status === 401) { location.assign('/login'); throw new Error('Session expired'); }
  if (!response.ok) throw new Error(`HTTP ${response.status}`);
  return response;
}

search?.addEventListener('input', filterMachines);
stateFilter?.addEventListener('change', filterMachines);
const query = new URLSearchParams(location.search);
if (query.has('created')) showNotice('Virtuaalikone luotiin sammutettuna. Käynnistä kone ja avaa konsoli käyttöjärjestelmän asentamista varten.');
else if (query.has('changed')) showNotice('Toimintopyyntö onnistui. Hallittu sammutus ja uudelleenkäynnistys voivat kestää hetken.');

// A submitted form stays disabled until navigation; browser back/forward restores it.
document.addEventListener('submit', event => {
  const button = event.submitter;
  if (button) { button.disabled = true; button.dataset.label = button.textContent; button.textContent = 'Käsitellään…'; }
});
window.addEventListener('pageshow', () => {
  for (const button of document.querySelectorAll('button[data-label]')) { button.disabled = false; button.textContent = button.dataset.label; }
});

let previous = null;
const guest = document.querySelector('[data-guest]');
const labels = { 1: 'Käynnissä', 2: 'Odottaa', 3: 'Keskeytetty', 4: 'Sammumassa', 5: 'Sammutettu', 6: 'Kaatunut', 7: 'Lepotila' };

/** Samples cumulative CPU time and reports utilization as a fraction of allocated vCPUs. */
async function refreshGuest() {
  const data = await (await request(`/api/machines/${guest.dataset.guest}`)).json();
  const now = performance.now();
  const current = document.querySelector('#guest-state');
  const label = labels[data.state] || 'Tuntematon';
  if (current.textContent !== label) {
    current.textContent = label;
    current.className = `status ${data.state === 1 ? 'running' : 'off'}`;
    showNotice('Koneen tila muuttui. Lataa sivu uudelleen, jotta toimintopainikkeet päivittyvät.');
  }
  let cpu = '—';
  if (previous && data.active && data.cpu_time_ns >= previous.cpu && data.vcpus > 0) {
    const ratio = (data.cpu_time_ns - previous.cpu) / ((now - previous.time) * 1e6 * data.vcpus) * 100;
    cpu = `${Math.max(0, Math.min(100, ratio)).toFixed(1)} %`;
  } else if (!data.active) cpu = '0.0 %';
  document.querySelector('#guest-cpu').textContent = cpu;
  document.querySelector('#guest-cpus').textContent = data.vcpus;
  document.querySelector('#guest-memory').textContent = data.memory_mib;
  previous = { cpu: data.cpu_time_ns, time: now };
}

/** Polls only visible pages, preserving focused controls and avoiding overlapping requests. */
async function refresh() {
  try {
    if (!document.hidden) {
      for (const fragment of document.querySelectorAll('[data-refresh]')) {
        if (!fragment.contains(document.activeElement)) {
          const response = await request(fragment.dataset.refresh);
          fragment.innerHTML = await response.text(); // Authenticated, escaped server-rendered markup only.
          filterMachines();
        }
      }
      if (guest) await refreshGuest();
      refreshStatus(true);
    }
  } catch { refreshStatus(false); }
  finally { setTimeout(refresh, 5000); }
}
if (guest || document.querySelector('[data-refresh]')) setTimeout(refresh, 5000);

