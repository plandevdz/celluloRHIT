//! Comptes nominatifs internes, mots de passe Argon2id et sessions serveur.

use argon2::password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::Argon2;
use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use base64::Engine;
use domain::Role;
use rand::RngCore;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::error::{AppError, AppResult};
use crate::state::AppState;

pub const SESSION_COOKIE: &str = "cellulo_session";
pub const MIN_PASSWORD_LEN: usize = 10;

/// Utilisateur authentifié pour la requête en cours.
#[derive(Debug, Clone)]
pub struct CurrentUser {
    pub id: Uuid,
    pub email: String,
    pub display_name: String,
    pub roles: Vec<Role>,
    /// Identifiant de corrélation porté par l'audit et les événements.
    pub correlation_id: Uuid,
}

impl CurrentUser {
    pub fn has(&self, role: Role) -> bool {
        self.roles.contains(&role)
    }

    pub fn has_any(&self, roles: &[Role]) -> bool {
        domain::roles::has_any(&self.roles, roles)
    }

    /// 403 si l'utilisateur n'a aucun des rôles demandés.
    pub fn require(&self, roles: &[Role]) -> AppResult<()> {
        if self.has_any(roles) {
            Ok(())
        } else {
            Err(AppError::forbidden())
        }
    }

    pub fn is_hr(&self) -> bool {
        self.has_any(domain::roles::HR_MANAGERS)
    }
}

pub async fn hash_password(password: String) -> anyhow::Result<String> {
    tokio::task::spawn_blocking(move || {
        let salt = SaltString::generate(&mut OsRng);
        Argon2::default()
            .hash_password(password.as_bytes(), &salt)
            .map(|h| h.to_string())
            .map_err(|e| anyhow::anyhow!("hachage du mot de passe : {e}"))
    })
    .await?
}

pub async fn verify_password(password: String, hash: String) -> bool {
    tokio::task::spawn_blocking(move || {
        PasswordHash::new(&hash)
            .map(|parsed| Argon2::default().verify_password(password.as_bytes(), &parsed).is_ok())
            .unwrap_or(false)
    })
    .await
    .unwrap_or(false)
}

pub fn check_password_policy(password: &str) -> AppResult<()> {
    if password.chars().count() < MIN_PASSWORD_LEN {
        return Err(AppError::invalid(
            "password",
            format!("au moins {MIN_PASSWORD_LEN} caractères"),
        ));
    }
    Ok(())
}

/// Jeton aléatoire (affiché ou mis en cookie) et son hachage (stocké).
pub fn new_token() -> (String, Vec<u8>) {
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    let token = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes);
    let hash = hash_token(&token);
    (token, hash)
}

pub fn hash_token(token: &str) -> Vec<u8> {
    Sha256::digest(token.as_bytes()).to_vec()
}

pub fn session_cookie(token: &str, secure: bool, max_age_secs: i64) -> String {
    format!(
        "{SESSION_COOKIE}={token}; Path=/; HttpOnly; SameSite=Strict; Max-Age={max_age_secs}{}",
        if secure { "; Secure" } else { "" }
    )
}

pub fn clear_cookie(secure: bool) -> String {
    session_cookie("", secure, 0)
}

pub fn cookie_value<'a>(parts: &'a Parts, name: &str) -> Option<&'a str> {
    parts
        .headers
        .get_all(axum::http::header::COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(';'))
        .filter_map(|kv| kv.trim().split_once('='))
        .find(|(k, _)| *k == name)
        .map(|(_, v)| v)
}

pub async fn load_roles(pool: &sqlx::PgPool, user_id: Uuid) -> AppResult<Vec<Role>> {
    let codes: Vec<String> =
        sqlx::query_scalar("SELECT role FROM user_roles WHERE user_id = $1 ORDER BY role")
            .bind(user_id)
            .fetch_all(pool)
            .await?;
    Ok(codes.iter().filter_map(|c| Role::parse(c)).collect())
}

impl FromRequestParts<AppState> for CurrentUser {
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Self::Rejection> {
        let token = cookie_value(parts, SESSION_COOKIE).ok_or(AppError::Unauthorized)?;
        if token.is_empty() {
            return Err(AppError::Unauthorized);
        }
        let hash = hash_token(token);
        let idle = state.config.session_idle_minutes;
        let row: Option<(Uuid, String, String)> = sqlx::query_as(
            "UPDATE sessions s SET last_seen_at = now()
               FROM users u
              WHERE s.token_hash = $1 AND u.id = s.user_id AND u.active
                AND s.revoked_at IS NULL AND s.expires_at > now()
                AND s.last_seen_at > now() - make_interval(mins => $2::int)
          RETURNING u.id, u.email, u.display_name",
        )
        .bind(&hash)
        .bind(idle as i32)
        .fetch_optional(&state.pool)
        .await?;
        let (id, email, display_name) = row.ok_or(AppError::Unauthorized)?;
        let roles = load_roles(&state.pool, id).await?;
        Ok(CurrentUser { id, email, display_name, roles, correlation_id: Uuid::new_v4() })
    }
}
