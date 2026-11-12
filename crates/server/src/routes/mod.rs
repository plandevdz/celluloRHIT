pub mod admin;
pub mod assets;
pub mod auth;
pub mod cases;
pub mod employees;
pub mod imports;
pub mod movements;
pub mod tasks;
pub mod trainings;

use axum::extract::{Request, State};
use axum::http::{Method, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use serde_json::json;

use crate::state::AppState;

/// En-tête obligatoire sur les commandes : un formulaire d'un autre site ne
/// peut pas l'ajouter sans autorisation CORS (jamais accordée). Complète le
/// cookie `SameSite=Strict`.
pub const CLIENT_HEADER: &str = "x-cellulo-client";

async fn require_client_header(req: Request, next: Next) -> Response {
    let safe = matches!(*req.method(), Method::GET | Method::HEAD | Method::OPTIONS);
    if !safe && !req.headers().contains_key(CLIENT_HEADER) {
        return (
            StatusCode::FORBIDDEN,
            Json(json!({ "error": "interdit", "message": "requête refusée : client non reconnu" })),
        )
            .into_response();
    }
    next.run(req).await
}

async fn health(State(state): State<AppState>) -> Response {
    let db = sqlx::query_scalar::<_, i32>("SELECT 1").fetch_one(&state.pool).await.is_ok();
    let status = if db { StatusCode::OK } else { StatusCode::SERVICE_UNAVAILABLE };
    (
        status,
        Json(json!({
            "status": if db { "ok" } else { "base indisponible" },
            "version": crate::VERSION,
            "mail_mode": state.config.mail_mode.as_str(),
        })),
    )
        .into_response()
}

async fn not_found() -> Response {
    (StatusCode::NOT_FOUND, Json(json!({ "error": "introuvable", "message": "route inconnue" }))).into_response()
}

pub fn api_router(state: &AppState) -> Router<AppState> {
    Router::new()
        .route("/health", get(health))
        .merge(auth::routes())
        .merge(admin::routes())
        .merge(employees::routes())
        .merge(cases::routes())
        .merge(tasks::routes())
        .merge(trainings::routes())
        .merge(assets::routes())
        .merge(movements::routes(state.config.max_upload_bytes))
        .merge(imports::routes(state.config.max_upload_bytes))
        .fallback(not_found)
        .layer(middleware::from_fn(require_client_header))
}
