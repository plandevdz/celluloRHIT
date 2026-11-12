use axum::extract::{Path, Query, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::NaiveDate;
use domain::Role;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::FromRow;
use uuid::Uuid;

use crate::audit::{self, Audit};
use crate::auth::CurrentUser;
use crate::error::{AppError, AppResult};
use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/trainings/catalog", get(catalog).post(add_catalog))
        .route("/trainings/sessions", get(sessions))
        .route("/trainings/sessions/{id}", post(update_session))
}

#[derive(Serialize, FromRow)]
struct CatalogRow {
    id: Uuid,
    name: String,
    active: bool,
}

async fn catalog(State(state): State<AppState>, _user: CurrentUser) -> AppResult<Json<Vec<CatalogRow>>> {
    let rows = sqlx::query_as::<_, CatalogRow>("SELECT id, name, active FROM training_catalog ORDER BY name")
        .fetch_all(&state.pool)
        .await?;
    Ok(Json(rows))
}

#[derive(Deserialize)]
struct CatalogInput {
    name: String,
}

async fn add_catalog(State(state): State<AppState>, user: CurrentUser, Json(i): Json<CatalogInput>) -> AppResult<Json<Value>> {
    user.require(&[Role::Formation, Role::Admin])?;
    let name = domain::text::clean(&i.name).ok_or_else(|| AppError::invalid("name", "intitulé obligatoire"))?;
    let mut tx = state.pool.begin().await?;
    let id: Uuid = sqlx::query_scalar("INSERT INTO training_catalog (name) VALUES ($1) RETURNING id")
        .bind(&name)
        .fetch_one(&mut *tx)
        .await?;
    audit::record(&mut tx, &user, Audit { action: "formation_catalogue", entity: "training_catalog", entity_id: Some(id), data: Some(json!({ "name": name })), ..Default::default() }).await?;
    tx.commit().await?;
    Ok(Json(json!({ "id": id })))
}

#[derive(Deserialize)]
struct SessionQuery {
    scope: Option<String>,
    status: Option<String>,
}

#[derive(Serialize, FromRow)]
struct SessionRow {
    id: Uuid,
    name: String,
    status: String,
    trainer_id: Option<Uuid>,
    trainer: Option<String>,
    planned_date: Option<NaiveDate>,
    done_date: Option<NaiveDate>,
    reason: Option<String>,
    result: Option<String>,
    employee_id: Uuid,
    employee_name: String,
    site_name: Option<String>,
    case_reference: Option<String>,
    due_date: Option<NaiveDate>,
}

async fn sessions(State(state): State<AppState>, user: CurrentUser, Query(q): Query<SessionQuery>) -> AppResult<Json<Vec<SessionRow>>> {
    user.require(&[Role::Formation, Role::Rh, Role::Admin])?;
    let mine = q.scope.as_deref() == Some("mine");
    let statuses: Vec<&str> = match q.status.as_deref() {
        None | Some("open") => vec!["a_planifier", "planifiee", "reportee"],
        Some("done") => vec!["realisee", "dispensee", "annulee"],
        Some(s) => vec![s],
    };
    let rows = sqlx::query_as::<_, SessionRow>(
        "SELECT s.id, tc.name, s.status, s.trainer_id, u.display_name AS trainer, s.planned_date, s.done_date,
                s.reason, s.result, e.id AS employee_id, e.first_name || ' ' || e.last_name AS employee_name,
                si.name AS site_name, c.reference AS case_reference, coalesce(c.due_date, c.effective_date) AS due_date
           FROM training_sessions s
           JOIN training_catalog tc ON tc.id = s.catalog_id
           JOIN employees e ON e.id = s.employee_id
           LEFT JOIN employee_positions p ON p.employee_id = e.id AND p.valid_to IS NULL
           LEFT JOIN sites si ON si.id = p.site_id
           LEFT JOIN users u ON u.id = s.trainer_id
           LEFT JOIN cases c ON c.id = s.case_id
          WHERE s.status = ANY($1) AND (NOT $2 OR s.trainer_id = $3)
          ORDER BY s.planned_date NULLS FIRST, due_date NULLS LAST",
    )
    .bind(&statuses)
    .bind(mine)
    .bind(user.id)
    .fetch_all(&state.pool)
    .await?;
    Ok(Json(rows))
}

#[derive(Deserialize)]
struct SessionUpdate {
    /// planifier | reporter | realiser | dispenser | annuler
    action: String,
    date: Option<NaiveDate>,
    reason: Option<String>,
    result: Option<String>,
    trainer_id: Option<Uuid>,
}

/// Une session vit indépendamment du reste du dossier : elle peut être
/// réalisée avant la remise du PC ou la création de l'adresse.
async fn update_session(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
    Json(i): Json<SessionUpdate>,
) -> AppResult<Json<Value>> {
    user.require(&[Role::Formation, Role::Admin, Role::Rh])?;
    let reason = i.reason.as_deref().and_then(domain::text::clean);
    let mut tx = state.pool.begin().await?;
    let row: Option<(String, Option<Uuid>, Option<Uuid>, Uuid)> = sqlx::query_as(
        "SELECT status, trainer_id, case_id, employee_id FROM training_sessions WHERE id = $1 FOR UPDATE",
    )
    .bind(id)
    .fetch_optional(&mut *tx)
    .await?;
    let (status, trainer_id, case_id, employee_id) = row.ok_or_else(|| AppError::not_found("session"))?;
    if ["realisee", "dispensee", "annulee"].contains(&status.as_str()) {
        return Err(AppError::conflict("session déjà soldée"));
    }
    let is_formation = user.has(Role::Formation);
    if i.action == "annuler" {
        user.require(domain::roles::HR_MANAGERS)?;
    } else if !is_formation && !user.has(Role::Admin) {
        return Err(AppError::forbidden());
    }
    if let Some(t) = trainer_id {
        if t != user.id && !user.has(Role::Admin) && i.action != "annuler" {
            return Err(AppError::conflict("session attribuée à un autre intervenant"));
        }
    }
    let new_trainer = match i.trainer_id {
        Some(t) => {
            let ok: bool = sqlx::query_scalar(
                "SELECT EXISTS (SELECT 1 FROM users u JOIN user_roles r ON r.user_id = u.id WHERE u.id = $1 AND u.active AND r.role = 'FORMATION')",
            )
            .bind(t)
            .fetch_one(&mut *tx)
            .await?;
            if !ok {
                return Err(AppError::invalid("trainer_id", "intervenant actif habilité FORMATION requis"));
            }
            Some(t)
        }
        None => trainer_id.or(is_formation.then_some(user.id)),
    };
    let (new_status, task_status) = match i.action.as_str() {
        "planifier" => {
            i.date.ok_or_else(|| AppError::invalid("date", "date prévue obligatoire"))?;
            ("planifiee", "en_cours")
        }
        "reporter" => {
            if reason.is_none() {
                return Err(AppError::invalid("reason", "motif du report obligatoire"));
            }
            ("reportee", "en_cours")
        }
        "realiser" => ("realisee", "termine"),
        "dispenser" => {
            if reason.is_none() {
                return Err(AppError::invalid("reason", "motif de dispense obligatoire"));
            }
            ("dispensee", "termine")
        }
        "annuler" => {
            if reason.is_none() {
                return Err(AppError::invalid("reason", "motif d'annulation obligatoire"));
            }
            ("annulee", "annule")
        }
        _ => return Err(AppError::invalid("action", "action inconnue")),
    };
    let done_date = (new_status == "realisee").then(|| i.date.unwrap_or_else(|| chrono::Utc::now().date_naive()));
    sqlx::query(
        "UPDATE training_sessions SET status = $2, trainer_id = $3,
                planned_date = CASE WHEN $2 IN ('planifiee', 'reportee') THEN coalesce($4, planned_date) ELSE planned_date END,
                done_date = $5, reason = coalesce($6, reason), result = coalesce($7, result), updated_at = now()
          WHERE id = $1",
    )
    .bind(id)
    .bind(new_status)
    .bind(new_trainer)
    .bind(i.date)
    .bind(done_date)
    .bind(&reason)
    .bind(i.result.as_deref().and_then(domain::text::clean))
    .execute(&mut *tx)
    .await?;
    let task_owner = if task_status == "annule" { None } else { new_trainer.or(Some(user.id)) };
    sqlx::query(
        "UPDATE tasks SET status = $2, owner_id = coalesce($3, owner_id),
                claimed_at = coalesce(claimed_at, now()),
                completed_at = CASE WHEN $2 = 'termine' THEN now() ELSE NULL END,
                completed_by = CASE WHEN $2 = 'termine' THEN $4 ELSE NULL END,
                version = version + 1, updated_at = now()
          WHERE training_session_id = $1 AND status NOT IN ('termine', 'annule')",
    )
    .bind(id)
    .bind(task_status)
    .bind(task_owner)
    .bind(user.id)
    .execute(&mut *tx)
    .await?;
    audit::record(
        &mut tx,
        &user,
        Audit {
            action: "formation_mise_a_jour",
            entity: "training_session",
            entity_id: Some(id),
            case_id,
            employee_id: Some(employee_id),
            data: Some(json!({ "action": i.action, "date": i.date, "motif": reason })),
        },
    )
    .await?;
    tx.commit().await?;
    Ok(Json(json!({ "status": new_status })))
}
