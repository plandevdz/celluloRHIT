//! Journal d'audit et outbox des notifications, écrits dans la même
//! transaction que la mutation métier.

use serde_json::Value;
use sqlx::PgConnection;
use uuid::Uuid;

use crate::auth::CurrentUser;
use crate::error::AppResult;

#[derive(Debug, Default)]
pub struct Audit<'a> {
    pub action: &'a str,
    pub entity: &'a str,
    pub entity_id: Option<Uuid>,
    pub case_id: Option<Uuid>,
    pub employee_id: Option<Uuid>,
    pub data: Option<Value>,
}

pub async fn record(conn: &mut PgConnection, actor: &CurrentUser, a: Audit<'_>) -> AppResult<()> {
    sqlx::query(
        "INSERT INTO audit_events (actor_id, action, entity, entity_id, case_id, employee_id, correlation_id, data)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
    )
    .bind(actor.id)
    .bind(a.action)
    .bind(a.entity)
    .bind(a.entity_id)
    .bind(a.case_id)
    .bind(a.employee_id)
    .bind(actor.correlation_id)
    .bind(a.data.unwrap_or_else(|| serde_json::json!({})))
    .execute(conn)
    .await?;
    Ok(())
}

/// Équipes sans aucun utilisateur actif (l'avis ne pourra pas être distribué).
pub async fn roles_without_recipient(conn: &mut PgConnection, roles: &[domain::Role]) -> AppResult<Vec<String>> {
    let codes: Vec<&str> = roles.iter().map(|r| r.as_str()).collect();
    let missing: Vec<String> = sqlx::query_scalar(
        "SELECT r FROM unnest($1::text[]) r
          WHERE NOT EXISTS (SELECT 1 FROM user_roles ur JOIN users u ON u.id = ur.user_id WHERE ur.role = r AND u.active)",
    )
    .bind(&codes)
    .fetch_all(conn)
    .await?;
    Ok(missing)
}

/// Avis à envoyer à des équipes (rôles) et/ou à des utilisateurs précis.
/// Un même avis n'est remis qu'une fois par adresse.
pub struct Notice<'a> {
    pub event_type: &'a str,
    pub case_id: Option<Uuid>,
    pub target_roles: Vec<domain::Role>,
    pub target_user_ids: Vec<Uuid>,
    pub subject: String,
    pub body: String,
    pub link_path: Option<String>,
}

pub async fn enqueue(conn: &mut PgConnection, actor: &CurrentUser, n: Notice<'_>) -> AppResult<()> {
    if n.target_roles.is_empty() && n.target_user_ids.is_empty() {
        return Ok(());
    }
    let roles: Vec<&str> = n.target_roles.iter().map(|r| r.as_str()).collect();
    sqlx::query(
        "INSERT INTO outbox (event_type, case_id, target_roles, target_user_ids, subject, body, link_path, correlation_id)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
    )
    .bind(n.event_type)
    .bind(n.case_id)
    .bind(&roles)
    .bind(&n.target_user_ids)
    .bind(n.subject)
    .bind(n.body)
    .bind(n.link_path)
    .bind(actor.correlation_id)
    .execute(conn)
    .await?;
    Ok(())
}
