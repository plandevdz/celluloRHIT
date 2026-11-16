//! Étiquettes QR et campagnes d'inventaire (M03, U3, T03). Un pointage
//! enregistre qui, où et quand ; il ne transfère jamais la garde. La
//! rectification du parc est une commande distincte (PATCH /assets).

use axum::body::Body;
use axum::extract::{Path, Query, State};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use domain::inventory::{classify, Known, ScanResult};
use domain::Role;
use rust_xlsxwriter::{Color, Format, Workbook};
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::audit::{self, Audit};
use crate::auth::CurrentUser;
use crate::error::{AppError, AppResult};
use crate::labels;
use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/assets/labels.pdf", get(labels_pdf))
        .route("/inventories", get(list).post(create))
        .route("/inventories/{id}", get(detail))
        .route("/inventories/{id}/scan", post(scan))
        .route("/inventories/{id}/report", get(report))
        .route("/inventories/{id}/report.xlsx", get(report_xlsx))
        .route("/inventories/{id}/arbitrations", post(arbitrate))
        .route("/inventories/{id}/close", post(close))
}

const PARC: &[Role] = &[Role::It, Role::Admin];

// ------------------------------------------------------------------ étiquettes

#[derive(Deserialize)]
struct LabelQuery {
    /// Identifiants séparés par des virgules.
    ids: String,
    format: Option<String>,
    start: Option<u32>,
}

async fn labels_pdf(State(state): State<AppState>, user: CurrentUser, Query(q): Query<LabelQuery>) -> AppResult<Response> {
    user.require(PARC)?;
    let ids: Vec<Uuid> = q
        .ids
        .split(',')
        .filter(|s| !s.trim().is_empty())
        .map(|s| Uuid::parse_str(s.trim()).map_err(|_| AppError::invalid("ids", "identifiant d'actif invalide")))
        .collect::<AppResult<_>>()?;
    if ids.is_empty() || ids.len() > 500 {
        return Err(AppError::invalid("ids", "de 1 à 500 actifs par impression"));
    }
    let fmt_name = q.format.unwrap_or_else(|| "a4-24".into());
    let fmt = labels::format(&fmt_name).ok_or_else(|| AppError::invalid("format", "format a4-24, a4-65 ou unitaire"))?;
    let rows: Vec<(String, String, String)> = sqlx::query_as(
        "SELECT qr_token, inventory_no, coalesce(nullif(trim(coalesce(brand, '') || ' ' || coalesce(model, '')), ''), label)
           FROM assets WHERE id = ANY($1) ORDER BY array_position($1, id)",
    )
    .bind(&ids)
    .fetch_all(&state.pool)
    .await?;
    if rows.len() != ids.len() {
        return Err(AppError::not_found("actif"));
    }
    let list: Vec<labels::Label> = rows.into_iter().map(|(token, inventory_no, text)| labels::Label { token, inventory_no, text }).collect();
    let count = list.len();
    let config = state.cfg();
    let start = q.start.unwrap_or(1);
    let pdf = tokio::task::spawn_blocking(move || labels::render(&list, fmt, start, &config.pdf_font, &config.pdf_font_bold))
        .await
        .map_err(anyhow::Error::from)??;
    let mut conn = state.pool.acquire().await?;
    audit::record(&mut conn, &user, Audit { action: "etiquettes_imprimees", entity: "asset", data: Some(json!({ "nombre": count, "format": fmt_name })), ..Default::default() }).await?;
    Ok((
        [
            (header::CONTENT_TYPE, "application/pdf".to_string()),
            (header::CONTENT_DISPOSITION, "inline; filename=\"etiquettes.pdf\"".to_string()),
        ],
        Body::from(pdf),
    )
        .into_response())
}

// ------------------------------------------------------------------ campagnes

async fn list(State(state): State<AppState>, user: CurrentUser) -> AppResult<Json<Value>> {
    user.require(PARC)?;
    let v: Value = sqlx::query_scalar(
        "SELECT coalesce(jsonb_agg(jsonb_build_object('id', c.id, 'reference', c.reference, 'name', c.name, 'status', c.status,
                    'site', s.name, 'location', l.name, 'reference_at', c.reference_at, 'closed_at', c.closed_at,
                    'expected', (SELECT count(*) FROM inventory_expected x WHERE x.campaign_id = c.id),
                    'seen', (SELECT count(DISTINCT o.asset_id) FROM inventory_observations o JOIN inventory_expected x
                               ON x.campaign_id = o.campaign_id AND x.asset_id = o.asset_id
                              WHERE o.campaign_id = c.id AND o.counted))
                    ORDER BY c.created_at DESC), '[]')
           FROM inventory_campaigns c JOIN sites s ON s.id = c.site_id LEFT JOIN locations l ON l.id = c.location_id",
    )
    .fetch_all(&state.pool)
    .await?
    .into_iter()
    .next()
    .unwrap_or(json!([]));
    Ok(Json(v))
}

#[derive(Deserialize)]
struct CreateInput {
    name: String,
    site_id: Uuid,
    location_id: Option<Uuid>,
}

/// Ouvre une campagne : le périmètre attendu et l'heure de référence sont
/// figés, pour un rapport reproductible.
async fn create(State(state): State<AppState>, user: CurrentUser, Json(i): Json<CreateInput>) -> AppResult<Json<Value>> {
    user.require(PARC)?;
    let name = domain::text::clean(&i.name).ok_or_else(|| AppError::invalid("name", "nom de campagne obligatoire"))?;
    let mut tx = state.pool.begin().await?;
    if let Some(l) = i.location_id {
        let ok: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM locations WHERE id = $1 AND site_id = $2)")
            .bind(l)
            .bind(i.site_id)
            .fetch_one(&mut *tx)
            .await?;
        if !ok {
            return Err(AppError::invalid("location_id", "le local doit appartenir au site"));
        }
    }
    let n: i64 = sqlx::query_scalar("SELECT nextval('inventory_ref')").fetch_one(&mut *tx).await?;
    let reference = format!("INV-{n:05}");
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO inventory_campaigns (reference, name, site_id, location_id, created_by) VALUES ($1, $2, $3, $4, $5) RETURNING id",
    )
    .bind(&reference)
    .bind(&name)
    .bind(i.site_id)
    .bind(i.location_id)
    .bind(user.id)
    .fetch_one(&mut *tx)
    .await?;
    let expected = sqlx::query(
        "INSERT INTO inventory_expected (campaign_id, asset_id, expected_location_id, holder_employee_id, asset_status)
         SELECT $1, a.id, a.location_id, g.employee_id, a.status
           FROM assets a LEFT JOIN assignments g ON g.asset_id = a.id AND g.ended_at IS NULL
          WHERE a.site_id = $2 AND ($3::uuid IS NULL OR a.location_id = $3) AND a.status <> 'reforme'",
    )
    .bind(id)
    .bind(i.site_id)
    .bind(i.location_id)
    .execute(&mut *tx)
    .await?
    .rows_affected();
    audit::record(&mut tx, &user, Audit { action: "inventaire_ouvert", entity: "inventory", entity_id: Some(id), data: Some(json!({ "reference": reference, "attendus": expected })), ..Default::default() }).await?;
    tx.commit().await?;
    Ok(Json(json!({ "id": id, "reference": reference, "expected": expected })))
}

async fn counters(conn: &mut sqlx::PgConnection, id: Uuid) -> AppResult<Value> {
    Ok(sqlx::query_scalar(
        "SELECT jsonb_build_object(
            'expected', (SELECT count(*) FROM inventory_expected WHERE campaign_id = $1),
            'seen_ok', (SELECT count(DISTINCT asset_id) FROM inventory_observations WHERE campaign_id = $1 AND counted AND result = 'vu'),
            'seen_elsewhere', (SELECT count(DISTINCT asset_id) FROM inventory_observations WHERE campaign_id = $1 AND counted AND result = 'ailleurs'),
            'out_of_scope', (SELECT count(DISTINCT asset_id) FROM inventory_observations WHERE campaign_id = $1 AND counted AND result = 'hors_perimetre'),
            'unknown', (SELECT count(DISTINCT scanned_code) FROM inventory_observations WHERE campaign_id = $1 AND result = 'inconnu'),
            'duplicates', (SELECT count(*) FROM inventory_observations WHERE campaign_id = $1 AND result = 'doublon'),
            'not_seen', (SELECT count(*) FROM inventory_expected x WHERE x.campaign_id = $1
                           AND NOT EXISTS (SELECT 1 FROM inventory_observations o WHERE o.campaign_id = $1 AND o.asset_id = x.asset_id AND o.counted)))",
    )
    .bind(id)
    .fetch_one(conn)
    .await?)
}

async fn detail(State(state): State<AppState>, user: CurrentUser, Path(id): Path<Uuid>) -> AppResult<Json<Value>> {
    user.require(PARC)?;
    let mut conn = state.pool.acquire().await?;
    let c: Option<Value> = sqlx::query_scalar(
        "SELECT jsonb_build_object('id', c.id, 'reference', c.reference, 'name', c.name, 'status', c.status, 'site_id', c.site_id,
                'site', s.name, 'location_id', c.location_id, 'location', l.name, 'reference_at', c.reference_at,
                'closed_at', c.closed_at, 'close_note', c.close_note,
                'locations', (SELECT coalesce(jsonb_agg(jsonb_build_object('id', x.id, 'name', x.name) ORDER BY x.name), '[]')
                                FROM locations x WHERE x.site_id = c.site_id AND x.active),
                'recent', (SELECT coalesce(jsonb_agg(r ORDER BY r->>'observed_at' DESC), '[]') FROM (
                              SELECT jsonb_build_object('scanned_code', o.scanned_code, 'result', o.result, 'observed_at', o.observed_at,
                                     'location', lo.name, 'inventory_no', a.inventory_no, 'label', a.label, 'operator', u.display_name) r
                                FROM inventory_observations o JOIN locations lo ON lo.id = o.location_id
                                LEFT JOIN assets a ON a.id = o.asset_id JOIN users u ON u.id = o.operator_id
                               WHERE o.campaign_id = c.id ORDER BY o.observed_at DESC LIMIT 20) t))
           FROM inventory_campaigns c JOIN sites s ON s.id = c.site_id LEFT JOIN locations l ON l.id = c.location_id
          WHERE c.id = $1",
    )
    .bind(id)
    .fetch_optional(&mut *conn)
    .await?;
    let mut c = c.ok_or_else(|| AppError::not_found("campagne"))?;
    c["counters"] = counters(&mut conn, id).await?;
    Ok(Json(c))
}

#[derive(Deserialize)]
struct ScanInput {
    code: String,
    location_id: Uuid,
    /// Clé générée par le poste de scan : un rejeu ne compte pas deux fois.
    command_key: String,
    #[serde(default)]
    manual: bool,
}

/// Pointage : jamais d'attribution, jamais de lien externe ; le même actif
/// au même endroit n'est compté qu'une fois.
async fn scan(State(state): State<AppState>, user: CurrentUser, Path(id): Path<Uuid>, Json(i): Json<ScanInput>) -> AppResult<Json<Value>> {
    user.require(PARC)?;
    let code = i.code.trim().to_uppercase();
    if code.is_empty() || code.len() > 200 {
        return Err(AppError::invalid("code", "code vide ou trop long"));
    }
    let key = domain::text::clean(&i.command_key).ok_or_else(|| AppError::invalid("command_key", "clé de commande obligatoire"))?;
    let mut tx = state.pool.begin().await?;
    let row: Option<(String, Uuid)> = sqlx::query_as("SELECT status, site_id FROM inventory_campaigns WHERE id = $1 FOR SHARE")
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?;
    let (status, site_id) = row.ok_or_else(|| AppError::not_found("campagne"))?;
    if status != "ouverte" {
        return Err(AppError::conflict("campagne close : plus de pointage"));
    }
    // Rejeu : même résultat, rien de nouveau.
    let replay: Option<(String, Option<Uuid>)> = sqlx::query_as("SELECT result, asset_id FROM inventory_observations WHERE campaign_id = $1 AND command_key = $2")
        .bind(id)
        .bind(&key)
        .fetch_optional(&mut *tx)
        .await?;
    if let Some((result, asset)) = replay {
        tx.commit().await?;
        let r = ScanResult::parse(&result).unwrap_or(ScanResult::Inconnu);
        return Ok(Json(json!({ "result": result, "message": r.message(), "asset_id": asset, "replayed": true })));
    }
    let loc_ok: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM locations WHERE id = $1 AND site_id = $2)")
        .bind(i.location_id)
        .bind(site_id)
        .fetch_one(&mut *tx)
        .await?;
    if !loc_ok {
        return Err(AppError::invalid("location_id", "local hors du site de la campagne"));
    }
    let asset: Option<(Uuid, String, String)> = sqlx::query_as(
        "SELECT id, inventory_no, label FROM assets WHERE upper(qr_token) = $1 OR upper(inventory_no) = $1 LIMIT 1",
    )
    .bind(&code)
    .fetch_optional(&mut *tx)
    .await?;
    let (known, asset_id, info) = match &asset {
        None => (Known::Unknown, None, Value::Null),
        Some((aid, inv, label)) => {
            let expected: Option<Option<Uuid>> =
                sqlx::query_scalar("SELECT expected_location_id FROM inventory_expected WHERE campaign_id = $1 AND asset_id = $2")
                    .bind(id)
                    .bind(aid)
                    .fetch_optional(&mut *tx)
                    .await?;
            let k = match expected {
                None => Known::OutOfScope,
                Some(loc) => Known::Expected(loc),
            };
            (k, Some(*aid), json!({ "inventory_no": inv, "label": label }))
        }
    };
    let already_here: bool = match asset_id {
        Some(a) => sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM inventory_observations WHERE campaign_id = $1 AND asset_id = $2 AND location_id = $3 AND counted)",
        )
        .bind(id)
        .bind(a)
        .bind(i.location_id)
        .fetch_one(&mut *tx)
        .await?,
        None => false,
    };
    let mut result = classify(known, &i.location_id, already_here);
    let counted = !matches!(result, ScanResult::Doublon | ScanResult::Inconnu);
    // Deux scans concurrents du même actif au même local : l'index unique
    // tranche, le second devient un doublon.
    let inserted = sqlx::query(
        "INSERT INTO inventory_observations (campaign_id, command_key, scanned_code, asset_id, location_id, operator_id, manual, result, counted)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
         ON CONFLICT (campaign_id, asset_id, location_id) WHERE counted AND asset_id IS NOT NULL DO NOTHING",
    )
    .bind(id)
    .bind(&key)
    .bind(&code)
    .bind(asset_id)
    .bind(i.location_id)
    .bind(user.id)
    .bind(i.manual)
    .bind(result.as_str())
    .bind(counted)
    .execute(&mut *tx)
    .await?
    .rows_affected();
    if inserted == 0 {
        result = ScanResult::Doublon;
        sqlx::query(
            "INSERT INTO inventory_observations (campaign_id, command_key, scanned_code, asset_id, location_id, operator_id, manual, result, counted)
             VALUES ($1, $2, $3, $4, $5, $6, $7, 'doublon', false)",
        )
        .bind(id)
        .bind(&key)
        .bind(&code)
        .bind(asset_id)
        .bind(i.location_id)
        .bind(user.id)
        .bind(i.manual)
        .execute(&mut *tx)
        .await?;
    }
    // Vu dans un autre local de cette campagne : conflit à examiner, jamais
    // une double présence validée.
    let other_locations: i64 = match asset_id {
        Some(a) => sqlx::query_scalar(
            "SELECT count(DISTINCT location_id) FROM inventory_observations WHERE campaign_id = $1 AND asset_id = $2 AND counted AND location_id <> $3",
        )
        .bind(id)
        .bind(a)
        .bind(i.location_id)
        .fetch_one(&mut *tx)
        .await?,
        None => 0,
    };
    let mut conn_counters = counters(&mut tx, id).await?;
    tx.commit().await?;
    conn_counters["last"] = json!(code);
    Ok(Json(json!({
        "result": result,
        "message": if other_locations > 0 && result != ScanResult::Doublon {
            "Déjà pointé dans un autre local de cette campagne : conflit à examiner"
        } else {
            result.message()
        },
        "conflict": other_locations > 0,
        "asset_id": asset_id,
        "asset": info,
        "counters": conn_counters,
    })))
}

// ------------------------------------------------------------------ rapport

const REPORT_SQL: &str = "
WITH exp AS (
    SELECT x.asset_id, x.expected_location_id, x.holder_employee_id, x.asset_status FROM inventory_expected x WHERE x.campaign_id = $1),
obs AS (
    SELECT o.* FROM inventory_observations o WHERE o.campaign_id = $1 AND o.counted),
ref AS (SELECT reference_at FROM inventory_campaigns WHERE id = $1)
SELECT jsonb_build_object(
  'vus', (SELECT coalesce(jsonb_agg(jsonb_build_object('asset_id', a.id, 'inventory_no', a.inventory_no, 'label', a.label,
              'location', l.name, 'observed_at', o.observed_at) ORDER BY a.inventory_no), '[]')
            FROM obs o JOIN assets a ON a.id = o.asset_id JOIN locations l ON l.id = o.location_id WHERE o.result = 'vu'),
  'ailleurs', (SELECT coalesce(jsonb_agg(jsonb_build_object('asset_id', a.id, 'inventory_no', a.inventory_no, 'label', a.label,
              'expected_location', el.name, 'observed_location', l.name, 'observed_location_id', l.id, 'observed_at', o.observed_at)
              ORDER BY a.inventory_no), '[]')
            FROM obs o JOIN assets a ON a.id = o.asset_id JOIN locations l ON l.id = o.location_id
            JOIN exp ON exp.asset_id = o.asset_id LEFT JOIN locations el ON el.id = exp.expected_location_id
           WHERE o.result = 'ailleurs'),
  'non_vus', (SELECT coalesce(jsonb_agg(jsonb_build_object('asset_id', a.id, 'inventory_no', a.inventory_no, 'label', a.label,
              'expected_location', el.name, 'status', exp.asset_status,
              'holder', (SELECT e.first_name || ' ' || e.last_name FROM employees e WHERE e.id = exp.holder_employee_id))
              ORDER BY a.inventory_no), '[]')
            FROM exp JOIN assets a ON a.id = exp.asset_id LEFT JOIN locations el ON el.id = exp.expected_location_id
           WHERE NOT EXISTS (SELECT 1 FROM obs WHERE obs.asset_id = exp.asset_id)),
  'hors_perimetre', (SELECT coalesce(jsonb_agg(jsonb_build_object('asset_id', a.id, 'inventory_no', a.inventory_no, 'label', a.label,
              'site', s.name, 'observed_location', l.name) ORDER BY a.inventory_no), '[]')
            FROM obs o JOIN assets a ON a.id = o.asset_id JOIN locations l ON l.id = o.location_id LEFT JOIN sites s ON s.id = a.site_id
           WHERE o.result = 'hors_perimetre'),
  'inconnus', (SELECT coalesce(jsonb_agg(jsonb_build_object('code', t.scanned_code, 'location', t.name, 'observed_at', t.at) ORDER BY t.scanned_code), '[]')
            FROM (SELECT o.scanned_code, min(l.name) AS name, min(o.observed_at) AS at FROM inventory_observations o
                    JOIN locations l ON l.id = o.location_id WHERE o.campaign_id = $1 AND o.result = 'inconnu' GROUP BY o.scanned_code) t),
  'plusieurs_locaux', (SELECT coalesce(jsonb_agg(jsonb_build_object('asset_id', a.id, 'inventory_no', a.inventory_no,
              'locations', t.locs) ORDER BY a.inventory_no), '[]')
            FROM (SELECT asset_id, jsonb_agg(DISTINCT l.name) AS locs FROM obs JOIN locations l ON l.id = obs.location_id
                   GROUP BY asset_id HAVING count(DISTINCT location_id) > 1) t JOIN assets a ON a.id = t.asset_id),
  'mouvements_depuis_reference', (SELECT coalesce(jsonb_agg(DISTINCT jsonb_build_object('asset_id', a.id, 'inventory_no', a.inventory_no,
              'movement', m.reference, 'kind', m.kind, 'at', m.effective_at)), '[]')
            FROM exp JOIN assets a ON a.id = exp.asset_id JOIN movement_lines ml ON ml.asset_id = a.id
            JOIN movements m ON m.id = ml.movement_id, ref WHERE m.effective_at > ref.reference_at),
  'arbitrages', (SELECT coalesce(jsonb_agg(jsonb_build_object('asset_id', r.asset_id, 'observation_id', r.observation_id,
              'decision', r.decision, 'note', r.note, 'by', u.display_name, 'at', r.decided_at) ORDER BY r.decided_at), '[]')
            FROM inventory_arbitrations r JOIN users u ON u.id = r.decided_by WHERE r.campaign_id = $1)
)";

async fn report_value(state: &AppState, id: Uuid) -> AppResult<Value> {
    let mut conn = state.pool.acquire().await?;
    let exists: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM inventory_campaigns WHERE id = $1)").bind(id).fetch_one(&mut *conn).await?;
    if !exists {
        return Err(AppError::not_found("campagne"));
    }
    let mut v: Value = sqlx::query_scalar(REPORT_SQL).bind(id).fetch_one(&mut *conn).await?;
    v["counters"] = counters(&mut conn, id).await?;
    Ok(v)
}

async fn report(State(state): State<AppState>, user: CurrentUser, Path(id): Path<Uuid>) -> AppResult<Json<Value>> {
    user.require(PARC)?;
    Ok(Json(report_value(&state, id).await?))
}

fn sheet(wb: &mut Workbook, name: &str, headers: &[&str], rows: Vec<Vec<String>>) -> Result<(), rust_xlsxwriter::XlsxError> {
    let ws = wb.add_worksheet();
    ws.set_name(name)?;
    let head = Format::new().set_bold().set_font_color(Color::White).set_background_color(Color::RGB(0x194667));
    let text = Format::new().set_num_format("@");
    for (c, h) in headers.iter().enumerate() {
        ws.write_string_with_format(0, c as u16, *h, &head)?;
        ws.set_column_width(c as u16, 24)?;
    }
    for (r, row) in rows.iter().enumerate() {
        for (c, v) in row.iter().enumerate() {
            ws.write_string_with_format(r as u32 + 1, c as u16, v, &text)?;
        }
    }
    ws.set_freeze_panes(1, 0)?;
    Ok(())
}

fn s(v: &Value, k: &str) -> String {
    match &v[k] {
        Value::Null => String::new(),
        Value::String(x) => x.clone(),
        other => other.to_string(),
    }
}

/// Rapport Excel : vus au bon endroit, vus ailleurs, non vus (à vérifier),
/// inconnus, hors périmètre, conflits et mouvements postérieurs.
async fn report_xlsx(State(state): State<AppState>, user: CurrentUser, Path(id): Path<Uuid>) -> AppResult<Response> {
    user.require(PARC)?;
    let r = report_value(&state, id).await?;
    let reference: String = sqlx::query_scalar("SELECT reference FROM inventory_campaigns WHERE id = $1").bind(id).fetch_one(&state.pool).await?;
    let rows = |k: &str, cols: &[&str]| -> Vec<Vec<String>> {
        r[k].as_array().cloned().unwrap_or_default().iter().map(|x| cols.iter().map(|c| s(x, c)).collect()).collect()
    };
    let mut wb = Workbook::new();
    let build = (|| -> Result<Vec<u8>, rust_xlsxwriter::XlsxError> {
        sheet(&mut wb, "Vus au bon endroit", &["Inventaire", "Libellé", "Local", "Pointé le"], rows("vus", &["inventory_no", "label", "location", "observed_at"]))?;
        sheet(&mut wb, "Vus ailleurs", &["Inventaire", "Libellé", "Local attendu", "Local observé", "Pointé le"], rows("ailleurs", &["inventory_no", "label", "expected_location", "observed_location", "observed_at"]))?;
        sheet(&mut wb, "Non vus (à vérifier)", &["Inventaire", "Libellé", "Local attendu", "État", "Détenteur"], rows("non_vus", &["inventory_no", "label", "expected_location", "status", "holder"]))?;
        sheet(&mut wb, "Inconnus", &["Code lu", "Local", "Pointé le"], rows("inconnus", &["code", "location", "observed_at"]))?;
        sheet(&mut wb, "Hors périmètre", &["Inventaire", "Libellé", "Site du parc", "Local observé"], rows("hors_perimetre", &["inventory_no", "label", "site", "observed_location"]))?;
        sheet(&mut wb, "Plusieurs locaux", &["Inventaire", "Locaux"], rows("plusieurs_locaux", &["inventory_no", "locations"]))?;
        sheet(&mut wb, "Mouvements postérieurs", &["Inventaire", "Mouvement", "Nature", "Date"], rows("mouvements_depuis_reference", &["inventory_no", "movement", "kind", "at"]))?;
        wb.save_to_buffer()
    })();
    let bytes = build.map_err(|e| anyhow::anyhow!("rapport Excel : {e}"))?;
    Ok((
        [
            (header::CONTENT_TYPE, "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet".to_string()),
            (header::CONTENT_DISPOSITION, format!("attachment; filename=\"{reference}-rapport.xlsx\"")),
        ],
        Body::from(bytes),
    )
        .into_response())
}

#[derive(Deserialize)]
struct ArbitrationInput {
    asset_id: Option<Uuid>,
    observation_code: Option<String>,
    decision: String,
    note: String,
}

/// Arbitrage d'un écart (à rechercher, expliqué, parc rectifié). Ne
/// modifie pas le parc : la rectification est une commande distincte.
async fn arbitrate(State(state): State<AppState>, user: CurrentUser, Path(id): Path<Uuid>, Json(i): Json<ArbitrationInput>) -> AppResult<Json<Value>> {
    user.require(PARC)?;
    if !["a_rechercher", "explique", "parc_rectifie"].contains(&i.decision.as_str()) {
        return Err(AppError::invalid("decision", "décision inconnue"));
    }
    let note = domain::text::clean(&i.note).ok_or_else(|| AppError::invalid("note", "motif obligatoire"))?;
    let mut tx = state.pool.begin().await?;
    let observation: Option<Uuid> = match &i.observation_code {
        Some(code) => sqlx::query_scalar("SELECT id FROM inventory_observations WHERE campaign_id = $1 AND scanned_code = upper($2) ORDER BY observed_at LIMIT 1")
            .bind(id)
            .bind(code)
            .fetch_optional(&mut *tx)
            .await?,
        None => None,
    };
    if i.asset_id.is_none() && observation.is_none() {
        return Err(AppError::invalid("asset_id", "précisez l'actif ou le code concerné"));
    }
    let arb: Uuid = sqlx::query_scalar(
        "INSERT INTO inventory_arbitrations (campaign_id, asset_id, observation_id, decision, note, decided_by)
         VALUES ($1, $2, $3, $4, $5, $6) RETURNING id",
    )
    .bind(id)
    .bind(i.asset_id)
    .bind(observation)
    .bind(&i.decision)
    .bind(&note)
    .bind(user.id)
    .fetch_one(&mut *tx)
    .await?;
    audit::record(&mut tx, &user, Audit { action: "inventaire_arbitrage", entity: "inventory", entity_id: Some(id), data: Some(json!({ "actif": i.asset_id, "decision": i.decision, "note": note })), ..Default::default() }).await?;
    tx.commit().await?;
    Ok(Json(json!({ "id": arb })))
}

#[derive(Deserialize, Default)]
struct CloseInput {
    note: Option<String>,
}

async fn close(State(state): State<AppState>, user: CurrentUser, Path(id): Path<Uuid>, body: Option<Json<CloseInput>>) -> AppResult<Json<Value>> {
    user.require(PARC)?;
    let note = body.and_then(|Json(i)| i.note).as_deref().and_then(domain::text::clean);
    let mut tx = state.pool.begin().await?;
    let n = sqlx::query(
        "UPDATE inventory_campaigns SET status = 'close', closed_at = now(), closed_by = $2, close_note = $3 WHERE id = $1 AND status = 'ouverte'",
    )
    .bind(id)
    .bind(user.id)
    .bind(&note)
    .execute(&mut *tx)
    .await?
    .rows_affected();
    if n == 0 {
        return Err(AppError::conflict("campagne introuvable ou déjà close"));
    }
    audit::record(&mut tx, &user, Audit { action: "inventaire_clos", entity: "inventory", entity_id: Some(id), data: Some(json!({ "note": note })), ..Default::default() }).await?;
    tx.commit().await?;
    Ok(Json(json!({ "ok": true, "status": StatusCode::OK.as_u16() })))
}
