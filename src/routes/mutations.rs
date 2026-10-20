//! CSRF-protected guest lifecycle, provisioning and disk removal handlers.

use crate::{
    App,
    auth::Session,
    backend,
    error::{Error, Result},
    firmware,
    provision::{self, Create, Edit},
    storage, views,
};
use axum::{
    Extension, Form,
    extract::{Path, State},
    response::{Html, Redirect},
};
use serde::Deserialize;

/// Lifecycle operation and optional typed destructive-action confirmation.
#[derive(Deserialize)]
pub struct Action {
    csrf: String,
    operation: String,
    #[serde(default)]
    confirmation: String,
}

/// Rechecks CSRF and current guest state before each mutation.
pub async fn action(
    State(app): State<App>,
    Extension(session): Extension<Session>,
    Path(id): Path<String>,
    Form(input): Form<Action>,
) -> Result<Redirect> {
    session.check_csrf(&input.csrf)?;
    let redirect = if input.operation == "undefine" {
        "/machines?changed=1".into()
    } else {
        format!("/machines/{id}?changed=1")
    };
    app.backend
        .run(move |conn| backend::action(conn, &id, &input.operation, &input.confirmation))
        .await?;
    Ok(Redirect::to(&redirect))
}

/// Presents current prerequisites for creating a guest.
pub async fn new(
    State(app): State<App>,
    Extension(session): Extension<Session>,
) -> Result<Html<String>> {
    let root = app.config.iso_dir.clone();
    let (pools, networks, isos, uefi) = app
        .backend
        .run(move |conn| {
            Ok((
                storage::pools(conn)?,
                storage::networks(conn)?,
                provision::isos(&root)?,
                firmware::available(conn),
            ))
        })
        .await?;
    Ok(Html(views::page(
        "Uusi virtuaalikone",
        "/machines",
        &session,
        &views::forms::create(&session, &pools, &networks, &isos, uefi),
    )))
}

/// Validates the first wizard step without allocating disks or defining a guest.
pub async fn preview(
    State(app): State<App>,
    Extension(session): Extension<Session>,
    Form(input): Form<Create>,
) -> Result<Html<String>> {
    session.check_csrf(&input.csrf)?;
    let checked = input.clone();
    let root = app.config.iso_dir.clone();
    app.backend
        .run(move |conn| {
            provision::validate(&checked, &root)?;
            firmware::validate(conn, checked.firmware)
        })
        .await?;
    Ok(Html(views::page(
        "Tarkista asetukset",
        "/machines",
        &session,
        &views::forms::preview(&input),
    )))
}

/// Revalidates all fields and creates one shut-off guest after explicit confirmation.
pub async fn create(
    State(app): State<App>,
    Extension(session): Extension<Session>,
    Form(input): Form<Create>,
) -> Result<Redirect> {
    session.check_csrf(&input.csrf)?;
    let root = app.config.iso_dir.clone();
    let id = app
        .backend
        .run(move |conn| provision::create(conn, &input, &root))
        .await?;
    Ok(Redirect::to(&format!("/machines/{id}?created=1")))
}

/// Refuses editing running/transient guests before showing the offline settings form.
pub async fn edit_page(
    State(app): State<App>,
    Extension(session): Extension<Session>,
    Path(id): Path<String>,
) -> Result<Html<String>> {
    let root = app.config.iso_dir.clone();
    let (vm, isos) = app
        .backend
        .run(move |conn| {
            let dom = backend::domain(conn, &id)?;
            if dom.is_active()? || !dom.is_persistent()? || dom.has_managed_save(0)? {
                return Err(Error::conflict(
                    "Sammuta kone ennen asetusten muokkaamista.",
                ));
            }
            Ok((backend::machine(&dom)?, provision::isos(&root)?))
        })
        .await?;
    Ok(Html(views::page(
        "Muokkaa asetuksia",
        "/machines",
        &session,
        &views::forms::edit(&session, &vm, &isos),
    )))
}

/// Persists supported offline settings after repeating the state and input checks.
pub async fn edit(
    State(app): State<App>,
    Extension(session): Extension<Session>,
    Path(id): Path<String>,
    Form(input): Form<Edit>,
) -> Result<Redirect> {
    session.check_csrf(&input.csrf)?;
    let destination = format!("/machines/{id}?changed=1");
    let root = app.config.iso_dir.clone();
    app.backend
        .run(move |conn| provision::edit(conn, &id, &input, &root))
        .await?;
    Ok(Redirect::to(&destination))
}

/// Explicit disk deletion is separate from removing a guest definition.
#[derive(Deserialize)]
pub struct DeleteVolume {
    csrf: String,
    pool: String,
    name: String,
    confirmation: String,
}

/// Deletes only detached application disks after the storage layer's reference checks.
pub async fn delete_volume(
    State(app): State<App>,
    Extension(session): Extension<Session>,
    Form(input): Form<DeleteVolume>,
) -> Result<Redirect> {
    session.check_csrf(&input.csrf)?;
    app.backend
        .run(move |conn| {
            storage::delete_volume(conn, &input.pool, &input.name, &input.confirmation)
        })
        .await?;
    Ok(Redirect::to("/storage?changed=1"))
}
