//! Outils de test : application branchée sur une base PostgreSQL isolée
//! (créée par `#[sqlx::test]`), utilisateurs fictifs et appels HTTP.

#![allow(dead_code)]

use axum::body::Body;
use axum::http::{header, Method, Request, StatusCode};
use axum::Router;
use http_body_util::BodyExt;
use serde_json::{json, Value};
use server::config::{Config, MailMode};
use server::{app, auth, AppState};
use sqlx::PgPool;
use tower::ServiceExt;
use uuid::Uuid;

pub struct TestApp {
    pub router: Router,
    pub state: AppState,
    _data: tempdir::TempDirGuard,
}

pub mod tempdir {
    pub struct TempDirGuard(pub std::path::PathBuf);
    impl Drop for TempDirGuard {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
}

pub fn test_config(data_dir: std::path::PathBuf) -> Config {
    let (font, bold) = if cfg!(windows) {
        ("C:/Windows/Fonts/arial.ttf", "C:/Windows/Fonts/arialbd.ttf")
    } else {
        ("/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf", "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf")
    };
    let data_dir_backup = data_dir.join("_sauvegardes");
    Config {
        database_url: String::new(),
        bind_addr: "127.0.0.1:0".into(),
        public_url: "https://test.local".into(),
        web_dir: data_dir.join("web"),
        data_dir,
        cookie_secure: false,
        session_idle_minutes: 60,
        session_max_hours: 10,
        mail_mode: MailMode::Journal,
        mail_test_recipient: None,
        mail_sender: None,
        mail_graph: None,
        mail_allowed_domains: vec![],
        archive_mode: server::config::ArchiveMode::Desactive,
        archive_graph: None,
        archive_target: None,
        pdf_font: font.into(),
        pdf_font_bold: bold.into(),
        max_upload_bytes: 2 * 1024 * 1024,
        alert_days: 30,
        alert_km: 1000,
        escalation_minutes: 60,
        disabled_modules: vec![],
        graph_authority: "https://login.microsoftonline.com".into(),
        graph_endpoint: "https://graph.microsoft.com".into(),
        env_mail_cert: None,
        env_archive_cert: None,
        backup_dir: data_dir_backup,
        pg_dump_bin: "pg_dump".into(),
        gpg_bin: "gpg".into(),
        caddy_data_dir: None,
        database_password: None,
        backup: None,
        overridden: vec![],
    }
}

impl TestApp {
    pub async fn new(pool: PgPool) -> Self {
        let dir = std::env::temp_dir().join(format!("cellulo-test-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let state = AppState::new(pool, test_config(dir.clone()));
        TestApp { router: app(state.clone()), state, _data: tempdir::TempDirGuard(dir) }
    }

    /// Instance avec une configuration ajustée (mode pilote, mode d'envoi).
    #[allow(dead_code)]
    pub async fn with_config(pool: PgPool, adjust: impl FnOnce(&mut Config)) -> Self {
        let dir = std::env::temp_dir().join(format!("cellulo-test-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut config = test_config(dir.clone());
        adjust(&mut config);
        let state = AppState::new(pool, config);
        TestApp { router: app(state.clone()), state, _data: tempdir::TempDirGuard(dir) }
    }

    pub fn pool(&self) -> &PgPool {
        &self.state.pool
    }

    /// Crée un utilisateur actif avec ses rôles et renvoie son cookie de session.
    pub async fn user(&self, name: &str, roles: &[&str]) -> Session {
        let email = format!("{}@exemple.test", name.to_lowercase().replace(' ', "."));
        let id: Uuid = sqlx::query_scalar("INSERT INTO users (email, display_name, password_hash) VALUES ($1, $2, 'x') RETURNING id")
            .bind(&email)
            .bind(name)
            .fetch_one(self.pool())
            .await
            .unwrap();
        for r in roles {
            sqlx::query("INSERT INTO user_roles (user_id, role) VALUES ($1, $2)")
                .bind(id)
                .bind(r)
                .execute(self.pool())
                .await
                .unwrap();
        }
        let (token, hash) = auth::new_token();
        sqlx::query("INSERT INTO sessions (token_hash, user_id, expires_at) VALUES ($1, $2, now() + interval '1 hour')")
            .bind(hash)
            .bind(id)
            .execute(self.pool())
            .await
            .unwrap();
        Session { id, email, cookie: format!("{}={token}", auth::SESSION_COOKIE) }
    }

    pub async fn site(&self, code: &str) -> Uuid {
        sqlx::query_scalar("INSERT INTO sites (code, name) VALUES ($1, $1) RETURNING id")
            .bind(code)
            .fetch_one(self.pool())
            .await
            .unwrap()
    }

    pub async fn call(&self, s: &Session, method: Method, uri: &str, body: Option<Value>) -> (StatusCode, Value) {
        self.call_with(s, method, uri, body, &[]).await
    }

    pub async fn call_with(
        &self,
        s: &Session,
        method: Method,
        uri: &str,
        body: Option<Value>,
        headers: &[(&str, &str)],
    ) -> (StatusCode, Value) {
        let mut req = Request::builder()
            .method(method)
            .uri(format!("/api/v1{uri}"))
            .header(header::COOKIE, &s.cookie)
            .header("x-cellulo-client", "web");
        for (k, v) in headers {
            req = req.header(*k, *v);
        }
        let req = match body {
            Some(b) => req.header(header::CONTENT_TYPE, "application/json").body(Body::from(b.to_string())),
            None => req.body(Body::empty()),
        }
        .unwrap();
        let resp = self.router.clone().oneshot(req).await.unwrap();
        let status = resp.status();
        let bytes = resp.into_body().collect().await.unwrap().to_bytes();
        let value = serde_json::from_slice(&bytes).unwrap_or_else(|_| json!({ "raw_len": bytes.len() }));
        (status, value)
    }

    /// Dépôt d'un fichier (multipart, champ « file »).
    pub async fn upload(&self, s: &Session, uri: &str, filename: &str, bytes: &[u8]) -> (StatusCode, Value) {
        let boundary = "CELLULOBOUNDARY";
        let mut body = format!(
            "--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"{filename}\"\r\nContent-Type: application/octet-stream\r\n\r\n"
        )
        .into_bytes();
        body.extend_from_slice(bytes);
        body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
        let req = Request::builder()
            .method(Method::POST)
            .uri(format!("/api/v1{uri}"))
            .header(header::COOKIE, &s.cookie)
            .header("x-cellulo-client", "web")
            .header(header::CONTENT_TYPE, format!("multipart/form-data; boundary={boundary}"))
            .body(Body::from(body))
            .unwrap();
        let (status, bytes) = self.raw(req).await;
        (status, serde_json::from_slice(&bytes).unwrap_or(json!({ "raw": String::from_utf8_lossy(&bytes) })))
    }

    pub async fn count(&self, sql: &str) -> i64 {
        sqlx::query_scalar(sql).fetch_one(self.pool()).await.unwrap()
    }

    pub async fn raw(&self, req: Request<Body>) -> (StatusCode, Vec<u8>) {
        let resp = self.router.clone().oneshot(req).await.unwrap();
        let status = resp.status();
        (status, resp.into_body().collect().await.unwrap().to_bytes().to_vec())
    }

    /// Annonce une arrivée minimale (cinq champs) avec besoins facultatifs.
    pub async fn announce(&self, s: &Session, site: Uuid, first: &str, last: &str, extra: Value) -> Value {
        let mut body = json!({
            "first_name": first, "last_name": last, "job_title": "Commercial",
            "site_id": site, "planned_arrival": "2026-12-01"
        });
        if let Value::Object(m) = extra {
            for (k, v) in m {
                body[k] = v;
            }
        }
        let key = Uuid::new_v4().to_string();
        let (status, v) = self
            .call_with(s, Method::POST, "/arrivals", Some(body), &[("idempotency-key", &key)])
            .await;
        assert_eq!(status, StatusCode::CREATED, "{v}");
        v
    }
}

#[derive(Clone)]
pub struct Session {
    pub id: Uuid,
    pub email: String,
    pub cookie: String,
}
