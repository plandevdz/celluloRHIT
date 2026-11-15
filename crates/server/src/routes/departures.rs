//! Départ (M06, U5) : dossier OFF, dernier jour et instant de coupure
//! séparés, contrôles de sortie figés puis rapprochés. L'application ne
//! coupe aucun compte externe : seule la confirmation de l'IT enregistre une
//! coupure. La sortie administrative n'attend pas les restitutions.

use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::{Json, Router};
use chrono::{DateTime, NaiveDate, Utc};
use domain::{CaseKind, Role};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::PgConnection;
use uuid::Uuid;

use crate::audit::{self, Audit, Notice};
use crate::auth::CurrentUser;
use crate::error::{AppError, AppResult};
use crate::idempotency::{self, Begin};
use crate::routes::cases::{fr_date, next_reference};
use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/departures", post(create))
        .route("/departures/{id}/cutoff", post(change_cutoff))
        .route("/departures/{id}/reschedule", post(reschedule))
        .route("/departures/{id}/cancel", post(cancel))
        .route("/exit-controls/{id}/confirm", post(confirm_control))
        .route("/exit-controls/{id}/gap", post(report_gap))
        .route("/exit-controls/{id}/loss", post(declare_loss))
        .route("/exit-controls/{id}/not-applicable", post(not_applicable))
}

pub const REASONS: &[&str] = &["demission", "fin_contrat", "rupture", "retraite", "autre"];

fn clean(v: &Option<String>) -> Option<String> {
    v.as_deref().and_then(domain::text::clean)
}

pub fn algiers(dt: DateTime<Utc>) -> String {
    (dt + chrono::Duration::hours(1)).format("%d/%m/%Y à %H:%M").to_string()
}

// ------------------------------------------------------------------ création

#[derive(Deserialize)]
struct CreateInput {
    employee_id: Uuid,
    #[serde(default = "default_reason")]
    reason: String,
    private_note: Option<String>,
    last_day: NaiveDate,
    cutoff_at: DateTime<Utc>,
    hr_contact: Option<String>,
    /// Transfert de données autorisé à organiser par l'IT.
    #[serde(default)]
    data_transfer: bool,
    /// Si une mutation est en cours : `annuler` ou `conserver` (choix RH).
    transfer_action: Option<String>,
}

fn default_reason() -> String {
    "demission".into()
}

struct ControlSpec {
    kind: &'static str,
    account_id: Option<Uuid>,
    license_allocation_id: Option<Uuid>,
    assignment_id: Option<Uuid>,
    vehicle_assignment_id: Option<Uuid>,
    label: String,
    detail: Option<String>,
    role: Role,
    title: String,
    gen_key: String,
    /// Coupure : exigible à l'instant convenu.
    at_cutoff: bool,
}

async fn controls_for(conn: &mut PgConnection, employee_id: Uuid, data_transfer: bool) -> AppResult<Vec<ControlSpec>> {
    let mut out = Vec::new();
    let accounts: Vec<(Uuid, String, String, Option<String>)> = sqlx::query_as(
        "SELECT id, system, identifier, platform FROM employee_accounts WHERE employee_id = $1 AND status <> 'coupe' ORDER BY system",
    )
    .bind(employee_id)
    .fetch_all(&mut *conn)
    .await?;
    for (id, system, identifier, platform) in accounts {
        let sys = match (system.as_str(), platform.as_deref()) {
            ("email", Some("m365")) => "Microsoft 365".to_string(),
            ("email", _) => "cPanel".to_string(),
            ("xena", _) => "Xena".to_string(),
            ("intelix", _) => "Intelix".to_string(),
            _ => "Autre système".to_string(),
        };
        out.push(ControlSpec {
            kind: "compte",
            account_id: Some(id),
            license_allocation_id: None,
            assignment_id: None,
            vehicle_assignment_id: None,
            label: format!("{sys} : {identifier}"),
            detail: Some(sys.clone()),
            role: Role::It,
            title: format!("Couper l'accès {sys} : {identifier}"),
            gen_key: format!("off:compte:{id}"),
            at_cutoff: true,
        });
    }
    let licenses: Vec<(Uuid, String)> = sqlx::query_as(
        "SELECT a.id, s.vendor || ' ' || s.product FROM license_allocations a JOIN software_subscriptions s ON s.id = a.subscription_id
          WHERE a.employee_id = $1 AND a.ended_at IS NULL",
    )
    .bind(employee_id)
    .fetch_all(&mut *conn)
    .await?;
    for (id, label) in licenses {
        out.push(ControlSpec {
            kind: "licence",
            account_id: None,
            license_allocation_id: Some(id),
            assignment_id: None,
            vehicle_assignment_id: None,
            label: format!("Licence {label}"),
            detail: None,
            role: Role::It,
            title: format!("Libérer ou transférer le siège {label}"),
            gen_key: format!("off:licence:{id}"),
            at_cutoff: false,
        });
    }
    // Tous les biens réellement détenus : arrivée, prêts directs, demandes.
    let holdings: Vec<(Uuid, String, Option<String>, i32, String)> = sqlx::query_as(
        "SELECT g.id, ml.label, ml.identifier, g.open_quantity, m.reference
           FROM assignments g JOIN movement_lines ml ON ml.id = g.handover_line_id JOIN movements m ON m.id = ml.movement_id
          WHERE g.employee_id = $1 AND g.ended_at IS NULL ORDER BY ml.label",
    )
    .bind(employee_id)
    .fetch_all(&mut *conn)
    .await?;
    for (id, label, identifier, qty, origin) in holdings {
        let full = format!("{label}{}{}", identifier.as_deref().map(|i| format!(" ({i})")).unwrap_or_default(), if qty > 1 { format!(" × {qty}") } else { String::new() });
        out.push(ControlSpec {
            kind: "bien",
            account_id: None,
            license_allocation_id: None,
            assignment_id: Some(id),
            vehicle_assignment_id: None,
            label: full.clone(),
            detail: Some(format!("remis par {origin}")),
            role: Role::It,
            title: format!("Récupérer : {full}"),
            gen_key: format!("off:bien:{id}"),
            at_cutoff: false,
        });
    }
    let vehicles: Vec<(Uuid, String)> = sqlx::query_as(
        "SELECT a.id, v.plate FROM vehicle_assignments a JOIN vehicles v ON v.id = a.vehicle_id WHERE a.employee_id = $1 AND a.ended_at IS NULL",
    )
    .bind(employee_id)
    .fetch_all(&mut *conn)
    .await?;
    for (id, plate) in vehicles {
        out.push(ControlSpec {
            kind: "vehicule",
            account_id: None,
            license_allocation_id: None,
            assignment_id: None,
            vehicle_assignment_id: Some(id),
            label: format!("Véhicule {plate}"),
            detail: None,
            role: Role::Rh,
            title: format!("Récupérer le véhicule {plate}"),
            gen_key: format!("off:vehicule:{id}"),
            at_cutoff: false,
        });
    }
    if data_transfer {
        out.push(ControlSpec {
            kind: "donnees",
            account_id: None,
            license_allocation_id: None,
            assignment_id: None,
            vehicle_assignment_id: None,
            label: "Transfert de données autorisé".into(),
            detail: None,
            role: Role::It,
            title: "Organiser le transfert de données autorisé".into(),
            gen_key: "off:donnees".into(),
            at_cutoff: false,
        });
    }
    Ok(out)
}

async fn insert_controls(
    conn: &mut PgConnection,
    departure_id: Uuid,
    case_id: Uuid,
    specs: &[ControlSpec],
    last_day: NaiveDate,
    cutoff_at: DateTime<Utc>,
) -> AppResult<()> {
    for s in specs {
        let control: Uuid = sqlx::query_scalar(
            "INSERT INTO exit_controls (departure_id, kind, account_id, license_allocation_id, assignment_id, vehicle_assignment_id, label, detail)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8) RETURNING id",
        )
        .bind(departure_id)
        .bind(s.kind)
        .bind(s.account_id)
        .bind(s.license_allocation_id)
        .bind(s.assignment_id)
        .bind(s.vehicle_assignment_id)
        .bind(&s.label)
        .bind(&s.detail)
        .fetch_one(&mut *conn)
        .await?;
        sqlx::query(
            "INSERT INTO tasks (case_id, gen_key, role, title, due_date, not_before, due_at, exit_control_id)
             VALUES ($1, $2, $3, $4, $5, $6, $6, $7) ON CONFLICT (case_id, gen_key) DO NOTHING",
        )
        .bind(case_id)
        .bind(&s.gen_key)
        .bind(s.role.as_str())
        .bind(&s.title)
        .bind(if s.at_cutoff { (cutoff_at + chrono::Duration::hours(1)).date_naive() } else { last_day })
        .bind(s.at_cutoff.then_some(cutoff_at))
        .bind(control)
        .execute(&mut *conn)
        .await?;
    }
    Ok(())
}

/// OFF-xxxxx : les dates sont décidées par le RH ; aucune paie, aucun
/// préavis calculé. Les équipes ne reçoivent ni le motif privé ni les pièces.
async fn create(State(state): State<AppState>, user: CurrentUser, headers: HeaderMap, Json(raw): Json<Value>) -> AppResult<Response> {
    user.require(domain::roles::HR_MANAGERS)?;
    let i: CreateInput = serde_json::from_value(raw.clone()).map_err(|e| AppError::invalid("body", format!("saisie incomplète : {e}")))?;
    if !REASONS.contains(&i.reason.as_str()) {
        return Err(AppError::invalid("reason", "motif opérationnel inconnu"));
    }
    let key = idempotency::key_from(&headers)?;
    let mut tx = state.pool.begin().await?;
    if let Some(k) = &key {
        if let Begin::Replay(status, body) = idempotency::begin(&mut tx, user.id, k, "POST /departures", &idempotency::hash_request("POST /departures", &raw)).await? {
            tx.commit().await?;
            return Ok((status, Json(body)).into_response());
        }
    }
    let emp: Option<(String, String, String)> = sqlx::query_as("SELECT status, first_name, last_name FROM employees WHERE id = $1 FOR UPDATE")
        .bind(i.employee_id)
        .fetch_optional(&mut *tx)
        .await?;
    let (status, first, last) = emp.ok_or_else(|| AppError::not_found("salarié"))?;
    if status == "sorti" {
        return Err(AppError::conflict("salarié déjà sorti"));
    }
    // Un départ prend priorité sur une mutation en cours : le RH choisit.
    let active_transfer: Option<String> = sqlx::query_scalar(
        "SELECT c.reference FROM transfers t JOIN cases c ON c.id = t.case_id
          WHERE t.employee_id = $1 AND t.status IN ('brouillon', 'confirmee', 'suspendue')",
    )
    .bind(i.employee_id)
    .fetch_optional(&mut *tx)
    .await?;
    let mut cancelled_transfer = None;
    if let Some(r) = &active_transfer {
        match i.transfer_action.as_deref() {
            Some("annuler") => {
                cancelled_transfer = crate::routes::transfers::cancel_active_for(&mut tx, &user, i.employee_id, "départ prioritaire").await?;
            }
            Some("conserver") => {}
            _ => {
                return Err(AppError::Conflict {
                    message: format!("mutation {r} en cours : choisissez de l'annuler ou de la conserver"),
                    detail: Some(json!({ "transfer_reference": r })),
                })
            }
        }
    }
    let reference = next_reference(&mut tx, CaseKind::Off).await?;
    let case_id: Uuid = sqlx::query_scalar(
        "INSERT INTO cases (reference, kind, employee_id, effective_date, due_date, created_by)
         VALUES ($1, 'OFF', $2, $3, $3, $4) RETURNING id",
    )
    .bind(&reference)
    .bind(i.employee_id)
    .bind(i.last_day)
    .bind(user.id)
    .fetch_one(&mut *tx)
    .await?;
    let departure_id: Uuid = sqlx::query_scalar(
        "INSERT INTO departures (case_id, employee_id, reason, private_note, last_day, cutoff_at, hr_contact)
         VALUES ($1, $2, $3, $4, $5, $6, $7) RETURNING id",
    )
    .bind(case_id)
    .bind(i.employee_id)
    .bind(&i.reason)
    .bind(clean(&i.private_note))
    .bind(i.last_day)
    .bind(i.cutoff_at)
    .bind(clean(&i.hr_contact))
    .fetch_one(&mut *tx)
    .await
    .map_err(|e| match AppError::from(e) {
        AppError::Conflict { .. } => AppError::conflict("un départ est déjà planifié pour ce salarié"),
        other => other,
    })?;
    sqlx::query("UPDATE employees SET leaving = true, planned_exit_date = $2, version = version + 1, updated_at = now() WHERE id = $1")
        .bind(i.employee_id)
        .bind(i.last_day)
        .execute(&mut *tx)
        .await?;
    let specs = controls_for(&mut tx, i.employee_id, i.data_transfer).await?;
    insert_controls(&mut tx, departure_id, case_id, &specs, i.last_day, i.cutoff_at).await?;
    let mut roles = vec![Role::It];
    if specs.iter().any(|s| s.role == Role::Rh) {
        roles.push(Role::Rh);
    }
    let access: Vec<&str> = specs.iter().filter(|s| s.kind == "compte").map(|s| s.label.as_str()).collect();
    let goods: Vec<&str> = specs.iter().filter(|s| s.kind != "compte").map(|s| s.label.as_str()).collect();
    let mut body = format!(
        "Départ de {first} {last} : dernier jour le {}, coupure des accès prévue le {} (heure d'Alger).\n",
        fr_date(i.last_day),
        algiers(i.cutoff_at)
    );
    body.push_str(&format!("\nAccès à traiter ({}) :\n", access.len()));
    for a in &access {
        body.push_str(&format!("- {a}\n"));
    }
    body.push_str(&format!("\nBiens à restituer ({}) :\n", goods.len()));
    for g in &goods {
        body.push_str(&format!("- {g}\n"));
    }
    audit::enqueue(
        &mut tx,
        &user,
        Notice {
            event_type: "depart_planifie",
            case_id: Some(case_id),
            target_roles: roles.clone(),
            target_user_ids: vec![],
            subject: format!("{reference} — Départ de {first} {last} le {}", fr_date(i.last_day)),
            body,
            link_path: Some(format!("/dossiers/{reference}")),
        },
    )
    .await?;
    audit::record(
        &mut tx,
        &user,
        Audit {
            action: "depart_planifie",
            entity: "departure",
            entity_id: Some(departure_id),
            case_id: Some(case_id),
            employee_id: Some(i.employee_id),
            data: Some(json!({ "reference": reference, "dernier_jour": i.last_day, "coupure": i.cutoff_at, "controles": specs.len(), "mutation_annulee": cancelled_transfer })),
        },
    )
    .await?;
    let response = json!({
        "case_id": case_id, "reference": reference, "departure_id": departure_id, "controls": specs.len(),
        "visible_to": roles, "mail_mode": state.config.mail_mode.as_str(), "cancelled_transfer": cancelled_transfer,
    });
    if let Some(k) = &key {
        idempotency::finish(&mut tx, user.id, k, StatusCode::CREATED, &response).await?;
    }
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(response)).into_response())
}

// ------------------------------------------------------------------ report, avance, annulation

struct Dep {
    id: Uuid,
    case_id: Uuid,
    employee_id: Uuid,
    status: String,
    version: i32,
    cutoff_at: DateTime<Utc>,
    reference: String,
    name: String,
}

async fn lock(conn: &mut PgConnection, id: Uuid) -> AppResult<Dep> {
    let row: Option<(Uuid, Uuid, Uuid, String, i32, DateTime<Utc>, String, String)> = sqlx::query_as(
        "SELECT d.id, d.case_id, d.employee_id, d.status, d.version, d.cutoff_at, c.reference, e.first_name || ' ' || e.last_name
           FROM departures d JOIN cases c ON c.id = d.case_id JOIN employees e ON e.id = d.employee_id
          WHERE d.id = $1 OR d.case_id = $1 FOR UPDATE OF d",
    )
    .bind(id)
    .fetch_optional(conn)
    .await?;
    let (id, case_id, employee_id, status, version, cutoff_at, reference, name) = row.ok_or_else(|| AppError::not_found("départ"))?;
    Ok(Dep { id, case_id, employee_id, status, version, cutoff_at, reference, name })
}

#[derive(Deserialize)]
struct CutoffInput {
    version: i32,
    cutoff_at: DateTime<Utc>,
    reason: String,
}

/// Coupure avancée (urgence) ou décalée : motif obligatoire, l'IT est
/// prévenue immédiatement.
async fn change_cutoff(State(state): State<AppState>, user: CurrentUser, Path(id): Path<Uuid>, Json(i): Json<CutoffInput>) -> AppResult<Json<Value>> {
    user.require(domain::roles::HR_MANAGERS)?;
    let reason = domain::text::clean(&i.reason).ok_or_else(|| AppError::invalid("reason", "motif obligatoire"))?;
    let mut tx = state.pool.begin().await?;
    let d = lock(&mut tx, id).await?;
    if d.status != "planifie" {
        return Err(AppError::conflict("départ déjà effectif ou annulé"));
    }
    if d.version != i.version {
        return Err(AppError::stale());
    }
    let advanced = i.cutoff_at < d.cutoff_at;
    sqlx::query(
        "UPDATE departures SET cutoff_at = $2, cutoff_notified_at = CASE WHEN $2 > now() THEN NULL ELSE cutoff_notified_at END,
                escalated_at = CASE WHEN $2 > now() THEN NULL ELSE escalated_at END, version = version + 1, updated_at = now()
          WHERE id = $1",
    )
    .bind(d.id)
    .bind(i.cutoff_at)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "UPDATE tasks SET not_before = $2, due_at = $2, due_date = ($2 AT TIME ZONE 'Africa/Algiers')::date, escalated_at = NULL,
                version = version + 1, updated_at = now()
          WHERE case_id = $1 AND gen_key LIKE 'off:compte:%' AND status IN ('a_faire', 'en_cours', 'bloque')",
    )
    .bind(d.case_id)
    .bind(i.cutoff_at)
    .execute(&mut *tx)
    .await?;
    audit::enqueue(
        &mut tx,
        &user,
        Notice {
            event_type: if advanced { "coupure_avancee" } else { "coupure_decalee" },
            case_id: Some(d.case_id),
            target_roles: vec![Role::It],
            target_user_ids: vec![],
            subject: format!("{} — Coupure {} : {}", d.reference, if advanced { "avancée" } else { "décalée" }, algiers(i.cutoff_at)),
            body: format!("La coupure des accès de {} est fixée au {} (heure d'Alger).\nMotif : {reason}\n", d.name, algiers(i.cutoff_at)),
            link_path: Some(format!("/dossiers/{}", d.reference)),
        },
    )
    .await?;
    audit::record(&mut tx, &user, Audit { action: if advanced { "coupure_avancee" } else { "coupure_decalee" }, entity: "departure", entity_id: Some(d.id), case_id: Some(d.case_id), employee_id: Some(d.employee_id), data: Some(json!({ "avant": d.cutoff_at, "apres": i.cutoff_at, "motif": reason })) }).await?;
    tx.commit().await?;
    Ok(Json(json!({ "version": d.version + 1, "advanced": advanced })))
}

#[derive(Deserialize)]
struct RescheduleInput {
    version: i32,
    last_day: NaiveDate,
    cutoff_at: Option<DateTime<Utc>>,
    reason: String,
}

/// Report du départ avant effet : seules les actions futures sont décalées.
async fn reschedule(State(state): State<AppState>, user: CurrentUser, Path(id): Path<Uuid>, Json(i): Json<RescheduleInput>) -> AppResult<Json<Value>> {
    user.require(domain::roles::HR_MANAGERS)?;
    let reason = domain::text::clean(&i.reason).ok_or_else(|| AppError::invalid("reason", "motif obligatoire"))?;
    let mut tx = state.pool.begin().await?;
    let d = lock(&mut tx, id).await?;
    if d.status != "planifie" {
        return Err(AppError::conflict("départ déjà effectif ou annulé"));
    }
    if d.version != i.version {
        return Err(AppError::stale());
    }
    let cutoff = i.cutoff_at.unwrap_or(d.cutoff_at);
    sqlx::query(
        "UPDATE departures SET last_day = $2, cutoff_at = $3,
                cutoff_notified_at = CASE WHEN $3 > now() THEN NULL ELSE cutoff_notified_at END,
                escalated_at = CASE WHEN $3 > now() THEN NULL ELSE escalated_at END,
                version = version + 1, updated_at = now() WHERE id = $1",
    )
    .bind(d.id)
    .bind(i.last_day)
    .bind(cutoff)
    .execute(&mut *tx)
    .await?;
    sqlx::query("UPDATE cases SET effective_date = $2, due_date = $2, version = version + 1, updated_at = now() WHERE id = $1")
        .bind(d.case_id)
        .bind(i.last_day)
        .execute(&mut *tx)
        .await?;
    sqlx::query("UPDATE employees SET planned_exit_date = $2 WHERE id = $1").bind(d.employee_id).bind(i.last_day).execute(&mut *tx).await?;
    sqlx::query(
        "UPDATE tasks SET due_date = CASE WHEN gen_key LIKE 'off:compte:%' THEN ($3 AT TIME ZONE 'Africa/Algiers')::date ELSE $2 END,
                not_before = CASE WHEN gen_key LIKE 'off:compte:%' THEN $3 ELSE not_before END,
                due_at = CASE WHEN gen_key LIKE 'off:compte:%' THEN $3 ELSE due_at END,
                version = version + 1, updated_at = now()
          WHERE case_id = $1 AND status IN ('a_faire', 'en_cours', 'bloque')",
    )
    .bind(d.case_id)
    .bind(i.last_day)
    .bind(cutoff)
    .execute(&mut *tx)
    .await?;
    audit::record(&mut tx, &user, Audit { action: "depart_reporte", entity: "departure", entity_id: Some(d.id), case_id: Some(d.case_id), employee_id: Some(d.employee_id), data: Some(json!({ "dernier_jour": i.last_day, "coupure": cutoff, "motif": reason })) }).await?;
    tx.commit().await?;
    Ok(Json(json!({ "version": d.version + 1 })))
}

#[derive(Deserialize)]
struct CancelInput {
    version: i32,
    reason: String,
    /// Comptes déjà coupés dont le rétablissement est demandé explicitement.
    #[serde(default)]
    restore_account_ids: Vec<Uuid>,
}

/// Annulation avant effet : les travaux futurs sont neutralisés. Une
/// coupure déjà faite n'est rétablie que sur demande explicite.
async fn cancel(State(state): State<AppState>, user: CurrentUser, Path(id): Path<Uuid>, Json(i): Json<CancelInput>) -> AppResult<Json<Value>> {
    user.require(domain::roles::HR_MANAGERS)?;
    let reason = domain::text::clean(&i.reason).ok_or_else(|| AppError::invalid("reason", "motif obligatoire"))?;
    let mut tx = state.pool.begin().await?;
    let d = lock(&mut tx, id).await?;
    if d.status != "planifie" {
        return Err(AppError::conflict("départ déjà effectif ou annulé : une sortie effective ne s'annule pas"));
    }
    if d.version != i.version {
        return Err(AppError::stale());
    }
    let cut: Vec<(Uuid, String)> = sqlx::query_as(
        "SELECT a.id, c.label FROM exit_controls c JOIN employee_accounts a ON a.id = c.account_id
          WHERE c.departure_id = $1 AND c.kind = 'compte' AND c.state = 'fait'",
    )
    .bind(d.id)
    .fetch_all(&mut *tx)
    .await?;
    for r in &i.restore_account_ids {
        if !cut.iter().any(|(a, _)| a == r) {
            return Err(AppError::invalid("restore_account_ids", "seul un accès coupé dans ce départ peut être rétabli"));
        }
    }
    sqlx::query("UPDATE departures SET status = 'annule', cancelled_at = now(), cancel_reason = $2, version = version + 1, updated_at = now() WHERE id = $1")
        .bind(d.id)
        .bind(&reason)
        .execute(&mut *tx)
        .await?;
    sqlx::query("UPDATE employees SET leaving = false, planned_exit_date = NULL, version = version + 1, updated_at = now() WHERE id = $1")
        .bind(d.employee_id)
        .execute(&mut *tx)
        .await?;
    let n = sqlx::query("UPDATE tasks SET status = 'annule', version = version + 1, updated_at = now() WHERE case_id = $1 AND status IN ('a_faire', 'en_cours', 'bloque')")
        .bind(d.case_id)
        .execute(&mut *tx)
        .await?
        .rows_affected();
    sqlx::query("UPDATE exit_controls SET state = 'non_applicable', note = coalesce(note || ' — ', '') || 'départ annulé', updated_at = now() WHERE departure_id = $1 AND state IN ('a_traiter', 'ecart')")
        .bind(d.id)
        .execute(&mut *tx)
        .await?;
    for (account, label) in cut.iter().filter(|(a, _)| i.restore_account_ids.contains(a)) {
        sqlx::query(
            "INSERT INTO tasks (case_id, gen_key, role, title) VALUES ($1, $2, 'IT', $3) ON CONFLICT (case_id, gen_key) DO NOTHING",
        )
        .bind(d.case_id)
        .bind(format!("off:retablir:{account}"))
        .bind(format!("Rétablir l'accès {label}"))
        .execute(&mut *tx)
        .await?;
    }
    let restoring = cut.iter().any(|(a, _)| i.restore_account_ids.contains(a));
    if !restoring {
        sqlx::query("UPDATE cases SET status = 'annule', closed_at = now(), closed_by = $2, close_note = $3, version = version + 1, updated_at = now() WHERE id = $1")
            .bind(d.case_id)
            .bind(user.id)
            .bind(format!("Départ annulé : {reason}"))
            .execute(&mut *tx)
            .await?;
    }
    audit::enqueue(
        &mut tx,
        &user,
        Notice {
            event_type: "depart_annule",
            case_id: Some(d.case_id),
            target_roles: vec![Role::It],
            target_user_ids: vec![],
            subject: format!("{} — Départ annulé : {}", d.reference, d.name),
            body: if restoring {
                format!("Le départ de {} est annulé. Rétablissement demandé pour : {}.\n", d.name, cut.iter().filter(|(a, _)| i.restore_account_ids.contains(a)).map(|(_, l)| l.as_str()).collect::<Vec<_>>().join(", "))
            } else {
                format!("Le départ de {} est annulé ; les actions non réalisées sont annulées.\n", d.name)
            },
            link_path: Some(format!("/dossiers/{}", d.reference)),
        },
    )
    .await?;
    audit::record(&mut tx, &user, Audit { action: "depart_annule", entity: "departure", entity_id: Some(d.id), case_id: Some(d.case_id), employee_id: Some(d.employee_id), data: Some(json!({ "motif": reason, "taches_annulees": n, "retablissements": i.restore_account_ids })) }).await?;
    tx.commit().await?;
    Ok(Json(json!({
        "status": "annule", "cancelled_tasks": n,
        "already_cut": cut.iter().map(|(a, l)| json!({ "account_id": a, "label": l })).collect::<Vec<_>>(),
        "restoration_requested": restoring,
    })))
}

// ------------------------------------------------------------------ contrôles de sortie

struct Control {
    id: Uuid,
    departure_id: Uuid,
    case_id: Uuid,
    employee_id: Uuid,
    kind: String,
    state: String,
    account_id: Option<Uuid>,
    license_allocation_id: Option<Uuid>,
    assignment_id: Option<Uuid>,
    vehicle_assignment_id: Option<Uuid>,
    label: String,
}

async fn lock_control(conn: &mut PgConnection, id: Uuid) -> AppResult<Control> {
    type Row = (Uuid, Uuid, Uuid, Uuid, String, String, Option<Uuid>, Option<Uuid>, Option<Uuid>, Option<Uuid>, String);
    let row: Option<Row> = sqlx::query_as(
        "SELECT c.id, c.departure_id, d.case_id, d.employee_id, c.kind, c.state, c.account_id, c.license_allocation_id,
                c.assignment_id, c.vehicle_assignment_id, c.label
           FROM exit_controls c JOIN departures d ON d.id = c.departure_id WHERE c.id = $1 FOR UPDATE OF c",
    )
    .bind(id)
    .fetch_optional(conn)
    .await?;
    let (id, departure_id, case_id, employee_id, kind, state, account_id, license_allocation_id, assignment_id, vehicle_assignment_id, label) =
        row.ok_or_else(|| AppError::not_found("contrôle de sortie"))?;
    Ok(Control { id, departure_id, case_id, employee_id, kind, state, account_id, license_allocation_id, assignment_id, vehicle_assignment_id, label })
}

fn role_for(kind: &str) -> &'static [Role] {
    if kind == "vehicule" {
        &[Role::Rh, Role::Admin]
    } else {
        &[Role::It, Role::Admin]
    }
}

/// Termine la tâche liée au contrôle (sans prise en charge préalable).
async fn finish_task(conn: &mut PgConnection, user: &CurrentUser, control: Uuid, resolution: &str, evidence: Value, note: Option<&str>) -> AppResult<()> {
    let task: Option<(Uuid, Uuid)> = sqlx::query_as(
        "UPDATE tasks SET status = 'termine', owner_id = coalesce(owner_id, $2), claimed_at = coalesce(claimed_at, now()),
                blocked_reason = NULL, resolution = $3, resolution_evidence = $4, done_note = $5,
                completed_at = now(), completed_by = $2, version = version + 1, updated_at = now()
          WHERE exit_control_id = $1 AND status IN ('a_faire', 'en_cours', 'bloque')
         RETURNING id, case_id",
    )
    .bind(control)
    .bind(user.id)
    .bind(resolution)
    .bind(&evidence)
    .bind(note)
    .fetch_optional(&mut *conn)
    .await?;
    if let Some((task, case_id)) = task {
        audit::record(conn, user, Audit { action: if resolution == "exception" { "tache_soldee_par_exception" } else { "tache_terminee" }, entity: "task", entity_id: Some(task), case_id: Some(case_id), data: Some(json!({ "resolution": resolution, "preuve": evidence })), ..Default::default() }).await?;
    }
    Ok(())
}

#[derive(Deserialize, Default)]
struct ConfirmInput {
    /// Heure réelle de la coupure (par défaut : maintenant).
    done_at: Option<DateTime<Utc>>,
    system: Option<String>,
    note: Option<String>,
}

/// Confirmation par l'opérateur : coupure réellement faite (heure,
/// système, opérateur), siège libéré, bien ou véhicule rendu.
async fn confirm_control(State(state): State<AppState>, user: CurrentUser, Path(id): Path<Uuid>, body: Option<Json<ConfirmInput>>) -> AppResult<Json<Value>> {
    let i = body.map(|Json(i)| i).unwrap_or_default();
    let mut tx = state.pool.begin().await?;
    let c = lock_control(&mut tx, id).await?;
    user.require(role_for(&c.kind))?;
    if !["a_traiter", "ecart"].contains(&c.state.as_str()) {
        return Err(AppError::conflict("contrôle déjà soldé"));
    }
    let now = Utc::now();
    let done_at = i.done_at.unwrap_or(now);
    if done_at > now + chrono::Duration::minutes(5) {
        return Err(AppError::invalid("done_at", "l'heure de réalisation ne peut pas être future"));
    }
    let note = clean(&i.note);
    let evidence = match c.kind.as_str() {
        "compte" => {
            let account = c.account_id.ok_or_else(|| anyhow::anyhow!("contrôle sans compte"))?;
            sqlx::query("UPDATE employee_accounts SET status = 'coupe', ended_at = $2, verified_at = now() WHERE id = $1")
                .bind(account)
                .bind(done_at)
                .execute(&mut *tx)
                .await?;
            audit::record(&mut tx, &user, Audit { action: "compte_statut", entity: "employee_account", entity_id: Some(account), employee_id: Some(c.employee_id), data: Some(json!({ "status": "coupe", "a": done_at, "depart": c.departure_id })), ..Default::default() }).await?;
            json!({ "account_id": account, "coupe_a": done_at, "systeme": clean(&i.system), "operateur": user.display_name })
        }
        "licence" => {
            let alloc = c.license_allocation_id.ok_or_else(|| anyhow::anyhow!("contrôle sans licence"))?;
            let ended: bool = sqlx::query_scalar("SELECT ended_at IS NOT NULL FROM license_allocations WHERE id = $1").bind(alloc).fetch_one(&mut *tx).await?;
            if !ended {
                crate::routes::software::release_seat(&mut tx, &user, alloc, note.clone().or(Some("départ".into()))).await?;
            }
            json!({ "license_allocation_id": alloc })
        }
        "bien" => {
            let a = c.assignment_id.ok_or_else(|| anyhow::anyhow!("contrôle sans bien"))?;
            let returned: Option<(Option<DateTime<Utc>>,)> = sqlx::query_as("SELECT ended_at FROM assignments WHERE id = $1").bind(a).fetch_optional(&mut *tx).await?;
            if returned.and_then(|r| r.0).is_none() {
                return Err(AppError::conflict("bien encore détenu : enregistrez d'abord sa restitution (Parc IT → Restitution)"));
            }
            json!({ "assignment_id": a })
        }
        "vehicule" => {
            let a = c.vehicle_assignment_id.ok_or_else(|| anyhow::anyhow!("contrôle sans véhicule"))?;
            let ended: bool = sqlx::query_scalar("SELECT ended_at IS NOT NULL FROM vehicle_assignments WHERE id = $1").bind(a).fetch_one(&mut *tx).await?;
            if !ended {
                return Err(AppError::conflict("véhicule encore confié : enregistrez d'abord sa restitution"));
            }
            json!({ "vehicle_assignment_id": a })
        }
        _ => json!({ "note": note }),
    };
    sqlx::query(
        "UPDATE exit_controls SET state = 'fait', done_at = $2, confirmed_by = $3, system = $4, note = coalesce($5, note), updated_at = now() WHERE id = $1",
    )
    .bind(c.id)
    .bind(done_at)
    .bind(user.id)
    .bind(clean(&i.system))
    .bind(&note)
    .execute(&mut *tx)
    .await?;
    finish_task(&mut tx, &user, c.id, "realise", evidence.clone(), note.as_deref()).await?;
    audit::record(&mut tx, &user, Audit { action: if c.kind == "compte" { "coupure_confirmee" } else { "controle_sortie_fait" }, entity: "exit_control", entity_id: Some(c.id), case_id: Some(c.case_id), employee_id: Some(c.employee_id), data: Some(json!({ "label": c.label, "preuve": evidence })) }).await?;
    tx.commit().await?;
    Ok(Json(json!({ "state": "fait", "done_at": done_at })))
}

/// Appelé par une restitution : le contrôle de sortie du bien rendu est
/// soldé automatiquement, avec la référence du mouvement.
pub async fn on_returned(conn: &mut PgConnection, user: &CurrentUser, assignment_id: Option<Uuid>, vehicle_assignment_id: Option<Uuid>, movement_ref: &str) -> AppResult<()> {
    let controls: Vec<Uuid> = sqlx::query_scalar(
        "SELECT c.id FROM exit_controls c JOIN departures d ON d.id = c.departure_id
          WHERE c.state IN ('a_traiter', 'ecart') AND d.status <> 'annule'
            AND ((c.assignment_id = $1 AND $1 IS NOT NULL) OR (c.vehicle_assignment_id = $2 AND $2 IS NOT NULL))",
    )
    .bind(assignment_id)
    .bind(vehicle_assignment_id)
    .fetch_all(&mut *conn)
    .await?;
    for c in controls {
        sqlx::query("UPDATE exit_controls SET state = 'fait', done_at = now(), confirmed_by = $2, note = coalesce(note || ' — ', '') || 'rendu : ' || $3, updated_at = now() WHERE id = $1")
            .bind(c)
            .bind(user.id)
            .bind(movement_ref)
            .execute(&mut *conn)
            .await?;
        finish_task(conn, user, c, "realise", json!({ "mouvement": movement_ref }), None).await?;
    }
    Ok(())
}

#[derive(Deserialize)]
struct GapInput {
    note: String,
    follow_up_date: Option<NaiveDate>,
}

/// « Signaler un écart » : le bien n'est pas rendu (ou l'accès pose
/// problème) ; il reste visible avec sa prochaine action et sa date.
async fn report_gap(State(state): State<AppState>, user: CurrentUser, Path(id): Path<Uuid>, Json(i): Json<GapInput>) -> AppResult<Json<Value>> {
    let note = domain::text::clean(&i.note).ok_or_else(|| AppError::invalid("note", "décrivez l'écart"))?;
    let mut tx = state.pool.begin().await?;
    let c = lock_control(&mut tx, id).await?;
    user.require(&[Role::It, Role::Rh, Role::Admin])?;
    if !["a_traiter", "ecart"].contains(&c.state.as_str()) {
        return Err(AppError::conflict("contrôle déjà soldé"));
    }
    sqlx::query("UPDATE exit_controls SET state = 'ecart', note = $2, follow_up_date = $3, updated_at = now() WHERE id = $1")
        .bind(c.id)
        .bind(&note)
        .bind(i.follow_up_date)
        .execute(&mut *tx)
        .await?;
    sqlx::query("UPDATE tasks SET due_date = coalesce($2, due_date), version = version + 1, updated_at = now() WHERE exit_control_id = $1 AND status IN ('a_faire', 'en_cours', 'bloque')")
        .bind(c.id)
        .bind(i.follow_up_date)
        .execute(&mut *tx)
        .await?;
    audit::record(&mut tx, &user, Audit { action: "ecart_sortie", entity: "exit_control", entity_id: Some(c.id), case_id: Some(c.case_id), employee_id: Some(c.employee_id), data: Some(json!({ "label": c.label, "note": note, "suivi": i.follow_up_date })) }).await?;
    tx.commit().await?;
    Ok(Json(json!({ "state": "ecart" })))
}

#[derive(Deserialize)]
struct ReasonInput {
    reason: String,
}

/// Perte : décision motivée du RH ou de l'ADMIN, distincte d'un retour.
/// La garde est close sans restitution ; le bien est réformé.
async fn declare_loss(State(state): State<AppState>, user: CurrentUser, Path(id): Path<Uuid>, Json(i): Json<ReasonInput>) -> AppResult<Json<Value>> {
    user.require(domain::roles::HR_MANAGERS)?;
    let reason = domain::text::clean(&i.reason).ok_or_else(|| AppError::invalid("reason", "motif de la décision obligatoire"))?;
    let mut tx = state.pool.begin().await?;
    let c = lock_control(&mut tx, id).await?;
    if !["a_traiter", "ecart"].contains(&c.state.as_str()) {
        return Err(AppError::conflict("contrôle déjà soldé"));
    }
    match c.kind.as_str() {
        "bien" => {
            let a = c.assignment_id.ok_or_else(|| anyhow::anyhow!("contrôle sans bien"))?;
            let row: Option<(Option<Uuid>, Option<Uuid>, Option<Uuid>, i32)> = sqlx::query_as(
                "UPDATE assignments SET open_quantity = 0, ended_at = now() WHERE id = $1 AND ended_at IS NULL
                 RETURNING asset_id, sim_id, accessory_id, quantity",
            )
            .bind(a)
            .fetch_optional(&mut *tx)
            .await?;
            if let Some((asset, sim, _, _)) = row {
                if let Some(asset) = asset {
                    sqlx::query("UPDATE assets SET status = 'reforme', notes = coalesce(notes || E'\\n', '') || $2, version = version + 1, updated_at = now() WHERE id = $1")
                        .bind(asset)
                        .bind(format!("Perte déclarée au départ : {reason}"))
                        .execute(&mut *tx)
                        .await?;
                }
                if let Some(sim) = sim {
                    sqlx::query("UPDATE sim_cards SET status = 'desactivee' WHERE id = $1").bind(sim).execute(&mut *tx).await?;
                }
            }
        }
        "vehicule" => {
            let a = c.vehicle_assignment_id.ok_or_else(|| anyhow::anyhow!("contrôle sans véhicule"))?;
            let v: Option<Uuid> = sqlx::query_scalar("UPDATE vehicle_assignments SET ended_at = now() WHERE id = $1 AND ended_at IS NULL RETURNING vehicle_id")
                .bind(a)
                .fetch_optional(&mut *tx)
                .await?;
            if let Some(v) = v {
                sqlx::query("UPDATE vehicles SET status = 'immobilise', notes = coalesce(notes || E'\\n', '') || $2, version = version + 1, updated_at = now() WHERE id = $1")
                    .bind(v)
                    .bind(format!("Non restitué au départ : {reason}"))
                    .execute(&mut *tx)
                    .await?;
            }
        }
        _ => return Err(AppError::invalid("kind", "une perte ne concerne qu'un bien ou un véhicule")),
    }
    sqlx::query("UPDATE exit_controls SET state = 'perte', note = $2, confirmed_by = $3, done_at = now(), updated_at = now() WHERE id = $1")
        .bind(c.id)
        .bind(&reason)
        .bind(user.id)
        .execute(&mut *tx)
        .await?;
    finish_task(&mut tx, &user, c.id, "exception", json!({ "motif": reason, "decision": "perte" }), Some(&reason)).await?;
    audit::record(&mut tx, &user, Audit { action: "perte_declaree", entity: "exit_control", entity_id: Some(c.id), case_id: Some(c.case_id), employee_id: Some(c.employee_id), data: Some(json!({ "label": c.label, "motif": reason })) }).await?;
    tx.commit().await?;
    Ok(Json(json!({ "state": "perte" })))
}

/// Sans objet (accès transféré avec autorisation, licence conservée par
/// décision…) : motivé par le RH ou l'ADMIN.
async fn not_applicable(State(state): State<AppState>, user: CurrentUser, Path(id): Path<Uuid>, Json(i): Json<ReasonInput>) -> AppResult<Json<Value>> {
    user.require(domain::roles::HR_MANAGERS)?;
    let reason = domain::text::clean(&i.reason).ok_or_else(|| AppError::invalid("reason", "motif obligatoire"))?;
    let mut tx = state.pool.begin().await?;
    let c = lock_control(&mut tx, id).await?;
    if !["a_traiter", "ecart"].contains(&c.state.as_str()) {
        return Err(AppError::conflict("contrôle déjà soldé"));
    }
    sqlx::query("UPDATE exit_controls SET state = 'non_applicable', note = $2, confirmed_by = $3, updated_at = now() WHERE id = $1")
        .bind(c.id)
        .bind(&reason)
        .bind(user.id)
        .execute(&mut *tx)
        .await?;
    finish_task(&mut tx, &user, c.id, "exception", json!({ "motif": reason, "decision": "sans_objet" }), Some(&reason)).await?;
    audit::record(&mut tx, &user, Audit { action: "controle_sans_objet", entity: "exit_control", entity_id: Some(c.id), case_id: Some(c.case_id), employee_id: Some(c.employee_id), data: Some(json!({ "label": c.label, "motif": reason })) }).await?;
    tx.commit().await?;
    Ok(Json(json!({ "state": "non_applicable" })))
}

// ------------------------------------------------------------------ worker

/// Traitements à échéance, rejouables après un redémarrage : coupure
/// exigible (alerte IT), relance si non confirmée (« accès à vérifier »),
/// sortie administrative au lendemain du dernier jour.
pub async fn process_due(pool: &sqlx::PgPool, today: NaiveDate, grace_minutes: i64) -> anyhow::Result<()> {
    // 1. Coupure exigible.
    let due: Vec<(Uuid, Uuid, String, String, DateTime<Utc>)> = sqlx::query_as(
        "SELECT d.id, d.case_id, c.reference, e.first_name || ' ' || e.last_name, d.cutoff_at
           FROM departures d JOIN cases c ON c.id = d.case_id JOIN employees e ON e.id = d.employee_id
          WHERE d.status = 'planifie' AND d.cutoff_at <= now() AND d.cutoff_notified_at IS NULL",
    )
    .fetch_all(pool)
    .await?;
    for (id, case_id, reference, name, cutoff) in due {
        let mut tx = pool.begin().await?;
        let claimed = sqlx::query("UPDATE departures SET cutoff_notified_at = now() WHERE id = $1 AND cutoff_notified_at IS NULL")
            .bind(id)
            .execute(&mut *tx)
            .await?
            .rows_affected();
        if claimed == 1 {
            let open: Vec<String> = sqlx::query_scalar("SELECT label FROM exit_controls WHERE departure_id = $1 AND kind = 'compte' AND state IN ('a_traiter', 'ecart') ORDER BY label")
                .bind(id)
                .fetch_all(&mut *tx)
                .await?;
            if !open.is_empty() {
                crate::worker::enqueue_system(
                    &mut tx,
                    "coupure_exigible",
                    Some(case_id),
                    &["IT"],
                    &format!("{reference} — Coupure des accès exigible : {name}"),
                    &format!(
                        "Heure de coupure atteinte ({}).\nAccès à couper puis à confirmer dans l'application :\n- {}\n",
                        algiers(cutoff),
                        open.join("\n- ")
                    ),
                    Some(&format!("/dossiers/{reference}")),
                )
                .await?;
            }
            audit::record_system(&mut tx, Audit { action: "coupure_exigible", entity: "departure", entity_id: Some(id), case_id: Some(case_id), data: Some(json!({ "acces": open })), ..Default::default() }).await?;
        }
        tx.commit().await?;
    }
    // 2. Relance : coupure non confirmée après le délai de grâce.
    let late: Vec<(Uuid, Uuid, String, String)> = sqlx::query_as(
        "SELECT d.id, d.case_id, c.reference, e.first_name || ' ' || e.last_name
           FROM departures d JOIN cases c ON c.id = d.case_id JOIN employees e ON e.id = d.employee_id
          WHERE d.status IN ('planifie', 'sortie_effective') AND d.escalated_at IS NULL
            AND d.cutoff_at + make_interval(mins => $1::int) <= now()
            AND EXISTS (SELECT 1 FROM exit_controls x WHERE x.departure_id = d.id AND x.kind = 'compte' AND x.state IN ('a_traiter', 'ecart'))",
    )
    .bind(grace_minutes as i32)
    .fetch_all(pool)
    .await?;
    for (id, case_id, reference, name) in late {
        let mut tx = pool.begin().await?;
        let claimed = sqlx::query("UPDATE departures SET escalated_at = now() WHERE id = $1 AND escalated_at IS NULL")
            .bind(id)
            .execute(&mut *tx)
            .await?
            .rows_affected();
        if claimed == 1 {
            let open: Vec<String> = sqlx::query_scalar("SELECT label FROM exit_controls WHERE departure_id = $1 AND kind = 'compte' AND state IN ('a_traiter', 'ecart') ORDER BY label")
                .bind(id)
                .fetch_all(&mut *tx)
                .await?;
            sqlx::query("UPDATE tasks SET escalated_at = now() WHERE case_id = $1 AND gen_key LIKE 'off:compte:%' AND status IN ('a_faire', 'en_cours', 'bloque')")
                .bind(case_id)
                .execute(&mut *tx)
                .await?;
            crate::worker::enqueue_system(
                &mut tx,
                "coupure_a_verifier",
                Some(case_id),
                &["IT", "RH", "ADMIN"],
                &format!("{reference} — Accès à vérifier : coupure non confirmée pour {name}"),
                &format!("La coupure prévue n'est pas confirmée. Accès à vérifier (non réputés coupés) :\n- {}\n", open.join("\n- ")),
                Some(&format!("/dossiers/{reference}")),
            )
            .await?;
            audit::record_system(&mut tx, Audit { action: "coupure_relancee", entity: "departure", entity_id: Some(id), case_id: Some(case_id), data: Some(json!({ "acces": open })), ..Default::default() }).await?;
        }
        tx.commit().await?;
    }
    // 3. Sortie administrative, sans attendre les restitutions.
    let exits: Vec<(Uuid, Uuid, Uuid, NaiveDate, String, String)> = sqlx::query_as(
        "SELECT d.id, d.case_id, d.employee_id, d.last_day, c.reference, e.first_name || ' ' || e.last_name
           FROM departures d JOIN cases c ON c.id = d.case_id JOIN employees e ON e.id = d.employee_id
          WHERE d.status = 'planifie' AND d.last_day < $1",
    )
    .bind(today)
    .fetch_all(pool)
    .await?;
    for (id, case_id, employee_id, last_day, reference, name) in exits {
        let mut tx = pool.begin().await?;
        let claimed = sqlx::query("UPDATE departures SET status = 'sortie_effective', exit_applied_at = now(), version = version + 1 WHERE id = $1 AND status = 'planifie'")
            .bind(id)
            .execute(&mut *tx)
            .await?
            .rows_affected();
        if claimed == 1 {
            sqlx::query("UPDATE employees SET status = 'sorti', exit_date = $2, leaving = false, version = version + 1, updated_at = now() WHERE id = $1")
                .bind(employee_id)
                .bind(last_day)
                .execute(&mut *tx)
                .await?;
            sqlx::query(
                "UPDATE employee_positions SET valid_to = GREATEST($2::date + 1, valid_from + 1) WHERE employee_id = $1 AND valid_to IS NULL",
            )
            .bind(employee_id)
            .bind(last_day)
            .execute(&mut *tx)
            .await?;
            let remaining: i64 = sqlx::query_scalar("SELECT count(*) FROM exit_controls WHERE departure_id = $1 AND state IN ('a_traiter', 'ecart')")
                .bind(id)
                .fetch_one(&mut *tx)
                .await?;
            audit::record_system(&mut tx, Audit { action: "sortie_effective", entity: "departure", entity_id: Some(id), case_id: Some(case_id), employee_id: Some(employee_id), data: Some(json!({ "dernier_jour": last_day, "restant": remaining })) }).await?;
            crate::worker::enqueue_system(
                &mut tx,
                "sortie_effective",
                Some(case_id),
                &["RH"],
                &format!("{reference} — Sortie administrative de {name}"),
                &format!("{name} est sorti(e) au {}. Points encore ouverts : {remaining}.\n", fr_date(last_day)),
                Some(&format!("/dossiers/{reference}")),
            )
            .await?;
        }
        tx.commit().await?;
    }
    Ok(())
}

/// Données du départ pour la page du dossier. Le motif privé n'est rendu
/// qu'au RH et à l'ADMIN ; l'IT voit les accès et biens, pas le véhicule.
pub async fn case_json(conn: &mut PgConnection, case_id: Uuid, hr: bool, kinds: &[&str], grace_minutes: i64) -> AppResult<Value> {
    let v: Option<Value> = sqlx::query_scalar(
        "SELECT jsonb_build_object('id', d.id, 'status', d.status, 'version', d.version, 'reason', d.reason,
                'private_note', CASE WHEN $2 THEN d.private_note ELSE NULL END,
                'last_day', d.last_day, 'cutoff_at', d.cutoff_at, 'hr_contact', d.hr_contact,
                'cutoff_notified_at', d.cutoff_notified_at, 'escalated_at', d.escalated_at, 'exit_applied_at', d.exit_applied_at,
                'cancelled_at', d.cancelled_at, 'cancel_reason', CASE WHEN $2 THEN d.cancel_reason ELSE NULL END,
                'access_due', d.cutoff_at <= now(), 'access_late', d.cutoff_at + make_interval(mins => $4::int) <= now(),
                'controls', (SELECT coalesce(jsonb_agg(jsonb_build_object('id', x.id, 'kind', x.kind, 'label', x.label, 'detail', x.detail,
                                 'account_id', x.account_id,
                                 'state', x.state, 'done_at', x.done_at, 'system', x.system, 'note', x.note, 'follow_up_date', x.follow_up_date,
                                 'confirmed_by', u.display_name,
                                 'task', (SELECT jsonb_build_object('id', k.id, 'status', k.status, 'owner', o.display_name)
                                            FROM tasks k LEFT JOIN users o ON o.id = k.owner_id WHERE k.exit_control_id = x.id LIMIT 1))
                                 ORDER BY array_position(ARRAY['compte','licence','bien','vehicule','donnees','autre'], x.kind), x.label), '[]')
                              FROM exit_controls x LEFT JOIN users u ON u.id = x.confirmed_by
                             WHERE x.departure_id = d.id AND x.kind = ANY($3)))
           FROM departures d WHERE d.case_id = $1",
    )
    .bind(case_id)
    .bind(hr)
    .bind(kinds)
    .bind(grace_minutes as i32)
    .fetch_optional(conn)
    .await?;
    Ok(v.unwrap_or(Value::Null))
}
