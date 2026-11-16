//! Parc véhicules RH (M03, U4) : véhicules propres ou loués, contrats,
//! pièces (assurance, contrôle technique), entretien, remises,
//! restitutions et retour au loueur. Réservé au RH et à l'ADMIN.

use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::{DateTime, NaiveDate, Utc};
use domain::vehicles::{can_hand_over, check_odometer, manual_status_allowed, VehicleKind, VehicleStatus};
use domain::Role;
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::PgConnection;
use uuid::Uuid;

use crate::audit::{self, Audit};
use crate::auth::CurrentUser;
use crate::error::{AppError, AppResult};
use crate::idempotency::{self, Begin};
use crate::routes::reprise::{VehicleAssignmentData, VehicleData};
use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/vehicles", get(list).post(create))
        .route("/vehicles/deadlines", get(deadlines))
        .route("/vehicles/{id}", get(detail).patch(update))
        .route("/vehicles/{id}/leases", post(add_lease))
        .route("/vehicles/{id}/documents", post(add_document))
        .route("/vehicles/{id}/maintenance", post(add_maintenance))
        .route("/vehicles/{id}/handover", post(handover))
        .route("/vehicles/{id}/return", post(vehicle_return))
        .route("/vehicles/{id}/lessor-return", post(lessor_return))
        .route("/employees/{id}/vehicles", get(employee_vehicles))
}

pub const FLEET: &[Role] = &[Role::Rh, Role::Admin];

/// Immatriculation comparable : majuscules, chiffres et lettres seulement.
pub fn normalize_plate(raw: &str) -> String {
    raw.chars().filter(|c| c.is_alphanumeric()).collect::<String>().to_uppercase()
}

fn clean(v: &Option<String>) -> Option<String> {
    v.as_deref().and_then(domain::text::clean)
}

// ------------------------------------------------------------------ lecture

#[derive(Deserialize)]
struct ListQuery {
    kind: Option<String>,
    status: Option<String>,
    q: Option<String>,
}

const LIST_SQL: &str = "
    SELECT coalesce(jsonb_agg(x ORDER BY x->>'plate'), '[]') FROM (
      SELECT jsonb_build_object(
        'id', v.id, 'plate', v.plate, 'kind', v.kind, 'brand', v.brand, 'model', v.model, 'status', v.status,
        'odometer_km', v.odometer_km, 'site_name', s.name, 'version', v.version,
        'holder_id', e.id, 'holder_name', e.first_name || ' ' || e.last_name, 'holder_since', a.started_at,
        'lessor', l.lessor, 'lease_end', l.end_date,
        'insurance_until', (SELECT d.valid_until FROM vehicle_documents d WHERE d.vehicle_id = v.id AND d.kind = 'assurance'
                             ORDER BY d.valid_until DESC NULLS LAST, d.created_at DESC LIMIT 1),
        'inspection_until', (SELECT d.valid_until FROM vehicle_documents d WHERE d.vehicle_id = v.id AND d.kind = 'controle_technique'
                              ORDER BY d.valid_until DESC NULLS LAST, d.created_at DESC LIMIT 1),
        'next_maintenance_date', (SELECT min(m.next_due_date) FROM maintenance_events m
                                   WHERE m.vehicle_id = v.id AND m.id IN (SELECT DISTINCT ON (kind) id FROM maintenance_events
                                                                           WHERE vehicle_id = v.id ORDER BY kind, done_on DESC, created_at DESC)),
        'next_maintenance_km', (SELECT min(m.next_due_km) FROM maintenance_events m
                                 WHERE m.vehicle_id = v.id AND m.id IN (SELECT DISTINCT ON (kind) id FROM maintenance_events
                                                                         WHERE vehicle_id = v.id ORDER BY kind, done_on DESC, created_at DESC))
      ) AS x
      FROM vehicles v
      LEFT JOIN sites s ON s.id = v.site_id
      LEFT JOIN vehicle_assignments a ON a.vehicle_id = v.id AND a.ended_at IS NULL
      LEFT JOIN employees e ON e.id = a.employee_id
      LEFT JOIN vehicle_leases l ON l.vehicle_id = v.id AND l.status = 'en_cours'
      WHERE ($1::text IS NULL OR v.kind = $1)
        AND ($2::text IS NULL OR v.status = $2)
        AND ($3::text IS NULL OR v.plate_norm LIKE $3 OR lower(coalesce(v.brand, '') || ' ' || coalesce(v.model, '')) LIKE lower($4)
             OR lower(coalesce(e.first_name || ' ' || e.last_name, '')) LIKE lower($4))
    ) t";

async fn list(State(state): State<AppState>, user: CurrentUser, Query(q): Query<ListQuery>) -> AppResult<Json<Value>> {
    user.require(FLEET)?;
    let search = q.q.as_deref().and_then(domain::text::clean);
    let v: Value = sqlx::query_scalar(LIST_SQL)
        .bind(q.kind.filter(|k| !k.is_empty()))
        .bind(q.status.filter(|k| !k.is_empty()))
        .bind(search.as_deref().map(|s| format!("%{}%", normalize_plate(s))))
        .bind(search.as_deref().map(|s| format!("%{s}%")))
        .fetch_one(&state.pool)
        .await?;
    Ok(Json(v))
}

async fn detail(State(state): State<AppState>, user: CurrentUser, Path(id): Path<Uuid>) -> AppResult<Json<Value>> {
    user.require(FLEET)?;
    let v: Option<Value> = sqlx::query_scalar(
        "SELECT jsonb_build_object(
            'vehicle', jsonb_build_object('id', v.id, 'plate', v.plate, 'kind', v.kind, 'brand', v.brand, 'model', v.model,
                       'status', v.status, 'odometer_km', v.odometer_km, 'odometer_at', v.odometer_at, 'site_id', v.site_id,
                       'site_name', s.name, 'notes', v.notes, 'version', v.version),
            'holder', (SELECT jsonb_build_object('assignment_id', a.id, 'employee_id', e.id, 'name', e.first_name || ' ' || e.last_name,
                              'since', a.started_at, 'handover_reference', m.reference)
                         FROM vehicle_assignments a JOIN employees e ON e.id = a.employee_id JOIN vehicle_movements m ON m.id = a.handover_id
                        WHERE a.vehicle_id = v.id AND a.ended_at IS NULL),
            'leases', (SELECT coalesce(jsonb_agg(jsonb_build_object('id', l.id, 'lessor', l.lessor, 'contract_no', l.contract_no,
                              'start_date', l.start_date, 'end_date', l.end_date, 'status', l.status, 'returned_on', l.returned_on,
                              'created_at', l.created_at) ORDER BY l.created_at DESC), '[]')
                         FROM vehicle_leases l WHERE l.vehicle_id = v.id),
            'documents', (SELECT coalesce(jsonb_agg(jsonb_build_object('id', d.id, 'kind', d.kind, 'reference', d.reference,
                              'valid_from', d.valid_from, 'valid_until', d.valid_until, 'note', d.note, 'created_at', d.created_at,
                              'current', d.id = (SELECT d2.id FROM vehicle_documents d2 WHERE d2.vehicle_id = v.id AND d2.kind = d.kind
                                                  ORDER BY d2.valid_until DESC NULLS LAST, d2.created_at DESC LIMIT 1))
                              ORDER BY d.kind, d.created_at DESC), '[]')
                         FROM vehicle_documents d WHERE d.vehicle_id = v.id),
            'maintenance', (SELECT coalesce(jsonb_agg(jsonb_build_object('id', m.id, 'kind', m.kind, 'done_on', m.done_on,
                              'odometer_km', m.odometer_km, 'description', m.description, 'provider', m.provider,
                              'next_due_date', m.next_due_date, 'next_due_km', m.next_due_km) ORDER BY m.done_on DESC, m.created_at DESC), '[]')
                         FROM maintenance_events m WHERE m.vehicle_id = v.id),
            'movements', (SELECT coalesce(jsonb_agg(jsonb_build_object('id', m.id, 'reference', m.reference, 'kind', m.kind,
                              'effective_at', m.effective_at, 'odometer_km', m.odometer_km, 'odometer_reason', m.odometer_reason,
                              'condition', m.condition, 'keys_count', m.keys_count, 'fuel_level', m.fuel_level, 'note', m.note,
                              'employee_id', e.id, 'employee', e.first_name || ' ' || e.last_name, 'operator', u.display_name,
                              'case_reference', c.reference) ORDER BY m.effective_at DESC, m.created_at DESC), '[]')
                         FROM vehicle_movements m LEFT JOIN employees e ON e.id = m.employee_id JOIN users u ON u.id = m.operator_id
                         LEFT JOIN cases c ON c.id = m.case_id
                        WHERE m.vehicle_id = v.id))
           FROM vehicles v LEFT JOIN sites s ON s.id = v.site_id WHERE v.id = $1",
    )
    .bind(id)
    .fetch_optional(&state.pool)
    .await?;
    Ok(Json(v.ok_or_else(|| AppError::not_found("véhicule"))?))
}

async fn employee_vehicles(State(state): State<AppState>, user: CurrentUser, Path(id): Path<Uuid>) -> AppResult<Json<Value>> {
    user.require(FLEET)?;
    let mut conn = state.pool.acquire().await?;
    Ok(Json(held_vehicles_json(&mut conn, id).await?))
}

/// Véhicules actuellement confiés au salarié.
pub async fn held_vehicles_json(conn: &mut PgConnection, employee_id: Uuid) -> AppResult<Value> {
    Ok(sqlx::query_scalar(
        "SELECT coalesce(jsonb_agg(jsonb_build_object('assignment_id', a.id, 'vehicle_id', v.id, 'plate', v.plate,
                    'label', trim(coalesce(v.brand, '') || ' ' || coalesce(v.model, '')), 'kind', v.kind,
                    'since', a.started_at, 'handover_reference', m.reference, 'km_start', m.odometer_km) ORDER BY a.started_at), '[]')
           FROM vehicle_assignments a JOIN vehicles v ON v.id = a.vehicle_id JOIN vehicle_movements m ON m.id = a.handover_id
          WHERE a.employee_id = $1 AND a.ended_at IS NULL",
    )
    .bind(employee_id)
    .fetch_one(conn)
    .await?)
}

#[derive(Deserialize)]
struct DeadlineQuery {
    days: Option<i32>,
}

/// Échéances : pièces, entretiens et fins de location à venir ou dépassées.
async fn deadlines(State(state): State<AppState>, user: CurrentUser, Query(q): Query<DeadlineQuery>) -> AppResult<Json<Value>> {
    user.require(FLEET)?;
    let days = q.days.unwrap_or(state.cfg().alert_days).clamp(1, 365);
    let mut conn = state.pool.acquire().await?;
    Ok(Json(deadlines_json(&mut conn, crate::documents::algiers_today(), days, state.cfg().alert_km).await?))
}

pub async fn deadlines_json(conn: &mut PgConnection, today: NaiveDate, days: i32, warn_km: i32) -> AppResult<Value> {
    Ok(sqlx::query_scalar(
        "WITH docs AS (
            SELECT DISTINCT ON (d.vehicle_id, d.kind) d.id, d.vehicle_id, d.kind, d.valid_until, d.reference
              FROM vehicle_documents d ORDER BY d.vehicle_id, d.kind, d.valid_until DESC NULLS LAST, d.created_at DESC),
         maint AS (
            SELECT DISTINCT ON (m.vehicle_id, m.kind) m.id, m.vehicle_id, m.kind, m.next_due_date, m.next_due_km
              FROM maintenance_events m ORDER BY m.vehicle_id, m.kind, m.done_on DESC, m.created_at DESC),
         items AS (
            SELECT 'document' AS source, docs.id, docs.vehicle_id, docs.kind, docs.valid_until AS due_date, NULL::int AS due_km,
                   docs.reference AS detail
              FROM docs WHERE docs.valid_until IS NOT NULL AND docs.valid_until <= $1::date + $2::int
            UNION ALL
            SELECT 'entretien', maint.id, maint.vehicle_id, maint.kind, maint.next_due_date, maint.next_due_km, NULL
              FROM maint JOIN vehicles v ON v.id = maint.vehicle_id
             WHERE (maint.next_due_date IS NOT NULL AND maint.next_due_date <= $1::date + $2::int)
                OR (maint.next_due_km IS NOT NULL AND v.odometer_km IS NOT NULL AND maint.next_due_km - v.odometer_km <= $3)
            UNION ALL
            SELECT 'location', l.id, l.vehicle_id, 'fin_location', l.end_date, NULL, l.lessor
              FROM vehicle_leases l WHERE l.status = 'en_cours' AND l.end_date IS NOT NULL AND l.end_date <= $1::date + $2::int)
         SELECT coalesce(jsonb_agg(jsonb_build_object('source', i.source, 'id', i.id, 'vehicle_id', v.id, 'plate', v.plate,
                    'kind', i.kind, 'due_date', i.due_date, 'due_km', i.due_km, 'odometer_km', v.odometer_km, 'detail', i.detail,
                    'overdue', (i.due_date IS NOT NULL AND i.due_date < $1::date)
                               OR (i.due_km IS NOT NULL AND v.odometer_km IS NOT NULL AND v.odometer_km >= i.due_km),
                    'holder', (SELECT e.first_name || ' ' || e.last_name FROM vehicle_assignments a JOIN employees e ON e.id = a.employee_id
                                WHERE a.vehicle_id = v.id AND a.ended_at IS NULL))
                    ORDER BY i.due_date NULLS LAST, v.plate), '[]')
           FROM items i JOIN vehicles v ON v.id = i.vehicle_id
          WHERE v.status NOT IN ('rendu_loueur', 'reforme')",
    )
    .bind(today)
    .bind(days)
    .bind(warn_km)
    .fetch_one(conn)
    .await?)
}

// ------------------------------------------------------------------ fiche véhicule

#[derive(Deserialize)]
struct LeaseInput {
    lessor: String,
    contract_no: Option<String>,
    start_date: Option<NaiveDate>,
    end_date: Option<NaiveDate>,
}

#[derive(Deserialize)]
struct CreateInput {
    plate: String,
    kind: VehicleKind,
    brand: Option<String>,
    model: Option<String>,
    site_id: Option<Uuid>,
    odometer_km: Option<i32>,
    notes: Option<String>,
    lease: Option<LeaseInput>,
}

async fn insert_lease(conn: &mut PgConnection, user_id: Option<Uuid>, vehicle_id: Uuid, l: &LeaseInput) -> AppResult<Uuid> {
    let lessor = domain::text::clean(&l.lessor).ok_or_else(|| AppError::invalid("lessor", "loueur obligatoire"))?;
    Ok(sqlx::query_scalar(
        "INSERT INTO vehicle_leases (vehicle_id, lessor, contract_no, start_date, end_date, created_by)
         VALUES ($1, $2, $3, $4, $5, $6) RETURNING id",
    )
    .bind(vehicle_id)
    .bind(lessor)
    .bind(clean(&l.contract_no))
    .bind(l.start_date)
    .bind(l.end_date)
    .bind(user_id)
    .fetch_one(conn)
    .await?)
}

async fn create(State(state): State<AppState>, user: CurrentUser, Json(i): Json<CreateInput>) -> AppResult<Json<Value>> {
    user.require(FLEET)?;
    let plate = domain::text::clean(&i.plate).ok_or_else(|| AppError::invalid("plate", "immatriculation obligatoire"))?.to_uppercase();
    if i.kind == VehicleKind::Location && i.lease.is_none() {
        return Err(AppError::invalid("lease", "véhicule loué : indiquez le loueur et le contrat"));
    }
    let mut tx = state.pool.begin().await?;
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO vehicles (plate, plate_norm, kind, brand, model, site_id, odometer_km, odometer_at, notes)
         VALUES ($1, $2, $3, $4, $5, $6, $7, CASE WHEN $7::int IS NULL THEN NULL ELSE now() END, $8) RETURNING id",
    )
    .bind(&plate)
    .bind(normalize_plate(&plate))
    .bind(i.kind.as_str())
    .bind(clean(&i.brand))
    .bind(clean(&i.model))
    .bind(i.site_id)
    .bind(i.odometer_km)
    .bind(clean(&i.notes))
    .fetch_one(&mut *tx)
    .await
    .map_err(|e| match AppError::from(e) {
        AppError::Conflict { .. } => AppError::conflict(format!("le véhicule {plate} existe déjà")),
        other => other,
    })?;
    if let (VehicleKind::Location, Some(l)) = (i.kind, &i.lease) {
        insert_lease(&mut tx, Some(user.id), id, l).await?;
    }
    audit::record(&mut tx, &user, Audit { action: "vehicule_cree", entity: "vehicle", entity_id: Some(id), data: Some(json!({ "plate": plate })), ..Default::default() }).await?;
    tx.commit().await?;
    Ok(Json(json!({ "id": id })))
}

#[derive(Deserialize)]
struct UpdateInput {
    version: i32,
    brand: Option<String>,
    model: Option<String>,
    site_id: Option<Uuid>,
    notes: Option<String>,
    status: Option<VehicleStatus>,
    reason: Option<String>,
}

async fn update(State(state): State<AppState>, user: CurrentUser, Path(id): Path<Uuid>, Json(i): Json<UpdateInput>) -> AppResult<Json<Value>> {
    user.require(FLEET)?;
    let mut tx = state.pool.begin().await?;
    let row: Option<(i32, String)> = sqlx::query_as("SELECT version, status FROM vehicles WHERE id = $1 FOR UPDATE")
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?;
    let (version, status) = row.ok_or_else(|| AppError::not_found("véhicule"))?;
    if version != i.version {
        return Err(AppError::stale());
    }
    let current = VehicleStatus::parse(&status).ok_or_else(|| anyhow::anyhow!("statut de véhicule inconnu"))?;
    let reason = clean(&i.reason);
    if let Some(to) = i.status {
        manual_status_allowed(current, to)?;
        if to != current && reason.is_none() {
            return Err(AppError::invalid("reason", "motif du changement d'état obligatoire"));
        }
    }
    sqlx::query(
        "UPDATE vehicles SET brand = coalesce($2, brand), model = coalesce($3, model), site_id = coalesce($4, site_id),
                notes = coalesce($5, notes), status = coalesce($6, status), version = version + 1, updated_at = now()
          WHERE id = $1",
    )
    .bind(id)
    .bind(clean(&i.brand))
    .bind(clean(&i.model))
    .bind(i.site_id)
    .bind(clean(&i.notes))
    .bind(i.status.map(|s| s.as_str()))
    .execute(&mut *tx)
    .await?;
    audit::record(&mut tx, &user, Audit { action: "vehicule_modifie", entity: "vehicle", entity_id: Some(id), data: Some(json!({ "status": i.status, "motif": reason })), ..Default::default() }).await?;
    tx.commit().await?;
    Ok(Json(json!({ "version": version + 1 })))
}

#[derive(Deserialize)]
struct AddLeaseInput {
    #[serde(flatten)]
    lease: LeaseInput,
}

/// Nouveau contrat ou renouvellement : le contrat en cours est conservé
/// comme « renouvelé ».
async fn add_lease(State(state): State<AppState>, user: CurrentUser, Path(id): Path<Uuid>, Json(i): Json<AddLeaseInput>) -> AppResult<Json<Value>> {
    user.require(FLEET)?;
    let mut tx = state.pool.begin().await?;
    let row: Option<(String, String)> = sqlx::query_as("SELECT kind, status FROM vehicles WHERE id = $1 FOR UPDATE")
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?;
    let (kind, status) = row.ok_or_else(|| AppError::not_found("véhicule"))?;
    if kind != "location" {
        return Err(AppError::conflict("véhicule en propriété : pas de contrat de location"));
    }
    if status == "rendu_loueur" {
        return Err(AppError::conflict("véhicule rendu au loueur"));
    }
    let renewed = sqlx::query("UPDATE vehicle_leases SET status = 'renouvele' WHERE vehicle_id = $1 AND status = 'en_cours'")
        .bind(id)
        .execute(&mut *tx)
        .await?
        .rows_affected();
    let lease_id = insert_lease(&mut tx, Some(user.id), id, &i.lease).await?;
    audit::record(&mut tx, &user, Audit { action: if renewed > 0 { "location_renouvelee" } else { "location_enregistree" }, entity: "vehicle", entity_id: Some(id), data: Some(json!({ "lease": lease_id })), ..Default::default() }).await?;
    tx.commit().await?;
    Ok(Json(json!({ "id": lease_id, "renewed": renewed > 0 })))
}

#[derive(Deserialize)]
struct DocumentInput {
    kind: String,
    reference: Option<String>,
    valid_from: Option<NaiveDate>,
    valid_until: Option<NaiveDate>,
    note: Option<String>,
}

/// Une pièce renouvelée s'ajoute ; la précédente reste dans l'historique.
async fn add_document(State(state): State<AppState>, user: CurrentUser, Path(id): Path<Uuid>, Json(i): Json<DocumentInput>) -> AppResult<Json<Value>> {
    user.require(FLEET)?;
    if !["assurance", "controle_technique", "carte_grise", "vignette", "autre"].contains(&i.kind.as_str()) {
        return Err(AppError::invalid("kind", "nature de pièce inconnue"));
    }
    let mut tx = state.pool.begin().await?;
    let doc: Uuid = sqlx::query_scalar(
        "INSERT INTO vehicle_documents (vehicle_id, kind, reference, valid_from, valid_until, note, created_by)
         VALUES ($1, $2, $3, $4, $5, $6, $7) RETURNING id",
    )
    .bind(id)
    .bind(&i.kind)
    .bind(clean(&i.reference))
    .bind(i.valid_from)
    .bind(i.valid_until)
    .bind(clean(&i.note))
    .bind(user.id)
    .fetch_one(&mut *tx)
    .await
    .map_err(|e| match AppError::from(e) {
        AppError::Internal(_) => AppError::not_found("véhicule"),
        other => other,
    })?;
    audit::record(&mut tx, &user, Audit { action: "piece_vehicule", entity: "vehicle", entity_id: Some(id), data: Some(json!({ "kind": i.kind, "valid_until": i.valid_until })), ..Default::default() }).await?;
    tx.commit().await?;
    Ok(Json(json!({ "id": doc })))
}

#[derive(Deserialize)]
struct MaintenanceInput {
    kind: String,
    done_on: NaiveDate,
    odometer_km: Option<i32>,
    odometer_reason: Option<String>,
    description: Option<String>,
    provider: Option<String>,
    next_due_date: Option<NaiveDate>,
    next_due_km: Option<i32>,
}

async fn add_maintenance(State(state): State<AppState>, user: CurrentUser, Path(id): Path<Uuid>, Json(i): Json<MaintenanceInput>) -> AppResult<Json<Value>> {
    user.require(FLEET)?;
    if !["vidange", "entretien", "reparation", "pneus", "autre"].contains(&i.kind.as_str()) {
        return Err(AppError::invalid("kind", "nature d'intervention inconnue"));
    }
    if i.done_on > crate::documents::algiers_today() {
        return Err(AppError::invalid("done_on", "une intervention réalisée ne peut pas être datée dans le futur"));
    }
    let mut tx = state.pool.begin().await?;
    let odometer: Option<Option<i32>> = sqlx::query_scalar("SELECT odometer_km FROM vehicles WHERE id = $1 FOR UPDATE")
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?;
    let odometer = odometer.ok_or_else(|| AppError::not_found("véhicule"))?;
    let reason = clean(&i.odometer_reason);
    let corrected = check_odometer(odometer, i.odometer_km, reason.as_deref())?;
    let event: Uuid = sqlx::query_scalar(
        "INSERT INTO maintenance_events (vehicle_id, kind, done_on, odometer_km, description, provider, next_due_date, next_due_km, created_by)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9) RETURNING id",
    )
    .bind(id)
    .bind(&i.kind)
    .bind(i.done_on)
    .bind(i.odometer_km)
    .bind(clean(&i.description))
    .bind(clean(&i.provider))
    .bind(i.next_due_date)
    .bind(i.next_due_km)
    .bind(user.id)
    .fetch_one(&mut *tx)
    .await?;
    update_odometer(&mut tx, id, i.odometer_km, odometer, corrected).await?;
    audit::record(&mut tx, &user, Audit { action: if corrected { "km_corrige" } else { "entretien_vehicule" }, entity: "vehicle", entity_id: Some(id), data: Some(json!({ "kind": i.kind, "km": i.odometer_km, "motif": reason })), ..Default::default() }).await?;
    tx.commit().await?;
    Ok(Json(json!({ "id": event })))
}

async fn update_odometer(conn: &mut PgConnection, id: Uuid, reading: Option<i32>, last: Option<i32>, corrected: bool) -> AppResult<()> {
    if let Some(km) = reading {
        if corrected || last.map(|l| km >= l).unwrap_or(true) {
            sqlx::query("UPDATE vehicles SET odometer_km = $2, odometer_at = now(), version = version + 1, updated_at = now() WHERE id = $1")
                .bind(id)
                .bind(km)
                .execute(conn)
                .await?;
        }
    }
    Ok(())
}

// ------------------------------------------------------------------ mouvements

async fn new_movement_ref(conn: &mut PgConnection) -> AppResult<String> {
    let n: i64 = sqlx::query_scalar("SELECT nextval('vehicle_movement_ref')").fetch_one(conn).await?;
    Ok(format!("VEH-{n:05}"))
}

#[derive(Deserialize)]
struct HandoverInput {
    employee_id: Uuid,
    case_id: Option<Uuid>,
    task_id: Option<Uuid>,
    effective_at: Option<DateTime<Utc>>,
    odometer_km: i32,
    odometer_reason: Option<String>,
    condition: Option<String>,
    keys_count: Option<i32>,
    fuel_level: Option<String>,
    note: Option<String>,
}

/// Remise d'un véhicule : date, kilométrage, état, clés et observations.
async fn handover(
    State(state): State<AppState>,
    user: CurrentUser,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(raw): Json<Value>,
) -> AppResult<Response> {
    user.require(FLEET)?;
    let i: HandoverInput = serde_json::from_value(raw.clone()).map_err(|e| AppError::invalid("body", format!("saisie incomplète : {e}")))?;
    let key = idempotency::key_from(&headers)?;
    let endpoint = format!("POST /vehicles/{id}/handover");
    let mut tx = state.pool.begin().await?;
    if let Some(k) = &key {
        if let Begin::Replay(status, body) = idempotency::begin(&mut tx, user.id, k, &endpoint, &idempotency::hash_request(&endpoint, &raw)).await? {
            tx.commit().await?;
            return Ok((status, Json(body)).into_response());
        }
    }
    let row: Option<(String, Option<i32>, String)> = sqlx::query_as("SELECT status, odometer_km, plate FROM vehicles WHERE id = $1 FOR UPDATE")
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?;
    let (status, odometer, plate) = row.ok_or_else(|| AppError::not_found("véhicule"))?;
    can_hand_over(VehicleStatus::parse(&status).ok_or_else(|| anyhow::anyhow!("statut inconnu"))?)?;
    let emp: Option<String> = sqlx::query_scalar("SELECT status FROM employees WHERE id = $1 FOR SHARE")
        .bind(i.employee_id)
        .fetch_optional(&mut *tx)
        .await?;
    match emp.as_deref() {
        None => return Err(AppError::not_found("salarié")),
        Some("sorti") => return Err(AppError::conflict("salarié sorti : aucune remise possible")),
        _ => {}
    }
    if let Some(case_id) = i.case_id {
        let row: Option<(Uuid, String)> = sqlx::query_as("SELECT employee_id, status FROM cases WHERE id = $1")
            .bind(case_id)
            .fetch_optional(&mut *tx)
            .await?;
        let (emp, st) = row.ok_or_else(|| AppError::invalid("case_id", "dossier introuvable"))?;
        if emp != i.employee_id {
            return Err(AppError::invalid("case_id", "le dossier concerne un autre salarié"));
        }
        if st != "ouvert" {
            return Err(AppError::conflict("dossier clos"));
        }
    }
    if let Some(task_id) = i.task_id {
        let task_case: Option<Uuid> = sqlx::query_scalar("SELECT case_id FROM tasks WHERE id = $1").bind(task_id).fetch_optional(&mut *tx).await?;
        if task_case.is_none() || task_case != i.case_id {
            return Err(AppError::invalid("task_id", "la tâche n'appartient pas à ce dossier"));
        }
    }
    let reason = clean(&i.odometer_reason);
    let corrected = check_odometer(odometer, Some(i.odometer_km), reason.as_deref())?;
    let at = i.effective_at.unwrap_or_else(Utc::now);
    let reference = new_movement_ref(&mut tx).await?;
    let movement: Uuid = sqlx::query_scalar(
        "INSERT INTO vehicle_movements (reference, vehicle_id, kind, employee_id, case_id, task_id, effective_at, odometer_km,
                                        odometer_reason, condition, keys_count, fuel_level, note, operator_id)
         VALUES ($1, $2, 'remise', $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13) RETURNING id",
    )
    .bind(&reference)
    .bind(id)
    .bind(i.employee_id)
    .bind(i.case_id)
    .bind(i.task_id)
    .bind(at)
    .bind(i.odometer_km)
    .bind(&reason)
    .bind(clean(&i.condition))
    .bind(i.keys_count)
    .bind(clean(&i.fuel_level))
    .bind(clean(&i.note))
    .bind(user.id)
    .fetch_one(&mut *tx)
    .await?;
    sqlx::query("INSERT INTO vehicle_assignments (vehicle_id, employee_id, handover_id, started_at) VALUES ($1, $2, $3, $4)")
        .bind(id)
        .bind(i.employee_id)
        .bind(movement)
        .bind(at)
        .execute(&mut *tx)
        .await?;
    sqlx::query("UPDATE vehicles SET status = 'affecte', version = version + 1, updated_at = now() WHERE id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    update_odometer(&mut tx, id, Some(i.odometer_km), odometer, corrected).await?;
    audit::record(
        &mut tx,
        &user,
        Audit {
            action: "vehicule_remis",
            entity: "vehicle_movement",
            entity_id: Some(movement),
            case_id: i.case_id,
            employee_id: Some(i.employee_id),
            data: Some(json!({ "reference": reference, "plate": plate, "km": i.odometer_km, "km_corrige": corrected, "motif_km": reason })),
        },
    )
    .await?;
    let response = json!({ "movement_id": movement, "reference": reference });
    if let Some(k) = &key {
        idempotency::finish(&mut tx, user.id, k, StatusCode::CREATED, &response).await?;
    }
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(response)).into_response())
}

#[derive(Deserialize)]
struct ReturnInput {
    effective_at: Option<DateTime<Utc>>,
    odometer_km: i32,
    odometer_reason: Option<String>,
    condition: Option<String>,
    keys_count: Option<i32>,
    fuel_level: Option<String>,
    note: Option<String>,
    case_id: Option<Uuid>,
    /// `celluloplast` (défaut) ou `loueur` : un véhicule loué peut repartir
    /// directement chez le loueur (deux mouvements distincts).
    destination: Option<String>,
}

/// Restitution par le titulaire : le véhicule revient chez Celluloplast,
/// ou (location) part chez le loueur dans un second mouvement.
async fn vehicle_return(
    State(state): State<AppState>,
    user: CurrentUser,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(raw): Json<Value>,
) -> AppResult<Response> {
    user.require(FLEET)?;
    let i: ReturnInput = serde_json::from_value(raw.clone()).map_err(|e| AppError::invalid("body", format!("saisie incomplète : {e}")))?;
    let to_lessor = match i.destination.as_deref() {
        None | Some("celluloplast") => false,
        Some("loueur") => true,
        Some(_) => return Err(AppError::invalid("destination", "celluloplast ou loueur")),
    };
    let key = idempotency::key_from(&headers)?;
    let endpoint = format!("POST /vehicles/{id}/return");
    let mut tx = state.pool.begin().await?;
    if let Some(k) = &key {
        if let Begin::Replay(status, body) = idempotency::begin(&mut tx, user.id, k, &endpoint, &idempotency::hash_request(&endpoint, &raw)).await? {
            tx.commit().await?;
            return Ok((status, Json(body)).into_response());
        }
    }
    let row: Option<(String, Option<i32>, String)> = sqlx::query_as("SELECT kind, odometer_km, plate FROM vehicles WHERE id = $1 FOR UPDATE")
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?;
    let (kind, odometer, plate) = row.ok_or_else(|| AppError::not_found("véhicule"))?;
    let held: Option<(Uuid, Uuid, Uuid)> = sqlx::query_as(
        "SELECT id, employee_id, handover_id FROM vehicle_assignments WHERE vehicle_id = $1 AND ended_at IS NULL FOR UPDATE",
    )
    .bind(id)
    .fetch_optional(&mut *tx)
    .await?;
    let (assignment, employee, origin) = held.ok_or_else(|| AppError::conflict("véhicule non affecté : rien à restituer"))?;
    if to_lessor && kind != "location" {
        return Err(AppError::invalid("destination", "véhicule en propriété : pas de retour au loueur"));
    }
    let reason = clean(&i.odometer_reason);
    let corrected = check_odometer(odometer, Some(i.odometer_km), reason.as_deref())?;
    let at = i.effective_at.unwrap_or_else(Utc::now);
    let reference = new_movement_ref(&mut tx).await?;
    let movement: Uuid = sqlx::query_scalar(
        "INSERT INTO vehicle_movements (reference, vehicle_id, kind, employee_id, case_id, effective_at, odometer_km, odometer_reason,
                                        condition, keys_count, fuel_level, note, operator_id, origin_movement_id)
         VALUES ($1, $2, 'restitution', $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13) RETURNING id",
    )
    .bind(&reference)
    .bind(id)
    .bind(employee)
    .bind(i.case_id)
    .bind(at)
    .bind(i.odometer_km)
    .bind(&reason)
    .bind(clean(&i.condition))
    .bind(i.keys_count)
    .bind(clean(&i.fuel_level))
    .bind(clean(&i.note))
    .bind(user.id)
    .bind(origin)
    .fetch_one(&mut *tx)
    .await?;
    sqlx::query("UPDATE vehicle_assignments SET ended_at = $2, return_id = $3 WHERE id = $1")
        .bind(assignment)
        .bind(at)
        .bind(movement)
        .execute(&mut *tx)
        .await?;
    crate::routes::departures::on_returned(&mut tx, &user, None, Some(assignment), &reference).await?;
    sqlx::query("UPDATE vehicles SET status = 'disponible', version = version + 1, updated_at = now() WHERE id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    update_odometer(&mut tx, id, Some(i.odometer_km), odometer, corrected).await?;
    audit::record(
        &mut tx,
        &user,
        Audit {
            action: "vehicule_restitue",
            entity: "vehicle_movement",
            entity_id: Some(movement),
            case_id: i.case_id,
            employee_id: Some(employee),
            data: Some(json!({ "reference": reference, "plate": plate, "km": i.odometer_km, "km_corrige": corrected })),
        },
    )
    .await?;
    let mut response = json!({ "movement_id": movement, "reference": reference });
    if to_lessor {
        let second = return_to_lessor(&mut tx, &user, id, at, Some(i.odometer_km), None, Some("Retour au loueur à la restitution".into())).await?;
        response["lessor_reference"] = json!(second);
    }
    if let Some(k) = &key {
        idempotency::finish(&mut tx, user.id, k, StatusCode::CREATED, &response).await?;
    }
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(response)).into_response())
}

async fn return_to_lessor(
    conn: &mut PgConnection,
    user: &CurrentUser,
    id: Uuid,
    at: DateTime<Utc>,
    odometer_km: Option<i32>,
    condition: Option<String>,
    note: Option<String>,
) -> AppResult<String> {
    let lease: Option<Uuid> = sqlx::query_scalar("SELECT id FROM vehicle_leases WHERE vehicle_id = $1 AND status = 'en_cours' FOR UPDATE")
        .bind(id)
        .fetch_optional(&mut *conn)
        .await?;
    let reference = new_movement_ref(conn).await?;
    let movement: Uuid = sqlx::query_scalar(
        "INSERT INTO vehicle_movements (reference, vehicle_id, kind, effective_at, odometer_km, condition, note, operator_id)
         VALUES ($1, $2, 'retour_loueur', $3, $4, $5, $6, $7) RETURNING id",
    )
    .bind(&reference)
    .bind(id)
    .bind(at)
    .bind(odometer_km)
    .bind(condition)
    .bind(note)
    .bind(user.id)
    .fetch_one(&mut *conn)
    .await?;
    if let Some(l) = lease {
        sqlx::query("UPDATE vehicle_leases SET status = 'restitue', returned_on = ($2 AT TIME ZONE 'Africa/Algiers')::date WHERE id = $1")
            .bind(l)
            .bind(at)
            .execute(&mut *conn)
            .await?;
    }
    sqlx::query("UPDATE vehicles SET status = 'rendu_loueur', version = version + 1, updated_at = now() WHERE id = $1")
        .bind(id)
        .execute(&mut *conn)
        .await?;
    audit::record(conn, user, Audit { action: "vehicule_rendu_loueur", entity: "vehicle_movement", entity_id: Some(movement), data: Some(json!({ "reference": reference, "vehicle": id })), ..Default::default() }).await?;
    Ok(reference)
}

#[derive(Deserialize)]
struct LessorReturnInput {
    effective_at: Option<DateTime<Utc>>,
    odometer_km: Option<i32>,
    odometer_reason: Option<String>,
    condition: Option<String>,
    note: Option<String>,
}

/// Retour au loueur par le RH : distinct d'une restitution par un salarié ;
/// le véhicule sort définitivement des disponibilités.
async fn lessor_return(State(state): State<AppState>, user: CurrentUser, Path(id): Path<Uuid>, Json(i): Json<LessorReturnInput>) -> AppResult<Json<Value>> {
    user.require(FLEET)?;
    let mut tx = state.pool.begin().await?;
    let row: Option<(String, String, Option<i32>)> = sqlx::query_as("SELECT kind, status, odometer_km FROM vehicles WHERE id = $1 FOR UPDATE")
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?;
    let (kind, status, odometer) = row.ok_or_else(|| AppError::not_found("véhicule"))?;
    if kind != "location" {
        return Err(AppError::conflict("véhicule en propriété : pas de retour au loueur"));
    }
    match status.as_str() {
        "affecte" => return Err(AppError::conflict("véhicule encore confié à un salarié : enregistrez d'abord sa restitution")),
        "rendu_loueur" => return Err(AppError::conflict("véhicule déjà rendu au loueur")),
        _ => {}
    }
    let reason = clean(&i.odometer_reason);
    let corrected = check_odometer(odometer, i.odometer_km, reason.as_deref())?;
    let at = i.effective_at.unwrap_or_else(Utc::now);
    let reference = return_to_lessor(&mut tx, &user, id, at, i.odometer_km, clean(&i.condition), clean(&i.note)).await?;
    update_odometer(&mut tx, id, i.odometer_km, odometer, corrected).await?;
    tx.commit().await?;
    Ok(Json(json!({ "reference": reference })))
}

// ------------------------------------------------------------------ reprise

pub async fn apply_vehicle_reprise(conn: &mut PgConnection, user: &CurrentUser, d: &VehicleData, target: Option<Uuid>) -> AppResult<Uuid> {
    let id: Uuid = match target {
        Some(id) => {
            sqlx::query(
                "UPDATE vehicles SET brand = coalesce($2, brand), model = coalesce($3, model),
                        odometer_km = GREATEST(odometer_km, $4), status = CASE WHEN status IN ('affecte', 'rendu_loueur') THEN status ELSE $5 END,
                        version = version + 1, updated_at = now()
                  WHERE id = $1",
            )
            .bind(id)
            .bind(&d.brand)
            .bind(&d.model)
            .bind(d.odometer_km)
            .bind(&d.status)
            .execute(&mut *conn)
            .await?;
            id
        }
        None => {
            sqlx::query_scalar(
                "INSERT INTO vehicles (plate, plate_norm, kind, brand, model, odometer_km, odometer_at, status)
                 VALUES ($1, $2, $3, $4, $5, $6, CASE WHEN $6::int IS NULL THEN NULL ELSE now() END, $7) RETURNING id",
            )
            .bind(d.plate.to_uppercase())
            .bind(&d.plate_norm)
            .bind(&d.kind)
            .bind(&d.brand)
            .bind(&d.model)
            .bind(d.odometer_km)
            .bind(&d.status)
            .fetch_one(&mut *conn)
            .await?
        }
    };
    if d.kind == "location" {
        let has_lease: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM vehicle_leases WHERE vehicle_id = $1)")
            .bind(id)
            .fetch_one(&mut *conn)
            .await?;
        if !has_lease {
            let returned = d.status == "rendu_loueur";
            sqlx::query(
                "INSERT INTO vehicle_leases (vehicle_id, lessor, end_date, status, returned_on, created_by)
                 VALUES ($1, $2, $3, $4, $5, $6)",
            )
            .bind(id)
            .bind(d.lessor.clone().unwrap_or_else(|| "Loueur à préciser".into()))
            .bind(d.lease_end)
            .bind(if returned { "restitue" } else { "en_cours" })
            .bind(returned.then(|| d.lease_end.unwrap_or_else(|| Utc::now().date_naive())))
            .bind(user.id)
            .execute(&mut *conn)
            .await?;
        }
    }
    for (kind, until) in [("assurance", d.insurance_until), ("controle_technique", d.inspection_until)] {
        if let Some(u) = until {
            sqlx::query(
                "INSERT INTO vehicle_documents (vehicle_id, kind, valid_until, note, created_by)
                 SELECT $1, $2, $3, 'Reprise SharePoint', $4
                  WHERE NOT EXISTS (SELECT 1 FROM vehicle_documents WHERE vehicle_id = $1 AND kind = $2 AND valid_until = $3)",
            )
            .bind(id)
            .bind(kind)
            .bind(u)
            .bind(user.id)
            .execute(&mut *conn)
            .await?;
        }
    }
    if let Some(holder) = d.holder_id {
        let held: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM vehicle_assignments WHERE vehicle_id = $1 AND ended_at IS NULL)")
            .bind(id)
            .fetch_one(&mut *conn)
            .await?;
        if !held {
            reprise_vehicle_movement(conn, user, id, holder, None, None, d.odometer_km, None, None, None).await?;
        }
    }
    Ok(id)
}

pub async fn apply_vehicle_assignment_reprise(conn: &mut PgConnection, user: &CurrentUser, d: &VehicleAssignmentData) -> AppResult<Uuid> {
    let employee = d.employee_id.ok_or_else(|| AppError::invalid("employe", "salarié à choisir"))?;
    reprise_vehicle_movement(conn, user, d.vehicle_id, employee, d.started_at, d.ended_at, d.km_start, d.km_end, d.condition_start.clone(), d.note_end.clone()).await
}

#[allow(clippy::too_many_arguments)]
async fn reprise_vehicle_movement(
    conn: &mut PgConnection,
    user: &CurrentUser,
    vehicle_id: Uuid,
    employee_id: Uuid,
    started_at: Option<DateTime<Utc>>,
    ended_at: Option<DateTime<Utc>>,
    km_start: Option<i32>,
    km_end: Option<i32>,
    condition: Option<String>,
    note_end: Option<String>,
) -> AppResult<Uuid> {
    let status: String = sqlx::query_scalar("SELECT status FROM vehicles WHERE id = $1 FOR UPDATE").bind(vehicle_id).fetch_one(&mut *conn).await?;
    if ended_at.is_none() {
        if status != "disponible" {
            return Err(AppError::conflict(format!("véhicule « {status} » : garde active non reprise")));
        }
    }
    let at = started_at.unwrap_or_else(Utc::now);
    let reference = new_movement_ref(conn).await?;
    let handover: Uuid = sqlx::query_scalar(
        "INSERT INTO vehicle_movements (reference, vehicle_id, kind, employee_id, effective_at, odometer_km, condition, note, operator_id)
         VALUES ($1, $2, 'reprise', $3, $4, $5, $6, 'Reprise SharePoint', $7) RETURNING id",
    )
    .bind(&reference)
    .bind(vehicle_id)
    .bind(employee_id)
    .bind(at)
    .bind(km_start)
    .bind(condition)
    .bind(user.id)
    .fetch_one(&mut *conn)
    .await?;
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO vehicle_assignments (vehicle_id, employee_id, handover_id, started_at, ended_at) VALUES ($1, $2, $3, $4, $5) RETURNING id",
    )
    .bind(vehicle_id)
    .bind(employee_id)
    .bind(handover)
    .bind(at)
    .bind(ended_at)
    .fetch_one(&mut *conn)
    .await?;
    if ended_at.is_none() {
        sqlx::query("UPDATE vehicles SET status = 'affecte', version = version + 1, updated_at = now() WHERE id = $1")
            .bind(vehicle_id)
            .execute(&mut *conn)
            .await?;
    } else if km_end.is_some() || note_end.is_some() {
        sqlx::query("UPDATE vehicle_movements SET note = note || coalesce(' — retour : ' || $2, '') || coalesce(' (' || $3::text || ' km)', '') WHERE id = $1")
            .bind(handover)
            .bind(note_end)
            .bind(km_end)
            .execute(&mut *conn)
            .await?;
    }
    if let Some(km) = km_end.or(km_start) {
        sqlx::query("UPDATE vehicles SET odometer_km = GREATEST(coalesce(odometer_km, 0), $2) WHERE id = $1")
            .bind(vehicle_id)
            .bind(km)
            .execute(&mut *conn)
            .await?;
    }
    Ok(id)
}
