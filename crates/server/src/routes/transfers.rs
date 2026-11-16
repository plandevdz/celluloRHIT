//! Mutation (M05, U5) : dossier MUT, comparaison figée « Actuel / Cible /
//! Action », confirmation qui ouvre les seules actions nécessaires, puis
//! application unique de la nouvelle situation à la date d'effet (worker).
//! La fiche effective n'est jamais modifiée avant l'application.

use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::{Json, Router};
use chrono::{Datelike, NaiveDate};
use domain::needs::NeedInput;
use domain::transitions::{check_effective_date, needs_to_add, propose_for_held, DeltaAction, Held};
use domain::{CaseKind, NeedAnswer, NeedCategory, Role};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::PgConnection;
use uuid::Uuid;

use crate::audit::{self, Audit, Notice};
use crate::auth::CurrentUser;
use crate::error::{AppError, AppResult};
use crate::idempotency::{self, Begin};
use crate::routes::cases::{add_training, apply_need, fr_date, next_reference, TrainingDto};
use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/transfers", post(create))
        .route("/transfers/{id}/deltas", post(set_deltas))
        .route("/transfers/{id}/confirm", post(confirm))
        .route("/transfers/{id}/reschedule", post(reschedule))
        .route("/transfers/{id}/cancel", post(cancel))
        .route("/transfers/{id}/refresh", post(refresh))
        .route("/employees/{id}/positions", post(dated_correction))
}

fn ymd(d: NaiveDate) -> (i32, u32, u32) {
    (d.year(), d.month(), d.day())
}

fn clean(v: &Option<String>) -> Option<String> {
    v.as_deref().and_then(domain::text::clean)
}

// ------------------------------------------------------------------ situation et écarts

pub struct Current {
    pub position_id: Uuid,
    pub job_title: String,
    pub site_id: Uuid,
    pub site_name: String,
    pub department: Option<String>,
    pub manager: Option<String>,
    pub manager_employee_id: Option<Uuid>,
    pub job_profile_id: Option<Uuid>,
    pub valid_from: NaiveDate,
}

pub async fn current_position(conn: &mut PgConnection, employee_id: Uuid) -> AppResult<Option<Current>> {
    let row: Option<(Uuid, String, Uuid, String, Option<String>, Option<String>, Option<Uuid>, Option<Uuid>, NaiveDate)> = sqlx::query_as(
        "SELECT p.id, p.job_title, p.site_id, s.name, p.department, p.manager, p.manager_employee_id, p.job_profile_id, p.valid_from
           FROM employee_positions p JOIN sites s ON s.id = p.site_id
          WHERE p.employee_id = $1 AND p.valid_to IS NULL",
    )
    .bind(employee_id)
    .fetch_optional(conn)
    .await?;
    Ok(row.map(|(position_id, job_title, site_id, site_name, department, manager, manager_employee_id, job_profile_id, valid_from)| Current {
        position_id,
        job_title,
        site_id,
        site_name,
        department,
        manager,
        manager_employee_id,
        job_profile_id,
        valid_from,
    }))
}

struct DeltaRow {
    object_key: String,
    object_kind: &'static str,
    object_ref: Option<Uuid>,
    category: Option<NeedCategory>,
    label: String,
    current: Option<String>,
    target: Option<String>,
    proposal: DeltaAction,
}

/// Photographie : poste, comptes, licences, biens, véhicules et
/// formations réalisés ; écarts proposés avec la cible.
async fn compare(conn: &mut PgConnection, employee_id: Uuid, cur: &Current, target_profile: Option<Uuid>) -> AppResult<(Value, Vec<DeltaRow>)> {
    let accounts: Vec<(Uuid, String, String, Option<String>)> = sqlx::query_as(
        "SELECT id, system, identifier, platform FROM employee_accounts WHERE employee_id = $1 AND status <> 'coupe' ORDER BY system",
    )
    .bind(employee_id)
    .fetch_all(&mut *conn)
    .await?;
    let licenses: Vec<(Uuid, String)> = sqlx::query_as(
        "SELECT a.id, s.vendor || ' ' || s.product FROM license_allocations a JOIN software_subscriptions s ON s.id = a.subscription_id
          WHERE a.employee_id = $1 AND a.ended_at IS NULL",
    )
    .bind(employee_id)
    .fetch_all(&mut *conn)
    .await?;
    let holdings: Vec<(Uuid, String, Option<String>, String, Option<String>, i32)> = sqlx::query_as(
        "SELECT g.id, g.item_type, x.kind, ml.label, ml.identifier, g.open_quantity
           FROM assignments g JOIN movement_lines ml ON ml.id = g.handover_line_id LEFT JOIN assets x ON x.id = g.asset_id
          WHERE g.employee_id = $1 AND g.ended_at IS NULL ORDER BY ml.label",
    )
    .bind(employee_id)
    .fetch_all(&mut *conn)
    .await?;
    let vehicles: Vec<(Uuid, String)> = sqlx::query_as(
        "SELECT a.id, v.plate FROM vehicle_assignments a JOIN vehicles v ON v.id = a.vehicle_id WHERE a.employee_id = $1 AND a.ended_at IS NULL",
    )
    .bind(employee_id)
    .fetch_all(&mut *conn)
    .await?;
    let done: Vec<(Uuid, String)> = sqlx::query_as(
        "SELECT DISTINCT c.id, c.name FROM training_sessions s JOIN training_catalog c ON c.id = s.catalog_id
          WHERE s.employee_id = $1 AND s.status = 'realisee'",
    )
    .bind(employee_id)
    .fetch_all(&mut *conn)
    .await?;
    let (target_needs, target_trainings): (Option<Vec<NeedCategory>>, Vec<(Uuid, String)>) = match target_profile {
        None => (None, vec![]),
        Some(p) => {
            let cats: Vec<String> = sqlx::query_scalar("SELECT category FROM profile_needs WHERE profile_id = $1").bind(p).fetch_all(&mut *conn).await?;
            let trainings: Vec<(Uuid, String)> = sqlx::query_as(
                "SELECT c.id, c.name FROM profile_trainings t JOIN training_catalog c ON c.id = t.catalog_id WHERE t.profile_id = $1 AND c.active",
            )
            .bind(p)
            .fetch_all(&mut *conn)
            .await?;
            (Some(cats.iter().filter_map(|c| NeedCategory::parse(c)).collect()), trainings)
        }
    };
    let target = target_needs.as_deref();
    let tgt = |c: Option<NeedCategory>| -> Option<String> {
        target.map(|t| if c.map(|c| t.contains(&c)).unwrap_or(false) { "prévu par le profil cible".to_string() } else { "non prévu par le profil cible".to_string() })
    };
    let mut deltas = Vec::new();
    let mut held: Vec<Held> = Vec::new();
    for (id, system, identifier, platform) in &accounts {
        let category = match system.as_str() {
            "email" => Some(NeedCategory::EmailPro),
            "xena" => Some(NeedCategory::Xena),
            "intelix" => Some(NeedCategory::Intelix),
            _ => None,
        };
        let h = Held { category, is_vehicle: false };
        let system_label = match system.as_str() {
            "email" => format!("Adresse email ({})", if platform.as_deref() == Some("m365") { "Microsoft 365" } else { "cPanel" }),
            "xena" => "Accès Xena".into(),
            "intelix" => "Accès Intelix".into(),
            _ => "Autre accès".into(),
        };
        deltas.push(DeltaRow {
            object_key: format!("compte:{id}"),
            object_kind: "compte",
            object_ref: Some(*id),
            category,
            label: format!("{system_label} : {identifier}"),
            current: Some("actif".into()),
            target: tgt(category),
            proposal: propose_for_held(&h, target),
        });
        held.push(h);
    }
    for (id, label) in &licenses {
        let h = Held { category: None, is_vehicle: false };
        deltas.push(DeltaRow {
            object_key: format!("licence:{id}"),
            object_kind: "licence",
            object_ref: Some(*id),
            category: None,
            label: format!("Licence {label}"),
            current: Some("siège attribué".into()),
            target: target.map(|_| "à confirmer par l'IT".to_string()),
            proposal: propose_for_held(&h, target),
        });
    }
    for (id, item_type, kind, label, identifier, qty) in &holdings {
        let category = match (item_type.as_str(), kind.as_deref()) {
            ("asset", Some("pc_portable" | "pc_fixe")) => Some(NeedCategory::Pc),
            ("asset", Some("telephone")) => Some(NeedCategory::Telephone),
            ("asset", Some("ecran")) => Some(NeedCategory::Ecran),
            ("asset", Some("tablette")) => Some(NeedCategory::Tablette),
            ("asset", Some("imprimante")) => Some(NeedCategory::Imprimante),
            ("accessoire", _) => Some(NeedCategory::Accessoire),
            ("sim", _) => Some(NeedCategory::Sim),
            _ => None,
        };
        let h = Held { category, is_vehicle: false };
        let qty_txt = if *qty > 1 { format!(" × {qty}") } else { String::new() };
        deltas.push(DeltaRow {
            object_key: format!("bien:{id}"),
            object_kind: "bien",
            object_ref: Some(*id),
            category,
            label: format!("{label}{}{qty_txt}", identifier.as_deref().map(|i| format!(" ({i})")).unwrap_or_default()),
            current: Some("détenu".into()),
            target: tgt(category),
            proposal: propose_for_held(&h, target),
        });
        held.push(h);
    }
    for (id, plate) in &vehicles {
        let h = Held { category: Some(NeedCategory::Vehicule), is_vehicle: true };
        deltas.push(DeltaRow {
            object_key: format!("vehicule:{id}"),
            object_kind: "vehicule",
            object_ref: Some(*id),
            category: Some(NeedCategory::Vehicule),
            label: format!("Véhicule {plate}"),
            current: Some("confié".into()),
            target: tgt(Some(NeedCategory::Vehicule)),
            proposal: propose_for_held(&h, target),
        });
        held.push(h);
    }
    if let Some(t) = target {
        for c in needs_to_add(t, &held) {
            deltas.push(DeltaRow {
                object_key: format!("besoin:{}", c.as_str()),
                object_kind: "besoin",
                object_ref: None,
                category: Some(c),
                label: c.label().to_string(),
                current: Some("non détenu".into()),
                target: Some("prévu par le profil cible".into()),
                proposal: DeltaAction::Ajouter,
            });
        }
    }
    for (id, name) in &target_trainings {
        let already = done.iter().any(|(d, _)| d == id);
        deltas.push(DeltaRow {
            object_key: format!("formation:{id}"),
            object_kind: "formation",
            object_ref: Some(*id),
            category: Some(NeedCategory::Formation),
            label: format!("Formation : {name}"),
            current: Some(if already { "déjà réalisée" } else { "non suivie" }.into()),
            target: Some("prévue par le profil cible".into()),
            proposal: if already { DeltaAction::Conserver } else { DeltaAction::Ajouter },
        });
    }
    let snapshot = json!({
        "position": {
            "id": cur.position_id, "job_title": cur.job_title, "site_id": cur.site_id, "site": cur.site_name,
            "department": cur.department, "manager": cur.manager, "manager_employee_id": cur.manager_employee_id,
            "job_profile_id": cur.job_profile_id, "valid_from": cur.valid_from,
        },
        "comptes": accounts.iter().map(|a| json!({ "id": a.0, "system": a.1, "identifier": a.2, "platform": a.3 })).collect::<Vec<_>>(),
        "licences": licenses.iter().map(|l| json!({ "id": l.0, "label": l.1 })).collect::<Vec<_>>(),
        "biens": holdings.iter().map(|h| json!({ "id": h.0, "label": h.3, "identifier": h.4, "quantity": h.5 })).collect::<Vec<_>>(),
        "vehicules": vehicles.iter().map(|v| json!({ "id": v.0, "plate": v.1 })).collect::<Vec<_>>(),
        "formations_realisees": done.iter().map(|d| json!({ "id": d.0, "name": d.1 })).collect::<Vec<_>>(),
    });
    Ok((snapshot, deltas))
}

async fn store_deltas(conn: &mut PgConnection, transfer_id: Uuid, deltas: &[DeltaRow]) -> AppResult<()> {
    for d in deltas {
        sqlx::query(
            "INSERT INTO transfer_deltas (transfer_id, object_key, object_kind, object_ref, category, label, current, target, proposal, decision)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $9)
             ON CONFLICT (transfer_id, object_key) DO UPDATE
                SET label = EXCLUDED.label, current = EXCLUDED.current, target = EXCLUDED.target, proposal = EXCLUDED.proposal",
        )
        .bind(transfer_id)
        .bind(&d.object_key)
        .bind(d.object_kind)
        .bind(d.object_ref)
        .bind(d.category.map(|c| c.as_str()))
        .bind(&d.label)
        .bind(&d.current)
        .bind(&d.target)
        .bind(d.proposal.as_str())
        .execute(&mut *conn)
        .await?;
    }
    Ok(())
}

// ------------------------------------------------------------------ création

#[derive(Deserialize)]
struct CreateInput {
    employee_id: Uuid,
    effective_date: NaiveDate,
    job_title: Option<String>,
    site_id: Option<Uuid>,
    department: Option<String>,
    manager_employee_id: Option<Uuid>,
    job_profile_id: Option<Uuid>,
    /// Wilayas couvertes après la mutation. Absent : inchangées.
    wilaya_ids: Option<Vec<Uuid>>,
    note: Option<String>,
}

/// MUT-xxxxx : la fiche effective n'est pas modifiée ; la cible reprend
/// les valeurs actuelles sauf modification explicite.
async fn create(State(state): State<AppState>, user: CurrentUser, headers: HeaderMap, Json(raw): Json<Value>) -> AppResult<Response> {
    user.require(domain::roles::HR_MANAGERS)?;
    let i: CreateInput = serde_json::from_value(raw.clone()).map_err(|e| AppError::invalid("body", format!("saisie incomplète : {e}")))?;
    let key = idempotency::key_from(&headers)?;
    let mut tx = state.pool.begin().await?;
    if let Some(k) = &key {
        if let Begin::Replay(status, body) = idempotency::begin(&mut tx, user.id, k, "POST /transfers", &idempotency::hash_request("POST /transfers", &raw)).await? {
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
        return Err(AppError::conflict("salarié sorti : aucune mutation"));
    }
    let planned_departure: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM departures WHERE employee_id = $1 AND status = 'planifie')")
        .bind(i.employee_id)
        .fetch_one(&mut *tx)
        .await?;
    if planned_departure {
        return Err(AppError::conflict("un départ est planifié : il prend priorité sur une mutation"));
    }
    let cur = current_position(&mut tx, i.employee_id).await?.ok_or_else(|| AppError::conflict("aucun poste en cours pour ce salarié"))?;
    check_effective_date(ymd(cur.valid_from), ymd(i.effective_date))?;
    let job_title = match &i.job_title {
        Some(t) => domain::text::clean(t).ok_or_else(|| AppError::invalid("job_title", "poste cible vide"))?,
        None => cur.job_title.clone(),
    };
    let site_id = i.site_id.unwrap_or(cur.site_id);
    let site_name: String = sqlx::query_scalar("SELECT name FROM sites WHERE id = $1 AND active")
        .bind(site_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| AppError::invalid("site_id", "site inconnu ou inactif"))?;
    let manager_employee_id = i.manager_employee_id.or(cur.manager_employee_id);
    if manager_employee_id == Some(i.employee_id) {
        return Err(AppError::invalid("manager_employee_id", "un salarié ne peut pas être son propre manager"));
    }
    let manager: Option<String> = match i.manager_employee_id {
        Some(m) => sqlx::query_scalar("SELECT first_name || ' ' || last_name FROM employees WHERE id = $1").bind(m).fetch_optional(&mut *tx).await?,
        None => cur.manager.clone(),
    };
    let department = match &i.department {
        Some(d) => domain::text::clean(d),
        None => cur.department.clone(),
    };
    let profile = i.job_profile_id.or(cur.job_profile_id);
    if job_title == cur.job_title && site_id == cur.site_id && department == cur.department && manager_employee_id == cur.manager_employee_id && profile == cur.job_profile_id {
        return Err(AppError::invalid("job_title", "la cible est identique au poste actuel"));
    }
    let reference = next_reference(&mut tx, CaseKind::Mut).await?;
    let case_id: Uuid = sqlx::query_scalar(
        "INSERT INTO cases (reference, kind, employee_id, effective_date, due_date, note, created_by)
         VALUES ($1, 'MUT', $2, $3, $3, $4, $5) RETURNING id",
    )
    .bind(&reference)
    .bind(i.employee_id)
    .bind(i.effective_date)
    .bind(clean(&i.note))
    .bind(user.id)
    .fetch_one(&mut *tx)
    .await?;
    let (snapshot, deltas) = compare(&mut tx, i.employee_id, &cur, profile).await?;
    let target_wilayas: Option<Vec<Uuid>> = match &i.wilaya_ids {
        None => None,
        Some(ws) => {
            let mut ws = ws.clone();
            ws.sort();
            ws.dedup();
            let known: i64 = sqlx::query_scalar("SELECT count(*) FROM wilayas WHERE id = ANY($1)").bind(&ws).fetch_one(&mut *tx).await?;
            if known != ws.len() as i64 {
                return Err(AppError::invalid("wilaya_ids", "wilaya inconnue"));
            }
            Some(ws)
        }
    };
    let transfer_id: Uuid = sqlx::query_scalar(
        "INSERT INTO transfers (case_id, employee_id, from_position_id, target_job_title, target_site_id, target_department,
                                target_manager, target_manager_employee_id, target_profile_id, effective_date, snapshot, target_wilaya_ids)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12) RETURNING id",
    )
    .bind(case_id)
    .bind(i.employee_id)
    .bind(cur.position_id)
    .bind(&job_title)
    .bind(site_id)
    .bind(&department)
    .bind(&manager)
    .bind(manager_employee_id)
    .bind(profile)
    .bind(i.effective_date)
    .bind(&snapshot)
    .bind(&target_wilayas)
    .fetch_one(&mut *tx)
    .await
    .map_err(|e| match AppError::from(e) {
        AppError::Conflict { .. } => AppError::conflict("une mutation est déjà en cours pour ce salarié"),
        other => other,
    })?;
    store_deltas(&mut tx, transfer_id, &deltas).await?;
    let _ = &target_wilayas;
    audit::record(
        &mut tx,
        &user,
        Audit {
            action: "mutation_preparee",
            entity: "transfer",
            entity_id: Some(transfer_id),
            case_id: Some(case_id),
            employee_id: Some(i.employee_id),
            data: Some(json!({ "reference": reference, "poste": job_title, "site": site_name, "effet": i.effective_date })),
        },
    )
    .await?;
    let response = json!({ "case_id": case_id, "reference": reference, "transfer_id": transfer_id, "employee": format!("{first} {last}") });
    if let Some(k) = &key {
        idempotency::finish(&mut tx, user.id, k, StatusCode::CREATED, &response).await?;
    }
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(response)).into_response())
}

// ------------------------------------------------------------------ décisions et confirmation

struct Locked {
    case_id: Uuid,
    employee_id: Uuid,
    status: String,
    version: i32,
    from_position_id: Uuid,
    effective_date: NaiveDate,
    reference: String,
}

async fn lock(conn: &mut PgConnection, id: Uuid) -> AppResult<Locked> {
    let row: Option<(Uuid, Uuid, String, i32, Uuid, NaiveDate, String)> = sqlx::query_as(
        "SELECT t.case_id, t.employee_id, t.status, t.version, t.from_position_id, t.effective_date, c.reference
           FROM transfers t JOIN cases c ON c.id = t.case_id WHERE t.id = $1 FOR UPDATE OF t",
    )
    .bind(id)
    .fetch_optional(conn)
    .await?;
    let (case_id, employee_id, status, version, from_position_id, effective_date, reference) = row.ok_or_else(|| AppError::not_found("mutation"))?;
    Ok(Locked { case_id, employee_id, status, version, from_position_id, effective_date, reference })
}

#[derive(Deserialize)]
struct DeltaDecision {
    id: Uuid,
    decision: DeltaAction,
    note: Option<String>,
}

#[derive(Deserialize)]
struct DeltasInput {
    version: i32,
    deltas: Vec<DeltaDecision>,
}

/// Choix « conserver / ajouter / reprendre / vérifier », avant confirmation.
async fn set_deltas(State(state): State<AppState>, user: CurrentUser, Path(id): Path<Uuid>, Json(i): Json<DeltasInput>) -> AppResult<Json<Value>> {
    user.require(domain::roles::HR_MANAGERS)?;
    let mut tx = state.pool.begin().await?;
    let t = lock(&mut tx, id).await?;
    if !["brouillon", "suspendue"].contains(&t.status.as_str()) {
        return Err(AppError::conflict("mutation déjà confirmée : reportez ou annulez"));
    }
    if t.version != i.version {
        return Err(AppError::stale());
    }
    for d in &i.deltas {
        let kind: Option<String> = sqlx::query_scalar("SELECT object_kind FROM transfer_deltas WHERE id = $1 AND transfer_id = $2")
            .bind(d.id)
            .bind(id)
            .fetch_optional(&mut *tx)
            .await?;
        let kind = kind.ok_or_else(|| AppError::invalid("deltas", "écart inconnu"))?;
        let allowed = match kind.as_str() {
            "besoin" | "formation" => matches!(d.decision, DeltaAction::Ajouter | DeltaAction::Conserver),
            _ => matches!(d.decision, DeltaAction::Conserver | DeltaAction::Reprendre | DeltaAction::Verifier),
        };
        if !allowed {
            return Err(AppError::invalid("deltas", format!("« {} » ne s'applique pas à cet élément", d.decision.label())));
        }
        sqlx::query("UPDATE transfer_deltas SET decision = $2, note = $3 WHERE id = $1")
            .bind(d.id)
            .bind(d.decision.as_str())
            .bind(clean(&d.note))
            .execute(&mut *tx)
            .await?;
    }
    sqlx::query("UPDATE transfers SET version = version + 1, updated_at = now() WHERE id = $1").bind(id).execute(&mut *tx).await?;
    audit::record(&mut tx, &user, Audit { action: "mutation_ecarts", entity: "transfer", entity_id: Some(id), case_id: Some(t.case_id), ..Default::default() }).await?;
    tx.commit().await?;
    Ok(Json(json!({ "version": t.version + 1 })))
}

#[derive(Deserialize)]
struct VersionInput {
    version: i32,
}

/// Confirmation de la cible : les seules actions nécessaires sont ouvertes,
/// en parallèle, dans les bonnes équipes. Rien n'est appliqué avant la date.
async fn confirm(State(state): State<AppState>, user: CurrentUser, Path(id): Path<Uuid>, Json(i): Json<VersionInput>) -> AppResult<Json<Value>> {
    user.require(domain::roles::HR_MANAGERS)?;
    let mut tx = state.pool.begin().await?;
    let t = lock(&mut tx, id).await?;
    if !["brouillon", "suspendue"].contains(&t.status.as_str()) {
        return Err(AppError::conflict(format!("mutation « {} » : confirmation impossible", t.status)));
    }
    if t.version != i.version {
        return Err(AppError::stale());
    }
    let cur = current_position(&mut tx, t.employee_id).await?;
    if cur.as_ref().map(|c| c.position_id) != Some(t.from_position_id) {
        return Err(AppError::conflict("le poste a changé depuis la comparaison : recalculez avant de confirmer"));
    }
    let deltas: Vec<(Uuid, String, String, Option<Uuid>, Option<String>, String, String)> = sqlx::query_as(
        "SELECT id, object_key, object_kind, object_ref, category, label, decision FROM transfer_deltas WHERE transfer_id = $1 ORDER BY object_key",
    )
    .bind(id)
    .fetch_all(&mut *tx)
    .await?;
    let mut roles: Vec<Role> = Vec::new();
    let mut lines: Vec<String> = Vec::new();
    for (delta_id, object_key, kind, object_ref, category, label, decision) in &deltas {
        let action = DeltaAction::parse(decision).unwrap_or(DeltaAction::Verifier);
        match (kind.as_str(), action) {
            (_, DeltaAction::Conserver) => {}
            ("besoin", DeltaAction::Ajouter) => {
                let cat = category.as_deref().and_then(NeedCategory::parse).ok_or_else(|| anyhow::anyhow!("catégorie d'écart inconnue"))?;
                let need = NeedInput { category: cat, answer: NeedAnswer::Oui, quantity: None, preference: None, detail: Some("Mutation : prévu par le profil cible".into()) };
                apply_need(&mut tx, &user, t.case_id, &need, Some(t.effective_date)).await?;
                sqlx::query("UPDATE tasks SET transfer_delta_id = $3 WHERE case_id = $1 AND gen_key = $2")
                    .bind(t.case_id)
                    .bind(cat.task_key())
                    .bind(delta_id)
                    .execute(&mut *tx)
                    .await?;
                let role = cat.responsible_role();
                if !roles.contains(&role) {
                    roles.push(role);
                }
                lines.push(format!("{} — ajouter : {label}", role.label()));
            }
            ("formation", DeltaAction::Ajouter) => {
                let catalog = object_ref.ok_or_else(|| anyhow::anyhow!("formation sans catalogue"))?;
                add_training(&mut tx, &user, t.case_id, t.employee_id, &TrainingDto { catalog_id: catalog, trainer_id: None }, Some(t.effective_date)).await?;
                if !roles.contains(&Role::Formation) {
                    roles.push(Role::Formation);
                }
                lines.push(format!("Formation — à planifier : {label}"));
            }
            (kind, DeltaAction::Reprendre | DeltaAction::Verifier) => {
                let role = if kind == "vehicule" { Role::Rh } else { Role::It };
                let verb = if action == DeltaAction::Reprendre { "Reprendre" } else { "Vérifier" };
                sqlx::query(
                    "INSERT INTO tasks (case_id, gen_key, role, title, due_date, transfer_delta_id) VALUES ($1, $2, $3, $4, $5, $6)
                     ON CONFLICT (case_id, gen_key) DO UPDATE SET transfer_delta_id = EXCLUDED.transfer_delta_id,
                            status = CASE WHEN tasks.status = 'annule' THEN 'a_faire' ELSE tasks.status END
                     ",
                )
                .bind(t.case_id)
                .bind(format!("mut:{}:{object_key}", action.as_str()))
                .bind(role.as_str())
                .bind(format!("{verb} : {label}"))
                .bind(t.effective_date)
                .bind(delta_id)
                .execute(&mut *tx)
                .await?;
                if !roles.contains(&role) {
                    roles.push(role);
                }
                lines.push(format!("{} — {} : {label}", role.label(), verb.to_lowercase()));
            }
            _ => {}
        }
    }
    sqlx::query(
        "UPDATE transfers SET status = 'confirmee', conflict = NULL, confirmed_at = now(), confirmed_by = $2,
                version = version + 1, updated_at = now() WHERE id = $1",
    )
    .bind(id)
    .bind(user.id)
    .execute(&mut *tx)
    .await?;
    if !roles.is_empty() {
        let name: String = sqlx::query_scalar("SELECT first_name || ' ' || last_name FROM employees WHERE id = $1").bind(t.employee_id).fetch_one(&mut *tx).await?;
        audit::enqueue(
            &mut tx,
            &user,
            Notice {
                event_type: "mutation_confirmee",
                case_id: Some(t.case_id),
                target_roles: roles.clone(),
                target_user_ids: vec![],
                subject: format!("{} — Mutation de {name} au {}", t.reference, fr_date(t.effective_date)),
                body: format!("Mutation {} de {name}, effet le {} :\n- {}\n", t.reference, fr_date(t.effective_date), lines.join("\n- ")),
                link_path: Some(format!("/dossiers/{}", t.reference)),
            },
        )
        .await?;
    }
    audit::record(&mut tx, &user, Audit { action: "mutation_confirmee", entity: "transfer", entity_id: Some(id), case_id: Some(t.case_id), employee_id: Some(t.employee_id), data: Some(json!({ "actions": lines })), }).await?;
    tx.commit().await?;
    Ok(Json(json!({ "status": "confirmee", "version": t.version + 1, "visible_to": roles, "mail_mode": state.cfg().mail_mode.as_str() })))
}

#[derive(Deserialize)]
struct RescheduleInput {
    version: i32,
    effective_date: NaiveDate,
    reason: String,
}

/// Report : seules les actions futures sont décalées.
async fn reschedule(State(state): State<AppState>, user: CurrentUser, Path(id): Path<Uuid>, Json(i): Json<RescheduleInput>) -> AppResult<Json<Value>> {
    user.require(domain::roles::HR_MANAGERS)?;
    let reason = domain::text::clean(&i.reason).ok_or_else(|| AppError::invalid("reason", "motif du report obligatoire"))?;
    let mut tx = state.pool.begin().await?;
    let t = lock(&mut tx, id).await?;
    if !["brouillon", "confirmee", "suspendue"].contains(&t.status.as_str()) {
        return Err(AppError::conflict("mutation appliquée ou annulée : une correction datée est nécessaire"));
    }
    if t.version != i.version {
        return Err(AppError::stale());
    }
    let cur = current_position(&mut tx, t.employee_id).await?.ok_or_else(|| AppError::conflict("aucun poste en cours"))?;
    check_effective_date(ymd(cur.valid_from), ymd(i.effective_date))?;
    sqlx::query("UPDATE transfers SET effective_date = $2, version = version + 1, updated_at = now() WHERE id = $1")
        .bind(id)
        .bind(i.effective_date)
        .execute(&mut *tx)
        .await?;
    sqlx::query("UPDATE cases SET effective_date = $2, due_date = $2, version = version + 1, updated_at = now() WHERE id = $1")
        .bind(t.case_id)
        .bind(i.effective_date)
        .execute(&mut *tx)
        .await?;
    let shifted = sqlx::query(
        "UPDATE tasks SET due_date = $2, version = version + 1, updated_at = now()
          WHERE case_id = $1 AND status IN ('a_faire', 'en_cours', 'bloque') AND due_date = $3",
    )
    .bind(t.case_id)
    .bind(i.effective_date)
    .bind(t.effective_date)
    .execute(&mut *tx)
    .await?
    .rows_affected();
    audit::record(&mut tx, &user, Audit { action: "mutation_reportee", entity: "transfer", entity_id: Some(id), case_id: Some(t.case_id), employee_id: Some(t.employee_id), data: Some(json!({ "avant": t.effective_date, "apres": i.effective_date, "motif": reason, "taches_decalees": shifted })) }).await?;
    tx.commit().await?;
    Ok(Json(json!({ "version": t.version + 1, "shifted_tasks": shifted })))
}

#[derive(Deserialize)]
struct CancelInput {
    version: i32,
    reason: String,
}

/// Annulation avant application : les actions non faites sont annulées,
/// l'historique est conservé. Après application : correction datée.
async fn cancel(State(state): State<AppState>, user: CurrentUser, Path(id): Path<Uuid>, Json(i): Json<CancelInput>) -> AppResult<Json<Value>> {
    user.require(domain::roles::HR_MANAGERS)?;
    let reason = domain::text::clean(&i.reason).ok_or_else(|| AppError::invalid("reason", "motif d'annulation obligatoire"))?;
    let mut tx = state.pool.begin().await?;
    let t = lock(&mut tx, id).await?;
    if t.version != i.version {
        return Err(AppError::stale());
    }
    let cancelled = cancel_locked(&mut tx, &user, id, &t, &reason).await?;
    tx.commit().await?;
    Ok(Json(json!({ "status": "annulee", "cancelled_tasks": cancelled })))
}

async fn cancel_locked(conn: &mut PgConnection, user: &CurrentUser, id: Uuid, t: &Locked, reason: &str) -> AppResult<u64> {
    match t.status.as_str() {
        "appliquee" => return Err(AppError::conflict("mutation déjà appliquée : enregistrez une correction de poste datée")),
        "annulee" => return Err(AppError::conflict("mutation déjà annulée")),
        _ => {}
    }
    sqlx::query("UPDATE transfers SET status = 'annulee', cancelled_at = now(), cancel_reason = $2, version = version + 1, updated_at = now() WHERE id = $1")
        .bind(id)
        .bind(reason)
        .execute(&mut *conn)
        .await?;
    let n = sqlx::query("UPDATE tasks SET status = 'annule', version = version + 1, updated_at = now() WHERE case_id = $1 AND status IN ('a_faire', 'en_cours', 'bloque')")
        .bind(t.case_id)
        .execute(&mut *conn)
        .await?
        .rows_affected();
    sqlx::query("UPDATE cases SET status = 'annule', closed_at = now(), closed_by = $2, close_note = $3, version = version + 1, updated_at = now() WHERE id = $1")
        .bind(t.case_id)
        .bind(user.id)
        .bind(format!("Mutation annulée : {reason}"))
        .execute(&mut *conn)
        .await?;
    audit::record(conn, user, Audit { action: "mutation_annulee", entity: "transfer", entity_id: Some(id), case_id: Some(t.case_id), employee_id: Some(t.employee_id), data: Some(json!({ "motif": reason, "taches_annulees": n })) }).await?;
    Ok(n)
}

/// Annule la mutation en cours d'un salarié (départ prioritaire).
pub async fn cancel_active_for(conn: &mut PgConnection, user: &CurrentUser, employee_id: Uuid, reason: &str) -> AppResult<Option<String>> {
    let id: Option<Uuid> = sqlx::query_scalar("SELECT id FROM transfers WHERE employee_id = $1 AND status IN ('brouillon', 'confirmee', 'suspendue')")
        .bind(employee_id)
        .fetch_optional(&mut *conn)
        .await?;
    let Some(id) = id else { return Ok(None) };
    let t = lock(conn, id).await?;
    cancel_locked(conn, user, id, &t, reason).await?;
    Ok(Some(t.reference))
}

/// Après une suspension (poste modifié) : nouvelle comparaison avec la
/// situation actuelle, à confirmer de nouveau. Les tâches déjà ouvertes
/// sont conservées (clé stable).
async fn refresh(State(state): State<AppState>, user: CurrentUser, Path(id): Path<Uuid>, Json(i): Json<VersionInput>) -> AppResult<Json<Value>> {
    user.require(domain::roles::HR_MANAGERS)?;
    let mut tx = state.pool.begin().await?;
    let t = lock(&mut tx, id).await?;
    if !["brouillon", "suspendue", "confirmee"].contains(&t.status.as_str()) {
        return Err(AppError::conflict("mutation appliquée ou annulée"));
    }
    if t.version != i.version {
        return Err(AppError::stale());
    }
    let cur = current_position(&mut tx, t.employee_id).await?.ok_or_else(|| AppError::conflict("aucun poste en cours"))?;
    let profile: Option<Uuid> = sqlx::query_scalar("SELECT target_profile_id FROM transfers WHERE id = $1").bind(id).fetch_one(&mut *tx).await?;
    let (snapshot, deltas) = compare(&mut tx, t.employee_id, &cur, profile).await?;
    store_deltas(&mut tx, id, &deltas).await?;
    sqlx::query(
        "UPDATE transfers SET from_position_id = $2, snapshot = $3, status = 'brouillon', conflict = NULL, version = version + 1, updated_at = now() WHERE id = $1",
    )
    .bind(id)
    .bind(cur.position_id)
    .bind(&snapshot)
    .execute(&mut *tx)
    .await?;
    audit::record(&mut tx, &user, Audit { action: "mutation_recalculee", entity: "transfer", entity_id: Some(id), case_id: Some(t.case_id), ..Default::default() }).await?;
    tx.commit().await?;
    Ok(Json(json!({ "version": t.version + 1, "status": "brouillon" })))
}

// ------------------------------------------------------------------ application à échéance (worker)

/// Applique une seule fois la nouvelle situation des mutations confirmées
/// arrivées à échéance. Si le poste a changé depuis la comparaison, ou si un
/// départ est planifié, l'application est suspendue et le RH prévenu.
pub async fn apply_due(pool: &sqlx::PgPool, today: NaiveDate) -> anyhow::Result<u32> {
    let due: Vec<Uuid> = sqlx::query_scalar("SELECT id FROM transfers WHERE status = 'confirmee' AND effective_date <= $1 ORDER BY effective_date")
        .bind(today)
        .fetch_all(pool)
        .await?;
    let mut applied = 0;
    for id in due {
        let mut tx = pool.begin().await?;
        type Row = (Uuid, Uuid, String, Uuid, NaiveDate, String, Uuid, Option<String>, Option<String>, Option<Uuid>, Option<Uuid>, String, Option<Vec<Uuid>>);
        let row: Option<Row> = sqlx::query_as(
            "SELECT t.case_id, t.employee_id, t.status, t.from_position_id, t.effective_date, t.target_job_title, t.target_site_id,
                    t.target_department, t.target_manager, t.target_manager_employee_id, t.target_profile_id, c.reference, t.target_wilaya_ids
               FROM transfers t JOIN cases c ON c.id = t.case_id WHERE t.id = $1 FOR UPDATE OF t SKIP LOCKED",
        )
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?;
        let Some((case_id, employee_id, status, from_position, effective, job_title, site_id, department, manager, manager_id, profile, reference, wilayas)) = row else {
            continue;
        };
        if status != "confirmee" {
            continue;
        }
        let emp: (String, String) = sqlx::query_as("SELECT status, first_name || ' ' || last_name FROM employees WHERE id = $1 FOR UPDATE")
            .bind(employee_id)
            .fetch_one(&mut *tx)
            .await?;
        let cur: Option<Uuid> = sqlx::query_scalar("SELECT id FROM employee_positions WHERE employee_id = $1 AND valid_to IS NULL")
            .bind(employee_id)
            .fetch_optional(&mut *tx)
            .await?;
        let departure: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM departures WHERE employee_id = $1 AND status = 'planifie')")
            .bind(employee_id)
            .fetch_one(&mut *tx)
            .await?;
        let problem = if emp.0 == "sorti" {
            Some("salarié sorti".to_string())
        } else if departure {
            Some("un départ est planifié".to_string())
        } else if cur != Some(from_position) {
            Some("le poste a changé depuis la comparaison".to_string())
        } else {
            None
        };
        if let Some(p) = problem {
            sqlx::query("UPDATE transfers SET status = 'suspendue', conflict = $2, version = version + 1, updated_at = now() WHERE id = $1")
                .bind(id)
                .bind(json!({ "motif": p, "constate_le": chrono::Utc::now() }))
                .execute(&mut *tx)
                .await?;
            audit::record_system(&mut tx, Audit { action: "mutation_suspendue", entity: "transfer", entity_id: Some(id), case_id: Some(case_id), employee_id: Some(employee_id), data: Some(json!({ "motif": p })) }).await?;
            crate::worker::enqueue_system(
                &mut tx,
                "mutation_suspendue",
                Some(case_id),
                &["RH"],
                &format!("{reference} — Mutation suspendue : {p}"),
                &format!("La mutation {reference} de {} n'a pas été appliquée : {p}.\nOuvrez le dossier pour recalculer ou annuler.", emp.1),
                Some(&format!("/dossiers/{reference}")),
            )
            .await?;
            tx.commit().await?;
            continue;
        }
        sqlx::query("UPDATE employee_positions SET valid_to = $2 WHERE id = $1").bind(from_position).bind(effective).execute(&mut *tx).await?;
        let new_position: Uuid = sqlx::query_scalar(
            "INSERT INTO employee_positions (employee_id, job_title, site_id, department, manager, manager_employee_id, job_profile_id, valid_from)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8) RETURNING id",
        )
        .bind(employee_id)
        .bind(&job_title)
        .bind(site_id)
        .bind(&department)
        .bind(&manager)
        .bind(manager_id)
        .bind(profile)
        .bind(effective)
        .fetch_one(&mut *tx)
        .await?;
        sqlx::query(
            "UPDATE transfers SET status = 'appliquee', applied_at = now(), applied_position_id = $2, version = version + 1, updated_at = now()
              WHERE id = $1 AND status = 'confirmee'",
        )
        .bind(id)
        .bind(new_position)
        .execute(&mut *tx)
        .await?;
        if let Some(ws) = &wilayas {
            sqlx::query("DELETE FROM employee_wilayas WHERE employee_id = $1").bind(employee_id).execute(&mut *tx).await?;
            for w in ws {
                sqlx::query("INSERT INTO employee_wilayas (employee_id, wilaya_id) VALUES ($1, $2) ON CONFLICT DO NOTHING")
                    .bind(employee_id)
                    .bind(w)
                    .execute(&mut *tx)
                    .await?;
            }
        }
        sqlx::query("UPDATE employees SET version = version + 1, updated_at = now() WHERE id = $1").bind(employee_id).execute(&mut *tx).await?;
        audit::record_system(
            &mut tx,
            Audit { action: "mutation_appliquee", entity: "transfer", entity_id: Some(id), case_id: Some(case_id), employee_id: Some(employee_id), data: Some(json!({ "poste": job_title, "effet": effective, "situation": new_position, "wilayas": wilayas })) },
        )
        .await?;
        crate::worker::enqueue_system(
            &mut tx,
            "mutation_appliquee",
            Some(case_id),
            &["RH"],
            &format!("{reference} — Poste appliqué pour {}", emp.1),
            &format!("La nouvelle situation de {} ({job_title}) est appliquée au {}.\nActions restantes : voir le dossier.", emp.1, fr_date(effective)),
            Some(&format!("/dossiers/{reference}")),
        )
        .await?;
        tx.commit().await?;
        applied += 1;
    }
    Ok(applied)
}

// ------------------------------------------------------------------ correction datée

#[derive(Deserialize)]
struct PositionInput {
    version: i32,
    job_title: String,
    site_id: Uuid,
    department: Option<String>,
    manager_employee_id: Option<Uuid>,
    job_profile_id: Option<Uuid>,
    valid_from: NaiveDate,
    reason: String,
}

/// Correction datée de la situation (après une mutation appliquée, ou pour
/// rectifier) : la situation en cours se termine, une nouvelle commence ;
/// le passé n'est pas réécrit.
async fn dated_correction(State(state): State<AppState>, user: CurrentUser, Path(employee_id): Path<Uuid>, Json(i): Json<PositionInput>) -> AppResult<Json<Value>> {
    user.require(domain::roles::HR_MANAGERS)?;
    let reason = domain::text::clean(&i.reason).ok_or_else(|| AppError::invalid("reason", "motif de la correction obligatoire"))?;
    let job_title = domain::text::clean(&i.job_title).ok_or_else(|| AppError::invalid("job_title", "poste obligatoire"))?;
    let mut tx = state.pool.begin().await?;
    let version: Option<(i32, String)> = sqlx::query_as("SELECT version, status FROM employees WHERE id = $1 FOR UPDATE")
        .bind(employee_id)
        .fetch_optional(&mut *tx)
        .await?;
    let (version, status) = version.ok_or_else(|| AppError::not_found("salarié"))?;
    if version != i.version {
        return Err(AppError::stale());
    }
    if status == "sorti" {
        return Err(AppError::conflict("salarié sorti"));
    }
    let cur = current_position(&mut tx, employee_id).await?.ok_or_else(|| AppError::conflict("aucun poste en cours"))?;
    check_effective_date(ymd(cur.valid_from), ymd(i.valid_from))?;
    let manager: Option<String> = match i.manager_employee_id {
        Some(m) => sqlx::query_scalar("SELECT first_name || ' ' || last_name FROM employees WHERE id = $1").bind(m).fetch_optional(&mut *tx).await?,
        None => None,
    };
    sqlx::query("UPDATE employee_positions SET valid_to = $2 WHERE id = $1").bind(cur.position_id).bind(i.valid_from).execute(&mut *tx).await?;
    let pos: Uuid = sqlx::query_scalar(
        "INSERT INTO employee_positions (employee_id, job_title, site_id, department, manager, manager_employee_id, job_profile_id, valid_from)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8) RETURNING id",
    )
    .bind(employee_id)
    .bind(&job_title)
    .bind(i.site_id)
    .bind(clean(&i.department))
    .bind(&manager)
    .bind(i.manager_employee_id)
    .bind(i.job_profile_id)
    .bind(i.valid_from)
    .fetch_one(&mut *tx)
    .await?;
    sqlx::query("UPDATE employees SET version = version + 1, updated_at = now() WHERE id = $1").bind(employee_id).execute(&mut *tx).await?;
    audit::record(
        &mut tx,
        &user,
        Audit { action: "correction_poste_datee", entity: "employee", entity_id: Some(employee_id), employee_id: Some(employee_id), data: Some(json!({ "avant": cur.job_title, "apres": job_title, "au": i.valid_from, "motif": reason, "situation": pos })), ..Default::default() },
    )
    .await?;
    tx.commit().await?;
    Ok(Json(json!({ "position_id": pos, "version": version + 1 })))
}

/// Données de la mutation pour la page du dossier (RH, ADMIN ; les autres
/// équipes ne voient que leurs tâches).
pub async fn case_json(conn: &mut PgConnection, case_id: Uuid) -> AppResult<Value> {
    let v: Option<Value> = sqlx::query_scalar(
        "SELECT jsonb_build_object('id', t.id, 'status', t.status, 'version', t.version, 'effective_date', t.effective_date,
                'target', jsonb_build_object('job_title', t.target_job_title, 'site_id', t.target_site_id, 'site', s.name,
                          'department', t.target_department, 'manager', t.target_manager, 'manager_employee_id', t.target_manager_employee_id,
                          'profile_id', t.target_profile_id, 'profile', jp.name,
                          'wilayas', CASE WHEN t.target_wilaya_ids IS NULL THEN NULL ELSE
                              (SELECT coalesce(jsonb_agg(w.code::text || ' ' || w.name ORDER BY w.code), '[]') FROM wilayas w WHERE w.id = ANY(t.target_wilaya_ids)) END),
                'snapshot', t.snapshot, 'conflict', t.conflict, 'confirmed_at', t.confirmed_at, 'applied_at', t.applied_at,
                'cancelled_at', t.cancelled_at, 'cancel_reason', t.cancel_reason,
                'position_changed', t.from_position_id IS DISTINCT FROM (SELECT p.id FROM employee_positions p WHERE p.employee_id = t.employee_id AND p.valid_to IS NULL)
                                    AND t.status NOT IN ('appliquee', 'annulee'),
                'deltas', (SELECT coalesce(jsonb_agg(jsonb_build_object('id', d.id, 'object_kind', d.object_kind, 'label', d.label,
                                  'current', d.current, 'target', d.target, 'proposal', d.proposal, 'decision', d.decision, 'note', d.note,
                                  'task_status', (SELECT k.status FROM tasks k WHERE k.transfer_delta_id = d.id ORDER BY k.created_at DESC LIMIT 1))
                                  ORDER BY array_position(ARRAY['compte','licence','bien','vehicule','besoin','formation'], d.object_kind), d.label), '[]')
                             FROM transfer_deltas d WHERE d.transfer_id = t.id))
           FROM transfers t JOIN sites s ON s.id = t.target_site_id LEFT JOIN job_profiles jp ON jp.id = t.target_profile_id
          WHERE t.case_id = $1",
    )
    .bind(case_id)
    .fetch_optional(conn)
    .await?;
    Ok(v.unwrap_or(Value::Null))
}
