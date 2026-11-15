//! États des lieux (M07) : chiffres et listes, filtrés côté serveur selon
//! le rôle, exportables en Excel. Aucune donnée de paie.

use axum::body::Body;
use axum::extract::State;
use axum::http::header;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use domain::Role;
use rust_xlsxwriter::{Color, Format, Workbook};
use serde_json::{json, Value};

use crate::audit::{self, Audit};
use crate::auth::CurrentUser;
use crate::error::AppResult;
use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new().route("/reports/overview", get(overview)).route("/reports/export.xlsx", get(export))
}

struct Scope {
    hr: bool,
    it: bool,
}

fn scope(user: &CurrentUser) -> AppResult<Scope> {
    user.require(&[Role::Rh, Role::Admin, Role::It])?;
    Ok(Scope { hr: user.is_hr(), it: user.has_any(&[Role::It, Role::Admin]) })
}

async fn q(state: &AppState, sql: &str) -> AppResult<Value> {
    Ok(sqlx::query_scalar(sql).bind(crate::documents::algiers_today()).fetch_one(&state.pool).await?)
}

const EMPLOYEES: &str = "SELECT jsonb_build_object(
    'a_venir', count(*) FILTER (WHERE status = 'a_venir'), 'actif', count(*) FILTER (WHERE status = 'actif'),
    'sorti', count(*) FILTER (WHERE status = 'sorti'), 'presence_a_confirmer', count(*) FILTER (WHERE status = 'a_venir' AND planned_arrival <= $1))
    FROM employees";
const CASES: &str = "SELECT coalesce(jsonb_object_agg(kind, n), '{}') FROM (SELECT kind, count(*) n FROM cases WHERE status = 'ouvert' AND $1::date IS NOT NULL GROUP BY kind) t";
const TASKS: &str = "SELECT coalesce(jsonb_agg(jsonb_build_object('role', role, 'sans_responsable', a, 'en_retard', r) ORDER BY role), '[]') FROM (
    SELECT role, count(*) FILTER (WHERE status = 'a_faire') a,
           count(*) FILTER (WHERE status IN ('a_faire', 'en_cours', 'bloque') AND (due_date < $1 OR due_at < now())) r
      FROM tasks WHERE status IN ('a_faire', 'en_cours', 'bloque') GROUP BY role) t";
const LATE_TASKS: &str = "SELECT coalesce(jsonb_agg(jsonb_build_object('reference', c.reference, 'title', t.title, 'role', t.role,
        'owner', u.display_name, 'due', coalesce(to_char(t.due_at AT TIME ZONE 'Africa/Algiers', 'DD/MM/YYYY HH24:MI'), to_char(t.due_date, 'DD/MM/YYYY')),
        'salarie', e.first_name || ' ' || e.last_name) ORDER BY t.due_date), '[]')
      FROM tasks t JOIN cases c ON c.id = t.case_id JOIN employees e ON e.id = c.employee_id LEFT JOIN users u ON u.id = t.owner_id
     WHERE t.status IN ('a_faire', 'en_cours', 'bloque') AND (t.due_date < $1 OR t.due_at < now())";
const ACCESS: &str = "SELECT coalesce(jsonb_agg(jsonb_build_object('reference', c.reference, 'salarie', e.first_name || ' ' || e.last_name,
        'acces', x.label, 'etat', x.state, 'coupure', to_char(d.cutoff_at AT TIME ZONE 'Africa/Algiers', 'DD/MM/YYYY HH24:MI'),
        'en_retard', d.cutoff_at < now()) ORDER BY d.cutoff_at), '[]')
      FROM exit_controls x JOIN departures d ON d.id = x.departure_id JOIN cases c ON c.id = d.case_id JOIN employees e ON e.id = d.employee_id
     WHERE x.kind = 'compte' AND x.state IN ('a_traiter', 'ecart') AND d.status <> 'annule' AND $1::date IS NOT NULL";
const RETURNS: &str = "SELECT coalesce(jsonb_agg(jsonb_build_object('reference', c.reference, 'salarie', e.first_name || ' ' || e.last_name,
        'bien', x.label, 'etat', x.state, 'suivi', to_char(x.follow_up_date, 'DD/MM/YYYY'), 'note', x.note) ORDER BY c.reference), '[]')
      FROM exit_controls x JOIN departures d ON d.id = x.departure_id JOIN cases c ON c.id = d.case_id JOIN employees e ON e.id = d.employee_id
     WHERE x.kind = ANY($2) AND x.state IN ('a_traiter', 'ecart') AND d.status <> 'annule' AND $1::date IS NOT NULL";
const SIGNATURES: &str = "SELECT coalesce(jsonb_agg(jsonb_build_object('mouvement', m.reference, 'nature', d.kind, 'salarie', e.first_name || ' ' || e.last_name,
        'dossier', c.reference, 'date', to_char(m.effective_at AT TIME ZONE 'Africa/Algiers', 'DD/MM/YYYY')) ORDER BY m.effective_at), '[]')
      FROM documents d JOIN movements m ON m.id = d.movement_id JOIN employees e ON e.id = m.employee_id LEFT JOIN cases c ON c.id = m.case_id
     WHERE d.status <> 'signe' AND $1::date IS NOT NULL";
const LICENSES: &str = "SELECT coalesce(jsonb_agg(jsonb_build_object('logiciel', s.vendor || ' ' || s.product, 'sieges', s.seats,
        'occupes', (SELECT count(*) FROM license_allocations a WHERE a.subscription_id = s.id
                      AND (a.ended_at IS NULL OR a.seat_free_at IS NULL OR a.seat_free_at > now())),
        'echeance', to_char(s.end_date, 'DD/MM/YYYY'), 'proche', s.end_date <= $1::date + 30) ORDER BY s.vendor), '[]')
      FROM software_subscriptions s WHERE s.active";
const HOLDINGS: &str = "SELECT coalesce(jsonb_agg(jsonb_build_object('salarie', e.first_name || ' ' || e.last_name, 'statut', e.status,
        'bien', ml.label, 'identifiant', ml.identifier, 'quantite', g.open_quantity, 'depuis', to_char(g.started_at AT TIME ZONE 'Africa/Algiers', 'DD/MM/YYYY'),
        'origine', m.reference) ORDER BY e.last_name, ml.label), '[]')
      FROM assignments g JOIN employees e ON e.id = g.employee_id JOIN movement_lines ml ON ml.id = g.handover_line_id
      JOIN movements m ON m.id = ml.movement_id WHERE g.ended_at IS NULL AND $1::date IS NOT NULL";
const VEHICLES: &str = "SELECT coalesce(jsonb_agg(jsonb_build_object('immatriculation', v.plate, 'type', v.kind, 'etat', v.status,
        'titulaire', (SELECT e.first_name || ' ' || e.last_name FROM vehicle_assignments a JOIN employees e ON e.id = a.employee_id
                       WHERE a.vehicle_id = v.id AND a.ended_at IS NULL),
        'km', v.odometer_km) ORDER BY v.plate), '[]')
      FROM vehicles v WHERE v.status NOT IN ('rendu_loueur', 'reforme') AND $1::date IS NOT NULL";
const INVENTORY: &str = "SELECT coalesce(jsonb_agg(jsonb_build_object('reference', c.reference, 'nom', c.name, 'etat', c.status,
        'attendus', (SELECT count(*) FROM inventory_expected x WHERE x.campaign_id = c.id),
        'non_vus', (SELECT count(*) FROM inventory_expected x WHERE x.campaign_id = c.id
                     AND NOT EXISTS (SELECT 1 FROM inventory_observations o WHERE o.campaign_id = c.id AND o.asset_id = x.asset_id AND o.counted)),
        'ailleurs', (SELECT count(DISTINCT asset_id) FROM inventory_observations o WHERE o.campaign_id = c.id AND o.counted AND o.result = 'ailleurs'),
        'inconnus', (SELECT count(DISTINCT scanned_code) FROM inventory_observations o WHERE o.campaign_id = c.id AND o.result = 'inconnu'))
        ORDER BY c.created_at DESC), '[]')
      FROM (SELECT * FROM inventory_campaigns ORDER BY created_at DESC LIMIT 5) c WHERE $1::date IS NOT NULL";

/// Retours manquants : le véhicule relève du RH, pas de l'IT.
async fn returns(state: &AppState, s: &Scope) -> AppResult<Value> {
    let kinds: Vec<&str> = if s.hr { vec!["bien", "vehicule", "licence"] } else { vec!["bien", "licence"] };
    Ok(sqlx::query_scalar(RETURNS).bind(crate::documents::algiers_today()).bind(&kinds).fetch_one(&state.pool).await?)
}

async fn overview(State(state): State<AppState>, user: CurrentUser) -> AppResult<Json<Value>> {
    let s = scope(&user)?;
    let mut out = json!({ "employees": q(&state, EMPLOYEES).await?, "tasks": q(&state, TASKS).await? });
    if s.hr {
        out["cases"] = q(&state, CASES).await?;
        out["vehicles"] = q(&state, VEHICLES).await?;
        let mut conn = state.pool.acquire().await?;
        out["vehicle_deadlines"] = crate::routes::vehicles::deadlines_json(&mut conn, crate::documents::algiers_today(), state.config.alert_days, state.config.alert_km).await?;
    }
    out["access_to_cut"] = q(&state, ACCESS).await?;
    out["returns_missing"] = returns(&state, &s).await?;
    out["signatures_missing"] = q(&state, SIGNATURES).await?;
    out["licenses"] = q(&state, LICENSES).await?;
    if s.it {
        out["inventories"] = q(&state, INVENTORY).await?;
    }
    out["late_tasks"] = q(&state, LATE_TASKS).await?;
    Ok(Json(out))
}

fn sheet(wb: &mut Workbook, name: &str, cols: &[(&str, &str)], rows: &Value) -> Result<(), rust_xlsxwriter::XlsxError> {
    let ws = wb.add_worksheet();
    ws.set_name(name)?;
    let head = Format::new().set_bold().set_font_color(Color::White).set_background_color(Color::RGB(0x194667));
    let text = Format::new().set_num_format("@");
    for (c, (_, h)) in cols.iter().enumerate() {
        ws.write_string_with_format(0, c as u16, *h, &head)?;
        ws.set_column_width(c as u16, 24)?;
    }
    for (r, row) in rows.as_array().cloned().unwrap_or_default().iter().enumerate() {
        for (c, (k, _)) in cols.iter().enumerate() {
            let v = match &row[*k] {
                Value::Null => String::new(),
                Value::String(s) => s.clone(),
                Value::Bool(b) => if *b { "oui" } else { "non" }.to_string(),
                other => other.to_string(),
            };
            ws.write_string_with_format(r as u32 + 1, c as u16, v, &text)?;
        }
    }
    ws.set_freeze_panes(1, 0)?;
    Ok(())
}

/// Export Excel des états, selon les droits de l'utilisateur.
async fn export(State(state): State<AppState>, user: CurrentUser) -> AppResult<Response> {
    let s = scope(&user)?;
    let access = q(&state, ACCESS).await?;
    let returns = returns(&state, &s).await?;
    let signatures = q(&state, SIGNATURES).await?;
    let licenses = q(&state, LICENSES).await?;
    let holdings = q(&state, HOLDINGS).await?;
    let late = q(&state, LATE_TASKS).await?;
    let vehicles = if s.hr { Some(q(&state, VEHICLES).await?) } else { None };
    let mut wb = Workbook::new();
    let build = (|| -> Result<Vec<u8>, rust_xlsxwriter::XlsxError> {
        sheet(&mut wb, "Tâches en retard", &[("reference", "Dossier"), ("salarie", "Salarié"), ("title", "Tâche"), ("role", "Équipe"), ("owner", "Responsable"), ("due", "Échéance")], &late)?;
        sheet(&mut wb, "Accès à couper", &[("reference", "Dossier"), ("salarie", "Salarié"), ("acces", "Accès"), ("etat", "État"), ("coupure", "Coupure prévue"), ("en_retard", "En retard")], &access)?;
        sheet(&mut wb, "Retours manquants", &[("reference", "Dossier"), ("salarie", "Salarié"), ("bien", "Élément"), ("etat", "État"), ("suivi", "Suivi"), ("note", "Note")], &returns)?;
        sheet(&mut wb, "Signatures manquantes", &[("mouvement", "Mouvement"), ("nature", "Décharge"), ("salarie", "Salarié"), ("dossier", "Dossier"), ("date", "Date")], &signatures)?;
        sheet(&mut wb, "Biens par détenteur", &[("salarie", "Salarié"), ("statut", "Statut"), ("bien", "Bien"), ("identifiant", "Identifiant"), ("quantite", "Quantité"), ("depuis", "Depuis"), ("origine", "Remise")], &holdings)?;
        sheet(&mut wb, "Licences", &[("logiciel", "Logiciel"), ("sieges", "Sièges"), ("occupes", "Occupés"), ("echeance", "Échéance")], &licenses)?;
        if let Some(v) = &vehicles {
            sheet(&mut wb, "Véhicules", &[("immatriculation", "Immatriculation"), ("type", "Type"), ("etat", "État"), ("titulaire", "Titulaire"), ("km", "Kilométrage")], v)?;
        }
        wb.save_to_buffer()
    })();
    let bytes = build.map_err(|e| anyhow::anyhow!("export des états : {e}"))?;
    let mut conn = state.pool.acquire().await?;
    audit::record(&mut conn, &user, Audit { action: "export_etats", entity: "report", ..Default::default() }).await?;
    Ok((
        [
            (header::CONTENT_TYPE, "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet".to_string()),
            (header::CONTENT_DISPOSITION, format!("attachment; filename=\"etats-{}.xlsx\"", crate::documents::algiers_today())),
        ],
        Body::from(bytes),
    )
        .into_response())
}
