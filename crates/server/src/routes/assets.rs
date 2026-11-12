use axum::extract::{Path, Query, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use domain::movements::{AssetKind, AssetStatus};
use domain::Role;
use rand::Rng;
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
        .route("/assets", get(list).post(create))
        .route("/assets/lookup/{code}", get(lookup))
        .route("/assets/{id}", get(detail).patch(update))
        .route("/accessories", get(list_accessories).post(create_accessory))
        .route("/accessories/{id}/stock", post(adjust_stock))
        .route("/sims", get(list_sims).post(create_sim))
}

const PARC: &[Role] = &[Role::It, Role::Admin];

/// Jeton QR stable et non secret : il identifie l'actif, sans nom ni site,
/// et ne donne aucun droit.
fn new_qr_token() -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";
    let mut rng = rand::thread_rng();
    let code: String = (0..10).map(|_| ALPHABET[rng.gen_range(0..ALPHABET.len())] as char).collect();
    format!("CP-{code}")
}

#[derive(Deserialize)]
struct ListQuery {
    q: Option<String>,
    status: Option<String>,
    kind: Option<String>,
    site_id: Option<Uuid>,
}

#[derive(Serialize, FromRow)]
struct AssetRow {
    id: Uuid,
    inventory_no: String,
    qr_token: String,
    kind: String,
    label: String,
    serial_no: Option<String>,
    status: String,
    site_id: Option<Uuid>,
    site_name: Option<String>,
    location_name: Option<String>,
    holder_id: Option<Uuid>,
    holder_name: Option<String>,
    version: i32,
}

const ASSET_SELECT: &str = "
    SELECT a.id, a.inventory_no, a.qr_token, a.kind, a.label, a.serial_no, a.status, a.site_id,
           s.name AS site_name, l.name AS location_name,
           e.id AS holder_id, e.first_name || ' ' || e.last_name AS holder_name, a.version
      FROM assets a
      LEFT JOIN sites s ON s.id = a.site_id
      LEFT JOIN locations l ON l.id = a.location_id
      LEFT JOIN assignments asg ON asg.asset_id = a.id AND asg.ended_at IS NULL
      LEFT JOIN employees e ON e.id = asg.employee_id";

async fn list(State(state): State<AppState>, user: CurrentUser, Query(q): Query<ListQuery>) -> AppResult<Json<Vec<AssetRow>>> {
    user.require(PARC)?;
    let search = q.q.as_deref().and_then(domain::text::clean).map(|s| format!("%{}%", s.to_lowercase()));
    let rows = sqlx::query_as::<_, AssetRow>(&format!(
        "{ASSET_SELECT}
          WHERE ($1::text IS NULL OR lower(a.inventory_no) LIKE $1 OR lower(a.label) LIKE $1
                 OR lower(coalesce(a.serial_no, '')) LIKE $1 OR lower(a.qr_token) LIKE $1)
            AND ($2::text IS NULL OR a.status = $2)
            AND ($3::text IS NULL OR a.kind = $3)
            AND ($4::uuid IS NULL OR a.site_id = $4)
          ORDER BY a.inventory_no LIMIT 500"
    ))
    .bind(&search)
    .bind(&q.status)
    .bind(&q.kind)
    .bind(q.site_id)
    .fetch_all(&state.pool)
    .await?;
    Ok(Json(rows))
}

#[derive(Deserialize)]
struct CreateInput {
    inventory_no: String,
    kind: AssetKind,
    label: String,
    serial_no: Option<String>,
    site_id: Option<Uuid>,
    location_id: Option<Uuid>,
    notes: Option<String>,
}

async fn create(State(state): State<AppState>, user: CurrentUser, Json(i): Json<CreateInput>) -> AppResult<Json<Value>> {
    user.require(PARC)?;
    let inventory_no = domain::text::clean(&i.inventory_no)
        .ok_or_else(|| AppError::invalid("inventory_no", "numéro d'inventaire obligatoire"))?
        .to_uppercase();
    let label = domain::text::clean(&i.label).ok_or_else(|| AppError::invalid("label", "libellé obligatoire"))?;
    let mut tx = state.pool.begin().await?;
    let (id, token): (Uuid, String) = sqlx::query_as(
        "INSERT INTO assets (inventory_no, qr_token, kind, label, serial_no, site_id, location_id, notes)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8) RETURNING id, qr_token",
    )
    .bind(&inventory_no)
    .bind(new_qr_token())
    .bind(i.kind.as_str())
    .bind(&label)
    .bind(i.serial_no.as_deref().and_then(domain::text::clean))
    .bind(i.site_id)
    .bind(i.location_id)
    .bind(i.notes.as_deref().and_then(domain::text::clean))
    .fetch_one(&mut *tx)
    .await?;
    audit::record(&mut tx, &user, Audit { action: "entree_stock", entity: "asset", entity_id: Some(id), data: Some(json!({ "inventory_no": inventory_no })), ..Default::default() }).await?;
    tx.commit().await?;
    Ok(Json(json!({ "id": id, "qr_token": token })))
}

/// Douchette ou saisie manuelle : jeton QR ou numéro d'inventaire.
/// Un code inconnu ne renvoie rien d'autre qu'un 404.
async fn lookup(State(state): State<AppState>, user: CurrentUser, Path(code): Path<String>) -> AppResult<Json<AssetRow>> {
    user.require(PARC)?;
    let code = code.trim().to_uppercase();
    let row = sqlx::query_as::<_, AssetRow>(&format!(
        "{ASSET_SELECT} WHERE upper(a.qr_token) = $1 OR upper(a.inventory_no) = $1 LIMIT 1"
    ))
    .bind(&code)
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("code inconnu : {code}")))?;
    Ok(Json(row))
}

async fn detail(State(state): State<AppState>, user: CurrentUser, Path(id): Path<Uuid>) -> AppResult<Json<Value>> {
    user.require(PARC)?;
    let asset = sqlx::query_as::<_, AssetRow>(&format!("{ASSET_SELECT} WHERE a.id = $1"))
        .bind(id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(|| AppError::not_found("actif"))?;
    let notes: Option<String> = sqlx::query_scalar("SELECT notes FROM assets WHERE id = $1")
        .bind(id)
        .fetch_one(&state.pool)
        .await?;
    let history: Value = sqlx::query_scalar(
        "SELECT coalesce(jsonb_agg(jsonb_build_object(
                    'movement_id', m.id, 'reference', m.reference, 'kind', m.kind, 'effective_at', m.effective_at,
                    'employee', e.first_name || ' ' || e.last_name, 'employee_id', e.id,
                    'operator', u.display_name, 'condition', ml.condition) ORDER BY m.effective_at DESC), '[]')
           FROM movement_lines ml JOIN movements m ON m.id = ml.movement_id
           JOIN employees e ON e.id = m.employee_id JOIN users u ON u.id = m.operator_id
          WHERE ml.asset_id = $1",
    )
    .bind(id)
    .fetch_one(&state.pool)
    .await?;
    Ok(Json(json!({ "asset": asset, "notes": notes, "history": history })))
}

#[derive(Deserialize)]
struct UpdateInput {
    version: i32,
    status: Option<AssetStatus>,
    label: Option<String>,
    serial_no: Option<String>,
    site_id: Option<Uuid>,
    location_id: Option<Uuid>,
    notes: Option<String>,
}

/// Contrôle, réparation, réforme, emplacement. L'affectation ne se fait que
/// par une remise, la libération que par une restitution.
async fn update(State(state): State<AppState>, user: CurrentUser, Path(id): Path<Uuid>, Json(i): Json<UpdateInput>) -> AppResult<Json<Value>> {
    user.require(PARC)?;
    let mut tx = state.pool.begin().await?;
    let row: Option<(i32, String)> = sqlx::query_as("SELECT version, status FROM assets WHERE id = $1 FOR UPDATE")
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?;
    let (version, status) = row.ok_or_else(|| AppError::not_found("actif"))?;
    if version != i.version {
        return Err(AppError::stale());
    }
    if let Some(new) = i.status {
        if new == AssetStatus::Affecte || status == "affecte" {
            return Err(AppError::invalid("status", "l'affectation passe par une remise ou une restitution"));
        }
    }
    sqlx::query(
        "UPDATE assets SET status = coalesce($2, status), label = coalesce($3, label), serial_no = coalesce($4, serial_no),
                site_id = coalesce($5, site_id), location_id = coalesce($6, location_id), notes = coalesce($7, notes),
                version = version + 1, updated_at = now()
          WHERE id = $1",
    )
    .bind(id)
    .bind(i.status.map(|s| s.as_str()))
    .bind(i.label.as_deref().and_then(domain::text::clean))
    .bind(i.serial_no.as_deref().and_then(domain::text::clean))
    .bind(i.site_id)
    .bind(i.location_id)
    .bind(i.notes.as_deref().and_then(domain::text::clean))
    .execute(&mut *tx)
    .await?;
    audit::record(&mut tx, &user, Audit { action: "actif_modifie", entity: "asset", entity_id: Some(id), data: Some(json!({ "status": i.status })), ..Default::default() }).await?;
    tx.commit().await?;
    Ok(Json(json!({ "version": version + 1 })))
}

#[derive(Serialize, FromRow)]
struct AccessoryRow {
    id: Uuid,
    label: String,
    site_id: Option<Uuid>,
    site_name: Option<String>,
    quantity_on_hand: i32,
    quantity_out: i64,
}

async fn list_accessories(State(state): State<AppState>, user: CurrentUser) -> AppResult<Json<Vec<AccessoryRow>>> {
    user.require(PARC)?;
    let rows = sqlx::query_as::<_, AccessoryRow>(
        "SELECT a.id, a.label, a.site_id, s.name AS site_name, a.quantity_on_hand,
                coalesce((SELECT sum(open_quantity) FROM assignments WHERE accessory_id = a.id AND ended_at IS NULL), 0)::bigint AS quantity_out
           FROM accessories a LEFT JOIN sites s ON s.id = a.site_id ORDER BY a.label",
    )
    .fetch_all(&state.pool)
    .await?;
    Ok(Json(rows))
}

#[derive(Deserialize)]
struct AccessoryInput {
    label: String,
    site_id: Option<Uuid>,
    quantity: i32,
}

async fn create_accessory(State(state): State<AppState>, user: CurrentUser, Json(i): Json<AccessoryInput>) -> AppResult<Json<Value>> {
    user.require(PARC)?;
    let label = domain::text::clean(&i.label).ok_or_else(|| AppError::invalid("label", "libellé obligatoire"))?;
    if i.quantity < 0 {
        return Err(AppError::invalid("quantity", "quantité positive"));
    }
    let mut tx = state.pool.begin().await?;
    let id: Uuid = sqlx::query_scalar("INSERT INTO accessories (label, site_id, quantity_on_hand) VALUES ($1, $2, $3) RETURNING id")
        .bind(&label)
        .bind(i.site_id)
        .bind(i.quantity)
        .fetch_one(&mut *tx)
        .await?;
    audit::record(&mut tx, &user, Audit { action: "accessoire_cree", entity: "accessory", entity_id: Some(id), data: Some(json!({ "label": label, "quantity": i.quantity })), ..Default::default() }).await?;
    tx.commit().await?;
    Ok(Json(json!({ "id": id })))
}

#[derive(Deserialize)]
struct StockInput {
    delta: i32,
    reason: String,
}

async fn adjust_stock(State(state): State<AppState>, user: CurrentUser, Path(id): Path<Uuid>, Json(i): Json<StockInput>) -> AppResult<Json<Value>> {
    user.require(PARC)?;
    let reason = domain::text::clean(&i.reason).ok_or_else(|| AppError::invalid("reason", "motif obligatoire"))?;
    let mut tx = state.pool.begin().await?;
    let q: Option<i32> = sqlx::query_scalar(
        "UPDATE accessories SET quantity_on_hand = quantity_on_hand + $2 WHERE id = $1 RETURNING quantity_on_hand",
    )
    .bind(id)
    .bind(i.delta)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|e| match AppError::from(e) {
        AppError::Conflict { .. } => AppError::invalid("delta", "le stock ne peut pas devenir négatif"),
        other => other,
    })?;
    let q = q.ok_or_else(|| AppError::not_found("accessoire"))?;
    audit::record(&mut tx, &user, Audit { action: "stock_ajuste", entity: "accessory", entity_id: Some(id), data: Some(json!({ "delta": i.delta, "motif": reason })), ..Default::default() }).await?;
    tx.commit().await?;
    Ok(Json(json!({ "quantity_on_hand": q })))
}

#[derive(Serialize, FromRow)]
struct SimRow {
    id: Uuid,
    iccid: String,
    status: String,
    line_number: Option<String>,
    operator: Option<String>,
    holder_id: Option<Uuid>,
    holder_name: Option<String>,
    created_at: DateTime<Utc>,
}

async fn list_sims(State(state): State<AppState>, user: CurrentUser) -> AppResult<Json<Vec<SimRow>>> {
    user.require(PARC)?;
    let rows = sqlx::query_as::<_, SimRow>(
        "SELECT s.id, s.iccid, s.status, l.number AS line_number, l.operator,
                e.id AS holder_id, e.first_name || ' ' || e.last_name AS holder_name, s.created_at
           FROM sim_cards s
           LEFT JOIN mobile_lines l ON l.id = s.line_id
           LEFT JOIN assignments a ON a.sim_id = s.id AND a.ended_at IS NULL
           LEFT JOIN employees e ON e.id = a.employee_id
          ORDER BY l.number NULLS LAST, s.iccid",
    )
    .fetch_all(&state.pool)
    .await?;
    Ok(Json(rows))
}

#[derive(Deserialize)]
struct SimInput {
    iccid: String,
    line_number: Option<String>,
    operator: Option<String>,
}

async fn create_sim(State(state): State<AppState>, user: CurrentUser, Json(i): Json<SimInput>) -> AppResult<Json<Value>> {
    user.require(PARC)?;
    let iccid: String = i.iccid.chars().filter(|c| !c.is_whitespace()).collect();
    if iccid.is_empty() {
        return Err(AppError::invalid("iccid", "référence SIM obligatoire"));
    }
    let mut tx = state.pool.begin().await?;
    let line_id: Option<Uuid> = match i.line_number.as_deref().and_then(domain::text::clean) {
        None => None,
        Some(number) => {
            let operator = i.operator.as_deref().and_then(domain::text::clean)
                .ok_or_else(|| AppError::invalid("operator", "opérateur obligatoire pour une ligne"))?;
            let id: Uuid = sqlx::query_scalar(
                "INSERT INTO mobile_lines (number, operator) VALUES ($1, $2)
                 ON CONFLICT (number) DO UPDATE SET operator = mobile_lines.operator RETURNING id",
            )
            .bind(&number)
            .bind(&operator)
            .fetch_one(&mut *tx)
            .await?;
            Some(id)
        }
    };
    let id: Uuid = sqlx::query_scalar("INSERT INTO sim_cards (iccid, line_id) VALUES ($1, $2) RETURNING id")
        .bind(&iccid)
        .bind(line_id)
        .fetch_one(&mut *tx)
        .await?;
    audit::record(&mut tx, &user, Audit { action: "sim_creee", entity: "sim", entity_id: Some(id), data: Some(json!({ "iccid": iccid })), ..Default::default() }).await?;
    tx.commit().await?;
    Ok(Json(json!({ "id": id })))
}
