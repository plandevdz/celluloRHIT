//! Serveur Celluloplast | Parcours salarié : API, worker et interface web.

pub mod audit;
pub mod auth;
pub mod config;
pub mod documents;
pub mod error;
pub mod idempotency;
pub mod pdf;
pub mod routes;
pub mod seed;
pub mod state;
pub mod worker;

use std::time::Duration;

use axum::http::{header, HeaderValue};
use axum::Router;
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use tower_http::services::{ServeDir, ServeFile};
use tower_http::set_header::SetResponseHeaderLayer;
use tower_http::trace::TraceLayer;

pub use state::AppState;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

pub static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("../../migrations");

const CSP: &str = "default-src 'self'; img-src 'self' data: blob:; style-src 'self' 'unsafe-inline'; \
                   script-src 'self'; connect-src 'self'; object-src 'none'; frame-src 'self' blob:; \
                   frame-ancestors 'none'; base-uri 'self'; form-action 'self'";

/// Application complète : API sous /api/v1, interface web statique ailleurs
/// (toute route inconnue renvoie index.html pour le routage côté navigateur).
pub fn app(state: AppState) -> Router {
    let web_dir = state.config.web_dir.clone();
    let api = routes::api_router(&state).layer(SetResponseHeaderLayer::overriding(
        header::CACHE_CONTROL,
        HeaderValue::from_static("no-store"),
    ));
    let spa = ServeDir::new(&web_dir).fallback(ServeFile::new(web_dir.join("index.html")));
    Router::new()
        .nest("/api/v1", api)
        .fallback_service(spa)
        .with_state(state)
        .layer(SetResponseHeaderLayer::overriding(header::CONTENT_SECURITY_POLICY, HeaderValue::from_static(CSP)))
        .layer(SetResponseHeaderLayer::overriding(header::X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff")))
        .layer(SetResponseHeaderLayer::overriding(header::X_FRAME_OPTIONS, HeaderValue::from_static("DENY")))
        .layer(SetResponseHeaderLayer::overriding(header::REFERRER_POLICY, HeaderValue::from_static("same-origin")))
        .layer(TraceLayer::new_for_http())
}

/// Connexion à la base, en attendant qu'elle soit prête (démarrage Docker).
pub async fn connect(database_url: &str) -> anyhow::Result<PgPool> {
    let mut last_error = None;
    for attempt in 1..=30 {
        match PgPoolOptions::new()
            .max_connections(20)
            .acquire_timeout(Duration::from_secs(5))
            .connect(database_url)
            .await
        {
            Ok(pool) => return Ok(pool),
            Err(e) => {
                tracing::warn!(attempt, error = %e, "base indisponible, nouvel essai");
                last_error = Some(e);
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
        }
    }
    Err(anyhow::anyhow!("connexion à la base impossible : {last_error:?}"))
}
