//! Small server-rendered HTML components with escaping at every data boundary.

/// Guest lists, details and lifecycle controls.
pub mod machines;
/// Creation and offline configuration forms.
pub mod forms;
/// Storage, network and configuration pages.
pub mod inventory;

use crate::{auth::Session, backend::Overview};

/// Escapes text for HTML content, quoted attributes and generated XML.
pub fn escape(value: &str) -> String {
    value.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
        .replace('"', "&quot;").replace('\'', "&#39;")
}

/// Builds the shared document using only local assets and no inline scripts.
pub fn document(title: &str, content: &str) -> String {
    format!(r#"<!doctype html><html lang="fi"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>{} · virtualvmweb</title><link rel="stylesheet" href="/assets/style.css"><script type="module" src="/assets/app.js"></script></head><body>{content}</body></html>"#, escape(title))
}

/// Wraps authenticated pages in a responsive navigation and logout form.
pub fn page(title: &str, active: &str, session: &Session, content: &str) -> String {
    let mut links = String::new();
    for (path, label, symbol) in [("/", "Yleiskuva", "◈"), ("/machines", "Virtuaalikoneet", "▤"), ("/storage", "Tallennus", "▱"), ("/networks", "Verkot", "◇"), ("/settings", "Asetukset", "⚙")] {
        let current = if path == active { " aria-current=page" } else { "" };
        links.push_str(&format!("<a href=\"{path}\"{current}><span aria-hidden=true>{symbol}</span>{label}</a>"));
    }
    document(title, &format!(r#"<a class="skip" href="#main">Siirry sisältöön</a><aside class="sidebar"><a class="brand" href="/"><span class="brand-icon">v</span><span>virtualvmweb<small>VIRTUAL MACHINE CONTROL</small></span></a><nav aria-label="Päänavigaatio">{links}</nav><div class="sidebar-bottom"><span class="badge">ALPINE · LIBVIRT</span><p>Yksi palvelin.<br>Selkeä hallinta.</p><form method="post" action="/logout">{}<button class="quiet">Kirjaudu ulos ↗</button></form></div></aside><main id="main"><header class="topbar"><span>Työtila <span class="muted">/ {}</span></span><span class="admin-mark">Ylläpitäjä</span></header><div class="content"><div id="notice" role="status" hidden></div>{content}</div><footer>virtualvmweb <span>0.1.0 · Alpine Linux</span></footer></main>"#, hidden("csrf", &session.csrf), escape(title)))
}

/// Produces a hidden field using a fixed field name and escaped value.
pub fn hidden(name: &str, value: &str) -> String {
    format!("<input type=hidden name=\"{}\" value=\"{}\">", escape(name), escape(value))
}

/// Renders a login form with browser-native password manager hints.
pub fn login(error: &str) -> String {
    document("Kirjaudu sisään", &format!(r#"<div class="login-layout"><section class="login-story"><span class="eyebrow">VIRTUALVMWEB</span><h1>Virtuaalikoneesi.<br>Yhdessä paikassa.</h1><p>Kevyt käyttöliittymä Alpine Linuxin virtualisointiin.</p><span class="badge">RUST + LIBVIRT</span></section><section class="login-panel"><span class="brand-icon">v</span><h2>Tervetuloa takaisin</h2><p class="muted">Kirjaudu palvelimen hallintaan.</p><p role="alert" class="error">{}</p><form method="post" action="/login"><label>Käyttäjätunnus<input name="username" autocomplete="username" required maxlength="128" autofocus></label><label>Salasana<input name="password" type="password" autocomplete="current-password" required maxlength="1024"></label><button class="primary wide">Kirjaudu sisään →</button></form></section></div>"#, escape(error)))
}

/// Formats binary capacities with an explicit unit.
pub fn gib(bytes: u64) -> String {
    format!("{:.1} GiB", bytes as f64 / 1_073_741_824.0)
}

/// Builds overview cards and a current guest table from a single host query.
pub fn dashboard(data: &Overview) -> String {
    let running = data.machines.iter().filter(|m| m.active).count();
    let free = data.free_mib.map_or("Ei saatavilla".into(), |m| format!("{:.1} GiB vapaana", m as f64 / 1024.0));
    format!(r#"<div class="section-heading"><div><span class="eyebrow">PALVELIN</span><h1>{}</h1><p class="muted">Virtuaaliympäristösi yhdellä silmäyksellä.</p></div><a class="button primary" href="/machines/new">＋ Uusi virtuaalikone</a></div><div class="stats"><article><span>Virtuaalikoneet</span><strong>{}</strong><small>{running} käynnissä</small></article><article><span>Prosessorit</span><strong>{}</strong><small>Palvelimen loogista CPU:ta</small></article><article><span>Palvelimen muisti</span><strong>{:.1}<em> GiB</em></strong><small>{free}</small></article><article><span>Libvirt-yhteys</span><strong class="connected">Yhdistetty</strong><small>Paikallinen yhteys</small></article></div><section class="panel"><div class="panel-title"><h2>Virtuaalikoneet</h2><a href="/machines">Näytä kaikki →</a></div>{}</section>"#, escape(&data.hostname), data.machines.len(), data.cpus, data.memory_mib as f64 / 1024.0, machines::table(&data.machines))
}

