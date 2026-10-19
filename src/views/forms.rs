//! Creation wizard and offline-edit forms using server-side validation.

use crate::{auth::Session, backend::Machine, provision::Create, storage::{Pool, Network}};
use super::{escape, hidden};

/// Renders an escaped select option for administrator-visible inventory names.
fn option(value: &str) -> String {
    format!("<option value=\"{}\">{}</option>", escape(value), escape(value))
}

/// Builds the first wizard step from currently available storage, networks and ISOs.
pub fn create(session: &Session, pools: &[Pool], networks: &[Network], isos: &[String]) -> String {
    let pool_options = pools.iter().filter(|p| p.active && p.directory).map(|p| option(&p.name)).collect::<String>();
    let network_options = networks.iter().filter(|n| n.active).map(|n| option(&n.name)).collect::<String>();
    let iso_options = isos.iter().map(|name| option(name)).collect::<String>();
    let ready = !pool_options.is_empty() && !network_options.is_empty() && !iso_options.is_empty();
    format!(r#"<a class="back" href="/machines">← Virtuaalikoneet</a><div class="section-heading"><div><span class="eyebrow">UUSI VIRTUAALIKONE</span><h1>Anna koneelle koti.</h1><p class="muted">1. Asetukset <span class="step-separator">→</span> 2. Yhteenveto <span class="step-separator">→</span> 3. Asenna käyttöjärjestelmä</p></div></div><section class="panel form-panel"><form method="post" action="/machines/preview">{}<h2>Perustiedot</h2><label>Koneen nimi<input name="name" required maxlength="64" pattern="[A-Za-z0-9_.\-]+" placeholder="esim. alpine-palvelin" autofocus></label><div class="form-grid"><label>vCPU<input name="cpus" type="number" min="1" max="256" value="2" required></label><label>Muisti (MiB)<input name="memory_mib" type="number" min="128" max="1048576" value="2048" required></label></div><h2>Tallennus ja asennus</h2><div class="form-grid"><label>Tallennuspooli<select name="pool" required>{pool_options}</select></label><label>Uusi qcow2-levy (GiB)<input name="disk_gib" type="number" min="1" max="16384" value="20" required></label></div><label>ISO-kuva<select name="iso" required>{iso_options}</select><small>Palvelimen ylläpitäjän ISO-hakemistosta.</small></label><label>Libvirt-verkko<select name="network" required>{network_options}</select></label><p class="hint">x86_64 · Q35 · BIOS · virtio-levy ja verkko. Kone luodaan sammutettuna. Tarvitset aktiivisen hakemistopoolin, verkon ja ISO-kuvan.</p><button class="primary" {}>Tarkista asetukset →</button></form></section>"#, hidden("csrf", &session.csrf), if ready { "" } else { "disabled" })
}

/// Displays all resource allocations before the final side-effecting POST.
pub fn preview(input: &Create) -> String {
    let mut fields = String::new();
    let mut rows = String::new();
    for (name, label, value) in [
        ("name", "Nimi", input.name.clone()), ("cpus", "vCPU", input.cpus.to_string()),
        ("memory_mib", "Muisti (MiB)", input.memory_mib.to_string()),
        ("disk_gib", "Uusi levy (GiB)", input.disk_gib.to_string()),
        ("pool", "Tallennuspooli", input.pool.clone()), ("network", "Verkko", input.network.clone()),
        ("iso", "ISO-kuva", input.iso.clone()),
    ] {
        fields.push_str(&hidden(name, &value));
        rows.push_str(&format!("<tr><th>{label}</th><td>{}</td></tr>", escape(&value)));
    }
    format!(r#"<div class="section-heading"><div><span class="eyebrow">VAIHE 2 / 3</span><h1>Tarkista asetukset</h1><p class="muted">Uusi levy luodaan vasta vahvistuksen jälkeen.</p></div></div><section class="panel form-panel"><table><tbody>{rows}</tbody></table><form method="post" action="/machines">{fields}{}<label class="check"><input type="checkbox" name="confirm" value="yes" required> Luo uusi levy ja virtuaalikone näillä asetuksilla.</label><div class="actions"><a class="button" href="/machines/new">Takaisin alkuun</a><button class="primary">Luo virtuaalikone →</button></div></form></section>"#, hidden("csrf", &input.csrf))
}

/// Leaves existing installation media untouched unless the checkbox explicitly requests a change.
pub fn edit(session: &Session, vm: &Machine, isos: &[String]) -> String {
    let options = isos.iter().map(|name| option(name)).collect::<String>();
    format!(r#"<a class="back" href="/machines/{}">← {}</a><h1>Muokkaa asetuksia</h1><section class="panel form-panel"><form method="post" action="/machines/{}/edit">{}<div class="form-grid"><label>vCPU<input name="cpus" type="number" min="1" max="256" value="{}" required></label><label>Muisti (MiB)<input name="memory_mib" type="number" min="128" max="1048576" value="{}" required></label></div><label class="check"><input type="checkbox" name="change_iso" value="true"> Vaihda tai irrota ensimmäisen CD-ROM-aseman ISO-kuva</label><label>Asennusmedia<select name="iso"><option value="">Irrota ISO-kuva</option>{options}</select></label><p class="hint">Muutokset tehdään sammutetun koneen pysyvään määritykseen. Muut laitteet säilyvät ennallaan.</p><button class="primary">Tallenna asetukset</button></form></section>"#, vm.id, escape(&vm.name), vm.id, hidden("csrf", &session.csrf), vm.vcpus, vm.memory_mib)
}
