use axum::extract::{Path, Query, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::{DateTime, NaiveDate, Utc};
use domain::Role;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::{FromRow, PgConnection};
use uuid::Uuid;

use crate::audit::{self, Audit};
use crate::auth::CurrentUser;
use crate::error::{AppError, AppResult};
use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/employees", get(list).post(create))
        .route("/employees/similar", get(similar))
        .route("/employees/arrivals-to-check", get(arrivals_to_check))
        .route("/employees/{id}/arrival", post(confirm_arrival))
        .route("/employees/{id}", get(detail).patch(update))
        .route("/employees/{id}/accounts", get(list_accounts).post(add_account))
        .route("/employees/{id}/accounts/{account_id}", axum::routing::patch(update_account))
        .route("/employees/{id}/m365", post(set_m365))
}

/// Peut voir la fiche ? RH/Admin/IT : oui. GRAPHISTE/FORMATION : seulement
/// les salariés pour lesquels leur équipe a du travail.
pub async fn can_view_employee(conn: &mut PgConnection, user: &CurrentUser, employee_id: Uuid) -> AppResult<bool> {
    if user.has_any(&[Role::Rh, Role::Admin, Role::It]) {
        return Ok(true);
    }
    let roles: Vec<&str> = user.roles.iter().map(|r| r.as_str()).collect();
    let ok: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM tasks t JOIN cases c ON c.id = t.case_id
                         WHERE c.employee_id = $1 AND (t.role = ANY($2) OR t.owner_id = $3))
             OR EXISTS (SELECT 1 FROM cases c WHERE c.employee_id = $1 AND c.kind = 'ONB' AND c.status = 'ouvert'
                         AND 'FORMATION' = ANY($2))",
    )
    .bind(employee_id)
    .bind(&roles)
    .bind(user.id)
    .fetch_one(conn)
    .await?;
    Ok(ok)
}

#[derive(Deserialize)]
struct ListQuery {
    q: Option<String>,
    status: Option<String>,
    site_id: Option<Uuid>,
    page: Option<i64>,
}

#[derive(Serialize, FromRow)]
struct EmployeeListRow {
    id: Uuid,
    matricule: Option<String>,
    first_name: String,
    last_name: String,
    status: String,
    planned_arrival: Option<NaiveDate>,
    job_title: Option<String>,
    site_name: Option<String>,
    open_cases: i64,
}

const PAGE_SIZE: i64 = 50;

async fn list(State(state): State<AppState>, user: CurrentUser, Query(q): Query<ListQuery>) -> AppResult<Json<Value>> {
    user.require(&[Role::Rh, Role::Admin, Role::It])?;
    let page = q.page.unwrap_or(1).max(1);
    let search = q.q.as_deref().and_then(domain::text::clean).map(|s| format!("%{}%", s.to_lowercase()));
    let rows = sqlx::query_as::<_, EmployeeListRow>(
        "SELECT e.id, e.matricule, e.first_name, e.last_name, e.status, e.planned_arrival,
                p.job_title, s.name AS site_name,
                (SELECT count(*) FROM cases c WHERE c.employee_id = e.id AND c.status = 'ouvert') AS open_cases
           FROM employees e
           LEFT JOIN employee_positions p ON p.employee_id = e.id AND p.valid_to IS NULL
           LEFT JOIN sites s ON s.id = p.site_id
          WHERE ($1::text IS NULL OR lower(e.first_name || ' ' || e.last_name) LIKE $1
                 OR lower(e.last_name || ' ' || e.first_name) LIKE $1 OR lower(coalesce(e.matricule, '')) LIKE $1)
            AND ($2::text IS NULL OR e.status = $2)
            AND ($3::uuid IS NULL OR p.site_id = $3)
          ORDER BY e.last_name, e.first_name
          LIMIT $4 OFFSET $5",
    )
    .bind(&search)
    .bind(&q.status)
    .bind(q.site_id)
    .bind(PAGE_SIZE)
    .bind((page - 1) * PAGE_SIZE)
    .fetch_all(&state.pool)
    .await?;
    let total: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM employees e
           LEFT JOIN employee_positions p ON p.employee_id = e.id AND p.valid_to IS NULL
          WHERE ($1::text IS NULL OR lower(e.first_name || ' ' || e.last_name) LIKE $1
                 OR lower(e.last_name || ' ' || e.first_name) LIKE $1 OR lower(coalesce(e.matricule, '')) LIKE $1)
            AND ($2::text IS NULL OR e.status = $2)
            AND ($3::uuid IS NULL OR p.site_id = $3)",
    )
    .bind(&search)
    .bind(&q.status)
    .bind(q.site_id)
    .fetch_one(&state.pool)
    .await?;
    Ok(Json(json!({ "items": rows, "total": total, "page": page, "page_size": PAGE_SIZE })))
}

#[derive(Deserialize)]
struct SimilarQuery {
    first_name: String,
    last_name: String,
    matricule: Option<String>,
}

#[derive(Serialize, FromRow)]
pub struct Candidate {
    id: Uuid,
    matricule: Option<String>,
    first_name: String,
    last_name: String,
    status: String,
    job_title: Option<String>,
    site_name: Option<String>,
    same_matricule: bool,
}

/// Candidats ressemblants (homonymes) et matricule déjà attribué : la
/// décision revient à l'utilisateur, jamais de fusion automatique.
async fn similar(State(state): State<AppState>, user: CurrentUser, Query(q): Query<SimilarQuery>) -> AppResult<Json<Vec<Candidate>>> {
    user.require(domain::roles::HR_MANAGERS)?;
    let key = domain::text::name_key(&q.first_name, &q.last_name);
    let matricule = q.matricule.as_deref().and_then(domain::text::normalize_matricule);
    let rows = sqlx::query_as::<_, Candidate>(
        "SELECT e.id, e.matricule, e.first_name, e.last_name, e.status, p.job_title, s.name AS site_name,
                ($2::text IS NOT NULL AND e.matricule_norm = $2) AS same_matricule
           FROM employees e
           LEFT JOIN employee_positions p ON p.employee_id = e.id AND p.valid_to IS NULL
           LEFT JOIN sites s ON s.id = p.site_id
          WHERE e.name_key = $1 OR ($2::text IS NOT NULL AND e.matricule_norm = $2)
          ORDER BY same_matricule DESC, e.last_name
          LIMIT 20",
    )
    .bind(&key)
    .bind(&matricule)
    .fetch_all(&state.pool)
    .await?;
    Ok(Json(rows))
}

/// Données d'identité communes à la création par annonce ou par saisie.
pub struct NewEmployee {
    pub first_name: String,
    pub last_name: String,
    pub matricule: Option<String>,
    pub job_title: String,
    pub site_id: Uuid,
    pub department: Option<String>,
    pub manager: Option<String>,
    pub status: &'static str,
    pub planned_arrival: Option<NaiveDate>,
    pub actual_arrival: Option<NaiveDate>,
}

pub fn required(field: &'static str, value: &str, label: &str) -> AppResult<String> {
    domain::text::clean(value).ok_or_else(|| AppError::invalid(field, format!("{label} obligatoire")))
}

pub async fn insert_employee(conn: &mut PgConnection, e: &NewEmployee) -> AppResult<Uuid> {
    let site_ok: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM sites WHERE id = $1 AND active)")
        .bind(e.site_id)
        .fetch_one(&mut *conn)
        .await?;
    if !site_ok {
        return Err(AppError::invalid("site_id", "site inconnu ou inactif"));
    }
    let matricule = e.matricule.as_deref().and_then(domain::text::clean);
    let matricule_norm = matricule.as_deref().and_then(domain::text::normalize_matricule);
    if let Some(m) = &matricule_norm {
        let taken: Option<String> = sqlx::query_scalar(
            "SELECT first_name || ' ' || last_name FROM employees WHERE matricule_norm = $1",
        )
        .bind(m)
        .fetch_optional(&mut *conn)
        .await?;
        if let Some(who) = taken {
            return Err(AppError::Conflict {
                message: format!("le matricule {m} est déjà attribué à {who}"),
                detail: Some(json!({ "field": "matricule" })),
            });
        }
    }
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO employees (matricule, matricule_norm, first_name, last_name, name_key, status, planned_arrival, actual_arrival)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8) RETURNING id",
    )
    .bind(&matricule)
    .bind(&matricule_norm)
    .bind(&e.first_name)
    .bind(&e.last_name)
    .bind(domain::text::name_key(&e.first_name, &e.last_name))
    .bind(e.status)
    .bind(e.planned_arrival)
    .bind(e.actual_arrival)
    .fetch_one(&mut *conn)
    .await?;
    let from = e.actual_arrival.or(e.planned_arrival).unwrap_or_else(|| Utc::now().date_naive());
    sqlx::query(
        "INSERT INTO employee_positions (employee_id, job_title, site_id, department, manager, valid_from)
         VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(id)
    .bind(&e.job_title)
    .bind(e.site_id)
    .bind(e.department.as_deref().and_then(domain::text::clean))
    .bind(e.manager.as_deref().and_then(domain::text::clean))
    .bind(from)
    .execute(&mut *conn)
    .await?;
    Ok(id)
}

#[derive(Deserialize)]
struct CreateInput {
    first_name: String,
    last_name: String,
    matricule: Option<String>,
    job_title: String,
    site_id: Uuid,
    department: Option<String>,
    arrival_date: NaiveDate,
    already_present: bool,
}

/// Fiche saisie hors parcours (personnel déjà présent ou à venir) :
/// n'annonce rien, ne crée ni tâche ni email.
async fn create(State(state): State<AppState>, user: CurrentUser, Json(input): Json<CreateInput>) -> AppResult<Json<Value>> {
    user.require(domain::roles::HR_MANAGERS)?;
    let e = NewEmployee {
        first_name: required("first_name", &input.first_name, "prénom")?,
        last_name: required("last_name", &input.last_name, "nom")?,
        matricule: input.matricule,
        job_title: required("job_title", &input.job_title, "poste")?,
        site_id: input.site_id,
        department: input.department,
        manager: None,
        status: if input.already_present { "actif" } else { "a_venir" },
        planned_arrival: (!input.already_present).then_some(input.arrival_date),
        actual_arrival: input.already_present.then_some(input.arrival_date),
    };
    let mut tx = state.pool.begin().await?;
    let id = insert_employee(&mut tx, &e).await?;
    audit::record(&mut tx, &user, Audit { action: "creation_fiche", entity: "employee", entity_id: Some(id), employee_id: Some(id), ..Default::default() }).await?;
    tx.commit().await?;
    Ok(Json(json!({ "id": id })))
}

#[derive(Serialize, FromRow)]
struct EmployeeDetail {
    id: Uuid,
    matricule: Option<String>,
    first_name: String,
    last_name: String,
    status: String,
    planned_arrival: Option<NaiveDate>,
    actual_arrival: Option<NaiveDate>,
    exit_date: Option<NaiveDate>,
    m365_verified_at: Option<DateTime<Utc>>,
    arrival_confirmed_at: Option<DateTime<Utc>>,
    arrival_confirmed_by_name: Option<String>,
    version: i32,
    job_title: Option<String>,
    department: Option<String>,
    manager: Option<String>,
    site_id: Option<Uuid>,
    site_name: Option<String>,
}

async fn detail(State(state): State<AppState>, user: CurrentUser, Path(id): Path<Uuid>) -> AppResult<Json<Value>> {
    let mut conn = state.pool.acquire().await?;
    if !can_view_employee(&mut conn, &user, id).await? {
        return Err(AppError::forbidden());
    }
    let e = sqlx::query_as::<_, EmployeeDetail>(
        "SELECT e.id, e.matricule, e.first_name, e.last_name, e.status, e.planned_arrival, e.actual_arrival,
                e.exit_date, e.m365_verified_at, e.arrival_confirmed_at, cu.display_name AS arrival_confirmed_by_name,
                e.version, p.job_title, p.department, p.manager, p.site_id, s.name AS site_name
           FROM employees e
           LEFT JOIN employee_positions p ON p.employee_id = e.id AND p.valid_to IS NULL
           LEFT JOIN sites s ON s.id = p.site_id
           LEFT JOIN users cu ON cu.id = e.arrival_confirmed_by
          WHERE e.id = $1",
    )
    .bind(id)
    .fetch_optional(&mut *conn)
    .await?
    .ok_or_else(|| AppError::not_found("salarié"))?;

    let hr = user.is_hr();
    // RH, ADMIN et IT : fiche de travail complète. GRAPHISTE et FORMATION :
    // identité utile à leur mission, leurs seuls dossiers, rien d'autre.
    let full = user.has_any(&[Role::Rh, Role::Admin, Role::It]);
    let roles: Vec<&str> = user.roles.iter().map(|r| r.as_str()).collect();

    let positions: Value = if full {
        sqlx::query_scalar(
            "SELECT coalesce(jsonb_agg(jsonb_build_object('job_title', p.job_title, 'site', s.name, 'department', p.department,
                                       'valid_from', p.valid_from, 'valid_to', p.valid_to) ORDER BY p.valid_from DESC), '[]')
               FROM employee_positions p JOIN sites s ON s.id = p.site_id WHERE p.employee_id = $1",
        )
        .bind(id)
        .fetch_one(&mut *conn)
        .await?
    } else {
        json!([])
    };

    let cases: Value = sqlx::query_scalar(
        "SELECT coalesce(jsonb_agg(jsonb_build_object('id', c.id, 'reference', c.reference, 'kind', c.kind,
                                   'status', c.status, 'created_at', c.created_at) ORDER BY c.created_at DESC), '[]')
           FROM cases c
          WHERE c.employee_id = $1
            AND ($2 OR EXISTS (SELECT 1 FROM tasks t WHERE t.case_id = c.id AND (t.role = ANY($3) OR t.owner_id = $4)))",
    )
    .bind(id)
    .bind(full)
    .bind(&roles)
    .bind(user.id)
    .fetch_one(&mut *conn)
    .await?;

    let holdings: Value = if full {
        crate::routes::movements::holdings_json(&mut conn, id).await?
    } else {
        json!([])
    };
    let trainings: Value = if hr || user.has(Role::Formation) {
        sqlx::query_scalar(
            "SELECT coalesce(jsonb_agg(jsonb_build_object('id', t.id, 'name', tc.name, 'status', t.status,
                                       'planned_date', t.planned_date, 'done_date', t.done_date) ORDER BY tc.name), '[]')
               FROM training_sessions t JOIN training_catalog tc ON tc.id = t.catalog_id WHERE t.employee_id = $1",
        )
        .bind(id)
        .fetch_one(&mut *conn)
        .await?
    } else {
        json!([])
    };
    // Coordonnées professionnelles utiles à une carte de visite.
    let contact: Value = if hr || user.has(Role::Graphiste) {
        sqlx::query_scalar(
            "SELECT jsonb_build_object(
                    'email_pro', (SELECT identifier FROM employee_accounts
                                   WHERE employee_id = $1 AND system = 'email' AND status <> 'coupe'
                                   ORDER BY created_at DESC LIMIT 1),
                    'mobile', (SELECT l.number FROM assignments a JOIN sim_cards s ON s.id = a.sim_id
                                 JOIN mobile_lines l ON l.id = s.line_id
                                WHERE a.employee_id = $1 AND a.ended_at IS NULL LIMIT 1))",
        )
        .bind(id)
        .fetch_one(&mut *conn)
        .await?
    } else {
        Value::Null
    };

    let employee = if full {
        serde_json::to_value(&e).map_err(anyhow::Error::from)?
    } else {
        json!({
            "id": e.id, "first_name": e.first_name, "last_name": e.last_name, "status": e.status,
            "job_title": e.job_title, "department": e.department, "site_id": e.site_id, "site_name": e.site_name,
        })
    };

    Ok(Json(json!({
        "scope": if full { "complet" } else { "equipe" },
        "employee": employee,
        "contact": contact,
        "positions": positions,
        "cases": cases,
        "holdings": holdings,
        "trainings": trainings,
    })))
}

#[derive(Deserialize)]
struct ArrivalInput {
    actual_arrival: NaiveDate,
    /// Obligatoire pour corriger une date déjà confirmée.
    correction_reason: Option<String>,
}

/// « Confirmer l'arrivée » : présence réelle constatée par le RH. Distincte
/// de la clôture du dossier (le PC peut arriver plus tard) ; rejouer la
/// même confirmation ne crée pas de second événement ; une correction de
/// date est explicite, motivée et tracée.
async fn confirm_arrival(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
    Json(input): Json<ArrivalInput>,
) -> AppResult<Json<Value>> {
    user.require(domain::roles::HR_MANAGERS)?;
    let today = crate::documents::algiers_today();
    if input.actual_arrival > today {
        return Err(AppError::invalid("actual_arrival", "la présence ne peut pas être confirmée pour une date future"));
    }
    let reason = input.correction_reason.as_deref().and_then(domain::text::clean);
    let mut tx = state.pool.begin().await?;
    let row: Option<(String, Option<NaiveDate>, Option<NaiveDate>)> = sqlx::query_as(
        "SELECT status, actual_arrival, planned_arrival FROM employees WHERE id = $1 FOR UPDATE",
    )
    .bind(id)
    .fetch_optional(&mut *tx)
    .await?;
    let (status, actual, planned) = row.ok_or_else(|| AppError::not_found("salarié"))?;
    let action = match (status.as_str(), actual) {
        ("sorti", _) => return Err(AppError::conflict("salarié sorti : la date d'arrivée n'est plus modifiable ici")),
        ("actif", Some(d)) if d == input.actual_arrival => {
            // Double clic ou rejeu : rien à faire, aucun nouvel événement.
            tx.commit().await?;
            return Ok(Json(json!({ "status": "actif", "actual_arrival": d, "changed": false })));
        }
        ("actif", Some(_)) => {
            if reason.is_none() {
                return Err(AppError::invalid(
                    "correction_reason",
                    "arrivée déjà confirmée à une autre date : indiquez le motif de la correction",
                ));
            }
            "date_arrivee_corrigee"
        }
        _ => "arrivee_confirmee",
    };
    sqlx::query(
        "UPDATE employees SET status = 'actif', actual_arrival = $2,
                arrival_confirmed_at = now(), arrival_confirmed_by = $3,
                version = version + 1, updated_at = now()
          WHERE id = $1",
    )
    .bind(id)
    .bind(input.actual_arrival)
    .bind(user.id)
    .execute(&mut *tx)
    .await?;
    // Situation unique créée à l'annonce : elle prend effet à la date réelle.
    sqlx::query(
        "UPDATE employee_positions SET valid_from = $2
          WHERE employee_id = $1 AND valid_to IS NULL
            AND (SELECT count(*) FROM employee_positions WHERE employee_id = $1) = 1",
    )
    .bind(id)
    .bind(input.actual_arrival)
    .execute(&mut *tx)
    .await?;
    audit::record(
        &mut tx,
        &user,
        Audit {
            action,
            entity: "employee",
            entity_id: Some(id),
            employee_id: Some(id),
            data: Some(json!({
                "date_prevue": planned, "date_avant": actual, "date_reelle": input.actual_arrival, "motif": reason,
            })),
            ..Default::default()
        },
    )
    .await?;
    tx.commit().await?;
    Ok(Json(json!({ "status": "actif", "actual_arrival": input.actual_arrival, "changed": true })))
}

#[derive(Serialize, FromRow)]
struct ArrivalToCheck {
    id: Uuid,
    first_name: String,
    last_name: String,
    planned_arrival: Option<NaiveDate>,
    site_name: Option<String>,
    onb_reference: Option<String>,
    onb_closed: bool,
}

/// Fiches « à venir » dont la date prévue est passée ou dont l'arrivée est
/// close : la présence doit être vérifiée par le RH (jamais déduite).
async fn arrivals_to_check(State(state): State<AppState>, user: CurrentUser) -> AppResult<Json<Vec<ArrivalToCheck>>> {
    user.require(domain::roles::HR_MANAGERS)?;
    let rows = sqlx::query_as::<_, ArrivalToCheck>(
        "SELECT e.id, e.first_name, e.last_name, e.planned_arrival, s.name AS site_name,
                c.reference AS onb_reference, coalesce(c.status = 'clos', false) AS onb_closed
           FROM employees e
           LEFT JOIN employee_positions p ON p.employee_id = e.id AND p.valid_to IS NULL
           LEFT JOIN sites s ON s.id = p.site_id
           LEFT JOIN LATERAL (SELECT reference, status FROM cases
                               WHERE employee_id = e.id AND kind = 'ONB' ORDER BY created_at DESC LIMIT 1) c ON true
          WHERE e.status = 'a_venir'
            AND (e.planned_arrival <= $1 OR c.status = 'clos')
          ORDER BY e.planned_arrival NULLS LAST, e.last_name",
    )
    .bind(crate::documents::algiers_today())
    .fetch_all(&state.pool)
    .await?;
    Ok(Json(rows))
}

#[derive(Deserialize)]
struct UpdateInput {
    version: i32,
    first_name: Option<String>,
    last_name: Option<String>,
    /// `Some("")` efface volontairement le matricule (tracé).
    matricule: Option<String>,
}

async fn update(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
    Json(input): Json<UpdateInput>,
) -> AppResult<Json<Value>> {
    user.require(domain::roles::HR_MANAGERS)?;
    let mut tx = state.pool.begin().await?;
    let current: Option<(i32, String, String, Option<String>)> = sqlx::query_as(
        "SELECT version, first_name, last_name, matricule FROM employees WHERE id = $1 FOR UPDATE",
    )
    .bind(id)
    .fetch_optional(&mut *tx)
    .await?;
    let (version, first, last, matricule) = current.ok_or_else(|| AppError::not_found("salarié"))?;
    if version != input.version {
        return Err(AppError::stale());
    }
    let first = match &input.first_name { Some(v) => required("first_name", v, "prénom")?, None => first };
    let last = match &input.last_name { Some(v) => required("last_name", v, "nom")?, None => last };
    let new_matricule = match &input.matricule { Some(v) => domain::text::clean(v), None => matricule.clone() };
    let norm = new_matricule.as_deref().and_then(domain::text::normalize_matricule);
    sqlx::query(
        "UPDATE employees SET first_name = $2, last_name = $3, name_key = $4, matricule = $5, matricule_norm = $6,
                version = version + 1, updated_at = now() WHERE id = $1",
    )
    .bind(id)
    .bind(&first)
    .bind(&last)
    .bind(domain::text::name_key(&first, &last))
    .bind(&new_matricule)
    .bind(&norm)
    .execute(&mut *tx)
    .await?;
    audit::record(
        &mut tx,
        &user,
        Audit {
            action: "modification_fiche",
            entity: "employee",
            entity_id: Some(id),
            employee_id: Some(id),
            data: Some(json!({ "matricule_avant": matricule, "matricule_apres": new_matricule })),
            ..Default::default()
        },
    )
    .await?;
    tx.commit().await?;
    Ok(Json(json!({ "version": version + 1 })))
}

#[derive(Serialize, FromRow)]
struct AccountRow {
    id: Uuid,
    system: String,
    platform: Option<String>,
    identifier: String,
    status: String,
    verified_at: Option<DateTime<Utc>>,
    ended_at: Option<DateTime<Utc>>,
    created_at: DateTime<Utc>,
}

async fn list_accounts(State(state): State<AppState>, user: CurrentUser, Path(id): Path<Uuid>) -> AppResult<Json<Vec<AccountRow>>> {
    user.require(&[Role::It, Role::Rh, Role::Admin])?;
    let rows = sqlx::query_as::<_, AccountRow>(
        "SELECT id, system, platform, identifier, status, verified_at, ended_at, created_at
           FROM employee_accounts WHERE employee_id = $1 ORDER BY created_at",
    )
    .bind(id)
    .fetch_all(&state.pool)
    .await?;
    Ok(Json(rows))
}

#[derive(Deserialize)]
struct AccountInput {
    system: String,
    platform: Option<String>,
    identifier: String,
}

/// L'IT consigne un compte réellement créé dans le système externe.
/// Aucun mot de passe n'est saisi ni stocké.
async fn add_account(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
    Json(input): Json<AccountInput>,
) -> AppResult<Json<Value>> {
    user.require(&[Role::It, Role::Admin])?;
    if !["email", "xena", "intelix", "autre"].contains(&input.system.as_str()) {
        return Err(AppError::invalid("system", "système inconnu"));
    }
    let platform = input.platform.filter(|p| !p.is_empty());
    if input.system == "email" && !matches!(platform.as_deref(), Some("cpanel") | Some("m365")) {
        return Err(AppError::invalid("platform", "plateforme cPanel ou Microsoft 365 obligatoire pour une adresse"));
    }
    let identifier = required("identifier", &input.identifier, "identifiant")?;
    let mut tx = state.pool.begin().await?;
    let account_id: Uuid = sqlx::query_scalar(
        "INSERT INTO employee_accounts (employee_id, system, platform, identifier, created_by, verified_at)
         VALUES ($1, $2, $3, $4, $5, now()) RETURNING id",
    )
    .bind(id)
    .bind(&input.system)
    .bind(&platform)
    .bind(&identifier)
    .bind(user.id)
    .fetch_one(&mut *tx)
    .await?;
    audit::record(
        &mut tx,
        &user,
        Audit {
            action: "compte_consigne",
            entity: "employee_account",
            entity_id: Some(account_id),
            employee_id: Some(id),
            data: Some(json!({ "system": input.system, "platform": platform, "identifier": identifier })),
            ..Default::default()
        },
    )
    .await?;
    tx.commit().await?;
    Ok(Json(json!({ "id": account_id })))
}

#[derive(Deserialize)]
struct AccountUpdate {
    status: String,
}

/// Coupure ou mise à vérifier : l'IT confirme ce qui est réellement fait.
async fn update_account(
    State(state): State<AppState>,
    user: CurrentUser,
    Path((id, account_id)): Path<(Uuid, Uuid)>,
    Json(input): Json<AccountUpdate>,
) -> AppResult<Json<Value>> {
    user.require(&[Role::It, Role::Admin])?;
    if !["actif", "a_verifier", "coupe"].contains(&input.status.as_str()) {
        return Err(AppError::invalid("status", "statut inconnu"));
    }
    let mut tx = state.pool.begin().await?;
    let n = sqlx::query(
        "UPDATE employee_accounts SET status = $3,
                ended_at = CASE WHEN $3 = 'coupe' THEN now() ELSE NULL END,
                verified_at = now()
          WHERE id = $2 AND employee_id = $1",
    )
    .bind(id)
    .bind(account_id)
    .bind(&input.status)
    .execute(&mut *tx)
    .await?
    .rows_affected();
    if n == 0 {
        return Err(AppError::not_found("compte"));
    }
    audit::record(&mut tx, &user, Audit { action: "compte_statut", entity: "employee_account", entity_id: Some(account_id), employee_id: Some(id), data: Some(json!({ "status": input.status })), ..Default::default() }).await?;
    tx.commit().await?;
    Ok(Json(json!({ "ok": true })))
}

#[derive(Deserialize)]
struct M365Input {
    verified: bool,
}

/// « Compte Microsoft 365 opérationnel — vérifié », indépendant de
/// l'hébergement de l'adresse.
async fn set_m365(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
    Json(input): Json<M365Input>,
) -> AppResult<Json<Value>> {
    user.require(&[Role::It, Role::Admin])?;
    let mut tx = state.pool.begin().await?;
    let n = sqlx::query(
        "UPDATE employees SET m365_verified_at = CASE WHEN $2 THEN now() ELSE NULL END,
                m365_verified_by = CASE WHEN $2 THEN $3 ELSE NULL END, updated_at = now()
          WHERE id = $1",
    )
    .bind(id)
    .bind(input.verified)
    .bind(user.id)
    .execute(&mut *tx)
    .await?
    .rows_affected();
    if n == 0 {
        return Err(AppError::not_found("salarié"));
    }
    audit::record(&mut tx, &user, Audit { action: "m365_verification", entity: "employee", entity_id: Some(id), employee_id: Some(id), data: Some(json!({ "verified": input.verified })), ..Default::default() }).await?;
    tx.commit().await?;
    Ok(Json(json!({ "ok": true })))
}
