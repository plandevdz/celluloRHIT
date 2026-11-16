//! Envoi Exchange (Graph) et archivage SharePoint contre un FAUX serveur
//! Graph local (127.0.0.1) : aucun appel à Microsoft, aucun vrai mail.
//! Couvre R13 : panne puis reprise, aucune fausse livraison annoncée.

mod common;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{post, put};
use axum::{Form, Json, Router};
use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use base64::Engine;
use common::{test_config, TestApp};
use rsa::pkcs1v15::{Signature, VerifyingKey};
use rsa::pkcs8::EncodePrivateKey;
use rsa::signature::Verifier;
use serde_json::{json, Value};
use server::config::{ArchiveMode, ArchiveTarget, MailMode};
use server::graph::GraphConfig;
use server::AppState;
use sha2::Sha256;
use sqlx::PgPool;

#[derive(Default)]
struct Mock {
    /// Réponses programmées par destinataire (codes HTTP successifs).
    plans: HashMap<String, Vec<u16>>,
    sent: Vec<(String, String, String)>,
    tokens: u32,
    uploads: Vec<(String, usize)>,
    upload_failures: u32,
}

type Shared = Arc<Mutex<Mock>>;

struct MockKeys {
    public: rsa::RsaPublicKey,
}

async fn token(State((m, k)): State<(Shared, Arc<MockKeys>)>, Form(f): Form<HashMap<String, String>>) -> Response {
    // L'assertion doit être un JWT RS256 signé par la clé du certificat.
    let assertion = f.get("client_assertion").cloned().unwrap_or_default();
    let parts: Vec<&str> = assertion.split('.').collect();
    assert_eq!(parts.len(), 3, "assertion JWT attendue");
    let header: Value = serde_json::from_slice(&URL_SAFE_NO_PAD.decode(parts[0]).unwrap()).unwrap();
    assert_eq!(header["alg"], "RS256");
    assert!(header["x5t"].is_string() && header["x5t#S256"].is_string());
    let sig = Signature::try_from(URL_SAFE_NO_PAD.decode(parts[2]).unwrap().as_slice()).unwrap();
    VerifyingKey::<Sha256>::new(k.public.clone())
        .verify(format!("{}.{}", parts[0], parts[1]).as_bytes(), &sig)
        .expect("signature de l'assertion valide");
    assert_eq!(f.get("grant_type").map(String::as_str), Some("client_credentials"));
    assert!(f.get("client_secret").is_none(), "aucun secret partagé");
    m.lock().unwrap().tokens += 1;
    Json(json!({ "access_token": "jeton-fictif", "expires_in": 3600, "token_type": "Bearer" })).into_response()
}

async fn send_mail(State((m, _)): State<(Shared, Arc<MockKeys>)>, Path(sender): Path<String>, headers: HeaderMap, Json(body): Json<Value>) -> Response {
    assert_eq!(headers.get("authorization").unwrap(), "Bearer jeton-fictif");
    assert_eq!(body["saveToSentItems"], false);
    let to = body["message"]["toRecipients"][0]["emailAddress"]["address"].as_str().unwrap().to_string();
    let mut g = m.lock().unwrap();
    let code = g.plans.get_mut(&to).and_then(|p| if p.is_empty() { None } else { Some(p.remove(0)) }).unwrap_or(202);
    if code == 202 {
        g.sent.push((sender, to, body["message"]["subject"].as_str().unwrap().to_string()));
        return StatusCode::ACCEPTED.into_response();
    }
    let mut resp = (StatusCode::from_u16(code).unwrap(), Json(json!({ "error": { "code": "ErrorAccessDenied" } }))).into_response();
    if code == 429 {
        resp.headers_mut().insert("retry-after", "7".parse().unwrap());
    }
    resp
}

async fn upload(State((m, _)): State<(Shared, Arc<MockKeys>)>, Path((site, drive, rest)): Path<(String, String, String)>, body: Bytes) -> Response {
    let mut g = m.lock().unwrap();
    if g.upload_failures > 0 {
        g.upload_failures -= 1;
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    }
    assert_eq!(site, "site-test");
    assert_eq!(drive, "bibliotheque-test");
    g.uploads.push((rest.clone(), body.len()));
    Json(json!({ "id": format!("item-{}", g.uploads.len()), "size": body.len() })).into_response()
}

async fn start_mock() -> (String, Shared, std::path::PathBuf, std::path::PathBuf) {
    let key = rsa::RsaPrivateKey::new(&mut rand::thread_rng(), 1024).unwrap();
    let dir = std::env::temp_dir().join(format!("cellulo-graph-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let key_file = dir.join("cle-test.pem");
    std::fs::write(&key_file, key.to_pkcs8_pem(Default::default()).unwrap().as_bytes()).unwrap();
    // Faux certificat : seule l'empreinte de ces octets est utilisée.
    let cert_file = dir.join("certificat-test.pem");
    let fake_der = STANDARD.encode(b"certificat fictif de test");
    let begin = ["-----BEGIN", " CERTIFICATE-----"].concat();
    let end = ["-----END", " CERTIFICATE-----"].concat();
    std::fs::write(&cert_file, format!("{begin}\n{fake_der}\n{end}\n")).unwrap();

    let shared: Shared = Arc::new(Mutex::new(Mock::default()));
    let keys = Arc::new(MockKeys { public: key.to_public_key() });
    let app = Router::new()
        .route("/{tenant}/oauth2/v2.0/token", post(token))
        .route("/v1.0/users/{sender}/sendMail", post(send_mail))
        .route("/v1.0/sites/{site}/drives/{drive}/root:/{*rest}", put(upload))
        .layer(axum::extract::DefaultBodyLimit::disable())
        .with_state((shared.clone(), keys));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    (format!("http://{addr}"), shared, cert_file, key_file)
}

fn graph(base: &str, cert: &std::path::Path, key: &std::path::Path, client: &str) -> GraphConfig {
    GraphConfig {
        tenant_id: "tenant-test".into(),
        client_id: client.into(),
        cert_file: cert.to_path_buf(),
        key_file: key.to_path_buf(),
        authority: base.into(),
        endpoint: base.into(),
        scope: "https://graph.microsoft.com/.default".into(),
    }
}

async fn graph_app(pool: PgPool, base: &str, cert: &std::path::Path, key: &std::path::Path, allowed: Vec<String>) -> (TestApp, AppState) {
    let app = TestApp::new(pool.clone()).await;
    let mut config = test_config(app.state.cfg().data_dir.clone());
    config.mail_mode = MailMode::Graph;
    config.mail_sender = Some("avis@exemple.test".into());
    config.mail_graph = Some(graph(base, cert, key, "app-envoi"));
    config.mail_allowed_domains = allowed;
    config.archive_mode = ArchiveMode::SharePoint;
    config.archive_graph = Some(graph(base, cert, key, "app-archive"));
    config.archive_target = Some(ArchiveTarget { site_id: "site-test".into(), drive_id: "bibliotheque-test".into(), root: "Celluloplast".into() });
    let state = AppState::try_new(pool, config).unwrap();
    (app, state)
}

#[sqlx::test(migrator = "server::MIGRATOR")]
async fn r13_envoi_graph_et_reprise(pool: PgPool) {
    let (base, mock, cert, key) = start_mock().await;
    let (app, state) = graph_app(pool, &base, &cert, &key, vec!["exemple.test".into()]).await;
    let rh = app.user("Rh", &["RH"]).await;
    let _it = app.user("Tech Limite", &["IT"]).await;
    let _formation = app.user("Formatrice Refus", &["FORMATION"]).await;
    let _hors = sqlx::query("INSERT INTO users (email, display_name) VALUES ('externe@autre-domaine.test', 'Externe')").execute(app.pool()).await.unwrap();
    sqlx::query("INSERT INTO user_roles (user_id, role) SELECT id, 'IT' FROM users WHERE email = 'externe@autre-domaine.test'")
        .execute(app.pool())
        .await
        .unwrap();
    {
        let mut g = mock.lock().unwrap();
        g.plans.insert("tech.limite@exemple.test".into(), vec![429]);
        g.plans.insert("formatrice.refus@exemple.test".into(), vec![403]);
    }
    let site = app.site("ALGER").await;
    app.announce(&rh, site, "Yanis", "Recrue", json!({})).await;

    server::worker::tick(&state).await.unwrap();
    let rows: Vec<(String, String, Option<String>)> =
        sqlx::query_as("SELECT recipient_email, status, last_result FROM deliveries ORDER BY recipient_email").fetch_all(app.pool()).await.unwrap();
    let status_of = |r: &str| rows.iter().find(|x| x.0 == r).map(|x| x.1.clone()).unwrap();
    assert_eq!(status_of("formatrice.refus@exemple.test"), "echec", "refus de droits : échec visible");
    assert_eq!(status_of("tech.limite@exemple.test"), "en_attente", "limitation : nouvel essai");
    assert_eq!(status_of("externe@autre-domaine.test"), "echec", "hors pilote : aucun envoi");
    assert!(rows.iter().all(|r| r.1 != "journalise"));
    // Retry-After respecté : la prochaine tentative est dans le futur.
    let next_ok: bool = sqlx::query_scalar("SELECT next_attempt_at > now() + interval '5 seconds' FROM deliveries WHERE recipient_email = 'tech.limite@exemple.test'")
        .fetch_one(app.pool())
        .await
        .unwrap();
    assert!(next_ok);
    assert_eq!(mock.lock().unwrap().sent.len(), 0, "rien n'est annoncé envoyé avant l'acceptation");

    // Échéance atteinte : le nouvel essai est accepté (202), sans doublon.
    sqlx::query("UPDATE deliveries SET next_attempt_at = now() WHERE status = 'en_attente'").execute(app.pool()).await.unwrap();
    server::worker::tick(&state).await.unwrap();
    server::worker::tick(&state).await.unwrap();
    let (st, result): (String, String) = sqlx::query_as("SELECT status, last_result FROM deliveries WHERE recipient_email = 'tech.limite@exemple.test'")
        .fetch_one(app.pool())
        .await
        .unwrap();
    assert_eq!(st, "accepte");
    assert!(result.contains("pas une preuve de livraison"));
    let sent = mock.lock().unwrap().sent.clone();
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].0, "avis@exemple.test");
    assert_eq!(mock.lock().unwrap().tokens, 1, "jeton réutilisé");

    // Reprise motivée par l'ADMIN après correction des droits.
    let admin = app.user("Admin", &["ADMIN"]).await;
    let id: uuid::Uuid = sqlx::query_scalar("SELECT id FROM deliveries WHERE recipient_email = 'formatrice.refus@exemple.test'").fetch_one(app.pool()).await.unwrap();
    let (status, _) = app.call(&admin, axum::http::Method::POST, &format!("/admin/deliveries/{id}/retry"), Some(json!({ "reason": "droits Exchange corrigés" }))).await;
    assert_eq!(status, StatusCode::OK);
    server::worker::tick(&state).await.unwrap();
    let st: String = sqlx::query_scalar("SELECT status FROM deliveries WHERE id = $1").bind(id).fetch_one(app.pool()).await.unwrap();
    assert_eq!(st, "accepte");
}

#[sqlx::test(migrator = "server::MIGRATOR")]
async fn r13_archive_sharepoint_reprise_apres_panne(pool: PgPool) {
    let (base, mock, cert, key) = start_mock().await;
    let (app, state) = graph_app(pool, &base, &cert, &key, vec![]).await;
    let it = app.user("Tech", &["IT"]).await;
    let rh = app.user("Rh", &["RH"]).await;
    let site = app.site("ALGER").await;
    let emp = app.announce(&rh, site, "Lina", "Archive", json!({})).await["employee_id"].as_str().unwrap().to_string();
    let (_, a) = app.call(&it, axum::http::Method::POST, "/assets", Some(json!({ "inventory_no": "PC-ARC", "kind": "pc_portable", "label": "Portable" }))).await;
    let (status, h) = app
        .call(&it, axum::http::Method::POST, "/handovers", Some(json!({ "employee_id": emp, "lines": [{ "type": "asset", "asset_id": a["id"] }] })))
        .await;
    assert_eq!(status, StatusCode::CREATED);
    let doc = h["document_id"].as_str().unwrap().to_string();
    server::documents::ensure_original(&state.pool, &state.cfg(), uuid::Uuid::parse_str(&doc).unwrap()).await.unwrap();

    // SharePoint indisponible : la garde est conservée, l'archive attend.
    mock.lock().unwrap().upload_failures = 1;
    server::worker::archive_documents(&state).await.unwrap();
    let (st, err): (String, Option<String>) = sqlx::query_as("SELECT archive_status, archive_error FROM document_versions").fetch_one(app.pool()).await.unwrap();
    assert_eq!(st, "en_attente");
    let err = err.unwrap();
    assert!(err.contains("503"), "{err}");
    assert_eq!(app.count("SELECT count(*) FROM assignments WHERE ended_at IS NULL").await, 1);

    // Reprise (redémarrage simulé : prochaine tentative échue).
    sqlx::query("UPDATE document_versions SET archive_next_at = now()").execute(app.pool()).await.unwrap();
    server::worker::archive_documents(&state).await.unwrap();
    let (st, archive_id, path): (String, Option<String>, String) =
        sqlx::query_as("SELECT archive_status, archive_id, storage_path FROM document_versions").fetch_one(app.pool()).await.unwrap();
    assert_eq!(st, "archive");
    assert_eq!(archive_id.as_deref(), Some("item-1"));
    let uploads = mock.lock().unwrap().uploads.clone();
    assert!(uploads[0].0.starts_with("Celluloplast/dossiers/"), "{uploads:?}");
    // La copie locale est conservée.
    assert!(state.cfg().data_dir.join(&path).exists());
    // Le statut « signé » exige toujours un scan : rien de présumé.
    let status: String = sqlx::query_scalar("SELECT status FROM documents").fetch_one(app.pool()).await.unwrap();
    assert_eq!(status, "genere");
}
