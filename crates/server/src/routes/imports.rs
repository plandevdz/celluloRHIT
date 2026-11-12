//! Import / export Excel du personnel (M07, contrat 04-Donnees.md).
//! Aperçu sans écriture métier → décisions du RH → application en une
//! transaction. Aucun parcours, tâche, compte ni avis n'est créé.

use std::collections::HashMap;

use axum::body::Body;
use axum::extract::{DefaultBodyLimit, Multipart, Path, Query, State};
use axum::http::header;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::{FromRow, PgConnection};
use uuid::Uuid;

use crate::audit::{self, Audit};
use crate::auth::CurrentUser;
use crate::documents::{algiers_today, sha256_hex};
use crate::error::{AppError, AppResult};
use crate::personnel_xlsx::{self as xlsx, ExportRow, HEADERS};
use crate::routes::employees::{insert_employee, NewEmployee};
use crate::state::AppState;

pub fn routes(max_upload: usize) -> Router<AppState> {
    Router::new()
        .route("/personnel/template.xlsx", get(template))
        .route("/personnel/export.xlsx", get(export))
        .route(
            "/personnel-imports",
            get(list_batches).post(preview).layer(DefaultBodyLimit::max(max_upload + 64 * 1024)),
        )
        .route("/personnel-imports/{id}", get(batch_detail))
        .route("/personnel-imports/{id}/rows/{row}/decision", post(decide))
        .route("/personnel-imports/{id}/commit", post(commit))
}

const XLSX_MIME: &str = "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet";

fn xlsx_response(bytes: Vec<u8>, filename: &str) -> Response {
    (
        [
            (header::CONTENT_TYPE, XLSX_MIME.to_string()),
            (header::CONTENT_DISPOSITION, format!("attachment; filename=\"{filename}\"")),
            (header::CACHE_CONTROL, "no-store".to_string()),
        ],
        Body::from(bytes),
    )
        .into_response()
}

// ------------------------------------------------------------------ modèle et export

async fn template(user: CurrentUser) -> AppResult<Response> {
    user.require(domain::roles::HR_MANAGERS)?;
    let bytes = xlsx::write(&[], 500).map_err(|e| anyhow::anyhow!("modèle Excel : {e}"))?;
    Ok(xlsx_response(bytes, "Personnel-modele.xlsx"))
}

#[derive(Deserialize)]
struct ExportQuery {
    status: Option<String>,
    site_id: Option<Uuid>,
}

#[derive(FromRow)]
struct ExportDbRow {
    id: Uuid,
    matricule: Option<String>,
    first_name: String,
    last_name: String,
    job_title: Option<String>,
    site_name: Option<String>,
    arrival: Option<NaiveDate>,
    department: Option<String>,
}

/// Export au format du modèle (réimportable : l'ID permet la mise à jour).
async fn export(State(state): State<AppState>, user: CurrentUser, Query(q): Query<ExportQuery>) -> AppResult<Response> {
    user.require(domain::roles::HR_MANAGERS)?;
    let rows = sqlx::query_as::<_, ExportDbRow>(
        "SELECT e.id, e.matricule, e.first_name, e.last_name, p.job_title, s.name AS site_name,
                coalesce(e.actual_arrival, e.planned_arrival) AS arrival, p.department
           FROM employees e
           LEFT JOIN employee_positions p ON p.employee_id = e.id AND p.valid_to IS NULL
           LEFT JOIN sites s ON s.id = p.site_id
          WHERE ($1::text IS NULL OR e.status = $1) AND ($2::uuid IS NULL OR p.site_id = $2)
          ORDER BY e.last_name, e.first_name",
    )
    .bind(&q.status)
    .bind(q.site_id)
    .fetch_all(&state.pool)
    .await?;
    let export: Vec<ExportRow> = rows
        .into_iter()
        .map(|r| ExportRow {
            id: r.id.to_string(),
            matricule: r.matricule,
            first_name: r.first_name,
            last_name: r.last_name,
            job_title: r.job_title,
            site: r.site_name,
            arrival: r.arrival,
            department: r.department,
        })
        .collect();
    let count = export.len();
    let bytes = xlsx::write(&export, 0).map_err(|e| anyhow::anyhow!("export Excel : {e}"))?;
    let mut conn = state.pool.acquire().await?;
    audit::record(
        &mut conn,
        &user,
        Audit {
            action: "export_personnel",
            entity: "employee",
            data: Some(json!({ "lignes": count, "statut": q.status, "site": q.site_id, "sha256": sha256_hex(&bytes) })),
            ..Default::default()
        },
    )
    .await?;
    Ok(xlsx_response(bytes, &format!("Personnel-{}.xlsx", algiers_today().format("%Y%m%d"))))
}

// ------------------------------------------------------------------ données de travail

/// Valeurs d'une ligne, après lecture et normalisation.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct RowData {
    id: Option<Uuid>,
    matricule: Option<String>,
    first_name: Option<String>,
    last_name: Option<String>,
    job_title: Option<String>,
    site: Option<String>,
    site_id: Option<Uuid>,
    arrival: Option<NaiveDate>,
    department: Option<String>,
}

impl RowData {
    fn matricule_norm(&self) -> Option<String> {
        self.matricule.as_deref().and_then(domain::text::normalize_matricule)
    }
    fn name_key(&self) -> Option<String> {
        match (&self.first_name, &self.last_name) {
            (Some(f), Some(l)) => Some(domain::text::name_key(f, l)),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, FromRow)]
struct Emp {
    id: Uuid,
    matricule: Option<String>,
    matricule_norm: Option<String>,
    first_name: String,
    last_name: String,
    name_key: String,
    status: String,
    version: i32,
    job_title: Option<String>,
    site_id: Option<Uuid>,
    site_name: Option<String>,
    department: Option<String>,
    actual_arrival: Option<NaiveDate>,
    planned_arrival: Option<NaiveDate>,
}

const EMP_SELECT: &str = "SELECT e.id, e.matricule, e.matricule_norm, e.first_name, e.last_name, e.name_key, e.status, e.version,
            p.job_title, p.site_id, s.name AS site_name, p.department, e.actual_arrival, e.planned_arrival
       FROM employees e
       LEFT JOIN employee_positions p ON p.employee_id = e.id AND p.valid_to IS NULL
       LEFT JOIN sites s ON s.id = p.site_id";

impl Emp {
    fn arrival(&self) -> Option<NaiveDate> {
        if self.status == "a_venir" {
            self.planned_arrival
        } else {
            self.actual_arrival
        }
    }
    fn candidate(&self) -> Value {
        json!({
            "id": self.id, "nom": format!("{} {}", self.first_name, self.last_name), "matricule": self.matricule,
            "poste": self.job_title, "site": self.site_name, "statut": self.status,
        })
    }
}

fn msg(level: &str, text: impl Into<String>) -> Value {
    json!({ "niveau": level, "texte": text.into() })
}

/// Changements proposés : une valeur vide signifie « conserver ».
fn diff(e: &Emp, d: &RowData) -> serde_json::Map<String, Value> {
    let mut ch = serde_json::Map::new();
    let mut put = |field: &str, before: Value, after: Value| {
        if before != after {
            ch.insert(field.into(), json!({ "avant": before, "apres": after }));
        }
    };
    if let Some(v) = &d.first_name {
        put("Prénom", json!(e.first_name), json!(v));
    }
    if let Some(v) = &d.last_name {
        put("Nom", json!(e.last_name), json!(v));
    }
    if let Some(n) = d.matricule_norm() {
        if e.matricule_norm.as_deref() != Some(n.as_str()) {
            put("Matricule", json!(e.matricule), json!(d.matricule));
        }
    }
    if let Some(v) = &d.job_title {
        put("Poste", json!(e.job_title), json!(v));
    }
    if d.site_id.is_some() && d.site_id != e.site_id {
        put("Site", json!(e.site_name), json!(d.site));
    }
    if let Some(v) = &d.department {
        put("Département", json!(e.department), json!(v));
    }
    if let Some(v) = d.arrival {
        put("Date d’arrivée", json!(e.arrival()), json!(v));
    }
    ch
}

struct Analysed {
    row_number: i32,
    data: RowData,
    proposal: &'static str,
    messages: Vec<Value>,
    employee: Option<(Uuid, i32)>,
    candidates: Vec<Value>,
    changes: serde_json::Map<String, Value>,
}

async fn load_sites(conn: &mut PgConnection) -> AppResult<Vec<(Uuid, String, String)>> {
    Ok(sqlx::query_as("SELECT id, code, name FROM sites WHERE active ORDER BY name").fetch_all(conn).await?)
}

fn find_site(sites: &[(Uuid, String, String)], label: &str) -> Option<Uuid> {
    let l = label.trim().to_lowercase();
    sites.iter().find(|(_, code, name)| code.to_lowercase() == l || name.to_lowercase() == l).map(|s| s.0)
}

/// Analyse d'un classeur : aucune écriture métier, seulement des propositions.
fn analyse(rows: Vec<xlsx::SheetRow>, sites: &[(Uuid, String, String)], emps: &[Emp]) -> Vec<Analysed> {
    let by_id: HashMap<Uuid, &Emp> = emps.iter().map(|e| (e.id, e)).collect();
    let by_mat: HashMap<&str, &Emp> = emps.iter().filter_map(|e| e.matricule_norm.as_deref().map(|m| (m, e))).collect();
    let mut by_name: HashMap<&str, Vec<&Emp>> = HashMap::new();
    for e in emps {
        by_name.entry(e.name_key.as_str()).or_default().push(e);
    }

    let mut out = Vec::new();
    for r in rows {
        let c = &r.cells;
        let mut messages = Vec::new();
        for (i, cell) in c.iter().enumerate() {
            if cell.formula {
                messages.push(msg("erreur", format!("{} : les formules sont refusées, saisissez la valeur", HEADERS[i])));
            }
            if cell.error {
                messages.push(msg("erreur", format!("{} : cellule en erreur (#N/A, #REF!…)", HEADERS[i])));
            }
        }
        let text = |i: usize| if c[i].formula { None } else { c[i].text.clone() };
        let mut d = RowData {
            first_name: text(xlsx::COL_PRENOM),
            last_name: text(xlsx::COL_NOM),
            job_title: text(xlsx::COL_POSTE),
            site: text(xlsx::COL_SITE),
            department: text(xlsx::COL_DEPARTEMENT),
            ..Default::default()
        };
        if let Some(raw) = text(xlsx::COL_ID) {
            match Uuid::parse_str(raw.trim()) {
                Ok(id) => d.id = Some(id),
                Err(_) => messages.push(msg("erreur", format!("ID salarié invalide : « {raw} »"))),
            }
        }
        if let Some(m) = text(xlsx::COL_MATRICULE) {
            if c[xlsx::COL_MATRICULE].numeric {
                messages.push(msg(
                    "erreur",
                    format!("Matricule saisi comme nombre ({m}) : formatez la colonne en Texte pour conserver les zéros initiaux"),
                ));
            } else {
                d.matricule = Some(m);
            }
        }
        if let Some(t) = text(xlsx::COL_DATE) {
            match c[xlsx::COL_DATE].date {
                Some(date) => d.arrival = Some(date),
                None => messages.push(msg("erreur", format!("Date d’arrivée illisible : « {t} » (attendu jj/mm/aaaa)"))),
            }
        }
        if let Some(s) = &d.site {
            match find_site(sites, s) {
                Some(id) => d.site_id = Some(id),
                None => messages.push(msg(
                    "erreur",
                    format!("Site inconnu : « {s} » (sites : {})", sites.iter().map(|x| x.2.as_str()).collect::<Vec<_>>().join(", ")),
                )),
            }
        }

        let mut a = Analysed {
            row_number: r.row_number as i32,
            data: d.clone(),
            proposal: "rejet",
            messages,
            employee: None,
            candidates: vec![],
            changes: serde_json::Map::new(),
        };
        let has_error = a.messages.iter().any(|m| m["niveau"] == "erreur");
        if !has_error {
            let mat = d.matricule_norm();
            if let Some(id) = d.id {
                // ID applicatif : mise à jour, jamais remplacement d'identité.
                match by_id.get(&id) {
                    None => a.messages.push(msg("erreur", "ID salarié inconnu dans l'application")),
                    Some(e) => match mat.as_deref().and_then(|m| by_mat.get(m)) {
                        Some(owner) if owner.id != e.id => a.messages.push(msg(
                            "erreur",
                            format!(
                                "Identifiants contradictoires : l'ID désigne {} {}, le matricule désigne {} {}",
                                e.first_name, e.last_name, owner.first_name, owner.last_name
                            ),
                        )),
                        _ => {
                            a.changes = diff(e, &d);
                            a.employee = Some((e.id, e.version));
                            a.proposal = if a.changes.is_empty() { "inchange" } else { "mise_a_jour" };
                        }
                    },
                }
            } else if let Some(owner) = mat.as_deref().and_then(|m| by_mat.get(m)) {
                // Matricule exact : correspondance proposée ; nom différent → à revoir.
                let same_name = d.name_key().map(|k| k == owner.name_key).unwrap_or(true);
                a.employee = Some((owner.id, owner.version));
                a.changes = diff(owner, &d);
                if same_name {
                    a.proposal = if a.changes.is_empty() { "inchange" } else { "mise_a_jour" };
                } else {
                    a.proposal = "a_revoir";
                    a.candidates = vec![owner.candidate()];
                    a.messages.push(msg(
                        "attention",
                        format!("Matricule de {} {}, mais nom différent : rattacher ou exclure", owner.first_name, owner.last_name),
                    ));
                }
            } else {
                // Nouvelle personne : champs obligatoires, puis homonymes.
                let missing: Vec<&str> = [
                    ("Prénom", d.first_name.is_none()),
                    ("Nom", d.last_name.is_none()),
                    ("Poste", d.job_title.is_none()),
                    ("Site", d.site_id.is_none()),
                    ("Date d’arrivée", d.arrival.is_none()),
                ]
                .iter()
                .filter(|(_, m)| *m)
                .map(|(n, _)| *n)
                .collect();
                if !missing.is_empty() {
                    a.messages.push(msg("erreur", format!("Nouvelle fiche : {} obligatoire(s)", missing.join(", "))));
                } else {
                    let homonyms = d.name_key().and_then(|k| by_name.get(k.as_str()).cloned()).unwrap_or_default();
                    if homonyms.is_empty() {
                        a.proposal = "creation";
                    } else {
                        a.proposal = "a_revoir";
                        a.candidates = homonyms.iter().map(|e| e.candidate()).collect();
                        a.messages.push(msg(
                            "attention",
                            format!(
                                "{} personne(s) du même nom déjà connue(s), sans matricule commun : choisir « nouvelle personne », « rattacher » ou « exclure »",
                                homonyms.len()
                            ),
                        ));
                    }
                }
            }
        }
        if a.messages.iter().any(|m| m["niveau"] == "erreur") {
            a.proposal = "rejet";
            a.employee = None;
            a.changes.clear();
        }
        out.push(a);
    }

    // Contrôles entre lignes du même fichier.
    let mut seen_id: HashMap<Uuid, i32> = HashMap::new();
    let mut seen_mat: HashMap<String, i32> = HashMap::new();
    for a in out.iter_mut() {
        if a.proposal == "rejet" {
            continue;
        }
        if let Some(id) = a.data.id {
            if let Some(first) = seen_id.insert(id, a.row_number) {
                a.proposal = "rejet";
                a.messages.push(msg("erreur", format!("ID déjà présent ligne {first}")));
                continue;
            }
        }
        if let Some(m) = a.data.matricule_norm() {
            if let Some(first) = seen_mat.insert(m.clone(), a.row_number) {
                a.proposal = "rejet";
                a.messages.push(msg("erreur", format!("Matricule {m} déjà présent ligne {first}")));
            }
        }
    }
    let mut groups: HashMap<String, Vec<i32>> = HashMap::new();
    for a in &out {
        if matches!(a.proposal, "creation" | "a_revoir") && a.data.id.is_none() && a.data.matricule.is_none() {
            if let Some(k) = a.data.name_key() {
                groups.entry(k).or_default().push(a.row_number);
            }
        }
    }
    for a in out.iter_mut() {
        if let Some(lines) = a.data.name_key().and_then(|k| groups.get(&k)).filter(|g| g.len() > 1) {
            if a.data.id.is_none() && a.data.matricule.is_none() && matches!(a.proposal, "creation" | "a_revoir") {
                a.proposal = "a_revoir";
                let others: Vec<String> = lines.iter().filter(|n| **n != a.row_number).map(|n| n.to_string()).collect();
                a.messages.push(msg(
                    "attention",
                    format!("Homonyme dans le fichier (ligne(s) {}) sans matricule : décider ligne par ligne", others.join(", ")),
                ));
            }
        }
    }
    out
}

// ------------------------------------------------------------------ aperçu

async fn preview(State(state): State<AppState>, user: CurrentUser, mut multipart: Multipart) -> AppResult<Json<Value>> {
    user.require(domain::roles::HR_MANAGERS)?;
    let mut file: Option<(String, Vec<u8>)> = None;
    while let Some(field) = multipart.next_field().await.map_err(|e| AppError::invalid("file", e.to_string()))? {
        if field.name() == Some("file") {
            let name = field.file_name().unwrap_or("Personnel.xlsx").chars().take(120).collect::<String>();
            let data = field.bytes().await.map_err(|e| AppError::invalid("file", e.to_string()))?;
            file = Some((name, data.to_vec()));
        }
    }
    let (file_name, bytes) = file.ok_or_else(|| AppError::invalid("file", "fichier manquant"))?;
    if bytes.is_empty() || bytes.len() > state.config.max_upload_bytes {
        return Err(AppError::invalid("file", "fichier vide ou trop volumineux"));
    }
    if !bytes.starts_with(b"PK\x03\x04") {
        return Err(AppError::invalid("file", "format refusé : classeur Excel .xlsx attendu"));
    }
    let rows = tokio::task::spawn_blocking(move || xlsx::read(&bytes).map(|r| (r, bytes)))
        .await
        .map_err(anyhow::Error::from)?
        .map_err(|e| AppError::invalid("file", e))?;
    let (rows, bytes) = rows;
    if rows.is_empty() {
        return Err(AppError::invalid("file", "aucune ligne à importer sous les en-têtes"));
    }
    let hash = sha256_hex(&bytes);

    let mut tx = state.pool.begin().await?;
    let sites = load_sites(&mut tx).await?;
    let emps = sqlx::query_as::<_, Emp>(EMP_SELECT).fetch_all(&mut *tx).await?;
    let analysed = analyse(rows, &sites, &emps);
    let batch_id: Uuid = sqlx::query_scalar(
        "INSERT INTO import_batches (created_by, file_name, file_sha256, format_version) VALUES ($1, $2, $3, $4) RETURNING id",
    )
    .bind(user.id)
    .bind(&file_name)
    .bind(&hash)
    .bind(xlsx::FORMAT_VERSION)
    .fetch_one(&mut *tx)
    .await?;
    for a in &analysed {
        sqlx::query(
            "INSERT INTO import_rows (batch_id, row_number, raw, proposal, messages, employee_id, employee_version, candidates, changes)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
        )
        .bind(batch_id)
        .bind(a.row_number)
        .bind(serde_json::to_value(&a.data).map_err(anyhow::Error::from)?)
        .bind(a.proposal)
        .bind(json!(a.messages))
        .bind(a.employee.map(|e| e.0))
        .bind(a.employee.map(|e| e.1))
        .bind(json!(a.candidates))
        .bind(Value::Object(a.changes.clone()))
        .execute(&mut *tx)
        .await?;
    }
    audit::record(
        &mut tx,
        &user,
        Audit {
            action: "import_personnel_apercu",
            entity: "import_batch",
            entity_id: Some(batch_id),
            data: Some(json!({ "fichier": file_name, "sha256": hash, "lignes": analysed.len() })),
            ..Default::default()
        },
    )
    .await?;
    tx.commit().await?;
    let mut conn = state.pool.acquire().await?;
    Ok(Json(load_batch(&mut conn, batch_id).await?))
}

// ------------------------------------------------------------------ consultation

async fn load_batch(conn: &mut PgConnection, id: Uuid) -> AppResult<Value> {
    let batch: Option<Value> = sqlx::query_scalar(
        "SELECT jsonb_build_object(
                'id', b.id, 'file_name', b.file_name, 'file_sha256', b.file_sha256, 'status', b.status,
                'format_version', b.format_version, 'summary', b.summary, 'created_at', b.created_at,
                'created_by', u.display_name, 'committed_at', b.committed_at,
                'already_applied', (SELECT jsonb_build_object('batch_id', o.id, 'committed_at', o.committed_at, 'file_name', o.file_name)
                                      FROM import_batches o
                                     WHERE o.file_sha256 = b.file_sha256 AND o.status = 'applique' AND o.id <> b.id LIMIT 1))
           FROM import_batches b JOIN users u ON u.id = b.created_by WHERE b.id = $1",
    )
    .bind(id)
    .fetch_optional(&mut *conn)
    .await?;
    let batch = batch.ok_or_else(|| AppError::not_found("lot d'import"))?;
    let rows: Value = sqlx::query_scalar(
        "SELECT coalesce(jsonb_agg(jsonb_build_object(
                'row_number', r.row_number, 'raw', r.raw, 'proposal', r.proposal, 'messages', r.messages,
                'employee_id', r.employee_id, 'candidates', r.candidates, 'changes', r.changes,
                'decision', r.decision, 'decision_employee_id', r.decision_employee_id,
                'result', r.result, 'result_employee_id', r.result_employee_id, 'result_message', r.result_message)
                ORDER BY r.row_number), '[]')
           FROM import_rows r WHERE r.batch_id = $1",
    )
    .bind(id)
    .fetch_one(&mut *conn)
    .await?;
    let counts: Value = sqlx::query_scalar(
        "SELECT coalesce(jsonb_object_agg(proposal, n), '{}') FROM (SELECT proposal, count(*) n FROM import_rows WHERE batch_id = $1 GROUP BY proposal) x",
    )
    .bind(id)
    .fetch_one(&mut *conn)
    .await?;
    Ok(json!({ "batch": batch, "counts": counts, "rows": rows }))
}

async fn batch_detail(State(state): State<AppState>, user: CurrentUser, Path(id): Path<Uuid>) -> AppResult<Json<Value>> {
    user.require(domain::roles::HR_MANAGERS)?;
    let mut conn = state.pool.acquire().await?;
    Ok(Json(load_batch(&mut conn, id).await?))
}

#[derive(Serialize, FromRow)]
struct BatchRow {
    id: Uuid,
    file_name: String,
    status: String,
    created_at: DateTime<Utc>,
    created_by: String,
    summary: Option<Value>,
}

async fn list_batches(State(state): State<AppState>, user: CurrentUser) -> AppResult<Json<Vec<BatchRow>>> {
    user.require(domain::roles::HR_MANAGERS)?;
    let rows = sqlx::query_as::<_, BatchRow>(
        "SELECT b.id, b.file_name, b.status, b.created_at, u.display_name AS created_by, b.summary
           FROM import_batches b JOIN users u ON u.id = b.created_by ORDER BY b.created_at DESC LIMIT 30",
    )
    .fetch_all(&state.pool)
    .await?;
    Ok(Json(rows))
}

// ------------------------------------------------------------------ décisions

#[derive(Deserialize)]
struct DecisionInput {
    /// creer | rattacher | exclure ; null pour annuler la décision.
    decision: Option<String>,
    employee_id: Option<Uuid>,
}

async fn decide(
    State(state): State<AppState>,
    user: CurrentUser,
    Path((id, row)): Path<(Uuid, i32)>,
    Json(input): Json<DecisionInput>,
) -> AppResult<Json<Value>> {
    user.require(domain::roles::HR_MANAGERS)?;
    let mut tx = state.pool.begin().await?;
    let status: Option<String> = sqlx::query_scalar("SELECT status FROM import_batches WHERE id = $1 FOR UPDATE")
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?;
    match status.as_deref() {
        None => return Err(AppError::not_found("lot d'import")),
        Some("apercu") => {}
        Some(_) => return Err(AppError::conflict("lot déjà appliqué ou abandonné : relancez un aperçu")),
    }
    let r: Option<(String, Option<Uuid>, Value)> = sqlx::query_as(
        "SELECT proposal, employee_id, candidates FROM import_rows WHERE batch_id = $1 AND row_number = $2",
    )
    .bind(id)
    .bind(row)
    .fetch_optional(&mut *tx)
    .await?;
    let (proposal, matched, candidates) = r.ok_or_else(|| AppError::not_found("ligne"))?;
    if proposal == "rejet" {
        return Err(AppError::invalid("decision", "ligne rejetée : corrigez le fichier puis relancez un aperçu"));
    }
    let mut version: Option<i32> = None;
    match input.decision.as_deref() {
        None | Some("exclure") => {}
        Some("creer") => {
            if !matches!(proposal.as_str(), "creation" | "a_revoir") || matched.is_some() {
                return Err(AppError::invalid("decision", "« nouvelle personne » impossible : cette ligne correspond à une fiche existante"));
            }
        }
        Some("rattacher") => {
            let target = input.employee_id.ok_or_else(|| AppError::invalid("employee_id", "choisissez la fiche à rattacher"))?;
            let allowed = candidates
                .as_array()
                .map(|c| c.iter().any(|x| x["id"].as_str() == Some(&target.to_string())))
                .unwrap_or(false);
            if !allowed {
                return Err(AppError::invalid("employee_id", "fiche non proposée pour cette ligne"));
            }
            version = sqlx::query_scalar("SELECT version FROM employees WHERE id = $1").bind(target).fetch_optional(&mut *tx).await?;
        }
        Some(_) => return Err(AppError::invalid("decision", "décision inconnue")),
    }
    sqlx::query(
        "UPDATE import_rows SET decision = $3, decision_employee_id = $4,
                employee_version = CASE WHEN $3 = 'rattacher' THEN $5 ELSE employee_version END,
                decided_by = $6, decided_at = now()
          WHERE batch_id = $1 AND row_number = $2",
    )
    .bind(id)
    .bind(row)
    .bind(&input.decision)
    .bind(if input.decision.as_deref() == Some("rattacher") { input.employee_id } else { None })
    .bind(version)
    .bind(user.id)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(Json(json!({ "ok": true })))
}

// ------------------------------------------------------------------ application

/// Applique les lignes valides en une transaction. Rejouer la commande
/// renvoie le même résultat ; un classeur déjà appliqué est refusé.
async fn commit(State(state): State<AppState>, user: CurrentUser, Path(id): Path<Uuid>) -> AppResult<Json<Value>> {
    user.require(domain::roles::HR_MANAGERS)?;
    let mut tx = state.pool.begin().await?;
    let b: Option<(String, String)> = sqlx::query_as("SELECT status, file_sha256 FROM import_batches WHERE id = $1 FOR UPDATE")
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?;
    let (status, hash) = b.ok_or_else(|| AppError::not_found("lot d'import"))?;
    if status == "applique" {
        // Double clic ou rejeu : rien n'est réappliqué.
        tx.commit().await?;
        let mut conn = state.pool.acquire().await?;
        return Ok(Json(load_batch(&mut conn, id).await?));
    }
    if status != "apercu" {
        return Err(AppError::conflict("lot abandonné : relancez un aperçu"));
    }
    let previous: Option<DateTime<Utc>> = sqlx::query_scalar(
        "SELECT committed_at FROM import_batches WHERE file_sha256 = $1 AND status = 'applique' AND id <> $2",
    )
    .bind(&hash)
    .bind(id)
    .fetch_optional(&mut *tx)
    .await?
    .flatten();
    if let Some(at) = previous {
        return Err(AppError::conflict(format!(
            "ce classeur a déjà été importé le {} : rien n'est réappliqué",
            crate::documents::fr_datetime(at)
        )));
    }

    type Pending = (i32, Value, String, Option<Uuid>, Option<i32>, Option<String>, Option<Uuid>);
    let rows: Vec<Pending> = sqlx::query_as(
        "SELECT row_number, raw, proposal, employee_id, employee_version, decision, decision_employee_id
           FROM import_rows WHERE batch_id = $1 ORDER BY row_number",
    )
    .bind(id)
    .fetch_all(&mut *tx)
    .await?;
    let today = algiers_today();
    let mut counts: HashMap<&'static str, i64> = HashMap::new();
    for (row_number, raw, proposal, matched, version, decision, decision_emp) in rows {
        let data: RowData = serde_json::from_value(raw).map_err(anyhow::Error::from)?;
        let action = match (proposal.as_str(), decision.as_deref()) {
            ("rejet", _) => Action::Skip("rejete", "ligne rejetée à l'aperçu"),
            (_, Some("exclure")) => Action::Skip("ignore", "exclue par le RH"),
            ("a_revoir", None) => Action::Skip("ignore", "ambiguë sans décision : non appliquée"),
            ("inchange", None) => Action::Skip("inchange", "aucune différence"),
            (_, Some("creer")) | ("creation", None) => Action::Create,
            (_, Some("rattacher")) => Action::Update(decision_emp.expect("fiche rattachée"), version),
            ("mise_a_jour", None) => Action::Update(matched.expect("fiche trouvée"), version),
            _ => Action::Skip("ignore", "cas non prévu : non appliquée"),
        };
        let (result, employee, message): (&'static str, Option<Uuid>, Option<String>) = match action {
            Action::Skip(r, m) => (r, None, Some(m.to_string())),
            Action::Create | Action::Update(..) => {
                sqlx::query("SAVEPOINT import_ligne").execute(&mut *tx).await?;
                let outcome = match action {
                    Action::Create => create(&mut tx, &user, &data, today).await.map(|e| ("cree", e)),
                    Action::Update(emp, v) => update(&mut tx, &user, emp, v, &data, today)
                        .await
                        .map(|changed| (if changed { "mis_a_jour" } else { "inchange" }, emp)),
                    Action::Skip(..) => unreachable!(),
                };
                match outcome {
                    Ok((r, e)) => {
                        sqlx::query("RELEASE SAVEPOINT import_ligne").execute(&mut *tx).await?;
                        (r, Some(e), None)
                    }
                    Err(e) => {
                        sqlx::query("ROLLBACK TO SAVEPOINT import_ligne").execute(&mut *tx).await?;
                        ("conflit", None, Some(e.to_string()))
                    }
                }
            }
        };
        *counts.entry(result).or_default() += 1;
        sqlx::query(
            "UPDATE import_rows SET result = $3, result_employee_id = $4, result_message = $5 WHERE batch_id = $1 AND row_number = $2",
        )
        .bind(id)
        .bind(row_number)
        .bind(result)
        .bind(employee)
        .bind(&message)
        .execute(&mut *tx)
        .await?;
    }
    let summary = json!(counts);
    sqlx::query(
        "UPDATE import_batches SET status = 'applique', summary = $2, committed_at = now(), committed_by = $3 WHERE id = $1",
    )
    .bind(id)
    .bind(&summary)
    .bind(user.id)
    .execute(&mut *tx)
    .await?;
    audit::record(
        &mut tx,
        &user,
        Audit {
            action: "import_personnel_applique",
            entity: "import_batch",
            entity_id: Some(id),
            data: Some(json!({ "sha256": hash, "resultat": summary })),
            ..Default::default()
        },
    )
    .await?;
    tx.commit().await?;
    let mut conn = state.pool.acquire().await?;
    Ok(Json(load_batch(&mut conn, id).await?))
}

enum Action {
    Skip(&'static str, &'static str),
    Create,
    Update(Uuid, Option<i32>),
}

/// Fiche créée par reprise : personnel existant (actif à sa date réelle)
/// ou à venir. Aucun dossier, aucune tâche, aucun avis.
async fn create(conn: &mut PgConnection, user: &CurrentUser, d: &RowData, today: NaiveDate) -> AppResult<Uuid> {
    let arrival = d.arrival.ok_or_else(|| AppError::invalid("arrival", "date d'arrivée manquante"))?;
    let present = arrival <= today;
    let id = insert_employee(
        conn,
        &NewEmployee {
            first_name: d.first_name.clone().unwrap_or_default(),
            last_name: d.last_name.clone().unwrap_or_default(),
            matricule: d.matricule.clone(),
            job_title: d.job_title.clone().unwrap_or_default(),
            site_id: d.site_id.ok_or_else(|| AppError::invalid("site", "site manquant"))?,
            department: d.department.clone(),
            manager: None,
            status: if present { "actif" } else { "a_venir" },
            planned_arrival: (!present).then_some(arrival),
            actual_arrival: present.then_some(arrival),
        },
    )
    .await?;
    if present {
        sqlx::query("UPDATE employees SET arrival_confirmed_at = now(), arrival_confirmed_by = $2 WHERE id = $1")
            .bind(id)
            .bind(user.id)
            .execute(&mut *conn)
            .await?;
    }
    audit::record(
        conn,
        user,
        Audit { action: "import_creation_fiche", entity: "employee", entity_id: Some(id), employee_id: Some(id), ..Default::default() },
    )
    .await?;
    Ok(id)
}

/// Mise à jour d'une fiche ; une valeur vide conserve l'existant. Refusée si
/// la fiche a changé depuis l'aperçu.
async fn update(
    conn: &mut PgConnection,
    user: &CurrentUser,
    employee_id: Uuid,
    expected_version: Option<i32>,
    d: &RowData,
    today: NaiveDate,
) -> AppResult<bool> {
    let e = sqlx::query_as::<_, Emp>(&format!("{EMP_SELECT} WHERE e.id = $1 FOR UPDATE OF e"))
        .bind(employee_id)
        .fetch_optional(&mut *conn)
        .await?
        .ok_or_else(|| AppError::not_found("salarié"))?;
    if expected_version.is_some_and(|v| v != e.version) {
        return Err(AppError::conflict("fiche modifiée depuis l'aperçu : relancez un aperçu"));
    }
    let changes = diff(&e, d);
    if changes.is_empty() {
        return Ok(false);
    }
    let first = d.first_name.clone().unwrap_or(e.first_name.clone());
    let last = d.last_name.clone().unwrap_or(e.last_name.clone());
    let (matricule, norm) = match d.matricule_norm() {
        Some(n) => {
            let taken: Option<Uuid> = sqlx::query_scalar("SELECT id FROM employees WHERE matricule_norm = $1 AND id <> $2")
                .bind(&n)
                .bind(employee_id)
                .fetch_optional(&mut *conn)
                .await?;
            if taken.is_some() {
                return Err(AppError::conflict(format!("matricule {n} déjà attribué à une autre fiche")));
            }
            (d.matricule.clone(), Some(n))
        }
        None => (e.matricule.clone(), e.matricule_norm.clone()),
    };
    let (actual, planned) = match d.arrival {
        Some(a) if e.status == "a_venir" => (e.actual_arrival, Some(a)),
        Some(a) => (Some(a), e.planned_arrival),
        None => (e.actual_arrival, e.planned_arrival),
    };
    sqlx::query(
        "UPDATE employees SET first_name = $2, last_name = $3, name_key = $4, matricule = $5, matricule_norm = $6,
                actual_arrival = $7, planned_arrival = $8, version = version + 1, updated_at = now()
          WHERE id = $1",
    )
    .bind(employee_id)
    .bind(&first)
    .bind(&last)
    .bind(domain::text::name_key(&first, &last))
    .bind(&matricule)
    .bind(&norm)
    .bind(actual)
    .bind(planned)
    .execute(&mut *conn)
    .await?;

    // Poste, site, département : nouvelle situation datée (historique conservé).
    let job = d.job_title.clone().or(e.job_title.clone());
    let site = d.site_id.or(e.site_id);
    let dept = d.department.clone().or(e.department.clone());
    if job != e.job_title || site != e.site_id || dept != e.department {
        let site = site.ok_or_else(|| AppError::invalid("site", "site manquant"))?;
        let job = job.ok_or_else(|| AppError::invalid("poste", "poste manquant"))?;
        let current: Option<(Uuid, NaiveDate)> =
            sqlx::query_as("SELECT id, valid_from FROM employee_positions WHERE employee_id = $1 AND valid_to IS NULL")
                .bind(employee_id)
                .fetch_optional(&mut *conn)
                .await?;
        match current {
            Some((pid, from)) if from >= today => {
                sqlx::query("UPDATE employee_positions SET job_title = $2, site_id = $3, department = $4 WHERE id = $1")
                    .bind(pid)
                    .bind(&job)
                    .bind(site)
                    .bind(&dept)
                    .execute(&mut *conn)
                    .await?;
            }
            other => {
                if let Some((pid, _)) = other {
                    sqlx::query("UPDATE employee_positions SET valid_to = $2 WHERE id = $1")
                        .bind(pid)
                        .bind(today)
                        .execute(&mut *conn)
                        .await?;
                }
                sqlx::query(
                    "INSERT INTO employee_positions (employee_id, job_title, site_id, department, valid_from) VALUES ($1, $2, $3, $4, $5)",
                )
                .bind(employee_id)
                .bind(&job)
                .bind(site)
                .bind(&dept)
                .bind(today)
                .execute(&mut *conn)
                .await?;
            }
        }
    }
    audit::record(
        conn,
        user,
        Audit {
            action: "import_mise_a_jour_fiche",
            entity: "employee",
            entity_id: Some(employee_id),
            employee_id: Some(employee_id),
            data: Some(Value::Object(changes)),
            ..Default::default()
        },
    )
    .await?;
    Ok(true)
}
