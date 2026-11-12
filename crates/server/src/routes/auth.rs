use axum::extract::State;
use axum::http::{header, HeaderMap, HeaderValue};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

use crate::audit::{self, Audit};
use crate::auth::{self, CurrentUser};
use crate::error::{AppError, AppResult};
use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/auth/login", post(login))
        .route("/auth/logout", post(logout))
        .route("/auth/me", get(me))
        .route("/auth/activate", post(activate))
}

const MAX_FAILED_LOGINS: i32 = 5;
const LOCK_MINUTES: i32 = 15;

#[derive(Serialize)]
pub struct Me {
    pub id: Uuid,
    pub email: String,
    pub display_name: String,
    pub roles: Vec<domain::Role>,
}

impl From<&CurrentUser> for Me {
    fn from(u: &CurrentUser) -> Self {
        Me { id: u.id, email: u.email.clone(), display_name: u.display_name.clone(), roles: u.roles.clone() }
    }
}

#[derive(Deserialize)]
struct LoginInput {
    email: String,
    password: String,
}

// Hachage factice pour que la durée de réponse ne révèle pas si un compte existe.
const DUMMY_HASH: &str = "$argon2id$v=19$m=19456,t=2,p=1$c29tZXNhbHRzb21lc2FsdA$4PdB0dAeDZf0bZr2Tj3UW8Q1pQ5JQyqH5mY0b8bJ0hE";

async fn login(State(state): State<AppState>, Json(input): Json<LoginInput>) -> AppResult<Response> {
    let email = input.email.trim().to_lowercase();
    let row: Option<(Uuid, String, String, bool, Option<String>, bool)> = sqlx::query_as(
        "SELECT id, email, display_name, active, password_hash,
                coalesce(locked_until > now(), false)
           FROM users WHERE lower(email) = $1",
    )
    .bind(&email)
    .fetch_optional(&state.pool)
    .await?;

    let Some((id, email, display_name, active, hash, locked)) = row else {
        auth::verify_password(input.password, DUMMY_HASH.into()).await;
        return Err(AppError::BadCredentials);
    };
    if locked {
        return Err(AppError::TooManyRequests(format!(
            "trop de tentatives : compte temporairement bloqué ({LOCK_MINUTES} min)"
        )));
    }
    let ok = match (active, hash) {
        (true, Some(h)) => auth::verify_password(input.password, h).await,
        _ => {
            auth::verify_password(input.password, DUMMY_HASH.into()).await;
            false
        }
    };
    if !ok {
        sqlx::query(
            "UPDATE users SET
                failed_logins = CASE WHEN failed_logins + 1 >= $2 THEN 0 ELSE failed_logins + 1 END,
                locked_until  = CASE WHEN failed_logins + 1 >= $2 THEN now() + make_interval(mins => $3) ELSE locked_until END
              WHERE id = $1",
        )
        .bind(id)
        .bind(MAX_FAILED_LOGINS)
        .bind(LOCK_MINUTES)
        .execute(&state.pool)
        .await?;
        return Err(AppError::BadCredentials);
    }

    let (token, token_hash) = auth::new_token();
    let max_hours = state.config.session_max_hours;
    let mut tx = state.pool.begin().await?;
    sqlx::query("UPDATE users SET failed_logins = 0, locked_until = NULL WHERE id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    sqlx::query(
        "INSERT INTO sessions (token_hash, user_id, expires_at) VALUES ($1, $2, now() + make_interval(hours => $3))",
    )
    .bind(&token_hash)
    .bind(id)
    .bind(max_hours as i32)
    .execute(&mut *tx)
    .await?;
    let roles = auth::load_roles(&state.pool, id).await?;
    let user = CurrentUser { id, email, display_name, roles, correlation_id: Uuid::new_v4() };
    audit::record(&mut tx, &user, Audit { action: "connexion", entity: "user", entity_id: Some(id), ..Default::default() }).await?;
    tx.commit().await?;

    let cookie = auth::session_cookie(&token, state.config.cookie_secure, max_hours * 3600);
    let mut resp = Json(Me::from(&user)).into_response();
    resp.headers_mut().insert(header::SET_COOKIE, HeaderValue::from_str(&cookie).map_err(anyhow::Error::from)?);
    Ok(resp)
}

async fn logout(State(state): State<AppState>, headers: HeaderMap) -> AppResult<Response> {
    let token = headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(';'))
        .filter_map(|kv| kv.trim().split_once('='))
        .find(|(k, _)| *k == auth::SESSION_COOKIE)
        .map(|(_, v)| v.to_string());
    if let Some(token) = token {
        sqlx::query("UPDATE sessions SET revoked_at = now() WHERE token_hash = $1 AND revoked_at IS NULL")
            .bind(auth::hash_token(&token))
            .execute(&state.pool)
            .await?;
    }
    let mut resp = Json(json!({ "ok": true })).into_response();
    resp.headers_mut().insert(
        header::SET_COOKIE,
        HeaderValue::from_str(&auth::clear_cookie(state.config.cookie_secure)).map_err(anyhow::Error::from)?,
    );
    Ok(resp)
}

async fn me(user: CurrentUser) -> Json<Me> {
    Json(Me::from(&user))
}

#[derive(Deserialize)]
struct ActivateInput {
    token: String,
    password: String,
}

/// Activation initiale à usage unique (ou réinitialisation décidée par l'Admin).
async fn activate(State(state): State<AppState>, Json(input): Json<ActivateInput>) -> AppResult<Json<serde_json::Value>> {
    auth::check_password_policy(&input.password)?;
    let hash = auth::hash_token(input.token.trim());
    let mut tx = state.pool.begin().await?;
    let row: Option<(Uuid, String, String)> = sqlx::query_as(
        "SELECT id, email, display_name FROM users
          WHERE activation_token_hash = $1 AND activation_expires_at > now() AND active
          FOR UPDATE",
    )
    .bind(&hash)
    .fetch_optional(&mut *tx)
    .await?;
    let (id, email, display_name) =
        row.ok_or_else(|| AppError::invalid("token", "lien d'activation invalide ou expiré"))?;
    let password_hash = auth::hash_password(input.password).await?;
    sqlx::query(
        "UPDATE users SET password_hash = $2, activation_token_hash = NULL, activation_expires_at = NULL,
                failed_logins = 0, locked_until = NULL, updated_at = now()
          WHERE id = $1",
    )
    .bind(id)
    .bind(password_hash)
    .execute(&mut *tx)
    .await?;
    // Un nouveau mot de passe invalide les sessions existantes.
    sqlx::query("UPDATE sessions SET revoked_at = now() WHERE user_id = $1 AND revoked_at IS NULL")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    let user = CurrentUser { id, email, display_name, roles: vec![], correlation_id: Uuid::new_v4() };
    audit::record(&mut tx, &user, Audit { action: "activation_compte", entity: "user", entity_id: Some(id), ..Default::default() }).await?;
    tx.commit().await?;
    Ok(Json(json!({ "ok": true })))
}
