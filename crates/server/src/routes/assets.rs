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
        .route("/accessories/{id}", axum::routing::patch(update_accessory))
        .route("/accessories/{id}/stock", post(adjust_stock))
        .route("/sims/{id}", axum::routing::patch(update_sim))
        .route("/sims", get(list_sims).post(create_sim))
        .route("/lines", get(list_lines))
        .route("/lines/{id}", axum::routing::patch(update_line))
}

const PARC: &[Role] = &[Role::It, Role::Admin];

/// Jeton QR stable et non secret : il identifie l'actif, sans nom ni site,
/// et ne donne aucun droit.
pub fn new_qr_token() -> String {
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
    /// Fiches à compléter : n° de série, marque, modèle ou site manquant.
    incomplet: Option<bool>,
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
    brand: Option<String>,
    model: Option<String>,
    intune_name: Option<String>,
    purchase_date: Option<chrono::NaiveDate>,
    warranty_end: Option<chrono::NaiveDate>,
    condition: Option<String>,
    location_id: Option<Uuid>,
    missing: Vec<String>,
}

const ASSET_SELECT: &str = "
    SELECT a.id, a.inventory_no, a.qr_token, a.kind, a.label, a.serial_no, a.status, a.site_id,
           s.name AS site_name, l.name AS location_name,
           e.id AS holder_id, e.first_name || ' ' || e.last_name AS holder_name, a.version,
           a.brand, a.model, a.intune_name, a.purchase_date, a.warranty_end, a.condition, a.location_id,
           array_remove(ARRAY[
               CASE WHEN a.serial_no IS NULL THEN 'n° de série' END,
               CASE WHEN a.brand IS NULL THEN 'marque' END,
               CASE WHEN a.model IS NULL THEN 'modèle' END,
               CASE WHEN a.site_id IS NULL THEN 'site' END], NULL) AS missing
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
                 OR lower(coalesce(a.serial_no, '')) LIKE $1 OR lower(a.qr_token) LIKE $1
                 OR lower(coalesce(a.intune_name, '')) LIKE $1 OR lower(coalesce(a.brand, '') || ' ' || coalesce(a.model, '')) LIKE $1)
            AND ($2::text IS NULL OR a.status = $2)
            AND ($3::text IS NULL OR a.kind = $3)
            AND ($4::uuid IS NULL OR a.site_id = $4)
            AND ($5::bool IS NOT TRUE OR a.serial_no IS NULL OR a.brand IS NULL OR a.model IS NULL OR a.site_id IS NULL)
          ORDER BY a.inventory_no LIMIT 2000"
    ))
    .bind(&search)
    .bind(&q.status)
    .bind(&q.kind)
    .bind(q.site_id)
    .bind(q.incomplet)
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
    brand: Option<String>,
    model: Option<String>,
    intune_name: Option<String>,
    purchase_date: Option<chrono::NaiveDate>,
    warranty_end: Option<chrono::NaiveDate>,
    condition: Option<String>,
    /// Matériel partagé mis directement en service (serveur, imprimante…).
    #[serde(default)]
    in_service: bool,
}

const CONDITIONS: &[&str] = &["neuf", "bon", "use", "hs"];

fn check_condition(c: &Option<String>) -> AppResult<Option<String>> {
    match c.as_deref().filter(|s| !s.is_empty()) {
        None => Ok(None),
        Some(v) if CONDITIONS.contains(&v) => Ok(Some(v.to_string())),
        Some(_) => Err(AppError::invalid("condition", "état neuf, bon, usé ou HS")),
    }
}

async fn create(State(state): State<AppState>, user: CurrentUser, Json(i): Json<CreateInput>) -> AppResult<Json<Value>> {
    user.require(PARC)?;
    let inventory_no = domain::text::clean(&i.inventory_no)
        .ok_or_else(|| AppError::invalid("inventory_no", "numéro d'inventaire obligatoire"))?
        .to_uppercase();
    let label = domain::text::clean(&i.label).ok_or_else(|| AppError::invalid("label", "libellé obligatoire"))?;
    let condition = check_condition(&i.condition)?;
    let mut tx = state.pool.begin().await?;
    let (id, token): (Uuid, String) = sqlx::query_as(
        "INSERT INTO assets (inventory_no, qr_token, kind, label, serial_no, site_id, location_id, notes,
                             brand, model, intune_name, purchase_date, warranty_end, condition, status)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15) RETURNING id, qr_token",
    )
    .bind(&inventory_no)
    .bind(new_qr_token())
    .bind(i.kind.as_str())
    .bind(&label)
    .bind(i.serial_no.as_deref().and_then(domain::text::clean))
    .bind(i.site_id)
    .bind(i.location_id)
    .bind(i.notes.as_deref().and_then(domain::text::clean))
    .bind(i.brand.as_deref().and_then(domain::text::clean))
    .bind(i.model.as_deref().and_then(domain::text::clean))
    .bind(i.intune_name.as_deref().and_then(domain::text::clean))
    .bind(i.purchase_date)
    .bind(i.warranty_end)
    .bind(&condition)
    .bind(if i.in_service { "en_service" } else { "disponible" })
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
                    'operator', u.display_name, 'condition', ml.condition, 'note', m.note,
                    'legacy_proof_url', m.legacy_proof_url, 'legacy_proof_ref', m.legacy_proof_ref) ORDER BY m.effective_at DESC), '[]')
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
    brand: Option<String>,
    model: Option<String>,
    intune_name: Option<String>,
    purchase_date: Option<chrono::NaiveDate>,
    warranty_end: Option<chrono::NaiveDate>,
    condition: Option<String>,
    /// Motif d'une rectification de l'emplacement (inventaire).
    reason: Option<String>,
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
    let condition = check_condition(&i.condition)?;
    sqlx::query(
        "UPDATE assets SET status = coalesce($2, status), label = coalesce($3, label), serial_no = coalesce($4, serial_no),
                site_id = coalesce($5, site_id), location_id = coalesce($6, location_id), notes = coalesce($7, notes),
                brand = coalesce($8, brand), model = coalesce($9, model), intune_name = coalesce($10, intune_name),
                purchase_date = coalesce($11, purchase_date), warranty_end = coalesce($12, warranty_end),
                condition = coalesce($13, condition),
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
    .bind(i.brand.as_deref().and_then(domain::text::clean))
    .bind(i.model.as_deref().and_then(domain::text::clean))
    .bind(i.intune_name.as_deref().and_then(domain::text::clean))
    .bind(i.purchase_date)
    .bind(i.warranty_end)
    .bind(&condition)
    .execute(&mut *tx)
    .await?;
    let reason = i.reason.as_deref().and_then(domain::text::clean);
    let action = if reason.is_some() && (i.site_id.is_some() || i.location_id.is_some()) { "rectification_emplacement" } else { "actif_modifie" };
    audit::record(&mut tx, &user, Audit { action, entity: "asset", entity_id: Some(id), data: Some(json!({ "status": i.status, "site": i.site_id, "local": i.location_id, "motif": reason })), ..Default::default() }).await?;
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
struct AccessoryUpdate {
    label: Option<String>,
    site_id: Option<Uuid>,
    #[serde(default)]
    clear_site: bool,
}

/// Libellé et site d'un accessoire (le stock passe par un ajustement motivé).
async fn update_accessory(State(state): State<AppState>, user: CurrentUser, Path(id): Path<Uuid>, Json(i): Json<AccessoryUpdate>) -> AppResult<Json<Value>> {
    user.require(PARC)?;
    let label = match &i.label {
        Some(l) => Some(domain::text::clean(l).ok_or_else(|| AppError::invalid("label", "libellé obligatoire"))?),
        None => None,
    };
    let mut tx = state.pool.begin().await?;
    let n = sqlx::query(
        "UPDATE accessories SET label = coalesce($2, label),
                site_id = CASE WHEN $4 THEN NULL ELSE coalesce($3, site_id) END WHERE id = $1",
    )
    .bind(id)
    .bind(&label)
    .bind(i.site_id)
    .bind(i.clear_site)
    .execute(&mut *tx)
    .await?
    .rows_affected();
    if n == 0 {
        return Err(AppError::not_found("accessoire"));
    }
    audit::record(&mut tx, &user, Audit { action: "accessoire_modifie", entity: "accessory", entity_id: Some(id), data: Some(json!({ "label": label, "site": i.site_id })), ..Default::default() }).await?;
    tx.commit().await?;
    Ok(Json(json!({ "ok": true })))
}

#[derive(Deserialize)]
struct SimUpdate {
    iccid: String,
}

/// Correction de la référence (ICCID) d'une SIM saisie ou reprise avec erreur.
async fn update_sim(State(state): State<AppState>, user: CurrentUser, Path(id): Path<Uuid>, Json(i): Json<SimUpdate>) -> AppResult<Json<Value>> {
    user.require(PARC)?;
    let iccid: String = i.iccid.chars().filter(|c| !c.is_whitespace()).collect();
    if iccid.is_empty() {
        return Err(AppError::invalid("iccid", "référence SIM obligatoire"));
    }
    let mut tx = state.pool.begin().await?;
    let before: Option<String> = sqlx::query_scalar("SELECT iccid FROM sim_cards WHERE id = $1 FOR UPDATE").bind(id).fetch_optional(&mut *tx).await?;
    let before = before.ok_or_else(|| AppError::not_found("SIM"))?;
    sqlx::query("UPDATE sim_cards SET iccid = $2 WHERE id = $1").bind(id).bind(&iccid).execute(&mut *tx).await?;
    audit::record(&mut tx, &user, Audit { action: "sim_modifiee", entity: "sim", entity_id: Some(id), data: Some(json!({ "avant": before, "apres": iccid })), ..Default::default() }).await?;
    tx.commit().await?;
    Ok(Json(json!({ "ok": true })))
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
    plan: Option<String>,
    line_type: Option<String>,
}

fn check_line_type(t: &Option<String>) -> AppResult<Option<String>> {
    match t.as_deref().filter(|s| !s.is_empty()) {
        None => Ok(None),
        Some(v @ ("voix" | "data" | "voix_data")) => Ok(Some(v.to_string())),
        Some(_) => Err(AppError::invalid("line_type", "voix, data ou voix_data")),
    }
}

async fn list_lines(State(state): State<AppState>, user: CurrentUser) -> AppResult<Json<Value>> {
    user.require(PARC)?;
    let v: Value = sqlx::query_scalar(
        "SELECT coalesce(jsonb_agg(jsonb_build_object('id', l.id, 'number', l.number, 'operator', l.operator, 'plan', l.plan,
                    'line_type', l.line_type, 'status', l.status,
                    'iccid', (SELECT s.iccid FROM sim_cards s WHERE s.line_id = l.id AND s.status <> 'desactivee' LIMIT 1),
                    'holder', (SELECT e.first_name || ' ' || e.last_name FROM sim_cards s JOIN assignments a ON a.sim_id = s.id AND a.ended_at IS NULL
                                 JOIN employees e ON e.id = a.employee_id WHERE s.line_id = l.id LIMIT 1))
                    ORDER BY l.number), '[]')
           FROM mobile_lines l",
    )
    .fetch_one(&state.pool)
    .await?;
    Ok(Json(v))
}

#[derive(Deserialize)]
struct LineUpdate {
    /// Correction du numéro ou de l'opérateur (saisie ou reprise erronée).
    number: Option<String>,
    operator: Option<String>,
    plan: Option<String>,
    line_type: Option<String>,
    status: Option<String>,
    reason: Option<String>,
}

/// Forfait, type et statut (suspension et résiliation motivées).
async fn update_line(State(state): State<AppState>, user: CurrentUser, Path(id): Path<Uuid>, Json(i): Json<LineUpdate>) -> AppResult<Json<Value>> {
    user.require(PARC)?;
    let line_type = check_line_type(&i.line_type)?;
    let reason = i.reason.as_deref().and_then(domain::text::clean);
    if let Some(s) = i.status.as_deref() {
        if !["active", "suspendue", "resiliee"].contains(&s) {
            return Err(AppError::invalid("status", "active, suspendue ou resiliee"));
        }
        if s != "active" && reason.is_none() {
            return Err(AppError::invalid("reason", "motif obligatoire pour suspendre ou résilier"));
        }
    }
    let mut tx = state.pool.begin().await?;
    let n = sqlx::query(
        "UPDATE mobile_lines SET plan = coalesce($2, plan), line_type = coalesce($3, line_type), status = coalesce($4, status),
                number = coalesce($5, number), operator = coalesce($6, operator) WHERE id = $1",
    )
    .bind(id)
    .bind(i.plan.as_deref().and_then(domain::text::clean))
    .bind(&line_type)
    .bind(&i.status)
    .bind(i.number.as_deref().map(|n| n.chars().filter(|c| !c.is_whitespace()).collect::<String>()).filter(|n| !n.is_empty()))
    .bind(i.operator.as_deref().and_then(domain::text::clean))
    .execute(&mut *tx)
    .await?
    .rows_affected();
    if n == 0 {
        return Err(AppError::not_found("ligne"));
    }
    audit::record(&mut tx, &user, Audit { action: "ligne_modifiee", entity: "mobile_line", entity_id: Some(id), data: Some(json!({ "status": i.status, "motif": reason, "numero": i.number, "operateur": i.operator })), ..Default::default() }).await?;
    tx.commit().await?;
    Ok(Json(json!({ "ok": true })))
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
                "INSERT INTO mobile_lines (number, operator, plan, line_type) VALUES ($1, $2, $3, $4)
                 ON CONFLICT (number) DO UPDATE SET plan = coalesce(EXCLUDED.plan, mobile_lines.plan),
                        line_type = coalesce(EXCLUDED.line_type, mobile_lines.line_type) RETURNING id",
            )
            .bind(&number)
            .bind(&operator)
            .bind(i.plan.as_deref().and_then(domain::text::clean))
            .bind(check_line_type(&i.line_type)?)
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
