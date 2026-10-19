//! Guest list and detail views with state-sensitive lifecycle forms.

use crate::{auth::Session, backend::Machine};
use super::{escape, hidden};

/// Maps libvirt states to readable labels and stable CSS classes.
pub fn state(value: u32) -> (&'static str, &'static str) {
    match value {
        1 => ("Käynnissä", "running"), 2 => ("Odottaa", "paused"),
        3 => ("Keskeytetty", "paused"), 4 => ("Sammumassa", "paused"),
        5 => ("Sammutettu", "off"), 6 => ("Kaatunut", "failed"),
        7 => ("Lepotila", "paused"), _ => ("Tuntematon", "off"),
    }
}

/// Shows assigned memory, not a misleading estimate of memory used inside the guest.
pub fn table(machines: &[Machine]) -> String {
    if machines.is_empty() {
        return "<div class=empty><span>▤</span><h3>Tilaa ensimmäiselle koneelle</h3><p>Luo virtuaalikone ISO-kuvasta tai tuo olemassa olevat koneet libvirtiin.</p><a class=button href=/machines/new>Luo virtuaalikone →</a></div>".into();
    }
    let mut rows = String::new();
    for vm in machines {
        let (label, class) = state(vm.state);
        rows.push_str(&format!(r#"<tr data-name="{}" data-state="{class}"><td><a class="machine-name" href="/machines/{}"><span class="machine-icon">▤</span>{}</a></td><td><span class="status {class}">{label}</span></td><td>{}</td><td>{} MiB</td><td>{}</td><td><a aria-label="Avaa {}" href="/machines/{}">Avaa →</a></td></tr>"#, escape(&vm.name.to_lowercase()), vm.id, escape(&vm.name), vm.vcpus, vm.memory_mib, if vm.autostart { "Käytössä" } else { "Pois" }, escape(&vm.name), vm.id));
    }
    format!("<div class=table-scroll><table><thead><tr><th>Nimi</th><th>Tila</th><th>vCPU</th><th>Varattu RAM</th><th>Automaattikäynnistys</th><th></th></tr></thead><tbody>{rows}</tbody></table></div><p class=empty-filter hidden>Hakua vastaavia koneita ei löytynyt.</p>")
}

/// Renders filters outside the periodically replaced table so typing is preserved.
pub fn list(machines: &[Machine]) -> String {
    format!(r#"<div class="section-heading"><div><span class="eyebrow">YMPÄRISTÖ</span><h1>Virtuaalikoneet</h1><p class="muted">Luo, käynnistä ja hallitse koneitasi.</p></div><a class="button primary" href="/machines/new">＋ Uusi virtuaalikone</a></div><section class="panel"><div class="filters"><label class="search-label">Hae nimellä<input type="search" id="machine-search" placeholder="Hae virtuaalikonetta…"></label><label>Tila<select id="state-filter"><option value="">Kaikki tilat</option value="running">Käynnissä</option><option value="off">Sammutettu</option><option value="paused">Keskeytetty / odottaa</option><option value="failed">Kaatunut</option></select></label><span class="live-label" id="refresh-status" role="status">Päivittyy 5 sekunnin välein</span></div><div data-refresh="/fragments/machines">{}</div></section>"#, table(machines))
}

/// Builds one state-changing form; the server rechecks its state and CSRF token.
fn action(vm: &Machine, session: &Session, operation: &str, label: &str) -> String {
    format!("<form method=post action=\"/machines/{}/action\">{}{}<button>{}</button></form>", vm.id, hidden("csrf", &session.csrf), hidden("operation", operation), escape(label))
}

/// Displays current guest facts, device inventory and separate destructive controls.
pub fn detail(vm: &Machine, session: &Session, devices: &[(String, String)], vnc: bool) -> String {
    let (label, class) = state(vm.state);
    let mut actions = String::new();
    if vm.active {
        actions.push_str(&action(vm, session, "shutdown", "Sammuta hallitusti"));
        actions.push_str(&action(vm, session, "reboot", "Käynnistä uudelleen"));
        if vm.state == 1 { actions.push_str(&action(vm, session, "pause", "Keskeytä")); }
        if vm.state == 3 { actions.push_str(&action(vm, session, "resume", "Jatka")); }
    } else {
        actions.push_str(&action(vm, session, "start", "▶ Käynnistä"));
        if vm.persistent { actions.push_str(&format!("<a class=button href=\"/machines/{}/edit\">Muokkaa asetuksia</a>", vm.id)); }
    }
    if vm.persistent {
        actions.push_str(&action(vm, session, if vm.autostart { "autostart-off" } else { "autostart-on" }, if vm.autostart { "Poista automaattikäynnistys" } else { "Ota automaattikäynnistys käyttöön" }));
    }
    let console = if vnc && vm.active { format!("<a class=\"button primary\" href=\"/machines/{}/console\">Avaa konsoli ↗</a>", vm.id) } else { "<span class=muted>Konsoli vaatii käynnissä olevan koneen ja VNC-näytön.</span>".into() };
    let device_rows = devices.iter().map(|(key, value)| format!("<tr><th>{}</th><td>{}</td></tr>", escape(key), escape(value))).collect::<String>();
    let danger_op = if vm.active { "force-off" } else { "undefine" };
    let danger_label = if vm.active { "Pakkosammuta kone" } else { "Poista koneen määritys" };
    format!(r#"<a class="back" href="/machines">← Virtuaalikoneet</a><div class="section-heading"><div><span class="eyebrow">VIRTUAALIKONE</span><h1>{}</h1><span class="status {class}" id="guest-state">{label}</span></div>{console}</div><div class="actions">{actions}</div><div class="stats" data-guest="{}"><article><span>Prosessorit</span><strong id="guest-cpus">{}</strong><small>Virtuaalista CPU:ta</small></article><article><span>Varattu muisti</span><strong id="guest-memory">{}</strong><small>MiB · ei vieraan muistinkulutus</small></article><article><span>CPU-kuorma</span><strong id="guest-cpu">—</strong><small>Osuus varatuista vCPU:ista</small></article><article><span>Automaattikäynnistys</span><strong class="text-value">{}</strong><small>Palvelimen käynnistyessä</small></article></div><section class="panel"><div class="panel-title"><h2>Laitteisto</h2><code>{}</code></div><div class="table-scroll"><table><tbody>{device_rows}</tbody></table></div></section><details class="panel danger-panel"><summary>Vaaralliset toiminnot</summary><p>Pakkosammutus voi menettää tallentamattomat tiedot. Määrityksen poisto säilyttää levyt; niiden poistaminen tehdään erikseen Tallennus-sivulla.</p><form method="post" action="/machines/{}/action">{}{}<label>Kirjoita koneen nimi vahvistukseksi<input name="confirmation" required autocomplete="off" placeholder="{}"></label><button class="danger">{danger_label}</button></form></details>"#, escape(&vm.name), vm.id, vm.vcpus, vm.memory_mib, if vm.autostart { "Käytössä" } else { "Pois" }, vm.id, vm.id, hidden("csrf", &session.csrf), hidden("operation", danger_op), escape(&vm.name))
}

/// Embeds only a domain UUID; the browser cannot select a proxy destination.
pub fn console(vm: &Machine) -> String {
    format!(r#"<a class="back" href="/machines/{}">← {}</a><div class="section-heading"><div><span class="eyebrow">ETÄNÄYTTÖ</span><h1>Konsoli</h1><p id="console-status" role="status">Yhdistetään…</p></div><div class="actions"><button id="send-cad">Ctrl + Alt + Del</button><button id="fullscreen">Koko näyttö</button></div></div><div id="console-screen" data-domain="{}"></div><script type="module" src="/assets/console.js"></script>"#, vm.id, escape(&vm.name), vm.id)
}

