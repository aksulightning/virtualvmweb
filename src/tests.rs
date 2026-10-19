//! Regression coverage for security boundaries, XML preservation and the real libvirt test driver.

use crate::{App, auth, backend, config::Config, provision, routes, views, xml};
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use std::{path::PathBuf, sync::OnceLock};
use tower::ServiceExt;

/// Builds a validated local test configuration with one cached password hash.
fn config() -> Config {
    static HASH: OnceLock<String> = OnceLock::new();
    Config {
        listen: "127.0.0.1:8080".parse().unwrap(),
        public_origin: "http://127.0.0.1:8080".into(),
        libvirt_uri: "test:///default".into(),
        admin_user: "admin".into(),
        password_hash: HASH
            .get_or_init(|| auth::hash_password("test-password-only").unwrap())
            .clone(),
        iso_dir: PathBuf::from("/tmp"),
        novnc_dir: PathBuf::from("/tmp/novnc"),
    }
}

/// Constructs same-origin requests and optionally supplies a session cookie.
fn request(method: &str, path: &str, body: &str, session: Option<&auth::Session>) -> Request<Body> {
    let mut builder = Request::builder()
        .method(method)
        .uri(path)
        .header("host", "127.0.0.1:8080")
        .header("origin", "http://127.0.0.1:8080")
        .header("content-type", "application/x-www-form-urlencoded");
    if let Some(session) = session {
        builder = builder.header("cookie", format!("vvw_session={}", session.id));
    }
    builder.body(Body::from(body.to_string())).unwrap()
}

/// Verifies the configured transport and rejects remote plaintext administration.
#[test]
fn configuration_rejects_unsafe_origins() {
    let mut cfg = config();
    assert!(cfg.validate().is_ok());
    cfg.public_origin = "http://vm.example.org".into();
    assert!(cfg.validate().is_err());
    cfg.public_origin = "https://vm.example.org".into();
    assert!(cfg.validate().is_ok());
    cfg.public_origin.push_str("/path");
    assert!(cfg.validate().is_err());
}

/// Covers password verification, CSRF rejection, rate limits and explicit revocation.
#[test]
fn sessions_and_passwords_fail_closed() {
    let cfg = config();
    assert!(auth::verify(&cfg.password_hash, "test-password-only"));
    assert!(!auth::verify(&cfg.password_hash, "wrong"));
    let auth = auth::Auth::default();
    let session = auth.create().unwrap();
    assert!(session.check_csrf(&session.csrf).is_ok());
    assert!(session.check_csrf("wrong").is_err());
    assert!(auth.find(&session.id).is_some());
    auth.revoke(&session.id).unwrap();
    assert!(auth.find(&session.id).is_none());
    for _ in 0..5 {
        auth.allow_attempt().unwrap();
    }
    assert_eq!(
        auth.allow_attempt().unwrap_err().0,
        StatusCode::TOO_MANY_REQUESTS
    );
    let cookie = auth::cookie("example", true, false);
    for flag in ["HttpOnly", "Secure", "SameSite=Strict", "Path=/"] {
        assert!(cookie.contains(flag));
    }
}

/// Ensures hostile names cannot become executable HTML or malformed XML attributes.
#[test]
fn output_is_escaped() {
    assert_eq!(views::escape("<&\"'>"), "&lt;&amp;&quot;&#39;&gt;");
    assert!(!views::login("<script>alert(1)</script>").contains("<script>alert"));
}

/// Proves traversal and symlinks escaping the ISO root are rejected before libvirt calls.
#[test]
fn iso_selection_stays_in_its_root() {
    let root = std::env::temp_dir().join(format!("vvw-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&root).unwrap();
    std::fs::write(root.join("valid.iso"), b"fixture").unwrap();
    std::os::unix::fs::symlink("/etc/passwd", root.join("outside.iso")).unwrap();
    assert!(provision::iso_path(&root, "valid.iso").is_ok());
    for invalid in [
        "../valid.iso",
        "/tmp/valid.iso",
        "outside.iso",
        "missing.iso",
        "x\0.iso",
    ] {
        assert!(provision::iso_path(&root, invalid).is_err());
    }
    assert_eq!(provision::isos(&root).unwrap(), ["valid.iso"]);
    std::fs::remove_dir_all(root).unwrap();
}

/// Confirms offline edits retain devices, UUID, boot settings and metadata.
#[test]
fn xml_edits_preserve_unrelated_configuration() {
    let mut root = xml::parse("<domain><uuid>keep-me</uuid><vcpu current='1'>2</vcpu><memory unit='KiB'>2048</memory><metadata><owner>team</owner></metadata><os><boot dev='hd'/></os><devices><interface type='bridge'><source bridge='br0'/></interface><disk type='file' device='cdrom'><source file='/old.iso'/><target dev='sda'/></disk></devices></domain>").unwrap();
    xml::edit_resources(&mut root, 4, 4096).unwrap();
    xml::edit_cdrom(&mut root, Some("/new&safe.iso")).unwrap();
    let out = xml::serialize(&root).unwrap();
    assert!(out.contains("keep-me") && out.contains("team") && out.contains("br0"));
    assert!(out.contains("/new&amp;safe.iso"));
    assert_eq!(root.get_child("vcpu").unwrap().get_text().unwrap(), "4");
    assert!(
        !root.get_child("vcpu")
            .unwrap()
            .attributes
            .contains_key("current")
    );
    let mut advanced = xml::parse("<domain><cpu><topology sockets='2'/></cpu></domain>").unwrap();
    assert!(xml::edit_resources(&mut advanced, 1, 1024).is_err());
}

/// Validates a generated guest profile and confirms VNC has no public listener.
#[test]
fn generated_domain_has_private_graphics() {
    let input = provision::Create {
        csrf: String::new(),
        name: "test".into(),
        cpus: 2,
        memory_mib: 2048,
        disk_gib: 20,
        pool: "default".into(),
        network: "default".into(),
        iso: "test.iso".into(),
        confirm: "yes".into(),
    };
    let root = xml::parse(&provision::domain_xml(
        &input,
        "d99a9121-72ca-467f-869e-3ee0f364a8f6",
        "/a&b.qcow2",
        "/a.iso",
    ))
    .unwrap();
    assert_eq!(xml::vnc_index(&root), Some(0));
    let graphics = root
        .get_child("devices")
        .unwrap()
        .get_child("graphics")
        .unwrap();
    assert_eq!(
        xml::attr(graphics.get_child("listen").unwrap(), "type"),
        "none"
    );
    assert!(provision::resources(0, 1024).is_err());
    assert!(provision::resources(2, 64).is_err());
}

/// Exercises the actual libvirt test driver through the same serialized application backend.
#[tokio::test]
async fn libvirt_lifecycle_and_state_checks() {
    let backend = backend::Backend::new("test:///default".into());
    backend
        .run(|conn| {
            let data = backend::overview(conn)?;
            let vm = data.machines.first().expect("test driver default guest");
            backend::action(conn, &vm.id, "pause", "")?;
            assert_eq!(backend::machine(&backend::domain(conn, &vm.id)?)?.state, 3);
            backend::action(conn, &vm.id, "resume", "")?;
            assert!(backend::action(conn, &vm.id, "force-off", "wrong name").is_err());
            backend::action(conn, &vm.id, "shutdown", "")?;
            backend::action(conn, &vm.id, "start", "")?;
            assert!(backend::action(conn, &vm.id, "undefine", &vm.name).is_err());
            Ok(())
        })
        .await
        .unwrap();
}

/// Covers unauthenticated access, hostile origins, CSRF, headers and session logout.
#[tokio::test]
async fn http_authorization_boundaries() {
    let app = App::new(config());
    let router = routes::router(app.clone());
    assert_eq!(
        router
            .clone()
            .oneshot(request("GET", "/machines", "", None))
            .await
            .unwrap()
            .status(),
        StatusCode::SEE_OTHER
    );
    assert_eq!(
        router
            .clone()
            .oneshot(request("GET", "/api/machines/invalid", "", None))
            .await
            .unwrap()
            .status(),
        StatusCode::UNAUTHORIZED
    );
    let session = app.auth.create().unwrap();
    let mut hostile = request(
        "POST",
        "/logout",
        &format!("csrf={}", session.csrf),
        Some(&session),
    );
    hostile
        .headers_mut()
        .insert("origin", "https://attacker.invalid".parse().unwrap());
    assert_eq!(
        router.clone().oneshot(hostile).await.unwrap().status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        router
            .clone()
            .oneshot(request("POST", "/logout", "csrf=wrong", Some(&session)))
            .await
            .unwrap()
            .status(),
        StatusCode::FORBIDDEN
    );
    let response = router
        .clone()
        .oneshot(request("GET", "/settings", "", Some(&session)))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert!(response.headers().contains_key("content-security-policy"));
    let text = String::from_utf8(
        response
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes()
            .to_vec(),
    )
    .unwrap();
    assert!(!text.contains(&app.config.password_hash));
    let response = router
        .oneshot(request(
            "POST",
            "/logout",
            &format!("csrf={}", session.csrf),
            Some(&session),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert!(app.auth.find(&session.id).is_none());
}

/// Exercises login hashing and verifies that the browser receives a protected session cookie.
#[tokio::test]
async fn http_login_rotates_sessions() {
    let app = App::new(config());
    let old = app.auth.create().unwrap();
    let router = routes::router(app.clone());
    let response = router
        .oneshot(request(
            "POST",
            "/login",
            "username=admin&password=test-password-only",
            Some(&old),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert!(
        response.headers()["set-cookie"]
            .to_str()
            .unwrap()
            .contains("HttpOnly")
    );
    assert!(app.auth.find(&old.id).is_none());
}
