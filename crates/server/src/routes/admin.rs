use axum::extract::{Path, Query, State};
use axum::routing::{get, patch, post};
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use domain::Role;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::FromRow;
use uuid::Uuid;

use crate::audit::{self, Audit};
use crate::auth::{self, CurrentUser};
use crate::error::{AppError, AppResult};
use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/admin/users", get(list_users).post(create_user))
        .route("/admin/users/{id}", patch(update_user))
        .route("/admin/users/{id}/activation", post(reset_activation))
        .route("/sites", get(list_sites))
        .route("/admin/sites", post(create_site))
        .route("/admin/sites/{id}/locations", post(create_location))
        .route("/admin/notifications", get(list_notifications))
        .route("/users/directory", get(directory))
}

const ACTIVATION_HOURS: i32 = 72;

#[derive(Serialize, FromRow)]
struct UserRow {
    id: Uuid,
    email: String,
    display_name: String,
    site_id: Option<Uuid>,
    active: bool,
    activated: bool,
    roles: Vec<String>,
    created_at: DateTime<Utc>,
}

async fn list_users(State(state): State<AppState>, user: CurrentUser) -> AppResult<Json<Vec<UserRow>>> {
    user.require(&[Role::Admin])?;
    let rows = sqlx::query_as::<_, UserRow>(
        "SELECT u.id, u.email, u.display_name, u.site_id, u.active, u.password_hash IS NOT NULL AS activated,
                coalesce(array_agg(r.role ORDER BY r.role) FILTER (WHERE r.role IS NOT NULL), '{}') AS roles,
                u.created_at
           FROM users u LEFT JOIN user_roles r ON r.user_id = u.id
          GROUP BY u.id ORDER BY u.display_name",
    )
    .fetch_all(&state.pool)
    .await?;
    Ok(Json(rows))
}

#[derive(Deserialize)]
struct CreateUser {
    email: String,
    display_name: String,
    roles: Vec<Role>,
    site_id: Option<Uuid>,
}

fn activation_link(state: &AppState, token: &str) -> String {
    format!("{}/activation?token={}", state.config.public_url.trim_end_matches('/'), token)
}

/// Crée un compte nominatif. Le lien d'activation à usage unique est rendu
/// une seule fois à l'administrateur ; seul son hachage est conservé.
async fn create_user(
    State(state): State<AppState>,
    user: CurrentUser,
    Json(input): Json<CreateUser>,
) -> AppResult<Json<Value>> {
    user.require(&[Role::Admin])?;
    let email = input.email.trim().to_lowercase();
    if !email.contains('@') || email.len() > 200 {
        return Err(AppError::invalid("email", "adresse email invalide"));
    }
    let name = domain::text::clean(&input.display_name)
        .ok_or_else(|| AppError::invalid("display_name", "nom obligatoire"))?;
    if input.roles.is_empty() {
        return Err(AppError::invalid("roles", "au moins un rôle"));
    }
    let (token, token_hash) = auth::new_token();
    let mut tx = state.pool.begin().await?;
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO users (email, display_name, site_id, activation_token_hash, activation_expires_at)
         VALUES ($1, $2, $3, $4, now() + make_interval(hours => $5)) RETURNING id",
    )
    .bind(&email)
    .bind(&name)
    .bind(input.site_id)
    .bind(&token_hash)
    .bind(ACTIVATION_HOURS)
    .fetch_one(&mut *tx)
    .await?;
    set_roles(&mut tx, id, &input.roles).await?;
    audit::record(
        &mut tx,
        &user,
        Audit {
            action: "creation_utilisateur",
            entity: "user",
            entity_id: Some(id),
            data: Some(json!({ "email": email, "roles": input.roles })),
            ..Default::default()
        },
    )
    .await?;
    tx.commit().await?;
    Ok(Json(json!({ "id": id, "activation_link": activation_link(&state, &token), "expires_in_hours": ACTIVATION_HOURS })))
}

async fn set_roles(conn: &mut sqlx::PgConnection, user_id: Uuid, roles: &[Role]) -> AppResult<()> {
    sqlx::query("DELETE FROM user_roles WHERE user_id = $1").bind(user_id).execute(&mut *conn).await?;
    for role in roles {
        sqlx::query("INSERT INTO user_roles (user_id, role) VALUES ($1, $2) ON CONFLICT DO NOTHING")
            .bind(user_id)
            .bind(role.as_str())
            .execute(&mut *conn)
            .await?;
    }
    Ok(())
}

#[derive(Deserialize)]
struct UpdateUser {
    display_name: Option<String>,
    active: Option<bool>,
    roles: Option<Vec<Role>>,
    site_id: Option<Option<Uuid>>,
}

async fn update_user(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
    Json(input): Json<UpdateUser>,
) -> AppResult<Json<Value>> {
    user.require(&[Role::Admin])?;
    if id == user.id {
        if input.active == Some(false) {
            return Err(AppError::invalid("active", "vous ne pouvez pas désactiver votre propre compte"));
        }
        if let Some(roles) = &input.roles {
            if !roles.contains(&Role::Admin) {
                return Err(AppError::invalid("roles", "vous ne pouvez pas retirer votre propre rôle ADMIN"));
            }
        }
    }
    let mut tx = state.pool.begin().await?;
    let exists: Option<Uuid> = sqlx::query_scalar("SELECT id FROM users WHERE id = $1 FOR UPDATE")
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?;
    exists.ok_or_else(|| AppError::not_found("utilisateur"))?;
    if let Some(name) = &input.display_name {
        let name = domain::text::clean(name).ok_or_else(|| AppError::invalid("display_name", "nom obligatoire"))?;
        sqlx::query("UPDATE users SET display_name = $2, updated_at = now() WHERE id = $1")
            .bind(id)
            .bind(name)
            .execute(&mut *tx)
            .await?;
    }
    if let Some(site) = input.site_id {
        sqlx::query("UPDATE users SET site_id = $2, updated_at = now() WHERE id = $1")
            .bind(id)
            .bind(site)
            .execute(&mut *tx)
            .await?;
    }
    if let Some(active) = input.active {
        sqlx::query("UPDATE users SET active = $2, updated_at = now() WHERE id = $1")
            .bind(id)
            .bind(active)
            .execute(&mut *tx)
            .await?;
        if !active {
            // Désactivation : révocation immédiate des sessions.
            sqlx::query("UPDATE sessions SET revoked_at = now() WHERE user_id = $1 AND revoked_at IS NULL")
                .bind(id)
                .execute(&mut *tx)
                .await?;
        }
    }
    if let Some(roles) = &input.roles {
        if roles.is_empty() {
            return Err(AppError::invalid("roles", "au moins un rôle"));
        }
        set_roles(&mut tx, id, roles).await?;
    }
    audit::record(
        &mut tx,
        &user,
        Audit {
            action: "modification_utilisateur",
            entity: "user",
            entity_id: Some(id),
            data: Some(json!({ "active": input.active, "roles": input.roles })),
            ..Default::default()
        },
    )
    .await?;
    tx.commit().await?;
    Ok(Json(json!({ "ok": true })))
}

/// Réinitialisation auditée : nouveau lien à usage unique, sessions révoquées.
async fn reset_activation(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
) -> AppResult<Json<Value>> {
    user.require(&[Role::Admin])?;
    let (token, token_hash) = auth::new_token();
    let mut tx = state.pool.begin().await?;
    let n = sqlx::query(
        "UPDATE users SET activation_token_hash = $2, activation_expires_at = now() + make_interval(hours => $3),
                password_hash = NULL, updated_at = now()
          WHERE id = $1",
    )
    .bind(id)
    .bind(&token_hash)
    .bind(ACTIVATION_HOURS)
    .execute(&mut *tx)
    .await?
    .rows_affected();
    if n == 0 {
        return Err(AppError::not_found("utilisateur"));
    }
    sqlx::query("UPDATE sessions SET revoked_at = now() WHERE user_id = $1 AND revoked_at IS NULL")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    audit::record(&mut tx, &user, Audit { action: "reinitialisation_mot_de_passe", entity: "user", entity_id: Some(id), ..Default::default() }).await?;
    tx.commit().await?;
    Ok(Json(json!({ "activation_link": activation_link(&state, &token), "expires_in_hours": ACTIVATION_HOURS })))
}

#[derive(Serialize, FromRow)]
struct SiteRow {
    id: Uuid,
    code: String,
    name: String,
    active: bool,
    locations: Value,
}

async fn list_sites(State(state): State<AppState>, _user: CurrentUser) -> AppResult<Json<Vec<SiteRow>>> {
    let rows = sqlx::query_as::<_, SiteRow>(
        "SELECT s.id, s.code, s.name, s.active,
                coalesce(jsonb_agg(jsonb_build_object('id', l.id, 'name', l.name) ORDER BY l.name)
                         FILTER (WHERE l.id IS NOT NULL), '[]') AS locations
           FROM sites s LEFT JOIN locations l ON l.site_id = s.id AND l.active
          GROUP BY s.id ORDER BY s.name",
    )
    .fetch_all(&state.pool)
    .await?;
    Ok(Json(rows))
}

#[derive(Deserialize)]
struct CreateSite {
    code: String,
    name: String,
}

async fn create_site(State(state): State<AppState>, user: CurrentUser, Json(input): Json<CreateSite>) -> AppResult<Json<Value>> {
    user.require(&[Role::Admin])?;
    let code = domain::text::clean(&input.code).ok_or_else(|| AppError::invalid("code", "code obligatoire"))?.to_uppercase();
    let name = domain::text::clean(&input.name).ok_or_else(|| AppError::invalid("name", "nom obligatoire"))?;
    let mut tx = state.pool.begin().await?;
    let id: Uuid = sqlx::query_scalar("INSERT INTO sites (code, name) VALUES ($1, $2) RETURNING id")
        .bind(&code)
        .bind(&name)
        .fetch_one(&mut *tx)
        .await?;
    audit::record(&mut tx, &user, Audit { action: "creation_site", entity: "site", entity_id: Some(id), data: Some(json!({ "code": code })), ..Default::default() }).await?;
    tx.commit().await?;
    Ok(Json(json!({ "id": id })))
}

#[derive(Deserialize)]
struct CreateLocation {
    name: String,
}

async fn create_location(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(site_id): Path<Uuid>,
    Json(input): Json<CreateLocation>,
) -> AppResult<Json<Value>> {
    user.require(&[Role::Admin, Role::It])?;
    let name = domain::text::clean(&input.name).ok_or_else(|| AppError::invalid("name", "nom obligatoire"))?;
    let mut tx = state.pool.begin().await?;
    let id: Uuid = sqlx::query_scalar("INSERT INTO locations (site_id, name) VALUES ($1, $2) RETURNING id")
        .bind(site_id)
        .bind(&name)
        .fetch_one(&mut *tx)
        .await?;
    audit::record(&mut tx, &user, Audit { action: "creation_local", entity: "location", entity_id: Some(id), ..Default::default() }).await?;
    tx.commit().await?;
    Ok(Json(json!({ "id": id })))
}

#[derive(Deserialize)]
struct NotifQuery {
    status: Option<String>,
}

#[derive(Serialize, FromRow)]
struct NotificationRow {
    outbox_id: Uuid,
    event_type: String,
    case_reference: Option<String>,
    target: String,
    subject: String,
    outbox_status: String,
    outbox_error: Option<String>,
    missing_roles: Vec<String>,
    created_at: DateTime<Utc>,
    deliveries: Value,
}

/// Console Admin : avis émis, destinataires, dernier résultat, essais.
async fn list_notifications(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(q): Query<NotifQuery>,
) -> AppResult<Json<Vec<NotificationRow>>> {
    user.require(&[Role::Admin])?;
    let rows = sqlx::query_as::<_, NotificationRow>(
        "SELECT o.id AS outbox_id, o.event_type, c.reference AS case_reference,
                array_to_string(o.target_roles || (SELECT coalesce(array_agg(u.display_name), '{}') FROM users u
                                                    WHERE u.id = ANY(o.target_user_ids)), ', ') AS target,
                o.subject, o.status AS outbox_status, o.last_error AS outbox_error, o.missing_roles, o.created_at,
                coalesce(jsonb_agg(jsonb_build_object(
                    'recipient', d.recipient_email, 'sent_to', d.sent_to, 'status', d.status,
                    'attempts', d.attempts, 'last_result', d.last_result, 'last_attempt_at', d.last_attempt_at)
                    ORDER BY d.recipient_email) FILTER (WHERE d.id IS NOT NULL), '[]') AS deliveries
           FROM outbox o
           LEFT JOIN cases c ON c.id = o.case_id
           LEFT JOIN deliveries d ON d.outbox_id = o.id
          WHERE ($1::text IS NULL OR o.status = $1 OR d.status = $1
                 OR ($1 = 'anomalie' AND (cardinality(o.missing_roles) > 0 OR o.status = 'erreur')))
          GROUP BY o.id, c.reference
          ORDER BY o.created_at DESC
          LIMIT 200",
    )
    .bind(q.status)
    .fetch_all(&state.pool)
    .await?;
    Ok(Json(rows))
}

#[derive(Deserialize)]
struct DirectoryQuery {
    role: Option<Role>,
}

#[derive(Serialize, FromRow)]
struct DirectoryRow {
    id: Uuid,
    display_name: String,
}

/// Annuaire minimal (nom seulement) pour désigner un intervenant.
async fn directory(
    State(state): State<AppState>,
    _user: CurrentUser,
    Query(q): Query<DirectoryQuery>,
) -> AppResult<Json<Vec<DirectoryRow>>> {
    let rows = sqlx::query_as::<_, DirectoryRow>(
        "SELECT DISTINCT u.id, u.display_name FROM users u
           JOIN user_roles r ON r.user_id = u.id
          WHERE u.active AND ($1::text IS NULL OR r.role = $1)
          ORDER BY u.display_name",
    )
    .bind(q.role.map(|r| r.as_str()))
    .fetch_all(&state.pool)
    .await?;
    Ok(Json(rows))
}
