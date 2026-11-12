use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::{DateTime, NaiveDate, Utc};
use domain::cases::format_reference;
use domain::needs::{task_for_need, task_for_training, NeedInput};
use domain::{CaseKind, NeedAnswer, NeedCategory, PcPreference, Role};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::{FromRow, PgConnection};
use uuid::Uuid;

use crate::audit::{self, Audit, Notice};
use crate::auth::CurrentUser;
use crate::error::{AppError, AppResult};
use crate::idempotency::{self, Begin};
use crate::routes::employees::{insert_employee, required, NewEmployee};
use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/arrivals", post(create_arrival))
        .route("/cases", get(list_cases))
        .route("/cases/{reference}", get(case_detail))
        .route("/cases/{id}/needs", post(update_needs))
        .route("/cases/{id}/close", post(close_case))
        .route("/employees/{id}/requests", post(create_request))
}

// ------------------------------------------------------------------ entrées

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct NeedDto {
    pub category: NeedCategory,
    pub answer: NeedAnswer,
    pub quantity: Option<i32>,
    pub preference: Option<PcPreference>,
    pub detail: Option<String>,
}

impl NeedDto {
    fn to_input(&self) -> NeedInput {
        NeedInput {
            category: self.category,
            answer: self.answer,
            quantity: self.quantity,
            preference: self.preference,
            detail: self.detail.as_deref().and_then(domain::text::clean),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TrainingDto {
    pub catalog_id: Uuid,
    pub trainer_id: Option<Uuid>,
}

#[derive(Debug, Deserialize, Serialize)]
struct ArrivalInput {
    /// Fiche existante explicitement choisie par l'utilisateur (homonyme
    /// écarté ou personne déjà connue). Sinon une fiche est créée.
    employee_id: Option<Uuid>,
    first_name: String,
    last_name: String,
    job_title: String,
    site_id: Uuid,
    planned_arrival: NaiveDate,
    matricule: Option<String>,
    department: Option<String>,
    manager: Option<String>,
    #[serde(default)]
    needs: Vec<NeedDto>,
    #[serde(default)]
    trainings: Vec<TrainingDto>,
    note: Option<String>,
}

// ------------------------------------------------------------------ outils communs

async fn next_reference(conn: &mut PgConnection, kind: CaseKind) -> AppResult<String> {
    let n: i64 = sqlx::query_scalar(&format!("SELECT nextval('{}')", kind.sequence_name()))
        .fetch_one(conn)
        .await?;
    Ok(format_reference(kind, n))
}

pub fn fr_date(d: NaiveDate) -> String {
    d.format("%d/%m/%Y").to_string()
}

fn describe_need(n: &NeedInput) -> String {
    let mut s = format!("{} : {}", n.category.label(), match n.answer {
        NeedAnswer::Oui => "oui",
        NeedAnswer::Non => "non",
        NeedAnswer::APreciser => "à préciser",
    });
    if let Some(q) = n.quantity {
        s.push_str(&format!(" (quantité {q})"));
    }
    if let Some(p) = n.preference {
        s.push_str(match p {
            PcPreference::Fixe => " — préférence : fixe",
            PcPreference::Portable => " — préférence : portable",
            PcPreference::SansPreference => " — sans préférence",
        });
    }
    if let Some(d) = &n.detail {
        s.push_str(&format!(" — {d}"));
    }
    s
}

/// Effet d'une saisie de besoin.
pub struct NeedEffect {
    pub impacted_role: Option<Role>,
    pub description: Option<String>,
    pub task_created: bool,
}

/// Enregistre (ou révise) un besoin et ajuste la tâche correspondante :
/// « oui » crée ou réactive la tâche ; quitter « oui » annule seulement le
/// travail encore à faire. Les révisions sont conservées.
pub async fn apply_need(
    conn: &mut PgConnection,
    user: &CurrentUser,
    case_id: Uuid,
    need: &NeedInput,
    due: Option<NaiveDate>,
) -> AppResult<NeedEffect> {
    need.validate()?;
    type Existing = (Uuid, String, Option<i32>, Option<String>, Option<String>, i32);
    let existing: Option<Existing> = sqlx::query_as(
        "SELECT id, answer, quantity, preference, detail, revision FROM case_needs
          WHERE case_id = $1 AND category = $2 FOR UPDATE",
    )
    .bind(case_id)
    .bind(need.category.as_str())
    .fetch_optional(&mut *conn)
    .await?;
    let pref = need.preference.map(|p| p.as_str().to_string());

    let (need_id, old_answer, revision) = match existing {
        Some((id, answer, q, p, d, rev)) => {
            if answer == need.answer.as_str() && q == need.quantity && p == pref && d == need.detail {
                return Ok(NeedEffect { impacted_role: None, description: None, task_created: false });
            }
            sqlx::query(
                "UPDATE case_needs SET answer = $2, quantity = $3, preference = $4, detail = $5,
                        revision = revision + 1, updated_by = $6, updated_at = now() WHERE id = $1",
            )
            .bind(id)
            .bind(need.answer.as_str())
            .bind(need.quantity)
            .bind(&pref)
            .bind(&need.detail)
            .bind(user.id)
            .execute(&mut *conn)
            .await?;
            (id, NeedAnswer::parse(&answer), rev + 1)
        }
        None => {
            let id: Uuid = sqlx::query_scalar(
                "INSERT INTO case_needs (case_id, category, answer, quantity, preference, detail, updated_by)
                 VALUES ($1, $2, $3, $4, $5, $6, $7) RETURNING id",
            )
            .bind(case_id)
            .bind(need.category.as_str())
            .bind(need.answer.as_str())
            .bind(need.quantity)
            .bind(&pref)
            .bind(&need.detail)
            .bind(user.id)
            .fetch_one(&mut *conn)
            .await?;
            (id, None, 1)
        }
    };
    sqlx::query(
        "INSERT INTO need_revisions (need_id, revision, answer, quantity, preference, detail, changed_by)
         VALUES ($1, $2, $3, $4, $5, $6, $7)",
    )
    .bind(need_id)
    .bind(revision)
    .bind(need.answer.as_str())
    .bind(need.quantity)
    .bind(&pref)
    .bind(&need.detail)
    .bind(user.id)
    .execute(&mut *conn)
    .await?;

    let role = need.category.responsible_role();
    let mut impacted = false;
    let mut task_created = false;
    if let Some(spec) = task_for_need(need) {
        let created: Option<Uuid> = sqlx::query_scalar(
            "INSERT INTO tasks (case_id, need_id, gen_key, role, title, due_date)
             VALUES ($1, $2, $3, $4, $5, $6)
             ON CONFLICT (case_id, gen_key) DO UPDATE
                SET status = 'a_faire', owner_id = NULL, blocked_reason = NULL, claimed_at = NULL,
                    version = tasks.version + 1, updated_at = now()
              WHERE tasks.status = 'annule'
             RETURNING id",
        )
        .bind(case_id)
        .bind(need_id)
        .bind(&spec.key)
        .bind(spec.role.as_str())
        .bind(&spec.title)
        .bind(due)
        .fetch_optional(&mut *conn)
        .await?;
        task_created = created.is_some();
        // Nouvelle tâche, ou précision sur une tâche déjà ouverte.
        impacted = task_created || old_answer == Some(NeedAnswer::Oui);
    } else if old_answer == Some(NeedAnswer::Oui) && need.category != NeedCategory::Formation {
        let cancelled = sqlx::query(
            "UPDATE tasks SET status = 'annule', updated_at = now(), version = version + 1
              WHERE case_id = $1 AND gen_key = $2 AND status IN ('a_faire', 'en_cours', 'bloque')",
        )
        .bind(case_id)
        .bind(need.category.task_key())
        .execute(&mut *conn)
        .await?
        .rows_affected();
        impacted = cancelled > 0;
    }
    audit::record(
        conn,
        user,
        Audit {
            action: "besoin_saisi",
            entity: "case_need",
            entity_id: Some(need_id),
            case_id: Some(case_id),
            data: Some(json!({ "category": need.category, "answer": need.answer, "preference": pref, "revision": revision })),
            ..Default::default()
        },
    )
    .await?;
    Ok(NeedEffect {
        impacted_role: impacted.then_some(role),
        description: Some(describe_need(need)),
        task_created,
    })
}

/// Crée une session de formation à planifier et sa tâche. Un intervenant
/// désigné doit être un utilisateur actif habilité FORMATION.
pub async fn add_training(
    conn: &mut PgConnection,
    user: &CurrentUser,
    case_id: Uuid,
    employee_id: Uuid,
    t: &TrainingDto,
    due: Option<NaiveDate>,
) -> AppResult<Option<String>> {
    let name: Option<String> = sqlx::query_scalar("SELECT name FROM training_catalog WHERE id = $1 AND active")
        .bind(t.catalog_id)
        .fetch_optional(&mut *conn)
        .await?;
    let name = name.ok_or_else(|| AppError::invalid("trainings", "formation inconnue ou inactive"))?;
    if let Some(trainer) = t.trainer_id {
        let ok: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM users u JOIN user_roles r ON r.user_id = u.id
                             WHERE u.id = $1 AND u.active AND r.role = 'FORMATION')",
        )
        .bind(trainer)
        .fetch_one(&mut *conn)
        .await?;
        if !ok {
            return Err(AppError::invalid("trainer_id", "l'intervenant doit être un utilisateur actif habilité FORMATION"));
        }
    }
    let session_id: Option<Uuid> = sqlx::query_scalar(
        "INSERT INTO training_sessions (case_id, employee_id, catalog_id, trainer_id)
         VALUES ($1, $2, $3, $4)
         ON CONFLICT (case_id, catalog_id) WHERE case_id IS NOT NULL DO NOTHING
         RETURNING id",
    )
    .bind(case_id)
    .bind(employee_id)
    .bind(t.catalog_id)
    .bind(t.trainer_id)
    .fetch_optional(&mut *conn)
    .await?;
    let Some(session_id) = session_id else {
        return Ok(None); // déjà prévue dans ce dossier
    };
    let spec = task_for_training(&t.catalog_id.to_string(), &name);
    sqlx::query(
        "INSERT INTO tasks (case_id, training_session_id, gen_key, role, title, due_date, owner_id, status, claimed_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, CASE WHEN $7::uuid IS NULL THEN 'a_faire' ELSE 'en_cours' END,
                 CASE WHEN $7::uuid IS NULL THEN NULL ELSE now() END)
         ON CONFLICT (case_id, gen_key) DO NOTHING",
    )
    .bind(case_id)
    .bind(session_id)
    .bind(&spec.key)
    .bind(spec.role.as_str())
    .bind(&spec.title)
    .bind(due)
    .bind(t.trainer_id)
    .execute(&mut *conn)
    .await?;
    audit::record(
        conn,
        user,
        Audit {
            action: "formation_prevue",
            entity: "training_session",
            entity_id: Some(session_id),
            case_id: Some(case_id),
            employee_id: Some(employee_id),
            data: Some(json!({ "formation": name, "intervenant": t.trainer_id })),
            ..Default::default()
        },
    )
    .await?;
    Ok(Some(name))
}

// ------------------------------------------------------------------ M01 annonce

async fn create_arrival(
    State(state): State<AppState>,
    user: CurrentUser,
    headers: HeaderMap,
    Json(raw): Json<Value>,
) -> AppResult<Response> {
    user.require(domain::roles::HR_MANAGERS)?;
    let key = idempotency::require_key(&headers)?;
    let input: ArrivalInput = serde_json::from_value(raw.clone())
        .map_err(|e| AppError::invalid("body", format!("saisie incomplète : {e}")))?;
    let first_name = required("first_name", &input.first_name, "prénom")?;
    let last_name = required("last_name", &input.last_name, "nom")?;
    let job_title = required("job_title", &input.job_title, "poste prévu")?;

    let mut tx = state.pool.begin().await?;
    let request_hash = idempotency::hash_request("POST /arrivals", &raw);
    if let Begin::Replay(status, body) = idempotency::begin(&mut tx, user.id, &key, "POST /arrivals", &request_hash).await? {
        tx.commit().await?;
        return Ok((status, Json(body)).into_response());
    }

    let employee_id = match input.employee_id {
        Some(id) => {
            let status: Option<String> = sqlx::query_scalar("SELECT status FROM employees WHERE id = $1 FOR UPDATE")
                .bind(id)
                .fetch_optional(&mut *tx)
                .await?;
            status.ok_or_else(|| AppError::invalid("employee_id", "fiche introuvable"))?;
            id
        }
        None => {
            insert_employee(
                &mut tx,
                &NewEmployee {
                    first_name: first_name.clone(),
                    last_name: last_name.clone(),
                    matricule: input.matricule.clone(),
                    job_title: job_title.clone(),
                    site_id: input.site_id,
                    department: input.department.clone(),
                    manager: input.manager.clone(),
                    status: "a_venir",
                    planned_arrival: Some(input.planned_arrival),
                    actual_arrival: None,
                },
            )
            .await?
        }
    };
    let site_name: String = sqlx::query_scalar("SELECT name FROM sites WHERE id = $1")
        .bind(input.site_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| AppError::invalid("site_id", "site inconnu"))?;

    let reference = next_reference(&mut tx, CaseKind::Onb).await?;
    let case_id: Uuid = sqlx::query_scalar(
        "INSERT INTO cases (reference, kind, employee_id, effective_date, due_date, note, created_by)
         VALUES ($1, 'ONB', $2, $3, $3, $4, $5) RETURNING id",
    )
    .bind(&reference)
    .bind(employee_id)
    .bind(input.planned_arrival)
    .bind(input.note.as_deref().and_then(domain::text::clean))
    .bind(user.id)
    .fetch_one(&mut *tx)
    .await?;

    // Tous les besoins existent dès l'annonce ; non renseigné = à préciser.
    let mut needs: Vec<NeedInput> = Vec::new();
    for category in NeedCategory::ALL {
        let given = input.needs.iter().find(|n| n.category == *category);
        let mut n = match given {
            Some(dto) => dto.to_input(),
            None => NeedInput { category: *category, answer: NeedAnswer::APreciser, quantity: None, preference: None, detail: None },
        };
        if *category == NeedCategory::Formation && !input.trainings.is_empty() {
            n.answer = NeedAnswer::Oui;
        }
        needs.push(n);
    }
    let mut lines_by_role: Vec<(Role, String)> = Vec::new();
    let mut tasks_created = 0;
    for n in &needs {
        let effect = apply_need(&mut tx, &user, case_id, n, Some(input.planned_arrival)).await?;
        if effect.task_created {
            tasks_created += 1;
        }
        if n.answer == NeedAnswer::Oui && n.category != NeedCategory::Formation {
            lines_by_role.push((n.category.responsible_role(), describe_need(n)));
        }
    }
    for t in &input.trainings {
        if let Some(name) = add_training(&mut tx, &user, case_id, employee_id, t, Some(input.planned_arrival)).await? {
            tasks_created += 1;
            lines_by_role.push((Role::Formation, format!("Formation à planifier : {name}")));
        }
    }

    let roles = domain::needs::roles_to_notify_on_arrival(&needs);
    let mut body = format!(
        "Nouvelle arrivée annoncée : {first_name} {last_name}\nPoste prévu : {job_title} — Site : {site_name}\nDate d'arrivée prévue : {}\n",
        fr_date(input.planned_arrival)
    );
    for role in &roles {
        body.push_str(&format!("\n{} :\n", role.label()));
        let lines: Vec<&String> = lines_by_role.iter().filter(|(r, _)| r == role).map(|(_, l)| l).collect();
        if lines.is_empty() {
            body.push_str("- aucun besoin précisé pour l'instant (à préciser avec le RH si nécessaire)\n");
        } else {
            for l in lines {
                body.push_str(&format!("- {l}\n"));
            }
        }
    }
    audit::enqueue(
        &mut tx,
        &user,
        Notice {
            event_type: "arrivee_annoncee",
            case_id: Some(case_id),
            target_roles: roles.clone(),
            target_user_ids: vec![],
            subject: format!("{reference} — Arrivée de {first_name} {last_name} le {}", fr_date(input.planned_arrival)),
            body,
            link_path: Some(format!("/dossiers/{reference}")),
        },
    )
    .await?;
    audit::record(
        &mut tx,
        &user,
        Audit {
            action: "arrivee_annoncee",
            entity: "case",
            entity_id: Some(case_id),
            case_id: Some(case_id),
            employee_id: Some(employee_id),
            data: Some(json!({ "reference": reference })),
        },
    )
    .await?;
    let response = json!({
        "case_id": case_id,
        "reference": reference,
        "employee_id": employee_id,
        "notified_roles": roles,
        "tasks_created": tasks_created,
    });
    idempotency::finish(&mut tx, user.id, &key, StatusCode::CREATED, &response).await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(response)).into_response())
}

// ------------------------------------------------------------------ consultation

pub async fn can_view_case(conn: &mut PgConnection, user: &CurrentUser, case_id: Uuid) -> AppResult<bool> {
    if user.is_hr() {
        return Ok(true);
    }
    let roles: Vec<&str> = user.roles.iter().map(|r| r.as_str()).collect();
    let ok: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM tasks t WHERE t.case_id = $1 AND (t.role = ANY($2) OR t.owner_id = $3))
             OR EXISTS (SELECT 1 FROM cases c WHERE c.id = $1 AND c.kind = 'ONB'
                         AND ('IT' = ANY($2) OR 'FORMATION' = ANY($2)))",
    )
    .bind(case_id)
    .bind(&roles)
    .bind(user.id)
    .fetch_one(conn)
    .await?;
    Ok(ok)
}

#[derive(Deserialize)]
struct CaseListQuery {
    kind: Option<String>,
    status: Option<String>,
    q: Option<String>,
    page: Option<i64>,
}

#[derive(Serialize, FromRow)]
struct CaseListRow {
    id: Uuid,
    reference: String,
    kind: String,
    status: String,
    employee_id: Uuid,
    first_name: String,
    last_name: String,
    site_name: Option<String>,
    effective_date: Option<NaiveDate>,
    created_at: DateTime<Utc>,
    open_tasks: i64,
    unowned_tasks: i64,
}

async fn list_cases(State(state): State<AppState>, user: CurrentUser, Query(q): Query<CaseListQuery>) -> AppResult<Json<Value>> {
    let page = q.page.unwrap_or(1).max(1);
    let roles: Vec<&str> = user.roles.iter().map(|r| r.as_str()).collect();
    let search = q.q.as_deref().and_then(domain::text::clean).map(|s| format!("%{}%", s.to_lowercase()));
    let rows = sqlx::query_as::<_, CaseListRow>(
        "SELECT c.id, c.reference, c.kind, c.status, c.employee_id, e.first_name, e.last_name, s.name AS site_name,
                c.effective_date, c.created_at,
                (SELECT count(*) FROM tasks t WHERE t.case_id = c.id AND t.status NOT IN ('termine', 'annule')) AS open_tasks,
                (SELECT count(*) FROM tasks t WHERE t.case_id = c.id AND t.status = 'a_faire') AS unowned_tasks
           FROM cases c
           JOIN employees e ON e.id = c.employee_id
           LEFT JOIN employee_positions p ON p.employee_id = e.id AND p.valid_to IS NULL
           LEFT JOIN sites s ON s.id = p.site_id
          WHERE ($1::text IS NULL OR c.kind = $1)
            AND ($2::text IS NULL OR c.status = $2)
            AND ($3::text IS NULL OR lower(c.reference) LIKE $3 OR lower(e.first_name || ' ' || e.last_name) LIKE $3
                 OR lower(coalesce(e.matricule, '')) LIKE $3)
            AND ($4 OR EXISTS (SELECT 1 FROM tasks t WHERE t.case_id = c.id AND (t.role = ANY($5) OR t.owner_id = $6))
                 OR (c.kind = 'ONB' AND ('IT' = ANY($5) OR 'FORMATION' = ANY($5))))
          ORDER BY c.created_at DESC
          LIMIT 50 OFFSET $7",
    )
    .bind(&q.kind)
    .bind(&q.status)
    .bind(&search)
    .bind(user.is_hr())
    .bind(&roles)
    .bind(user.id)
    .bind((page - 1) * 50)
    .fetch_all(&state.pool)
    .await?;
    Ok(Json(json!({ "items": rows, "page": page })))
}

#[derive(Serialize, FromRow)]
struct CaseRow {
    id: Uuid,
    reference: String,
    kind: String,
    status: String,
    effective_date: Option<NaiveDate>,
    due_date: Option<NaiveDate>,
    note: Option<String>,
    version: i32,
    created_at: DateTime<Utc>,
    created_by_name: String,
    closed_at: Option<DateTime<Utc>>,
    close_note: Option<String>,
    parent_reference: Option<String>,
    employee_id: Uuid,
    first_name: String,
    last_name: String,
    matricule: Option<String>,
    employee_status: String,
    job_title: Option<String>,
    site_id: Option<Uuid>,
    site_name: Option<String>,
}

async fn case_detail(State(state): State<AppState>, user: CurrentUser, Path(reference): Path<String>) -> AppResult<Json<Value>> {
    let mut conn = state.pool.acquire().await?;
    let mut c = sqlx::query_as::<_, CaseRow>(
        "SELECT c.id, c.reference, c.kind, c.status, c.effective_date, c.due_date, c.note, c.version, c.created_at,
                u.display_name AS created_by_name, c.closed_at, c.close_note, pc.reference AS parent_reference,
                e.id AS employee_id, e.first_name, e.last_name, e.matricule, e.status AS employee_status,
                p.job_title, p.site_id, s.name AS site_name
           FROM cases c
           JOIN employees e ON e.id = c.employee_id
           JOIN users u ON u.id = c.created_by
           LEFT JOIN cases pc ON pc.id = c.parent_case_id
           LEFT JOIN employee_positions p ON p.employee_id = e.id AND p.valid_to IS NULL
           LEFT JOIN sites s ON s.id = p.site_id
          WHERE c.reference = upper($1) OR c.id::text = $1",
    )
    .bind(&reference)
    .fetch_optional(&mut *conn)
    .await?
    .ok_or_else(|| AppError::not_found("dossier"))?;
    if !can_view_case(&mut conn, &user, c.id).await? {
        return Err(AppError::forbidden());
    }
    if !user.is_hr() {
        c.note = None; // les notes RH restent au RH
    }
    let visible_categories: Vec<&str> = if user.is_hr() {
        NeedCategory::ALL.iter().map(|c| c.as_str()).collect()
    } else {
        NeedCategory::ALL.iter().filter(|c| user.has(c.responsible_role())).map(|c| c.as_str()).collect()
    };
    let needs: Value = sqlx::query_scalar(
        "SELECT coalesce(jsonb_agg(jsonb_build_object(
                    'id', n.id, 'category', n.category, 'answer', n.answer, 'quantity', n.quantity,
                    'preference', n.preference, 'detail', n.detail, 'revision', n.revision, 'updated_at', n.updated_at,
                    'updated_by', u.display_name,
                    'history', (SELECT coalesce(jsonb_agg(jsonb_build_object('revision', r.revision, 'answer', r.answer,
                                      'preference', r.preference, 'quantity', r.quantity, 'detail', r.detail,
                                      'changed_at', r.changed_at) ORDER BY r.revision), '[]')
                                  FROM need_revisions r WHERE r.need_id = n.id))
                    ORDER BY array_position(ARRAY['pc','telephone','sim','email_pro','xena','intelix','carte_visite','vehicule','formation'], n.category)), '[]')
           FROM case_needs n JOIN users u ON u.id = n.updated_by
          WHERE n.case_id = $1 AND n.category = ANY($2)",
    )
    .bind(c.id)
    .bind(&visible_categories)
    .fetch_one(&mut *conn)
    .await?;
    let tasks: Value = sqlx::query_scalar(
        "SELECT coalesce(jsonb_agg(jsonb_build_object(
                    'id', t.id, 'title', t.title, 'role', t.role, 'status', t.status, 'owner', o.display_name,
                    'owner_id', t.owner_id, 'due_date', t.due_date, 'blocked_reason', t.blocked_reason,
                    'done_note', t.done_note, 'claimed_at', t.claimed_at, 'completed_at', t.completed_at,
                    'version', t.version, 'need_category', n.category)
                    ORDER BY t.role, t.created_at), '[]')
           FROM tasks t LEFT JOIN users o ON o.id = t.owner_id LEFT JOIN case_needs n ON n.id = t.need_id
          WHERE t.case_id = $1",
    )
    .bind(c.id)
    .fetch_one(&mut *conn)
    .await?;
    let trainings: Value = sqlx::query_scalar(
        "SELECT coalesce(jsonb_agg(jsonb_build_object(
                    'id', s.id, 'catalog_id', s.catalog_id, 'name', tc.name, 'status', s.status, 'trainer', u.display_name,
                    'planned_date', s.planned_date, 'done_date', s.done_date, 'reason', s.reason) ORDER BY tc.name), '[]')
           FROM training_sessions s JOIN training_catalog tc ON tc.id = s.catalog_id LEFT JOIN users u ON u.id = s.trainer_id
          WHERE s.case_id = $1",
    )
    .bind(c.id)
    .fetch_one(&mut *conn)
    .await?;
    let movements = if user.has_any(&[Role::Rh, Role::Admin, Role::It]) {
        crate::routes::movements::case_movements_json(&mut conn, c.id).await?
    } else {
        json!([])
    };
    let children: Value = sqlx::query_scalar(
        "SELECT coalesce(jsonb_agg(jsonb_build_object('reference', reference, 'kind', kind, 'status', status) ORDER BY created_at), '[]')
           FROM cases WHERE parent_case_id = $1",
    )
    .bind(c.id)
    .fetch_one(&mut *conn)
    .await?;
    let history: Value = sqlx::query_scalar(
        "SELECT coalesce(jsonb_agg(jsonb_build_object('at', a.at, 'action', a.action, 'actor', u.display_name) ORDER BY a.at DESC), '[]')
           FROM (SELECT * FROM audit_events WHERE case_id = $1 ORDER BY at DESC LIMIT 100) a
           LEFT JOIN users u ON u.id = a.actor_id",
    )
    .bind(c.id)
    .fetch_one(&mut *conn)
    .await?;
    let notifications: Value = sqlx::query_scalar(
        "SELECT coalesce(jsonb_agg(jsonb_build_object('subject', o.subject, 'status', o.status, 'created_at', o.created_at,
                    'recipients', (SELECT count(*) FROM deliveries d WHERE d.outbox_id = o.id),
                    'delivered', (SELECT count(*) FROM deliveries d WHERE d.outbox_id = o.id AND d.status IN ('journalise', 'accepte')),
                    'failed', (SELECT count(*) FROM deliveries d WHERE d.outbox_id = o.id AND d.status IN ('echec', 'a_verifier')))
                    ORDER BY o.created_at DESC), '[]')
           FROM outbox o WHERE o.case_id = $1",
    )
    .bind(c.id)
    .fetch_one(&mut *conn)
    .await?;
    Ok(Json(json!({
        "case": c,
        "needs": needs,
        "tasks": tasks,
        "trainings": trainings,
        "movements": movements,
        "children": children,
        "history": history,
        "notifications": notifications,
    })))
}

// ------------------------------------------------------------------ révision des besoins

#[derive(Deserialize)]
struct NeedsUpdate {
    version: i32,
    #[serde(default)]
    needs: Vec<NeedDto>,
    #[serde(default)]
    trainings: Vec<TrainingDto>,
}

/// Ajouts et précisions sur un dossier ouvert : seules les nouvelles tâches
/// sont créées et seules les équipes impactées sont prévenues.
async fn update_needs(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(case_id): Path<Uuid>,
    Json(input): Json<NeedsUpdate>,
) -> AppResult<Json<Value>> {
    user.require(domain::roles::HR_MANAGERS)?;
    let mut tx = state.pool.begin().await?;
    let row: Option<(String, String, i32, Uuid, Option<NaiveDate>)> = sqlx::query_as(
        "SELECT reference, status, version, employee_id, coalesce(due_date, effective_date) FROM cases WHERE id = $1 FOR UPDATE",
    )
    .bind(case_id)
    .fetch_optional(&mut *tx)
    .await?;
    let (reference, status, version, employee_id, due) = row.ok_or_else(|| AppError::not_found("dossier"))?;
    if status != "ouvert" {
        return Err(AppError::conflict("dossier clos ou annulé : créez une demande ponctuelle"));
    }
    if version != input.version {
        return Err(AppError::stale());
    }
    let mut impacted: Vec<Role> = Vec::new();
    let mut lines: Vec<String> = Vec::new();
    let mut needs = input.needs.clone();
    if !input.trainings.is_empty() && !needs.iter().any(|n| n.category == NeedCategory::Formation) {
        needs.push(NeedDto { category: NeedCategory::Formation, answer: NeedAnswer::Oui, quantity: None, preference: None, detail: None });
    }
    for dto in &needs {
        let effect = apply_need(&mut tx, &user, case_id, &dto.to_input(), due).await?;
        if let Some(role) = effect.impacted_role {
            if !impacted.contains(&role) {
                impacted.push(role);
            }
            if let Some(d) = effect.description {
                lines.push(format!("{} — {d}", role.label()));
            }
        }
    }
    for t in &input.trainings {
        if let Some(name) = add_training(&mut tx, &user, case_id, employee_id, t, due).await? {
            if !impacted.contains(&Role::Formation) {
                impacted.push(Role::Formation);
            }
            lines.push(format!("Formation — à planifier : {name}"));
        }
    }
    sqlx::query("UPDATE cases SET version = version + 1, updated_at = now() WHERE id = $1")
        .bind(case_id)
        .execute(&mut *tx)
        .await?;
    if !impacted.is_empty() {
        let name: String = sqlx::query_scalar("SELECT first_name || ' ' || last_name FROM employees WHERE id = $1")
            .bind(employee_id)
            .fetch_one(&mut *tx)
            .await?;
        audit::enqueue(
            &mut tx,
            &user,
            Notice {
                event_type: "besoins_modifies",
                case_id: Some(case_id),
                target_roles: impacted.clone(),
                target_user_ids: vec![],
                subject: format!("{reference} — Besoins mis à jour pour {name}"),
                body: format!("Le dossier {reference} ({name}) a été complété :\n- {}\n", lines.join("\n- ")),
                link_path: Some(format!("/dossiers/{reference}")),
            },
        )
        .await?;
    }
    tx.commit().await?;
    Ok(Json(json!({ "version": version + 1, "notified_roles": impacted })))
}

// ------------------------------------------------------------------ M04 demande ponctuelle

#[derive(Deserialize)]
struct RequestInput {
    category: NeedCategory,
    detail: Option<String>,
    quantity: Option<i32>,
    preference: Option<PcPreference>,
    due_date: Option<NaiveDate>,
    parent_case_id: Option<Uuid>,
    /// Pour une formation isolée.
    training: Option<TrainingDto>,
    /// Confirme la création malgré un besoin similaire déjà ouvert.
    #[serde(default)]
    force: bool,
}

async fn create_request(
    State(state): State<AppState>,
    user: CurrentUser,
    headers: HeaderMap,
    Path(employee_id): Path<Uuid>,
    Json(raw): Json<Value>,
) -> AppResult<Response> {
    user.require(domain::roles::HR_MANAGERS)?;
    let input: RequestInput =
        serde_json::from_value(raw.clone()).map_err(|e| AppError::invalid("body", format!("saisie incomplète : {e}")))?;
    let key = idempotency::key_from(&headers)?;
    let endpoint = format!("POST /employees/{employee_id}/requests");
    let mut tx = state.pool.begin().await?;
    if let Some(k) = &key {
        if let Begin::Replay(status, body) =
            idempotency::begin(&mut tx, user.id, k, &endpoint, &idempotency::hash_request(&endpoint, &raw)).await?
        {
            tx.commit().await?;
            return Ok((status, Json(body)).into_response());
        }
    }
    let emp: Option<(String, String, String)> =
        sqlx::query_as("SELECT status, first_name, last_name FROM employees WHERE id = $1 FOR UPDATE")
            .bind(employee_id)
            .fetch_optional(&mut *tx)
            .await?;
    let (emp_status, first, last) = emp.ok_or_else(|| AppError::not_found("salarié"))?;
    if emp_status == "sorti" {
        return Err(AppError::conflict("salarié sorti : aucune nouvelle demande"));
    }
    if input.category == NeedCategory::Formation && input.training.is_none() {
        return Err(AppError::invalid("training", "choisissez la formation demandée"));
    }
    if let Some(parent) = input.parent_case_id {
        let same: Option<Uuid> = sqlx::query_scalar("SELECT employee_id FROM cases WHERE id = $1")
            .bind(parent)
            .fetch_optional(&mut *tx)
            .await?;
        if same != Some(employee_id) {
            return Err(AppError::invalid("parent_case_id", "le dossier d'origine doit concerner le même salarié"));
        }
    }
    if !input.force {
        let existing: Option<String> = sqlx::query_scalar(
            "SELECT c.reference FROM cases c JOIN case_needs n ON n.case_id = c.id
               JOIN tasks t ON t.need_id = n.id
              WHERE c.employee_id = $1 AND c.status = 'ouvert' AND n.category = $2 AND n.answer = 'oui'
                AND t.status NOT IN ('termine', 'annule')
              LIMIT 1",
        )
        .bind(employee_id)
        .bind(input.category.as_str())
        .fetch_optional(&mut *tx)
        .await?;
        if let Some(r) = existing {
            return Err(AppError::Conflict {
                message: format!("un besoin similaire est déjà ouvert dans {r}"),
                detail: Some(json!({ "existing_reference": r })),
            });
        }
    }
    let reference = next_reference(&mut tx, CaseKind::Dem).await?;
    let case_id: Uuid = sqlx::query_scalar(
        "INSERT INTO cases (reference, kind, employee_id, parent_case_id, due_date, created_by)
         VALUES ($1, 'DEM', $2, $3, $4, $5) RETURNING id",
    )
    .bind(&reference)
    .bind(employee_id)
    .bind(input.parent_case_id)
    .bind(input.due_date)
    .bind(user.id)
    .fetch_one(&mut *tx)
    .await?;
    let need = NeedInput {
        category: input.category,
        answer: NeedAnswer::Oui,
        quantity: input.quantity,
        preference: input.preference,
        detail: input.detail.as_deref().and_then(domain::text::clean),
    };
    apply_need(&mut tx, &user, case_id, &need, input.due_date).await?;
    if let Some(t) = &input.training {
        add_training(&mut tx, &user, case_id, employee_id, t, input.due_date).await?;
    }
    let role = input.category.responsible_role();
    let due_txt = input.due_date.map(|d| format!("\nÉchéance souhaitée : {}", fr_date(d))).unwrap_or_default();
    audit::enqueue(
        &mut tx,
        &user,
        Notice {
            event_type: "demande_ponctuelle",
            case_id: Some(case_id),
            target_roles: vec![role],
            target_user_ids: vec![],
            subject: format!("{reference} — {} pour {first} {last}", input.category.label()),
            body: format!("Demande ponctuelle {reference} pour {first} {last} :\n- {}{due_txt}\n", describe_need(&need)),
            link_path: Some(format!("/dossiers/{reference}")),
        },
    )
    .await?;
    audit::record(
        &mut tx,
        &user,
        Audit {
            action: "demande_ponctuelle",
            entity: "case",
            entity_id: Some(case_id),
            case_id: Some(case_id),
            employee_id: Some(employee_id),
            data: Some(json!({ "reference": reference, "category": input.category, "parent": input.parent_case_id })),
        },
    )
    .await?;
    let response = json!({ "case_id": case_id, "reference": reference, "notified_roles": [role] });
    if let Some(k) = &key {
        idempotency::finish(&mut tx, user.id, k, StatusCode::CREATED, &response).await?;
    }
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(response)).into_response())
}

// ------------------------------------------------------------------ clôture

#[derive(Deserialize)]
struct CloseInput {
    version: i32,
    note: Option<String>,
    /// Exception motivée (ADMIN) pour clore malgré des points non soldés.
    exception_reason: Option<String>,
}

async fn close_case(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(case_id): Path<Uuid>,
    Json(input): Json<CloseInput>,
) -> AppResult<Json<Value>> {
    user.require(domain::roles::HR_MANAGERS)?;
    let mut tx = state.pool.begin().await?;
    let row: Option<(String, i32)> = sqlx::query_as("SELECT status, version FROM cases WHERE id = $1 FOR UPDATE")
        .bind(case_id)
        .fetch_optional(&mut *tx)
        .await?;
    let (status, version) = row.ok_or_else(|| AppError::not_found("dossier"))?;
    if status != "ouvert" {
        return Err(AppError::conflict("dossier déjà clos ou annulé"));
    }
    if version != input.version {
        return Err(AppError::stale());
    }
    let open_tasks: Vec<String> = sqlx::query_scalar(
        "SELECT title || ' (' || role || ')' FROM tasks WHERE case_id = $1 AND status NOT IN ('termine', 'annule') ORDER BY title",
    )
    .bind(case_id)
    .fetch_all(&mut *tx)
    .await?;
    let unsigned: Vec<String> = sqlx::query_scalar(
        "SELECT 'Décharge non signée : ' || m.reference FROM documents d JOIN movements m ON m.id = d.movement_id
          WHERE m.case_id = $1 AND d.status <> 'signe' ORDER BY m.reference",
    )
    .bind(case_id)
    .fetch_all(&mut *tx)
    .await?;
    let mut unresolved: Vec<String> = open_tasks.into_iter().map(|t| format!("Tâche ouverte : {t}")).collect();
    unresolved.extend(unsigned);
    let exception = input.exception_reason.as_deref().and_then(domain::text::clean);
    if !unresolved.is_empty() {
        match &exception {
            None => {
                return Err(AppError::Conflict {
                    message: "des points ne sont pas soldés".into(),
                    detail: Some(json!({ "unresolved": unresolved })),
                })
            }
            Some(_) if !user.has(Role::Admin) => {
                return Err(AppError::Forbidden("seul un ADMIN peut clore avec une exception motivée".into()))
            }
            Some(_) => {}
        }
    }
    let note = match (&exception, input.note.as_deref().and_then(domain::text::clean)) {
        (Some(e), Some(n)) => Some(format!("{n}\nException : {e}")),
        (Some(e), None) => Some(format!("Exception : {e}")),
        (None, n) => n,
    };
    sqlx::query(
        "UPDATE cases SET status = 'clos', closed_at = now(), closed_by = $2, close_note = $3,
                version = version + 1, updated_at = now() WHERE id = $1",
    )
    .bind(case_id)
    .bind(user.id)
    .bind(&note)
    .execute(&mut *tx)
    .await?;
    audit::record(
        &mut tx,
        &user,
        Audit {
            action: if exception.is_some() { "cloture_avec_exception" } else { "cloture" },
            entity: "case",
            entity_id: Some(case_id),
            case_id: Some(case_id),
            data: Some(json!({ "unresolved": unresolved, "exception": exception })),
            ..Default::default()
        },
    )
    .await?;
    tx.commit().await?;
    Ok(Json(json!({ "ok": true, "unresolved": unresolved })))
}
