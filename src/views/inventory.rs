//! Read-only infrastructure views and explicitly confirmed detached-disk removal.

use super::{escape, gib, hidden};
use crate::{
    auth::Session,
    config::Config,
    storage::{Network, Pool},
};

/// Lists capacity and volumes; destructive controls are limited to application disk names.
pub fn storage(pools: &[Pool], session: &Session) -> String {
    let mut body = "<span class=eyebrow>RESURSSIT</span><h1>Tallennus</h1><p class=muted>Libvirtin tallennuspoolit ja levyt. Koneen poistaminen säilyttää sen levyt.</p>".to_string();
    if pools.is_empty() {
        body.push_str("<section class=panel><p>Ei tallennuspooleja. Luo hakemistopooli palvelimen libvirt-työkaluilla.</p></section>");
    }
    for pool in pools {
        body.push_str(&format!("<section class=panel><div class=panel-title><div><h2>{}</h2><p class=muted>{} · {} vapaana / {}</p></div><span class=badge>{}</span></div><div class=table-scroll><table><thead><tr><th>Levy</th><th>Enimmäiskoko</th><th>Varattu tila</th><th></th></tr></thead><tbody>", escape(&pool.name), if pool.active { "Aktiivinen" } else { "Ei aktiivinen" }, gib(pool.available), gib(pool.capacity), if pool.directory { "Hakemistopooli" } else { "Muu pooli" }));
        for volume in &pool.volumes {
            let remove = if volume.managed && pool.directory {
                format!(
                    "<details><summary>Poista irrallinen levy</summary><form method=post action=/volumes/delete>{}{}{}<label>Kirjoita levyn koko nimi<input name=confirmation required autocomplete=off></label><button class=danger>Poista levy pysyvästi</button></form></details>",
                    hidden("csrf", &session.csrf),
                    hidden("pool", &pool.name),
                    hidden("name", &volume.name)
                )
            } else {
                "<span class=muted>Hallinta libvirt-työkaluilla</span>".into()
            };
            body.push_str(&format!(
                "<tr><td><code>{}</code></td><td>{}</td><td>{}</td><td>{remove}</td></tr>",
                escape(&volume.name),
                gib(volume.capacity),
                gib(volume.allocation)
            ));
        }
        body.push_str("</tbody></table></div></section>");
    }
    body
}

/// Shows network status without exposing host network mutation in version one.
pub fn networks(networks: &[Network]) -> String {
    let rows = networks.iter().map(|network| format!("<tr><th>{}</th><td><span class=\"status {}\">{}</span></td><td>{}</td><td>{}</td></tr>", escape(&network.name), if network.active { "running" } else { "off" }, if network.active { "Aktiivinen" } else { "Ei aktiivinen" }, escape(&network.bridge), if network.autostart { "Käytössä" } else { "Pois" })).collect::<String>();
    format!(
        "<span class=eyebrow>YHTEYDET</span><h1>Verkot</h1><p class=muted>Olemassa olevat libvirt-verkot. Verkkojen määrityksiä hallitaan palvelimella.</p><section class=panel><div class=table-scroll><table><thead><tr><th>Nimi</th><th>Tila</th><th>Silta</th><th>Automaattikäynnistys</th></tr></thead><tbody>{rows}</tbody></table></div></section>"
    )
}

/// Displays useful deployment facts while omitting password hashes and session secrets.
pub fn settings(config: &Config) -> String {
    let mut rows = String::new();
    for (label, value) in [
        ("Libvirt", config.libvirt_uri.clone()),
        ("Selainosoite", config.public_origin.clone()),
        ("ISO-hakemisto", config.iso_dir.display().to_string()),
        ("Ylläpitäjä", config.admin_user.clone()),
    ] {
        rows.push_str(&format!(
            "<tr><th>{label}</th><td><code>{}</code></td></tr>",
            escape(&value)
        ));
    }
    format!(
        "<span class=eyebrow>PALVELU</span><h1>Asetukset</h1><section class=panel><table><tbody>{rows}<tr><th>Istunnon enimmäiskesto</th><td>8 tuntia</td></tr></tbody></table></section><p class=hint>Asetuksia ja salasanaa muutetaan palvelimen määritystiedostossa. Palvelun uudelleenkäynnistys mitätöi kaikki istunnot.</p>"
    )
}
