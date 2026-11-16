//! Logiciels (M03 IT, U4) : abonnements ou licences perpétuelles, sièges
//! achetés, attribués et disponibles. L'attribution verrouille
//! l'abonnement : la capacité achetée n'est jamais dépassée.

use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::NaiveDate;
use domain::Role;
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::PgConnection;
use uuid::Uuid;

use crate::audit::{self, Audit};
use crate::auth::CurrentUser;
use crate::error::{AppError, AppResult};
use crate::idempotency::{self, Begin};
use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/software", get(list).post(create))
        .route("/software/{id}", get(detail).patch(update))
        .route("/software/{id}/allocations", post(allocate))
        .route("/license-allocations/{id}/release", post(release))
        .route("/employees/{id}/licenses", get(employee_licenses))
}

const PARC: &[Role] = &[Role::It, Role::Admin];
const READ: &[Role] = &[Role::It, Role::Admin, Role::Rh];

/// Condition SQL « siège occupé » pour une attribution `a`.
pub const SEAT_OCCUPIED: &str = "(a.ended_at IS NULL OR a.seat_free_at IS NULL OR a.seat_free_at > now())";

fn clean(v: &Option<String>) -> Option<String> {
    v.as_deref().and_then(domain::text::clean)
}

async fn list(State(state): State<AppState>, user: CurrentUser) -> AppResult<Json<Value>> {
    user.require(READ)?;
    let v: Value = sqlx::query_scalar(&format!(
        "SELECT coalesce(jsonb_agg(jsonb_build_object(
                    'id', s.id, 'vendor', s.vendor, 'product', s.product, 'offer', s.offer, 'contract_ref', s.contract_ref,
                    'license_kind', s.license_kind, 'start_date', s.start_date, 'end_date', s.end_date, 'auto_renew', s.auto_renew,
                    'seats', s.seats, 'seat_release', s.seat_release, 'active', s.active, 'version', s.version,
                    'used', (SELECT count(*) FROM license_allocations a WHERE a.subscription_id = s.id AND {SEAT_OCCUPIED}),
                    'holders', (SELECT count(*) FROM license_allocations a WHERE a.subscription_id = s.id AND a.ended_at IS NULL))
                    ORDER BY s.vendor, s.product), '[]')
           FROM software_subscriptions s"
    ))
    .fetch_one(&state.pool)
    .await?;
    Ok(Json(v))
}

async fn detail(State(state): State<AppState>, user: CurrentUser, Path(id): Path<Uuid>) -> AppResult<Json<Value>> {
    user.require(READ)?;
    let v: Option<Value> = sqlx::query_scalar(&format!(
        "SELECT jsonb_build_object(
                'subscription', to_jsonb(s) || jsonb_build_object(
                    'used', (SELECT count(*) FROM license_allocations a WHERE a.subscription_id = s.id AND {SEAT_OCCUPIED})),
                'allocations', (SELECT coalesce(jsonb_agg(jsonb_build_object(
                        'id', a.id, 'employee_id', e.id, 'employee', e.first_name || ' ' || e.last_name,
                        'started_at', a.started_at, 'ended_at', a.ended_at, 'seat_free_at', a.seat_free_at,
                        'note', a.note, 'release_note', a.release_note, 'assigned_by', u.display_name,
                        'case_reference', c.reference) ORDER BY a.ended_at NULLS FIRST, a.started_at DESC), '[]')
                    FROM license_allocations a JOIN employees e ON e.id = a.employee_id JOIN users u ON u.id = a.assigned_by
                    LEFT JOIN cases c ON c.id = a.case_id WHERE a.subscription_id = s.id))
           FROM software_subscriptions s WHERE s.id = $1"
    ))
    .bind(id)
    .fetch_optional(&state.pool)
    .await?;
    Ok(Json(v.ok_or_else(|| AppError::not_found("abonnement"))?))
}

/// Licences actives d'un salarié.
pub async fn employee_licenses_json(conn: &mut PgConnection, employee_id: Uuid) -> AppResult<Value> {
    Ok(sqlx::query_scalar(
        "SELECT coalesce(jsonb_agg(jsonb_build_object('allocation_id', a.id, 'subscription_id', s.id,
                    'label', s.vendor || ' — ' || s.product || coalesce(' (' || s.offer || ')', ''),
                    'since', a.started_at, 'end_date', s.end_date) ORDER BY s.vendor, s.product), '[]')
           FROM license_allocations a JOIN software_subscriptions s ON s.id = a.subscription_id
          WHERE a.employee_id = $1 AND a.ended_at IS NULL",
    )
    .bind(employee_id)
    .fetch_one(conn)
    .await?)
}

async fn employee_licenses(State(state): State<AppState>, user: CurrentUser, Path(id): Path<Uuid>) -> AppResult<Json<Value>> {
    user.require(READ)?;
    let mut conn = state.pool.acquire().await?;
    Ok(Json(employee_licenses_json(&mut conn, id).await?))
}

#[derive(Deserialize)]
struct SubscriptionInput {
    version: Option<i32>,
    vendor: String,
    product: String,
    offer: Option<String>,
    contract_ref: Option<String>,
    license_kind: String,
    start_date: Option<NaiveDate>,
    end_date: Option<NaiveDate>,
    #[serde(default)]
    auto_renew: bool,
    seats: i32,
    seat_release: Option<String>,
    notes: Option<String>,
    active: Option<bool>,
}

fn validate(i: &SubscriptionInput) -> AppResult<(String, String, String)> {
    let vendor = domain::text::clean(&i.vendor).ok_or_else(|| AppError::invalid("vendor", "éditeur obligatoire"))?;
    let product = domain::text::clean(&i.product).ok_or_else(|| AppError::invalid("product", "produit obligatoire"))?;
    if !["abonnement", "perpetuelle"].contains(&i.license_kind.as_str()) {
        return Err(AppError::invalid("license_kind", "abonnement ou licence perpétuelle"));
    }
    if i.license_kind == "perpetuelle" && (i.end_date.is_some() || i.auto_renew) {
        return Err(AppError::invalid("end_date", "une licence perpétuelle n'a ni échéance ni renouvellement"));
    }
    if i.seats < 0 {
        return Err(AppError::invalid("seats", "nombre de sièges positif"));
    }
    let release = i.seat_release.clone().unwrap_or_else(|| "immediat".into());
    if !["immediat", "fin_periode", "non_reattribuable"].contains(&release.as_str()) {
        return Err(AppError::invalid("seat_release", "règle de libération inconnue"));
    }
    if release == "fin_periode" && i.end_date.is_none() {
        return Err(AppError::invalid("seat_release", "libération en fin de période : indiquez la date de fin"));
    }
    Ok((vendor, product, release))
}

async fn create(State(state): State<AppState>, user: CurrentUser, Json(i): Json<SubscriptionInput>) -> AppResult<Json<Value>> {
    user.require(PARC)?;
    let (vendor, product, release) = validate(&i)?;
    let mut tx = state.pool.begin().await?;
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO software_subscriptions (vendor, product, offer, contract_ref, license_kind, start_date, end_date, auto_renew,
                                             seats, seat_release, notes)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11) RETURNING id",
    )
    .bind(&vendor)
    .bind(&product)
    .bind(clean(&i.offer))
    .bind(clean(&i.contract_ref))
    .bind(&i.license_kind)
    .bind(i.start_date)
    .bind(i.end_date)
    .bind(i.auto_renew)
    .bind(i.seats)
    .bind(&release)
    .bind(clean(&i.notes))
    .fetch_one(&mut *tx)
    .await?;
    audit::record(&mut tx, &user, Audit { action: "abonnement_cree", entity: "software", entity_id: Some(id), data: Some(json!({ "vendor": vendor, "product": product, "seats": i.seats })), ..Default::default() }).await?;
    tx.commit().await?;
    Ok(Json(json!({ "id": id })))
}

/// Renouvellement, ajout ou retrait de sièges. Retirer des sièges en
/// dessous du nombre occupé est refusé.
async fn update(State(state): State<AppState>, user: CurrentUser, Path(id): Path<Uuid>, Json(i): Json<SubscriptionInput>) -> AppResult<Json<Value>> {
    user.require(PARC)?;
    let (vendor, product, release) = validate(&i)?;
    let mut tx = state.pool.begin().await?;
    let version: Option<i32> = sqlx::query_scalar("SELECT version FROM software_subscriptions WHERE id = $1 FOR UPDATE")
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?;
    let version = version.ok_or_else(|| AppError::not_found("abonnement"))?;
    if i.version != Some(version) {
        return Err(AppError::stale());
    }
    let used: i64 = sqlx::query_scalar(&format!(
        "SELECT count(*) FROM license_allocations a WHERE a.subscription_id = $1 AND {SEAT_OCCUPIED}"
    ))
    .bind(id)
    .fetch_one(&mut *tx)
    .await?;
    if (i.seats as i64) < used {
        return Err(AppError::conflict(format!("{used} siège(s) occupé(s) : libérez-en avant de réduire la capacité")));
    }
    sqlx::query(
        "UPDATE software_subscriptions SET vendor = $2, product = $3, offer = $4, contract_ref = $5, license_kind = $6,
                start_date = $7, end_date = $8, auto_renew = $9, seats = $10, seat_release = $11, notes = $12,
                active = coalesce($13, active), version = version + 1, updated_at = now()
          WHERE id = $1",
    )
    .bind(id)
    .bind(&vendor)
    .bind(&product)
    .bind(clean(&i.offer))
    .bind(clean(&i.contract_ref))
    .bind(&i.license_kind)
    .bind(i.start_date)
    .bind(i.end_date)
    .bind(i.auto_renew)
    .bind(i.seats)
    .bind(&release)
    .bind(clean(&i.notes))
    .bind(i.active)
    .execute(&mut *tx)
    .await?;
    audit::record(&mut tx, &user, Audit { action: "abonnement_modifie", entity: "software", entity_id: Some(id), data: Some(json!({ "seats": i.seats, "end_date": i.end_date })), ..Default::default() }).await?;
    tx.commit().await?;
    Ok(Json(json!({ "version": version + 1 })))
}

#[derive(Deserialize)]
struct AllocateInput {
    employee_id: Uuid,
    case_id: Option<Uuid>,
    note: Option<String>,
}

pub async fn allocate_seat(
    conn: &mut PgConnection,
    user: &CurrentUser,
    subscription_id: Uuid,
    employee_id: Uuid,
    case_id: Option<Uuid>,
    note: Option<String>,
) -> AppResult<Uuid> {
    // Verrou : deux attributions simultanées du dernier siège se suivent.
    let row: Option<(i32, bool, Option<NaiveDate>, String, String)> = sqlx::query_as(
        "SELECT seats, active, end_date, vendor, product FROM software_subscriptions WHERE id = $1 FOR UPDATE",
    )
    .bind(subscription_id)
    .fetch_optional(&mut *conn)
    .await?;
    let (seats, active, end_date, vendor, product) = row.ok_or_else(|| AppError::not_found("abonnement"))?;
    if !active {
        return Err(AppError::conflict("abonnement inactif"));
    }
    if let Some(end) = end_date {
        if end < crate::documents::algiers_today() {
            return Err(AppError::conflict(format!("abonnement échu le {} : renouvelez-le d'abord", end.format("%d/%m/%Y"))));
        }
    }
    let status: Option<String> = sqlx::query_scalar("SELECT status FROM employees WHERE id = $1").bind(employee_id).fetch_optional(&mut *conn).await?;
    match status.as_deref() {
        None => return Err(AppError::not_found("salarié")),
        Some("sorti") => return Err(AppError::conflict("salarié sorti")),
        _ => {}
    }
    let used: i64 = sqlx::query_scalar(&format!(
        "SELECT count(*) FROM license_allocations a WHERE a.subscription_id = $1 AND {SEAT_OCCUPIED}"
    ))
    .bind(subscription_id)
    .fetch_one(&mut *conn)
    .await?;
    if used >= seats as i64 {
        return Err(AppError::Conflict {
            message: format!("plus de siège disponible pour {vendor} {product} ({used}/{seats} occupés)"),
            detail: Some(json!({ "used": used, "seats": seats })),
        });
    }
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO license_allocations (subscription_id, employee_id, case_id, assigned_by, note) VALUES ($1, $2, $3, $4, $5) RETURNING id",
    )
    .bind(subscription_id)
    .bind(employee_id)
    .bind(case_id)
    .bind(user.id)
    .bind(note)
    .fetch_one(&mut *conn)
    .await
    .map_err(|e| match AppError::from(e) {
        AppError::Conflict { .. } => AppError::conflict("ce salarié détient déjà un siège de cet abonnement"),
        other => other,
    })?;
    audit::record(
        conn,
        user,
        Audit {
            action: "siege_attribue",
            entity: "license_allocation",
            entity_id: Some(id),
            case_id,
            employee_id: Some(employee_id),
            data: Some(json!({ "logiciel": format!("{vendor} {product}") })),
        },
    )
    .await?;
    Ok(id)
}

async fn allocate(
    State(state): State<AppState>,
    user: CurrentUser,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(raw): Json<Value>,
) -> AppResult<Response> {
    user.require(PARC)?;
    let i: AllocateInput = serde_json::from_value(raw.clone()).map_err(|e| AppError::invalid("body", format!("saisie incomplète : {e}")))?;
    let key = idempotency::key_from(&headers)?;
    let endpoint = format!("POST /software/{id}/allocations");
    let mut tx = state.pool.begin().await?;
    if let Some(k) = &key {
        if let Begin::Replay(status, body) = idempotency::begin(&mut tx, user.id, k, &endpoint, &idempotency::hash_request(&endpoint, &raw)).await? {
            tx.commit().await?;
            return Ok((status, Json(body)).into_response());
        }
    }
    let allocation = allocate_seat(&mut tx, &user, id, i.employee_id, i.case_id, clean(&i.note)).await?;
    let response = json!({ "id": allocation });
    if let Some(k) = &key {
        idempotency::finish(&mut tx, user.id, k, StatusCode::CREATED, &response).await?;
    }
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(response)).into_response())
}

#[derive(Deserialize, Default)]
struct ReleaseInput {
    note: Option<String>,
}

/// Libère un siège : immédiatement, en fin de période ou jamais, selon le
/// contrat. Indépendant de toute restitution d'actif physique.
pub async fn release_seat(conn: &mut PgConnection, user: &CurrentUser, allocation_id: Uuid, note: Option<String>) -> AppResult<Value> {
    let row: Option<(Uuid, Uuid, Option<chrono::DateTime<chrono::Utc>>, String, Option<NaiveDate>)> = sqlx::query_as(
        "SELECT a.subscription_id, a.employee_id, a.ended_at, s.seat_release, s.end_date
           FROM license_allocations a JOIN software_subscriptions s ON s.id = a.subscription_id
          WHERE a.id = $1 FOR UPDATE OF a",
    )
    .bind(allocation_id)
    .fetch_optional(&mut *conn)
    .await?;
    let (subscription, employee, ended, rule, end_date) = row.ok_or_else(|| AppError::not_found("attribution"))?;
    if ended.is_some() {
        return Err(AppError::conflict("siège déjà libéré"));
    }
    let free_at: Option<String> = match rule.as_str() {
        "immediat" => Some("now()".into()),
        "fin_periode" => end_date.map(|d| format!("'{}'::date::timestamptz", d)),
        _ => None,
    };
    let sql = format!(
        "UPDATE license_allocations SET ended_at = now(), released_by = $2, release_note = $3, seat_free_at = {}
          WHERE id = $1 RETURNING seat_free_at",
        free_at.unwrap_or_else(|| "NULL".into())
    );
    let seat_free_at: Option<chrono::DateTime<chrono::Utc>> = sqlx::query_scalar(&sql)
        .bind(allocation_id)
        .bind(user.id)
        .bind(&note)
        .fetch_one(&mut *conn)
        .await?;
    audit::record(
        conn,
        user,
        Audit {
            action: "siege_libere",
            entity: "license_allocation",
            entity_id: Some(allocation_id),
            employee_id: Some(employee),
            data: Some(json!({ "abonnement": subscription, "regle": rule, "disponible_le": seat_free_at, "note": note })),
            ..Default::default()
        },
    )
    .await?;
    Ok(json!({ "released": true, "rule": rule, "seat_free_at": seat_free_at }))
}

async fn release(State(state): State<AppState>, user: CurrentUser, Path(id): Path<Uuid>, body: Option<Json<ReleaseInput>>) -> AppResult<Json<Value>> {
    user.require(PARC)?;
    let note = body.and_then(|Json(i)| i.note).as_deref().and_then(domain::text::clean);
    let mut tx = state.pool.begin().await?;
    let v = release_seat(&mut tx, &user, id, note).await?;
    tx.commit().await?;
    Ok(Json(v))
}

/// Abonnements arrivant à échéance (alerte IT avant renouvellement).
pub async fn renewals_json(conn: &mut PgConnection, today: NaiveDate, days: i32) -> AppResult<Value> {
    Ok(sqlx::query_scalar(
        "SELECT coalesce(jsonb_agg(jsonb_build_object('id', s.id, 'label', s.vendor || ' ' || s.product, 'end_date', s.end_date,
                    'auto_renew', s.auto_renew, 'overdue', s.end_date < $1) ORDER BY s.end_date), '[]')
           FROM software_subscriptions s
          WHERE s.active AND s.license_kind = 'abonnement' AND s.end_date IS NOT NULL AND s.end_date <= $1::date + $2::int",
    )
    .bind(today)
    .bind(days)
    .fetch_one(conn)
    .await?)
}
