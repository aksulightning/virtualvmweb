//! Authenticated read endpoints and server-rendered inventory fragments.

use axum::{extract::{Path, State}, response::Html, Extension, Json};
use crate::{App, auth::Session, backend, error::{Error, Result}, storage, views, xml};

/// Loads host facts while keeping the surrounding navigation stable during refreshes.
pub async fn dashboard(State(app): State<App>, Extension(session): Extension<Session>) -> Result<Html<String>> {
    let result = app.backend.run(backend::overview).await;
    let content = match result {
        Ok(data) => views::dashboard(&data),
        Err(_) => "<section class=panel><h1>Libvirt-yhteys ei ole käytettävissä</h1><p>Tarkista libvirtd-palvelu ja palvelukäyttäjän käyttöoikeudet. Yhteyttä yritetään uudelleen automaattisesti.</p></section>".into(),
    };
    Ok(Html(views::page("Yleiskuva", "/", &session, &format!("<p id=refresh-status class=live-label role=status>Päivittyy 5 sekunnin välein</p><div data-refresh=/fragments/dashboard>{content}</div>"))))
}

/// Returns only dashboard content for a signed-in poll request.
pub async fn dashboard_fragment(State(app): State<App>) -> Result<Html<String>> {
    Ok(Html(views::dashboard(&app.backend.run(backend::overview).await?)))
}

/// Renders the complete guest list with accessible search and state filters.
pub async fn machines(State(app): State<App>, Extension(session): Extension<Session>) -> Result<Html<String>> {
    let data = app.backend.run(backend::overview).await?;
    Ok(Html(views::page("Virtuaalikoneet", "/machines", &session, &views::machines::list(&data.machines))))
}

/// Returns escaped table markup; no credentials or raw XML appear in the fragment.
pub async fn machines_fragment(State(app): State<App>) -> Result<Html<String>> {
    Ok(Html(views::machines::table(&app.backend.run(backend::overview).await?.machines)))
}

/// Loads guest hardware from live XML and presents only selected, non-secret fields.
pub async fn detail(State(app): State<App>, Extension(session): Extension<Session>, Path(id): Path<String>) -> Result<Html<String>> {
    let (vm, devices, vnc) = app.backend.run(move |conn| {
        let dom = backend::domain(conn, &id)?;
        let root = xml::parse(&dom.get_xml_desc(0)?)?;
        Ok((backend::machine(&dom)?, xml::hardware(&root), xml::vnc_index(&root).is_some()))
    }).await?;
    Ok(Html(views::page(&vm.name, "/machines", &session, &views::machines::detail(&vm, &session, &devices, vnc))))
}

/// Supplies current state and raw CPU counters for the detail page's sampled utilization.
pub async fn machine_json(State(app): State<App>, Path(id): Path<String>) -> Result<Json<backend::Machine>> {
    Ok(Json(app.backend.run(move |conn| backend::machine(&backend::domain(conn, &id)?)).await?))
}

/// Renders the console only for a running VNC guest; upgrade rechecks authorization and state.
pub async fn console_page(State(app): State<App>, Extension(session): Extension<Session>, Path(id): Path<String>) -> Result<Html<String>> {
    let vm = app.backend.run(move |conn| {
        let dom = backend::domain(conn, &id)?;
        if !dom.is_active()? || xml::vnc_index(&xml::parse(&dom.get_xml_desc(0)?)?).is_none() {
            return Err(Error::conflict("Konsoli vaatii käynnissä olevan VNC-koneen."));
        }
        backend::machine(&dom)
    }).await?;
    Ok(Html(views::page("Konsoli", "/machines", &session, &views::machines::console(&vm))))
}

/// Reads pools and volumes without changing storage state.
pub async fn storage(State(app): State<App>, Extension(session): Extension<Session>) -> Result<Html<String>> {
    let pools = app.backend.run(storage::pools).await?;
    Ok(Html(views::page("Tallennus", "/storage", &session, &views::inventory::storage(&pools, &session))))
}

/// Reads network status without mutating host interfaces.
pub async fn networks(State(app): State<App>, Extension(session): Extension<Session>) -> Result<Html<String>> {
    let networks = app.backend.run(storage::networks).await?;
    Ok(Html(views::page("Verkot", "/networks", &session, &views::inventory::networks(&networks))))
}

/// Exposes only non-secret configuration fields.
pub async fn settings(State(app): State<App>, Extension(session): Extension<Session>) -> Html<String> {
    Html(views::page("Asetukset", "/settings", &session, &views::inventory::settings(&app.config)))
}

