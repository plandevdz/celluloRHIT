use axum::extract::{Path, Query, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::{DateTime, NaiveDate, Utc};
use domain::needs::Evidence;
use domain::tasks::transition;
use domain::{NeedCategory, Role, TaskAction, TaskResolution, TaskStatus};
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
        .route("/tasks", get(list))
        .route("/tasks/{id}/claim", post(claim))
        .route("/tasks/{id}/release", post(release))
        .route("/tasks/{id}/block", post(block))
        .route("/tasks/{id}/unblock", post(unblock))
        .route("/tasks/{id}/complete", post(complete))
        .route("/tasks/{id}/reassign", post(reassign))
}

#[derive(Deserialize)]
struct ListQuery {
    /// `mine` : mes tâches ; `team` (défaut) : files de mes équipes.
    scope: Option<String>,
    role: Option<Role>,
    /// `open` (défaut), `done` ou un statut précis.
    status: Option<String>,
}

#[derive(Serialize, FromRow)]
struct TaskRow {
    id: Uuid,
    title: String,
    role: String,
    status: String,
    owner_id: Option<Uuid>,
    owner_name: Option<String>,
    due_date: Option<NaiveDate>,
    blocked_reason: Option<String>,
    claimed_at: Option<DateTime<Utc>>,
    completed_at: Option<DateTime<Utc>>,
    version: i32,
    case_id: Uuid,
    case_reference: String,
    case_kind: String,
    employee_id: Uuid,
    employee_name: String,
    site_name: Option<String>,
    need_category: Option<String>,
    need_preference: Option<String>,
    training_session_id: Option<Uuid>,
    resolution: Option<String>,
}

/// « Mon travail » : files cumulées de toutes mes habilitations, sans
/// doublon (une tâche n'appartient qu'à une équipe).
async fn list(State(state): State<AppState>, user: CurrentUser, Query(q): Query<ListQuery>) -> AppResult<Json<Vec<TaskRow>>> {
    let mut roles: Vec<&str> = user.roles.iter().map(|r| r.as_str()).collect();
    if let Some(r) = q.role {
        if !user.has(r) && !user.has(Role::Admin) {
            return Err(AppError::forbidden());
        }
        roles = vec![r.as_str()];
    } else if user.has(Role::Admin) && q.scope.as_deref() == Some("all") {
        roles = Role::ALL.iter().map(|r| r.as_str()).collect();
    }
    let mine = q.scope.as_deref() == Some("mine");
    let status_filter: Vec<&str> = match q.status.as_deref() {
        None | Some("open") => vec!["a_faire", "en_cours", "bloque"],
        Some("done") => vec!["termine", "annule"],
        Some(s) => vec![s],
    };
    let rows = sqlx::query_as::<_, TaskRow>(
        "SELECT t.id, t.title, t.role, t.status, t.owner_id, o.display_name AS owner_name, t.due_date,
                t.blocked_reason, t.claimed_at, t.completed_at, t.version,
                c.id AS case_id, c.reference AS case_reference, c.kind AS case_kind,
                e.id AS employee_id, e.first_name || ' ' || e.last_name AS employee_name, s.name AS site_name,
                n.category AS need_category, n.preference AS need_preference, t.training_session_id, t.resolution
           FROM tasks t
           JOIN cases c ON c.id = t.case_id
           JOIN employees e ON e.id = c.employee_id
           LEFT JOIN employee_positions p ON p.employee_id = e.id AND p.valid_to IS NULL
           LEFT JOIN sites s ON s.id = p.site_id
           LEFT JOIN users o ON o.id = t.owner_id
           LEFT JOIN case_needs n ON n.id = t.need_id
          WHERE t.status = ANY($1)
            AND (CASE WHEN $2 THEN t.owner_id = $3 ELSE (t.role = ANY($4) OR t.owner_id = $3) END)
          ORDER BY (t.status = 'a_faire') DESC, t.due_date NULLS LAST, t.created_at
          LIMIT 500",
    )
    .bind(&status_filter)
    .bind(mine)
    .bind(user.id)
    .bind(&roles)
    .fetch_all(&state.pool)
    .await?;
    Ok(Json(rows))
}

/// Prise en charge atomique : un seul gagnant, 409 avec le détenteur réel
/// pour les suivants.
async fn claim(State(state): State<AppState>, user: CurrentUser, Path(id): Path<Uuid>) -> AppResult<Json<Value>> {
    let roles: Vec<&str> = user.roles.iter().map(|r| r.as_str()).collect();
    let mut tx = state.pool.begin().await?;
    let won: Option<(Uuid, i32)> = sqlx::query_as(
        "UPDATE tasks SET status = 'en_cours', owner_id = $2, claimed_at = now(), version = version + 1, updated_at = now()
          WHERE id = $1 AND status = 'a_faire' AND role = ANY($3)
         RETURNING case_id, version",
    )
    .bind(id)
    .bind(user.id)
    .bind(&roles)
    .fetch_optional(&mut *tx)
    .await?;
    let Some((case_id, version)) = won else {
        let row: Option<(String, String, Option<String>)> = sqlx::query_as(
            "SELECT t.role, t.status, u.display_name FROM tasks t LEFT JOIN users u ON u.id = t.owner_id WHERE t.id = $1",
        )
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?;
        let (role, status, owner) = row.ok_or_else(|| AppError::not_found("tâche"))?;
        if !roles.contains(&role.as_str()) {
            return Err(AppError::forbidden());
        }
        return Err(AppError::Conflict {
            message: match &owner {
                Some(o) => format!("déjà prise en charge par {o}"),
                None => format!("tâche non disponible (statut {status})"),
            },
            detail: Some(json!({ "owner": owner, "status": status })),
        });
    };
    audit::record(&mut tx, &user, Audit { action: "tache_prise", entity: "task", entity_id: Some(id), case_id: Some(case_id), ..Default::default() }).await?;
    tx.commit().await?;
    Ok(Json(json!({ "status": "en_cours", "version": version })))
}

struct Locked {
    case_id: Uuid,
    role: String,
    status: TaskStatus,
    owner_id: Option<Uuid>,
    need_category: Option<String>,
    need_quantity: Option<i32>,
    training_session_id: Option<Uuid>,
    employee_id: Uuid,
}

type LockedRow = (Uuid, String, String, Option<Uuid>, Option<String>, Option<i32>, Option<Uuid>, Uuid);

async fn lock_task(conn: &mut sqlx::PgConnection, id: Uuid) -> AppResult<Locked> {
    let row: Option<LockedRow> = sqlx::query_as(
        "SELECT t.case_id, t.role, t.status, t.owner_id, n.category, n.quantity, t.training_session_id, c.employee_id
           FROM tasks t JOIN cases c ON c.id = t.case_id LEFT JOIN case_needs n ON n.id = t.need_id
          WHERE t.id = $1 FOR UPDATE OF t",
    )
    .bind(id)
    .fetch_optional(conn)
    .await?;
    let (case_id, role, status, owner_id, need_category, need_quantity, training_session_id, employee_id) =
        row.ok_or_else(|| AppError::not_found("tâche"))?;
    Ok(Locked {
        case_id,
        role,
        status: TaskStatus::parse(&status).ok_or_else(|| anyhow::anyhow!("statut de tâche inconnu"))?,
        owner_id,
        need_category,
        need_quantity,
        training_session_id,
        employee_id,
    })
}

async fn act(
    state: &AppState,
    user: &CurrentUser,
    id: Uuid,
    action: TaskAction,
    reason: Option<String>,
) -> AppResult<Json<Value>> {
    let mut tx = state.pool.begin().await?;
    let t = lock_task(&mut tx, id).await?;
    let in_team = user.roles.iter().any(|r| r.as_str() == t.role);
    if !in_team && t.owner_id != Some(user.id) {
        return Err(AppError::forbidden());
    }
    let is_owner = t.owner_id == Some(user.id);
    let next = transition(t.status, action, is_owner)?;
    let reason = reason.as_deref().and_then(domain::text::clean);

    if action == TaskAction::Block && reason.is_none() {
        return Err(AppError::invalid("reason", "indiquez la raison de l'attente"));
    }
    if action == TaskAction::Complete {
        return Err(AppError::invalid("action", "utilisez la commande de fin de tâche avec sa preuve"));
    }

    let owner = if next == TaskStatus::AFaire { None } else { t.owner_id };
    sqlx::query(
        "UPDATE tasks SET status = $2, owner_id = $3,
                blocked_reason = CASE WHEN $2 = 'bloque' THEN $4 ELSE NULL END,
                claimed_at = CASE WHEN $2 = 'a_faire' THEN NULL ELSE claimed_at END,
                version = version + 1, updated_at = now()
          WHERE id = $1",
    )
    .bind(id)
    .bind(next.as_str())
    .bind(owner)
    .bind(&reason)
    .execute(&mut *tx)
    .await?;
    let action_name = match action {
        TaskAction::Claim => "tache_prise",
        TaskAction::Release => "tache_liberee",
        TaskAction::Block => "tache_en_attente",
        TaskAction::Unblock => "tache_reprise",
        TaskAction::Complete => unreachable!(),
    };
    audit::record(
        &mut tx,
        user,
        Audit {
            action: action_name,
            entity: "task",
            entity_id: Some(id),
            case_id: Some(t.case_id),
            data: reason.as_ref().map(|r| json!({ "raison": r })),
            ..Default::default()
        },
    )
    .await?;
    tx.commit().await?;
    Ok(Json(json!({ "status": next })))
}

#[derive(Deserialize, Default)]
struct ReasonInput {
    reason: Option<String>,
}

#[derive(Deserialize, Default)]
struct CompleteInput {
    /// remis | deja_detenu | exception | realise ; déduit si absent.
    resolution: Option<TaskResolution>,
    /// Pour « déjà détenu » : affectations actives qui couvrent le besoin.
    #[serde(default)]
    assignment_ids: Vec<Uuid>,
    note: Option<String>,
}

async fn release(State(state): State<AppState>, user: CurrentUser, Path(id): Path<Uuid>) -> AppResult<Json<Value>> {
    act(&state, &user, id, TaskAction::Release, None).await
}

async fn block(State(state): State<AppState>, user: CurrentUser, Path(id): Path<Uuid>, Json(i): Json<ReasonInput>) -> AppResult<Json<Value>> {
    act(&state, &user, id, TaskAction::Block, i.reason).await
}

async fn unblock(State(state): State<AppState>, user: CurrentUser, Path(id): Path<Uuid>) -> AppResult<Json<Value>> {
    act(&state, &user, id, TaskAction::Unblock, None).await
}

/// Fin de tâche avec sa preuve. Matériel : « remis » (lignes de remise de la
/// bonne catégorie, en quantité suffisante), « déjà détenu » (affectation
/// active vérifiable) ou « exception motivée » (jamais comptée comme
/// matériel remis). Accès : compte consigné ou exception motivée.
async fn complete(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
    body: Option<Json<CompleteInput>>,
) -> AppResult<Json<Value>> {
    let input = body.map(|Json(i)| i).unwrap_or_default();
    let note = input.note.as_deref().and_then(domain::text::clean);
    let mut tx = state.pool.begin().await?;
    let t = lock_task(&mut tx, id).await?;
    let in_team = user.roles.iter().any(|r| r.as_str() == t.role);
    if !in_team && t.owner_id != Some(user.id) {
        return Err(AppError::forbidden());
    }
    transition(t.status, TaskAction::Complete, t.owner_id == Some(user.id))?;
    if t.training_session_id.is_some() {
        return Err(AppError::invalid("task", "une formation se termine depuis sa session (réalisée ou dispensée)"));
    }
    let category = t.need_category.as_deref().and_then(NeedCategory::parse);
    let (resolution, evidence) = resolve(&mut tx, &t, category, &input, note.as_deref()).await?;

    sqlx::query(
        "UPDATE tasks SET status = 'termine', blocked_reason = NULL, done_note = $2,
                resolution = $3, resolution_evidence = $4,
                completed_at = now(), completed_by = $5, version = version + 1, updated_at = now()
          WHERE id = $1",
    )
    .bind(id)
    .bind(&note)
    .bind(resolution.as_str())
    .bind(&evidence)
    .bind(user.id)
    .execute(&mut *tx)
    .await?;
    audit::record(
        &mut tx,
        &user,
        Audit {
            action: if resolution == TaskResolution::Exception { "tache_soldee_par_exception" } else { "tache_terminee" },
            entity: "task",
            entity_id: Some(id),
            case_id: Some(t.case_id),
            data: Some(json!({ "resolution": resolution, "preuve": evidence, "observation": note })),
            ..Default::default()
        },
    )
    .await?;
    tx.commit().await?;
    Ok(Json(json!({ "status": "termine", "resolution": resolution, "evidence": evidence })))
}

async fn resolve(
    conn: &mut sqlx::PgConnection,
    t: &Locked,
    category: Option<NeedCategory>,
    input: &CompleteInput,
    note: Option<&str>,
) -> AppResult<(TaskResolution, Value)> {
    let evidence_kind = category.map(|c| c.evidence()).unwrap_or(Evidence::None);
    let wanted = input.resolution.unwrap_or(match evidence_kind {
        Evidence::Equipment => TaskResolution::Remis,
        _ => TaskResolution::Realise,
    });

    if wanted == TaskResolution::Exception {
        let motif = note.ok_or_else(|| AppError::invalid("note", "le motif de l'exception est obligatoire"))?;
        return Ok((TaskResolution::Exception, json!({ "motif": motif })));
    }

    match (evidence_kind, wanted) {
        (Evidence::Equipment, TaskResolution::Remis) => {
            let c = category.expect("catégorie matériel");
            let needed = t.need_quantity.unwrap_or(1);
            // Lignes réellement remises à ce salarié dans ce dossier.
            let lines: Vec<(Uuid, String, String, Option<String>, i32)> = sqlx::query_as(
                "SELECT ml.id, m.reference, ml.item_type, a.kind, ml.quantity
                   FROM movement_lines ml
                   JOIN movements m ON m.id = ml.movement_id
                   LEFT JOIN assets a ON a.id = ml.asset_id
                  WHERE m.kind = 'remise' AND m.case_id = $1 AND m.employee_id = $2
                  ORDER BY m.effective_at, ml.line_no",
            )
            .bind(t.case_id)
            .bind(t.employee_id)
            .fetch_all(&mut *conn)
            .await?;
            let matching: Vec<_> = lines.iter().filter(|l| c.matches_item(&l.2, l.3.as_deref())).collect();
            let delivered: i32 = matching.iter().map(|l| l.4).sum();
            if delivered < needed {
                return Err(AppError::invalid(
                    "resolution",
                    format!(
                        "{} : {delivered}/{needed} remis dans ce dossier. Enregistrez la remise, ou choisissez « Déjà détenu » ou « Exception motivée ».",
                        c.label()
                    ),
                ));
            }
            Ok((
                TaskResolution::Remis,
                json!({
                    "movement_line_ids": matching.iter().map(|l| l.0).collect::<Vec<_>>(),
                    "movements": matching.iter().map(|l| l.1.clone()).collect::<std::collections::BTreeSet<_>>(),
                    "quantite": delivered,
                }),
            ))
        }
        (Evidence::Equipment, TaskResolution::DejaDetenu) => {
            let c = category.expect("catégorie matériel");
            if input.assignment_ids.is_empty() {
                return Err(AppError::invalid("assignment_ids", "choisissez le bien déjà détenu par le salarié"));
            }
            let held: Vec<(Uuid, String, Option<String>, i32, String)> = sqlx::query_as(
                "SELECT asg.id, asg.item_type, a.kind, asg.open_quantity, m.reference
                   FROM assignments asg
                   JOIN movement_lines ml ON ml.id = asg.handover_line_id
                   JOIN movements m ON m.id = ml.movement_id
                   LEFT JOIN assets a ON a.id = asg.asset_id
                  WHERE asg.id = ANY($1) AND asg.employee_id = $2 AND asg.ended_at IS NULL",
            )
            .bind(&input.assignment_ids)
            .bind(t.employee_id)
            .fetch_all(&mut *conn)
            .await?;
            if held.len() != input.assignment_ids.len() || !held.iter().all(|h| c.matches_item(&h.1, h.2.as_deref())) {
                return Err(AppError::invalid(
                    "assignment_ids",
                    format!("affectation non vérifiable : le bien doit être un {} actuellement détenu par ce salarié", c.label()),
                ));
            }
            let total: i32 = held.iter().map(|h| h.3).sum();
            if total < t.need_quantity.unwrap_or(1) {
                return Err(AppError::invalid("assignment_ids", "quantité détenue insuffisante pour le besoin"));
            }
            Ok((
                TaskResolution::DejaDetenu,
                json!({
                    "assignment_ids": held.iter().map(|h| h.0).collect::<Vec<_>>(),
                    "remises_d_origine": held.iter().map(|h| h.4.clone()).collect::<Vec<_>>(),
                }),
            ))
        }
        (Evidence::Account, TaskResolution::Realise) => {
            let system = category.and_then(|c| c.account_system()).expect("système de compte");
            let account: Option<(Uuid, String, Option<String>)> = sqlx::query_as(
                "SELECT id, identifier, platform FROM employee_accounts
                  WHERE employee_id = $1 AND system = $2 AND status <> 'coupe' ORDER BY created_at DESC LIMIT 1",
            )
            .bind(t.employee_id)
            .bind(system)
            .fetch_optional(&mut *conn)
            .await?;
            let (account_id, identifier, platform) = account.ok_or_else(|| {
                AppError::invalid(
                    "resolution",
                    if system == "email" {
                        "consignez d'abord l'adresse réelle (cPanel ou Microsoft 365), ou choisissez « Exception motivée »"
                    } else {
                        "consignez d'abord l'identifiant réellement ouvert, ou choisissez « Exception motivée »"
                    },
                )
            })?;
            Ok((TaskResolution::Realise, json!({ "account_id": account_id, "identifiant": identifier, "plateforme": platform })))
        }
        (Evidence::None, TaskResolution::Realise) => Ok((TaskResolution::Realise, json!({}))),
        (_, other) => Err(AppError::invalid(
            "resolution",
            format!("« {} » ne s'applique pas à cette tâche", other.label()),
        )),
    }
}

#[derive(Deserialize)]
struct ReassignInput {
    owner_id: Option<Uuid>,
    reason: String,
}

/// Correction d'affectation par l'ADMIN, motivée et auditée.
async fn reassign(State(state): State<AppState>, user: CurrentUser, Path(id): Path<Uuid>, Json(i): Json<ReassignInput>) -> AppResult<Json<Value>> {
    user.require(&[Role::Admin])?;
    let reason = domain::text::clean(&i.reason).ok_or_else(|| AppError::invalid("reason", "motif obligatoire"))?;
    let mut tx = state.pool.begin().await?;
    let t = lock_task(&mut tx, id).await?;
    if !t.status.is_open() {
        return Err(AppError::conflict("tâche terminée ou annulée"));
    }
    if let Some(owner) = i.owner_id {
        let ok: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM users u JOIN user_roles r ON r.user_id = u.id WHERE u.id = $1 AND u.active AND r.role = $2)",
        )
        .bind(owner)
        .bind(&t.role)
        .fetch_one(&mut *tx)
        .await?;
        if !ok {
            return Err(AppError::invalid("owner_id", "la personne doit être active et habilitée pour cette équipe"));
        }
    }
    let status = if i.owner_id.is_some() { "en_cours" } else { "a_faire" };
    sqlx::query(
        "UPDATE tasks SET owner_id = $2, status = $3, claimed_at = CASE WHEN $2::uuid IS NULL THEN NULL ELSE now() END,
                blocked_reason = NULL, version = version + 1, updated_at = now() WHERE id = $1",
    )
    .bind(id)
    .bind(i.owner_id)
    .bind(status)
    .execute(&mut *tx)
    .await?;
    audit::record(&mut tx, &user, Audit { action: "tache_reaffectee", entity: "task", entity_id: Some(id), case_id: Some(t.case_id), data: Some(json!({ "owner": i.owner_id, "motif": reason })), ..Default::default() }).await?;
    tx.commit().await?;
    Ok(Json(json!({ "status": status })))
}
