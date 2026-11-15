use axum::body::Body;
use axum::extract::{DefaultBodyLimit, Multipart, Path, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use domain::cases::format_movement_reference;
use domain::movements::{can_hand_over, check_return_quantity, AssetStatus};
use domain::Role;
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::PgConnection;
use uuid::Uuid;

use crate::audit::{self, Audit};
use crate::auth::CurrentUser;
use crate::documents;
use crate::error::{AppError, AppResult};
use crate::idempotency::{self, Begin};
use crate::state::AppState;

pub fn routes(max_upload: usize) -> Router<AppState> {
    Router::new()
        .route("/handovers", post(create_handover))
        .route("/returns", post(create_return))
        .route("/movements/{id}", get(movement_detail))
        .route("/employees/{id}/holdings", get(holdings))
        .route("/documents/{id}/original", get(download_original))
        .route(
            "/documents/{id}/scan",
            post(upload_scan).layer(DefaultBodyLimit::max(max_upload + 64 * 1024)),
        )
        .route("/documents/{id}/scan/{version}", get(download_scan))
}

const PARC: &[Role] = &[Role::It, Role::Admin];

// ------------------------------------------------------------------ lectures partagées

/// Biens encore détenus par le salarié, avec la remise d'origine.
pub async fn holdings_json(conn: &mut PgConnection, employee_id: Uuid) -> AppResult<Value> {
    let v: Value = sqlx::query_scalar(
        "SELECT coalesce(jsonb_agg(jsonb_build_object(
                    'assignment_id', a.id, 'origin_line_id', a.handover_line_id, 'item_type', a.item_type,
                    'asset_kind', (SELECT kind FROM assets WHERE id = a.asset_id),
                    'label', ml.label, 'identifier', ml.identifier, 'quantity', a.quantity,
                    'open_quantity', a.open_quantity, 'since', a.started_at,
                    'origin_reference', m.reference, 'case_reference', c.reference)
                    ORDER BY a.started_at, ml.line_no), '[]')
           FROM assignments a
           JOIN movement_lines ml ON ml.id = a.handover_line_id
           JOIN movements m ON m.id = ml.movement_id
           LEFT JOIN cases c ON c.id = m.case_id
          WHERE a.employee_id = $1 AND a.ended_at IS NULL",
    )
    .bind(employee_id)
    .fetch_one(conn)
    .await?;
    Ok(v)
}

pub async fn case_movements_json(conn: &mut PgConnection, case_id: Uuid) -> AppResult<Value> {
    let v: Value = sqlx::query_scalar(
        "SELECT coalesce(jsonb_agg(x ORDER BY x->>'effective_at'), '[]') FROM (
            SELECT jsonb_build_object(
                'id', m.id, 'reference', m.reference, 'kind', m.kind, 'effective_at', m.effective_at,
                'operator', u.display_name, 'substitution_reason', m.substitution_reason, 'note', m.note,
                'document_id', d.id, 'document_status', d.status,
                'lines', (SELECT jsonb_agg(jsonb_build_object('label', ml.label, 'identifier', ml.identifier,
                                 'quantity', ml.quantity, 'condition', ml.condition) ORDER BY ml.line_no)
                            FROM movement_lines ml WHERE ml.movement_id = m.id)) AS x
              FROM movements m JOIN users u ON u.id = m.operator_id
              LEFT JOIN documents d ON d.movement_id = m.id
             WHERE m.case_id = $1) t",
    )
    .bind(case_id)
    .fetch_one(conn)
    .await?;
    Ok(v)
}

async fn holdings(State(state): State<AppState>, user: CurrentUser, Path(id): Path<Uuid>) -> AppResult<Json<Value>> {
    user.require(&[Role::It, Role::Rh, Role::Admin])?;
    let mut conn = state.pool.acquire().await?;
    Ok(Json(holdings_json(&mut conn, id).await?))
}

// ------------------------------------------------------------------ remise

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum HandoverLine {
    Asset { asset_id: Uuid, condition: Option<String>, note: Option<String> },
    Accessoire { accessory_id: Uuid, quantity: i32, condition: Option<String>, note: Option<String> },
    Sim { sim_id: Uuid, note: Option<String> },
}

#[derive(Deserialize)]
struct HandoverInput {
    employee_id: Uuid,
    case_id: Option<Uuid>,
    task_id: Option<Uuid>,
    effective_at: Option<DateTime<Utc>>,
    substitution_reason: Option<String>,
    note: Option<String>,
    lines: Vec<HandoverLine>,
}

async fn employee_can_receive(conn: &mut PgConnection, employee_id: Uuid) -> AppResult<()> {
    let status: Option<String> = sqlx::query_scalar("SELECT status FROM employees WHERE id = $1 FOR SHARE")
        .bind(employee_id)
        .fetch_optional(conn)
        .await?;
    match status.as_deref() {
        None => Err(AppError::not_found("salarié")),
        Some("sorti") => Err(AppError::conflict("salarié sorti : aucune remise possible")),
        _ => Ok(()),
    }
}

async fn new_movement(
    conn: &mut PgConnection,
    user: &CurrentUser,
    kind: &str,
    employee_id: Uuid,
    case_id: Option<Uuid>,
    task_id: Option<Uuid>,
    effective_at: DateTime<Utc>,
    substitution: Option<String>,
    note: Option<String>,
) -> AppResult<(Uuid, String)> {
    let n: i64 = sqlx::query_scalar("SELECT nextval('movement_ref')").fetch_one(&mut *conn).await?;
    let reference = format_movement_reference(n);
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO movements (reference, kind, employee_id, case_id, task_id, operator_id, effective_at, substitution_reason, note)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9) RETURNING id",
    )
    .bind(&reference)
    .bind(kind)
    .bind(employee_id)
    .bind(case_id)
    .bind(task_id)
    .bind(user.id)
    .bind(effective_at)
    .bind(substitution)
    .bind(note)
    .fetch_one(conn)
    .await?;
    Ok((id, reference))
}

async fn create_document(conn: &mut PgConnection, movement_id: Uuid, kind: &str) -> AppResult<Uuid> {
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO documents (movement_id, kind, template_version) VALUES ($1, $2, $3) RETURNING id",
    )
    .bind(movement_id)
    .bind(kind)
    .bind(format!("{kind}-v1"))
    .fetch_one(conn)
    .await?;
    Ok(id)
}

/// Lance la génération du PDF après validation, sans bloquer la réponse.
/// Le worker rattrape toute génération manquée.
fn spawn_pdf(state: &AppState, document_id: Uuid) {
    let pool = state.pool.clone();
    let config = state.config.clone();
    tokio::spawn(async move {
        if let Err(e) = documents::ensure_original(&pool, &config, document_id).await {
            tracing::warn!(document = %document_id, error = %e, "génération PDF différée au worker");
        }
    });
}

/// Une remise regroupe plusieurs lignes ; chaque ressource est verrouillée,
/// le titulaire et les quantités contrôlés, et tout est enregistré ensemble.
async fn create_handover(
    State(state): State<AppState>,
    user: CurrentUser,
    headers: HeaderMap,
    Json(raw): Json<Value>,
) -> AppResult<Response> {
    user.require(PARC)?;
    let input: HandoverInput =
        serde_json::from_value(raw.clone()).map_err(|e| AppError::invalid("body", format!("saisie incomplète : {e}")))?;
    if input.lines.is_empty() {
        return Err(AppError::invalid("lines", "au moins un élément remis"));
    }
    let key = idempotency::key_from(&headers)?;
    let mut tx = state.pool.begin().await?;
    if let Some(k) = &key {
        if let Begin::Replay(status, body) =
            idempotency::begin(&mut tx, user.id, k, "POST /handovers", &idempotency::hash_request("POST /handovers", &raw)).await?
        {
            tx.commit().await?;
            return Ok((status, Json(body)).into_response());
        }
    }
    employee_can_receive(&mut tx, input.employee_id).await?;

    // Dossier et tâche facultatifs, mais cohérents.
    let mut preference: Option<String> = None;
    if let Some(case_id) = input.case_id {
        let row: Option<(Uuid, String)> = sqlx::query_as("SELECT employee_id, status FROM cases WHERE id = $1")
            .bind(case_id)
            .fetch_optional(&mut *tx)
            .await?;
        let (emp, status) = row.ok_or_else(|| AppError::invalid("case_id", "dossier introuvable"))?;
        if emp != input.employee_id {
            return Err(AppError::invalid("case_id", "le dossier concerne un autre salarié"));
        }
        if status != "ouvert" {
            return Err(AppError::conflict("dossier clos : enregistrez la remise hors dossier ou via une demande ponctuelle"));
        }
    }
    if let Some(task_id) = input.task_id {
        let task_case: Option<Uuid> = sqlx::query_scalar("SELECT case_id FROM tasks WHERE id = $1")
            .bind(task_id)
            .fetch_optional(&mut *tx)
            .await?;
        let task_case = task_case.ok_or_else(|| AppError::invalid("task_id", "tâche introuvable"))?;
        if Some(task_case) != input.case_id {
            return Err(AppError::invalid("task_id", "la tâche n'appartient pas à ce dossier"));
        }
    }
    // La préférence de PC exprimée par le RH dans ce dossier s'applique à
    // toute remise du dossier, quelle que soit la tâche d'où l'on vient.
    if let Some(case_id) = input.case_id {
        preference = sqlx::query_scalar(
            "SELECT preference FROM case_needs WHERE case_id = $1 AND category = 'pc' AND answer = 'oui'",
        )
        .bind(case_id)
        .fetch_optional(&mut *tx)
        .await?
        .flatten();
    }

    struct Prepared {
        item_type: &'static str,
        asset_id: Option<Uuid>,
        accessory_id: Option<Uuid>,
        sim_id: Option<Uuid>,
        label: String,
        identifier: Option<String>,
        quantity: i32,
        condition: Option<String>,
        note: Option<String>,
        asset_kind: Option<String>,
    }
    let mut prepared: Vec<Prepared> = Vec::new();
    for line in &input.lines {
        match line {
            HandoverLine::Asset { asset_id, condition, note } => {
                if prepared.iter().any(|p| p.asset_id == Some(*asset_id)) {
                    return Err(AppError::invalid("lines", "un même actif figure deux fois"));
                }
                let row: Option<(String, String, String, Option<String>, String)> = sqlx::query_as(
                    "SELECT status, label, inventory_no, serial_no, kind FROM assets WHERE id = $1 FOR UPDATE",
                )
                .bind(asset_id)
                .fetch_optional(&mut *tx)
                .await?;
                let (status, label, inv, serial, kind) = row.ok_or_else(|| AppError::invalid("lines", "actif introuvable"))?;
                let status = AssetStatus::parse(&status).ok_or_else(|| anyhow::anyhow!("statut d'actif inconnu"))?;
                can_hand_over(status).map_err(|e| AppError::conflict(format!("{inv} : {e}")))?;
                sqlx::query("UPDATE assets SET status = 'affecte', version = version + 1, updated_at = now() WHERE id = $1")
                    .bind(asset_id)
                    .execute(&mut *tx)
                    .await?;
                let identifier = match serial {
                    Some(s) => format!("{inv} / {s}"),
                    None => inv,
                };
                prepared.push(Prepared {
                    item_type: "asset",
                    asset_id: Some(*asset_id),
                    accessory_id: None,
                    sim_id: None,
                    label,
                    identifier: Some(identifier),
                    quantity: 1,
                    condition: condition.as_deref().and_then(domain::text::clean),
                    note: note.as_deref().and_then(domain::text::clean),
                    asset_kind: Some(kind),
                });
            }
            HandoverLine::Accessoire { accessory_id, quantity, condition, note } => {
                if *quantity < 1 {
                    return Err(AppError::invalid("quantity", "quantité au moins égale à 1"));
                }
                let row: Option<(String, i32)> =
                    sqlx::query_as("SELECT label, quantity_on_hand FROM accessories WHERE id = $1 FOR UPDATE")
                        .bind(accessory_id)
                        .fetch_optional(&mut *tx)
                        .await?;
                let (label, on_hand) = row.ok_or_else(|| AppError::invalid("lines", "accessoire introuvable"))?;
                if on_hand < *quantity {
                    return Err(AppError::conflict(format!("stock insuffisant pour « {label} » ({on_hand} disponible(s))")));
                }
                sqlx::query("UPDATE accessories SET quantity_on_hand = quantity_on_hand - $2 WHERE id = $1")
                    .bind(accessory_id)
                    .bind(quantity)
                    .execute(&mut *tx)
                    .await?;
                prepared.push(Prepared {
                    item_type: "accessoire",
                    asset_id: None,
                    accessory_id: Some(*accessory_id),
                    sim_id: None,
                    label,
                    identifier: None,
                    quantity: *quantity,
                    condition: condition.as_deref().and_then(domain::text::clean),
                    note: note.as_deref().and_then(domain::text::clean),
                    asset_kind: None,
                });
            }
            HandoverLine::Sim { sim_id, note } => {
                let row: Option<(String, String, Option<String>)> = sqlx::query_as(
                    "SELECT s.status, s.iccid, l.number FROM sim_cards s LEFT JOIN mobile_lines l ON l.id = s.line_id
                      WHERE s.id = $1 FOR UPDATE OF s",
                )
                .bind(sim_id)
                .fetch_optional(&mut *tx)
                .await?;
                let (status, iccid, number) = row.ok_or_else(|| AppError::invalid("lines", "SIM introuvable"))?;
                if status != "disponible" {
                    return Err(AppError::conflict(format!("SIM {iccid} non disponible ({status})")));
                }
                sqlx::query("UPDATE sim_cards SET status = 'affecte' WHERE id = $1")
                    .bind(sim_id)
                    .execute(&mut *tx)
                    .await?;
                prepared.push(Prepared {
                    item_type: "sim",
                    asset_id: None,
                    accessory_id: None,
                    sim_id: Some(*sim_id),
                    label: match &number {
                        Some(n) => format!("Carte SIM — ligne {n}"),
                        None => "Carte SIM".into(),
                    },
                    identifier: Some(iccid),
                    quantity: 1,
                    condition: None,
                    note: note.as_deref().and_then(domain::text::clean),
                    asset_kind: None,
                });
            }
        }
    }

    // Substitution : la préférence RH reste intacte, la raison est tracée.
    let substitution = input.substitution_reason.as_deref().and_then(domain::text::clean);
    let kinds: Vec<&str> = prepared.iter().filter_map(|p| p.asset_kind.as_deref()).collect();
    let pref = preference.as_deref().and_then(domain::PcPreference::parse);
    if domain::needs::substitution_needed(pref, &kinds) && substitution.is_none() {
        return Err(AppError::invalid(
            "substitution_reason",
            format!(
                "préférence RH « {} » : indiquez la raison de la substitution",
                preference.as_deref().unwrap_or_default()
            ),
        ));
    }

    let effective_at = input.effective_at.unwrap_or_else(Utc::now);
    let (movement_id, reference) = new_movement(
        &mut tx,
        &user,
        "remise",
        input.employee_id,
        input.case_id,
        input.task_id,
        effective_at,
        substitution.clone(),
        input.note.as_deref().and_then(domain::text::clean),
    )
    .await?;
    for (i, p) in prepared.iter().enumerate() {
        let line_id: Uuid = sqlx::query_scalar(
            "INSERT INTO movement_lines (movement_id, line_no, item_type, asset_id, accessory_id, sim_id, label, identifier, quantity, condition, note)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11) RETURNING id",
        )
        .bind(movement_id)
        .bind(i as i32 + 1)
        .bind(p.item_type)
        .bind(p.asset_id)
        .bind(p.accessory_id)
        .bind(p.sim_id)
        .bind(&p.label)
        .bind(&p.identifier)
        .bind(p.quantity)
        .bind(&p.condition)
        .bind(&p.note)
        .fetch_one(&mut *tx)
        .await?;
        sqlx::query(
            "INSERT INTO assignments (employee_id, handover_line_id, item_type, asset_id, accessory_id, sim_id, quantity, open_quantity, started_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $7, $8)",
        )
        .bind(input.employee_id)
        .bind(line_id)
        .bind(p.item_type)
        .bind(p.asset_id)
        .bind(p.accessory_id)
        .bind(p.sim_id)
        .bind(p.quantity)
        .bind(effective_at)
        .execute(&mut *tx)
        .await?;
    }
    let document_id = create_document(&mut tx, movement_id, "affectation").await?;
    audit::record(
        &mut tx,
        &user,
        Audit {
            action: "remise",
            entity: "movement",
            entity_id: Some(movement_id),
            case_id: input.case_id,
            employee_id: Some(input.employee_id),
            data: Some(json!({ "reference": reference, "lignes": prepared.len(), "substitution": substitution })),
        },
    )
    .await?;
    let response = json!({ "movement_id": movement_id, "reference": reference, "document_id": document_id });
    if let Some(k) = &key {
        idempotency::finish(&mut tx, user.id, k, StatusCode::CREATED, &response).await?;
    }
    tx.commit().await?;
    spawn_pdf(&state, document_id);
    Ok((StatusCode::CREATED, Json(response)).into_response())
}

// ------------------------------------------------------------------ restitution

#[derive(Deserialize)]
struct ReturnLine {
    origin_line_id: Uuid,
    quantity: Option<i32>,
    condition: Option<String>,
    note: Option<String>,
}

#[derive(Deserialize)]
struct ReturnInput {
    employee_id: Uuid,
    case_id: Option<Uuid>,
    effective_at: Option<DateTime<Utc>>,
    note: Option<String>,
    lines: Vec<ReturnLine>,
}

/// Restitution ligne par ligne, référencée aux remises d'origine. Seules les
/// quantités rendues sont soldées ; le reste demeure chez le salarié.
async fn create_return(
    State(state): State<AppState>,
    user: CurrentUser,
    headers: HeaderMap,
    Json(raw): Json<Value>,
) -> AppResult<Response> {
    user.require(PARC)?;
    let input: ReturnInput =
        serde_json::from_value(raw.clone()).map_err(|e| AppError::invalid("body", format!("saisie incomplète : {e}")))?;
    if input.lines.is_empty() {
        return Err(AppError::invalid("lines", "au moins un élément restitué"));
    }
    let key = idempotency::key_from(&headers)?;
    let mut tx = state.pool.begin().await?;
    if let Some(k) = &key {
        if let Begin::Replay(status, body) =
            idempotency::begin(&mut tx, user.id, k, "POST /returns", &idempotency::hash_request("POST /returns", &raw)).await?
        {
            tx.commit().await?;
            return Ok((status, Json(body)).into_response());
        }
    }
    if let Some(case_id) = input.case_id {
        let emp: Option<Uuid> = sqlx::query_scalar("SELECT employee_id FROM cases WHERE id = $1")
            .bind(case_id)
            .fetch_optional(&mut *tx)
            .await?;
        if emp != Some(input.employee_id) {
            return Err(AppError::invalid("case_id", "le dossier concerne un autre salarié"));
        }
    }
    let effective_at = input.effective_at.unwrap_or_else(Utc::now);
    let (movement_id, reference) = new_movement(
        &mut tx,
        &user,
        "restitution",
        input.employee_id,
        input.case_id,
        None,
        effective_at,
        None,
        input.note.as_deref().and_then(domain::text::clean),
    )
    .await?;
    let mut seen = Vec::new();
    for (i, line) in input.lines.iter().enumerate() {
        if seen.contains(&line.origin_line_id) {
            return Err(AppError::invalid("lines", "une même ligne d'origine figure deux fois"));
        }
        seen.push(line.origin_line_id);
        type Held = (Uuid, String, Option<Uuid>, Option<Uuid>, Option<Uuid>, i32, String, Option<String>);
        let held: Option<Held> = sqlx::query_as(
            "SELECT a.id, a.item_type, a.asset_id, a.accessory_id, a.sim_id, a.open_quantity, ml.label, ml.identifier
               FROM assignments a JOIN movement_lines ml ON ml.id = a.handover_line_id
              WHERE a.handover_line_id = $1 AND a.employee_id = $2 AND a.ended_at IS NULL
              FOR UPDATE OF a",
        )
        .bind(line.origin_line_id)
        .bind(input.employee_id)
        .fetch_optional(&mut *tx)
        .await?;
        let (assignment_id, item_type, asset_id, accessory_id, sim_id, open_qty, label, identifier) =
            held.ok_or_else(|| AppError::invalid("lines", "élément non détenu par ce salarié ou déjà rendu"))?;
        let qty = line.quantity.unwrap_or(open_qty);
        check_return_quantity(qty, open_qty)?;
        sqlx::query(
            "UPDATE assignments SET open_quantity = open_quantity - $2,
                    ended_at = CASE WHEN open_quantity - $2 = 0 THEN $3 ELSE NULL END
              WHERE id = $1",
        )
        .bind(assignment_id)
        .bind(qty)
        .bind(effective_at)
        .execute(&mut *tx)
        .await?;
        match item_type.as_str() {
            // Un poste rendu passe en contrôle avant d'être de nouveau disponible.
            "asset" => {
                sqlx::query("UPDATE assets SET status = 'controle', version = version + 1, updated_at = now() WHERE id = $1")
                    .bind(asset_id)
                    .execute(&mut *tx)
                    .await?;
            }
            "accessoire" => {
                sqlx::query("UPDATE accessories SET quantity_on_hand = quantity_on_hand + $2 WHERE id = $1")
                    .bind(accessory_id)
                    .bind(qty)
                    .execute(&mut *tx)
                    .await?;
            }
            "sim" => {
                sqlx::query("UPDATE sim_cards SET status = 'disponible' WHERE id = $1")
                    .bind(sim_id)
                    .execute(&mut *tx)
                    .await?;
            }
            _ => {}
        }
        sqlx::query(
            "INSERT INTO movement_lines (movement_id, line_no, item_type, asset_id, accessory_id, sim_id, label, identifier,
                                         quantity, condition, note, origin_line_id)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)",
        )
        .bind(movement_id)
        .bind(i as i32 + 1)
        .bind(&item_type)
        .bind(asset_id)
        .bind(accessory_id)
        .bind(sim_id)
        .bind(&label)
        .bind(&identifier)
        .bind(qty)
        .bind(line.condition.as_deref().and_then(domain::text::clean))
        .bind(line.note.as_deref().and_then(domain::text::clean))
        .bind(line.origin_line_id)
        .execute(&mut *tx)
        .await?;
    }
    let document_id = create_document(&mut tx, movement_id, "restitution").await?;
    audit::record(
        &mut tx,
        &user,
        Audit {
            action: "restitution",
            entity: "movement",
            entity_id: Some(movement_id),
            case_id: input.case_id,
            employee_id: Some(input.employee_id),
            data: Some(json!({ "reference": reference, "lignes": input.lines.len() })),
        },
    )
    .await?;
    let response = json!({ "movement_id": movement_id, "reference": reference, "document_id": document_id });
    if let Some(k) = &key {
        idempotency::finish(&mut tx, user.id, k, StatusCode::CREATED, &response).await?;
    }
    tx.commit().await?;
    spawn_pdf(&state, document_id);
    Ok((StatusCode::CREATED, Json(response)).into_response())
}

// ------------------------------------------------------------------ détail et documents

async fn movement_detail(State(state): State<AppState>, user: CurrentUser, Path(id): Path<Uuid>) -> AppResult<Json<Value>> {
    user.require(&[Role::It, Role::Rh, Role::Admin])?;
    let v: Option<Value> = sqlx::query_scalar(
        "SELECT jsonb_build_object(
                'id', m.id, 'reference', m.reference, 'kind', m.kind, 'effective_at', m.effective_at,
                'employee_id', e.id, 'employee_name', e.first_name || ' ' || e.last_name,
                'case_reference', c.reference, 'operator', u.display_name,
                'substitution_reason', m.substitution_reason, 'note', m.note,
                'lines', (SELECT jsonb_agg(jsonb_build_object('label', ml.label, 'identifier', ml.identifier,
                              'quantity', ml.quantity, 'condition', ml.condition, 'note', ml.note,
                              'origin_reference', om.reference) ORDER BY ml.line_no)
                            FROM movement_lines ml
                            LEFT JOIN movement_lines ol ON ol.id = ml.origin_line_id
                            LEFT JOIN movements om ON om.id = ol.movement_id
                           WHERE ml.movement_id = m.id),
                'document', (SELECT jsonb_build_object('id', d.id, 'kind', d.kind, 'status', d.status,
                              'template_version', d.template_version,
                              'versions', (SELECT coalesce(jsonb_agg(jsonb_build_object('version', v.version,
                                              'file_kind', v.file_kind, 'mime_type', v.mime_type, 'size_bytes', v.size_bytes,
                                              'sha256', v.sha256, 'archive_status', v.archive_status, 'created_at', v.created_at)
                                              ORDER BY v.file_kind, v.version), '[]')
                                           FROM document_versions v WHERE v.document_id = d.id))
                               FROM documents d WHERE d.movement_id = m.id))
           FROM movements m
           JOIN employees e ON e.id = m.employee_id
           JOIN users u ON u.id = m.operator_id
           LEFT JOIN cases c ON c.id = m.case_id
          WHERE m.id = $1",
    )
    .bind(id)
    .fetch_optional(&state.pool)
    .await?;
    Ok(Json(v.ok_or_else(|| AppError::not_found("mouvement"))?))
}

async fn send_file(state: &AppState, relative: &str, mime: &str, filename: &str) -> AppResult<Response> {
    let bytes = tokio::fs::read(state.config.data_dir.join(relative))
        .await
        .map_err(|e| anyhow::anyhow!("lecture du document {relative} : {e}"))?;
    Ok((
        [
            (header::CONTENT_TYPE, mime.to_string()),
            (header::CONTENT_DISPOSITION, format!("inline; filename=\"{filename}\"")),
            (header::CACHE_CONTROL, "no-store".to_string()),
        ],
        Body::from(bytes),
    )
        .into_response())
}

async fn download_original(State(state): State<AppState>, user: CurrentUser, Path(id): Path<Uuid>) -> AppResult<Response> {
    user.require(&[Role::It, Role::Rh, Role::Admin])?;
    let exists: Option<String> = sqlx::query_scalar("SELECT status FROM documents WHERE id = $1")
        .bind(id)
        .fetch_optional(&state.pool)
        .await?;
    exists.ok_or_else(|| AppError::not_found("document"))?;
    // Génération à la demande si le worker n'est pas encore passé.
    documents::ensure_original(&state.pool, &state.config, id).await?;
    let (path, reference): (String, String) = sqlx::query_as(
        "SELECT v.storage_path, m.reference FROM document_versions v
           JOIN documents d ON d.id = v.document_id JOIN movements m ON m.id = d.movement_id
          WHERE v.document_id = $1 AND v.file_kind = 'original' ORDER BY v.version DESC LIMIT 1",
    )
    .bind(id)
    .fetch_one(&state.pool)
    .await?;
    send_file(&state, &path, "application/pdf", &format!("{reference}-decharge.pdf")).await
}

/// Dépôt du scan signé. L'état « signé » exige ce scan, lié à l'original
/// imprimé ; le fichier est contrôlé (taille, signature binaire) et n'est
/// jamais exécuté.
async fn upload_scan(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
    mut multipart: Multipart,
) -> AppResult<Json<Value>> {
    user.require(PARC)?;
    let mut bytes: Option<Vec<u8>> = None;
    while let Some(field) = multipart.next_field().await.map_err(|e| AppError::invalid("file", e.to_string()))? {
        if field.name() == Some("file") {
            let data = field.bytes().await.map_err(|e| AppError::invalid("file", e.to_string()))?;
            bytes = Some(data.to_vec());
        }
    }
    let bytes = bytes.ok_or_else(|| AppError::invalid("file", "fichier manquant"))?;
    if bytes.is_empty() || bytes.len() > state.config.max_upload_bytes {
        return Err(AppError::invalid("file", format!("taille maximale {} Mo", state.config.max_upload_bytes / 1024 / 1024)));
    }
    let (mime, ext) = documents::sniff_scan(&bytes)
        .ok_or_else(|| AppError::invalid("file", "format refusé : PDF, PNG ou JPEG uniquement"))?;

    let mut tx = state.pool.begin().await?;
    let row: Option<(String, String, Option<String>, Uuid, Option<Uuid>)> = sqlx::query_as(
        "SELECT m.reference, m.kind, c.reference, m.employee_id, m.case_id FROM documents d
           JOIN movements m ON m.id = d.movement_id LEFT JOIN cases c ON c.id = m.case_id
          WHERE d.id = $1 FOR UPDATE OF d",
    )
    .bind(id)
    .fetch_optional(&mut *tx)
    .await?;
    let (mref, mkind, case_ref, employee_id, case_id) = row.ok_or_else(|| AppError::not_found("document"))?;
    let has_original: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM document_versions WHERE document_id = $1 AND file_kind = 'original')",
    )
    .bind(id)
    .fetch_one(&mut *tx)
    .await?;
    if !has_original {
        return Err(AppError::conflict("imprimez d'abord la décharge originale : le scan doit correspondre à la version imprimée"));
    }
    let version: i32 = sqlx::query_scalar(
        "SELECT coalesce(max(version), 0) + 1 FROM document_versions WHERE document_id = $1 AND file_kind = 'scan'",
    )
    .bind(id)
    .fetch_one(&mut *tx)
    .await?;
    let relative = documents::movement_dir(case_ref.as_deref(), &mkind, &mref).join(format!("v1-scan-{version}.{ext}"));
    documents::write_file(&state.config.data_dir, &relative, &bytes).await?;
    let sha = documents::sha256_hex(&bytes);
    sqlx::query(
        "INSERT INTO document_versions (document_id, version, file_kind, storage_path, sha256, size_bytes, mime_type, archive_status, created_by)
         VALUES ($1, $2, 'scan', $3, $4, $5, $6, 'desactive', $7)",
    )
    .bind(id)
    .bind(version)
    .bind(relative.to_string_lossy().replace('\\', "/"))
    .bind(&sha)
    .bind(bytes.len() as i64)
    .bind(mime)
    .bind(user.id)
    .execute(&mut *tx)
    .await?;
    sqlx::query("UPDATE documents SET status = 'signe', updated_at = now() WHERE id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    audit::record(
        &mut tx,
        &user,
        Audit {
            action: "scan_signe_depose",
            entity: "document",
            entity_id: Some(id),
            case_id,
            employee_id: Some(employee_id),
            data: Some(json!({ "movement": mref, "version": version, "sha256": sha })),
        },
    )
    .await?;
    tx.commit().await?;
    Ok(Json(json!({ "status": "signe", "version": version })))
}

async fn download_scan(
    State(state): State<AppState>,
    user: CurrentUser,
    Path((id, version)): Path<(Uuid, i32)>,
) -> AppResult<Response> {
    user.require(&[Role::It, Role::Rh, Role::Admin])?;
    let row: Option<(String, String, String)> = sqlx::query_as(
        "SELECT v.storage_path, v.mime_type, m.reference FROM document_versions v
           JOIN documents d ON d.id = v.document_id JOIN movements m ON m.id = d.movement_id
          WHERE v.document_id = $1 AND v.file_kind = 'scan' AND v.version = $2",
    )
    .bind(id)
    .bind(version)
    .fetch_optional(&state.pool)
    .await?;
    let (path, mime, reference) = row.ok_or_else(|| AppError::not_found("scan"))?;
    let ext = path.rsplit('.').next().unwrap_or("bin").to_string();
    send_file(&state, &path, &mime, &format!("{reference}-scan-{version}.{ext}")).await
}
