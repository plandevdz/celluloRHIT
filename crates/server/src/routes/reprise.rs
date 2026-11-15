//! Reprise des listes SharePoint (02-Technique T08) : chaque liste exportée
//! (CSV ou .xlsx) est chargée en aperçu, sans écriture métier ; les lignes
//! ambiguës attendent une décision nominative ; l'application se fait en
//! une transaction, ligne par ligne (SAVEPOINT). L'ancien ID SharePoint est
//! conservé pour préserver les liens et rejouer sans doublon. Aucune tâche,
//! aucun parcours et aucun avis ne sont créés par une reprise.

use std::collections::HashMap;

use axum::extract::{DefaultBodyLimit, Multipart, Path, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::{DateTime, NaiveDate, Utc};
use domain::legacy::{self, fold};
use domain::Role;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::PgConnection;
use uuid::Uuid;

use crate::audit::{self, Audit};
use crate::auth::CurrentUser;
use crate::documents::sha256_hex;
use crate::error::{AppError, AppResult};
use crate::routes::employees::{insert_employee, NewEmployee};
use crate::state::AppState;
use crate::tabular::{self, Row, Table};

pub fn routes(max_upload: usize) -> Router<AppState> {
    Router::new()
        .route("/reprise/lists", get(lists))
        .route("/reprise/batches", get(batches))
        .route(
            "/reprise/{list}/preview",
            post(preview).layer(DefaultBodyLimit::max(max_upload + 64 * 1024)),
        )
        .route("/reprise/batches/{id}", get(batch_detail))
        .route("/reprise/batches/{id}/rows/{row}/decision", post(decide))
        .route("/reprise/batches/{id}/commit", post(commit))
        .route("/reprise/batches/{id}/abandon", post(abandon))
}

// ------------------------------------------------------------------ listes reprises

/// Listes reprises, dans l'ordre conseillé (les références d'abord).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum List {
    Wilayas,
    CatalogueFormations,
    UtilisateursApp,
    ProfilsPoste,
    Personnel,
    MaterielIT,
    LignesMobiles,
    Affectations,
    Formations,
    Vehicules,
    AffectationsVehicules,
}

const ALL: [List; 11] = [
    List::Wilayas,
    List::CatalogueFormations,
    List::UtilisateursApp,
    List::ProfilsPoste,
    List::Personnel,
    List::MaterielIT,
    List::LignesMobiles,
    List::Affectations,
    List::Formations,
    List::Vehicules,
    List::AffectationsVehicules,
];

impl List {
    pub fn name(self) -> &'static str {
        match self {
            List::Wilayas => "Wilayas",
            List::CatalogueFormations => "CatalogueFormations",
            List::UtilisateursApp => "UtilisateursApp",
            List::ProfilsPoste => "ProfilsPoste",
            List::Personnel => "Personnel",
            List::MaterielIT => "MaterielIT",
            List::LignesMobiles => "LignesMobiles",
            List::Affectations => "Affectations",
            List::Formations => "Formations",
            List::Vehicules => "Vehicules",
            List::AffectationsVehicules => "AffectationsVehicules",
        }
    }

    pub fn parse(s: &str) -> Option<List> {
        ALL.into_iter().find(|l| l.name().eq_ignore_ascii_case(s))
    }

    fn label(self) -> &'static str {
        match self {
            List::Wilayas => "Wilayas",
            List::CatalogueFormations => "Catalogue des formations",
            List::UtilisateursApp => "Utilisateurs de l'application",
            List::ProfilsPoste => "Profils de poste",
            List::Personnel => "Personnel",
            List::MaterielIT => "Matériel IT",
            List::LignesMobiles => "Lignes mobiles",
            List::Affectations => "Affectations (matériel et lignes)",
            List::Formations => "Formations suivies",
            List::Vehicules => "Véhicules",
            List::AffectationsVehicules => "Affectations de véhicules",
        }
    }

    fn entity(self) -> &'static str {
        match self {
            List::Wilayas => "wilaya",
            List::CatalogueFormations => "training_catalog",
            List::UtilisateursApp => "user",
            List::ProfilsPoste => "job_profile",
            List::Personnel => "employee",
            List::MaterielIT => "asset",
            List::LignesMobiles => "mobile_line",
            List::Affectations => "assignment",
            List::Formations => "training_session",
            List::Vehicules => "vehicle",
            List::AffectationsVehicules => "vehicle_assignment",
        }
    }

    /// Colonne titre : libellé affiché dans l'export, puis noms internes.
    fn title_cols(self) -> &'static [&'static str] {
        match self {
            List::Wilayas => &["Libelle", "Libellé", "Title", "Titre"],
            List::CatalogueFormations | List::ProfilsPoste => &["Intitule", "Intitulé", "Title", "Titre"],
            List::UtilisateursApp => &["NomPrenom", "Nom Prénom", "Title", "Titre"],
            List::Personnel => &["NomComplet", "Nom complet", "Title", "Titre"],
            List::MaterielIT => &["NumInventaire", "N° inventaire", "Title", "Titre"],
            List::LignesMobiles => &["Numero", "Numéro", "Title", "Titre"],
            List::Vehicules => &["Immatriculation", "Title", "Titre"],
            List::Affectations | List::Formations | List::AffectationsVehicules => &["Title", "Titre"],
        }
    }

    /// Colonnes attendues (obligatoires marquées), pour l'écran et le contrôle.
    fn columns(self) -> Vec<(&'static str, bool)> {
        match self {
            List::Wilayas => vec![("CodeWilaya", true), ("Wilaya", false), ("Libelle", false), ("Region", false), ("OrdreRegion", false)],
            List::CatalogueFormations => vec![
                ("Intitule", true),
                ("Categorie", false),
                ("DureeHeures", false),
                ("Description", false),
                ("Support", false),
                ("FormateurEmail", false),
            ],
            List::UtilisateursApp => vec![("NomPrenom", true), ("Email", true), ("Equipe", true), ("Role", false), ("Actif", false)],
            List::ProfilsPoste => vec![
                ("Intitule", true),
                ("Departement", false),
                ("MaterielStandard", false),
                ("BesoinEmailPro", false),
                ("BesoinXena", false),
                ("BesoinIntellix", false),
                ("LigneMobile", false),
                ("CarteVisite", false),
                ("Vehicule", false),
                ("Terrain", false),
                ("FormationsRequises", false),
                ("Description", false),
                ("FonctionsTypes", false),
                ("AccesACreer", false),
            ],
            List::Personnel => vec![
                ("ID", false),
                ("NomComplet", false),
                ("Prenom", true),
                ("NomFamille", true),
                ("Matricule", false),
                ("Statut", false),
                ("DateEntree", false),
                ("DateSortie", false),
                ("Site", true),
                ("Fonction", false),
                ("Departement", false),
                ("Manager", false),
                ("TypeContrat", false),
                ("ProfilPoste", false),
                ("Wilayas", false),
                ("PosteFixe", false),
                ("EmailPro", false),
                ("PlateformeMessagerie", false),
                ("M365Operationnel", false),
                ("DateActivationM365", false),
            ],
            List::MaterielIT => vec![
                ("ID", false),
                ("NumInventaire", true),
                ("Categorie", true),
                ("Marque", false),
                ("Modele", false),
                ("NumSerie", false),
                ("NomIntune", false),
                ("DateAchat", false),
                ("FinGarantie", false),
                ("Etat", false),
                ("Statut", false),
                ("AttribueA", false),
                ("Observations", false),
            ],
            List::LignesMobiles => vec![
                ("ID", false),
                ("Numero", true),
                ("Operateur", true),
                ("ICCID", false),
                ("Forfait", false),
                ("TypeLigne", false),
                ("Statut", false),
                ("AttribueA", false),
            ],
            List::Affectations => vec![
                ("ID", false),
                ("TypeObjet", true),
                ("Materiel", false),
                ("Ligne", false),
                ("Employe", true),
                ("DateAttribution", false),
                ("DateRestitution", false),
                ("EtatRetour", false),
                ("Observations", false),
            ],
            List::Formations => vec![
                ("ID", false),
                ("Formation", true),
                ("Employe", true),
                ("Statut", false),
                ("DatePrevue", false),
                ("DateRealisee", false),
                ("Evaluation", false),
                ("FormateurEmail", false),
            ],
            List::Vehicules => vec![
                ("ID", false),
                ("Immatriculation", true),
                ("TypeVehicule", true),
                ("Marque", false),
                ("Modele", false),
                ("Kilometrage", false),
                ("Statut", false),
                ("Loueur", false),
                ("DateFinLocation", false),
                ("EcheanceAssurance", false),
                ("EcheanceCT", false),
                ("AttribueA", false),
            ],
            List::AffectationsVehicules => vec![
                ("ID", false),
                ("Vehicule", true),
                ("Employe", true),
                ("DateRemise", false),
                ("DateRestitution", false),
                ("KmDepart", false),
                ("KmRetour", false),
                ("EtatDepart", false),
                ("ObservationsRetour", false),
            ],
        }
    }
}

async fn lists(user: CurrentUser) -> AppResult<Json<Value>> {
    user.require(&[Role::Admin])?;
    Ok(Json(json!(ALL
        .iter()
        .map(|l| json!({
            "name": l.name(),
            "label": l.label(),
            "columns": l.columns().iter().map(|(c, req)| json!({ "name": c, "required": req })).collect::<Vec<_>>(),
        }))
        .collect::<Vec<_>>())))
}

// ------------------------------------------------------------------ outils

fn msg(level: &str, text: impl Into<String>) -> Value {
    json!({ "niveau": level, "texte": text.into() })
}

/// Variantes de nom de la colonne « identifiant » d'une recherche.
fn id_cols(base: &str) -> Vec<String> {
    vec![format!("{base}Id"), format!("{base}LookupId"), format!("{base}.Id"), format!("{base} ID"), format!("{base}_Id")]
}

fn text_of(row: &Row, base: &str) -> Option<String> {
    row.text(&[base])
}

fn lookup_id(row: &Row, base: &str) -> Option<String> {
    let names = id_cols(base);
    let refs: Vec<&str> = names.iter().map(|s| s.as_str()).collect();
    row.text(&refs)
}

#[derive(Default)]
struct Plan {
    proposal: &'static str,
    messages: Vec<Value>,
    target_id: Option<Uuid>,
    candidates: Vec<Value>,
    resolved: Value,
}

impl Plan {
    fn err(&mut self, t: impl Into<String>) {
        self.messages.push(msg("erreur", t));
    }
    fn warn(&mut self, t: impl Into<String>) {
        self.messages.push(msg("attention", t));
    }
    fn info(&mut self, t: impl Into<String>) {
        self.messages.push(msg("info", t));
    }
    fn has_error(&self) -> bool {
        self.messages.iter().any(|m| m["niveau"] == "erreur")
    }
}

/// Résultat de la résolution d'une recherche vers un salarié.
enum Who {
    Found(Uuid),
    /// Plusieurs personnes possibles : décision nominative.
    Ambiguous(Vec<Value>),
    /// Rapprochement par le seul nom : jamais automatique.
    NameOnly(Vec<Value>),
    NotFound,
}

async fn legacy_entity(conn: &mut PgConnection, list: &str, source_id: &str) -> AppResult<Option<Uuid>> {
    Ok(sqlx::query_scalar("SELECT entity_id FROM legacy_ids WHERE source_list = $1 AND source_id = $2")
        .bind(list)
        .bind(source_id)
        .fetch_optional(conn)
        .await?)
}

/// Salarié désigné par une recherche « Personnel » de l'export : ancien ID
/// si l'export le fournit, sinon titre de la fiche reprise (lien SharePoint),
/// sinon proposition par le nom à confirmer.
async fn resolve_employee(conn: &mut PgConnection, row: &Row, base: &str) -> AppResult<(Option<String>, Who)> {
    if let Some(id) = lookup_id(row, base) {
        if let Some(e) = legacy_entity(conn, "Personnel", &id).await? {
            let name: String = sqlx::query_scalar("SELECT first_name || ' ' || last_name FROM employees WHERE id = $1")
                .bind(e)
                .fetch_one(&mut *conn)
                .await?;
            return Ok((Some(name), Who::Found(e)));
        }
    }
    let Some(title) = text_of(row, base) else { return Ok((None, Who::NotFound)) };
    let by_legacy: Vec<(Uuid, String)> = sqlx::query_as(
        "SELECT l.entity_id, e.first_name || ' ' || e.last_name FROM legacy_ids l JOIN employees e ON e.id = l.entity_id
          WHERE l.source_list = 'Personnel' AND lower(l.source_title) = lower($1)",
    )
    .bind(&title)
    .fetch_all(&mut *conn)
    .await?;
    let candidates = |v: &[(Uuid, String)]| v.iter().map(|(id, n)| json!({ "id": id, "nom": n })).collect::<Vec<_>>();
    match by_legacy.len() {
        1 => return Ok((Some(title), Who::Found(by_legacy[0].0))),
        n if n > 1 => return Ok((Some(title), Who::Ambiguous(candidates(&by_legacy)))),
        _ => {}
    }
    let first_last = title.split_whitespace().collect::<Vec<_>>();
    let key = if first_last.len() >= 2 {
        domain::text::name_key(first_last[0], &first_last[1..].join(" "))
    } else {
        domain::text::name_key(&title, "")
    };
    let by_name: Vec<(Uuid, String)> = sqlx::query_as(
        "SELECT id, first_name || ' ' || last_name FROM employees WHERE name_key = $1 ORDER BY last_name LIMIT 10",
    )
    .bind(&key)
    .fetch_all(&mut *conn)
    .await?;
    if by_name.is_empty() {
        Ok((Some(title), Who::NotFound))
    } else {
        Ok((Some(title), Who::NameOnly(candidates(&by_name))))
    }
}

/// Applique la résolution d'un salarié au plan : trouvé, à décider, ou erreur.
fn plan_employee(plan: &mut Plan, label: &str, who: Who, title: Option<&str>, required: bool) -> Option<Uuid> {
    match who {
        Who::Found(id) => Some(id),
        Who::Ambiguous(c) => {
            plan.warn(format!("{label} « {} » : plusieurs fiches reprises portent ce nom, choisissez la bonne", title.unwrap_or("")));
            plan.candidates = c;
            plan.proposal = "a_revoir";
            None
        }
        Who::NameOnly(c) => {
            plan.warn(format!(
                "{label} « {} » : fiche non reprise depuis Personnel ; rapprochement par le nom à confirmer",
                title.unwrap_or("")
            ));
            plan.candidates = c;
            plan.proposal = "a_revoir";
            None
        }
        Who::NotFound => {
            if let Some(t) = title {
                if required {
                    plan.err(format!("{label} « {t} » introuvable : reprenez d'abord la liste Personnel"));
                } else {
                    plan.warn(format!("{label} « {t} » introuvable : lien non repris"));
                }
            } else if required {
                plan.err(format!("{label} obligatoire"));
            }
            None
        }
    }
}

/// Référentiels chargés une fois par aperçu.
struct Ctx {
    sites: Vec<(Uuid, String, String)>,
    profiles: HashMap<String, Uuid>,
    wilayas: HashMap<String, Uuid>,
    catalog: HashMap<String, Uuid>,
    users: HashMap<String, (Uuid, bool)>,
}

impl Ctx {
    async fn load(conn: &mut PgConnection) -> AppResult<Ctx> {
        let sites: Vec<(Uuid, String, String)> = sqlx::query_as("SELECT id, code, name FROM sites WHERE active").fetch_all(&mut *conn).await?;
        let profiles: Vec<(Uuid, String)> = sqlx::query_as("SELECT id, name FROM job_profiles").fetch_all(&mut *conn).await?;
        let wilayas: Vec<(Uuid, i32, String)> = sqlx::query_as("SELECT id, code, name FROM wilayas").fetch_all(&mut *conn).await?;
        let catalog: Vec<(Uuid, String)> = sqlx::query_as("SELECT id, name FROM training_catalog").fetch_all(&mut *conn).await?;
        let users: Vec<(Uuid, String, bool)> = sqlx::query_as(
            "SELECT u.id, lower(u.email), EXISTS (SELECT 1 FROM user_roles r WHERE r.user_id = u.id AND r.role = 'FORMATION') AND u.active
               FROM users u",
        )
        .fetch_all(&mut *conn)
        .await?;
        let mut w = HashMap::new();
        for (id, code, name) in wilayas {
            w.insert(fold(&name), id);
            w.insert(code.to_string(), id);
            w.insert(format!("{code:02}"), id);
        }
        Ok(Ctx {
            sites,
            profiles: profiles.into_iter().map(|(id, n)| (fold(&n), id)).collect(),
            wilayas: w,
            catalog: catalog.into_iter().map(|(id, n)| (fold(&n), id)).collect(),
            users: users.into_iter().map(|(id, e, f)| (e, (id, f))).collect(),
        })
    }

    fn site(&self, label: &str) -> Option<Uuid> {
        let l = fold(label);
        self.sites.iter().find(|(_, code, name)| fold(code) == l || fold(name) == l).map(|s| s.0)
    }

    fn site_names(&self) -> String {
        self.sites.iter().map(|s| s.2.as_str()).collect::<Vec<_>>().join(", ")
    }

    /// « Oran » ou « 31 - Oran » ou « 31 ».
    fn wilaya(&self, label: &str) -> Option<Uuid> {
        let l = fold(label);
        if let Some(id) = self.wilayas.get(&l) {
            return Some(*id);
        }
        let code: String = l.chars().take_while(|c| c.is_ascii_digit()).collect();
        if !code.is_empty() {
            if let Some(id) = self.wilayas.get(&code) {
                return Some(*id);
            }
        }
        let name = l.trim_start_matches(|c: char| c.is_ascii_digit() || c == ' ').trim();
        self.wilayas.get(name).copied()
    }
}

// ------------------------------------------------------------------ données résolues

#[derive(Serialize, Deserialize, Default)]
struct WilayaData {
    code: i32,
    name: String,
    region: Option<String>,
    sort_order: Option<i32>,
}

#[derive(Serialize, Deserialize, Default)]
struct CatalogData {
    name: String,
    category: Option<String>,
    duration_hours: Option<f64>,
    description: Option<String>,
    support_url: Option<String>,
    default_trainer_id: Option<Uuid>,
}

#[derive(Serialize, Deserialize, Default)]
struct UserData {
    email: String,
    display_name: String,
    roles: Vec<String>,
    active: bool,
}

#[derive(Serialize, Deserialize, Default)]
struct ProfileNeed {
    category: String,
    preference: Option<String>,
    detail: Option<String>,
}

#[derive(Serialize, Deserialize, Default)]
struct ProfileData {
    name: String,
    department: Option<String>,
    description: Option<String>,
    typical_titles: Option<String>,
    access_notes: Option<String>,
    field_based: bool,
    needs: Vec<ProfileNeed>,
    trainings: Vec<Uuid>,
}

#[derive(Serialize, Deserialize, Default)]
struct PersonData {
    first_name: String,
    last_name: String,
    full_name: Option<String>,
    matricule: Option<String>,
    status: String,
    entry_date: Option<NaiveDate>,
    exit_date: Option<NaiveDate>,
    site_id: Option<Uuid>,
    job_title: Option<String>,
    department: Option<String>,
    manager_title: Option<String>,
    manager_source_id: Option<String>,
    manager_name: Option<String>,
    contract_type: Option<String>,
    profile_id: Option<Uuid>,
    wilaya_ids: Vec<Uuid>,
    landline: Option<String>,
    email_pro: Option<String>,
    email_platform: Option<String>,
    m365_verified_at: Option<DateTime<Utc>>,
}

#[derive(Serialize, Deserialize, Default)]
struct AssetData {
    inventory_no: String,
    kind: String,
    label: String,
    brand: Option<String>,
    model: Option<String>,
    serial_no: Option<String>,
    intune_name: Option<String>,
    purchase_date: Option<NaiveDate>,
    warranty_end: Option<NaiveDate>,
    condition: Option<String>,
    status: String,
    notes: Option<String>,
    holder_id: Option<Uuid>,
}

#[derive(Serialize, Deserialize, Default)]
struct LineData {
    number: String,
    operator: String,
    iccid: Option<String>,
    plan: Option<String>,
    line_type: Option<String>,
    status: String,
    holder_id: Option<Uuid>,
}

#[derive(Serialize, Deserialize, Default)]
struct AssignmentData {
    asset_id: Option<Uuid>,
    sim_id: Option<Uuid>,
    label: String,
    employee_id: Option<Uuid>,
    started_at: Option<DateTime<Utc>>,
    ended_at: Option<DateTime<Utc>>,
    condition: Option<String>,
    note: Option<String>,
}

#[derive(Serialize, Deserialize, Default)]
struct SessionData {
    catalog_id: Uuid,
    employee_id: Option<Uuid>,
    status: String,
    planned_date: Option<NaiveDate>,
    done_date: Option<NaiveDate>,
    evaluation: Option<String>,
    trainer_id: Option<Uuid>,
}

#[derive(Serialize, Deserialize, Default)]
pub struct VehicleData {
    pub plate: String,
    pub plate_norm: String,
    pub kind: String,
    pub brand: Option<String>,
    pub model: Option<String>,
    pub odometer_km: Option<i32>,
    pub status: String,
    pub lessor: Option<String>,
    pub lease_end: Option<NaiveDate>,
    pub insurance_until: Option<NaiveDate>,
    pub inspection_until: Option<NaiveDate>,
    pub holder_id: Option<Uuid>,
}

#[derive(Serialize, Deserialize, Default)]
pub struct VehicleAssignmentData {
    pub vehicle_id: Uuid,
    pub plate: String,
    pub employee_id: Option<Uuid>,
    pub started_at: Option<DateTime<Utc>>,
    pub ended_at: Option<DateTime<Utc>>,
    pub km_start: Option<i32>,
    pub km_end: Option<i32>,
    pub condition_start: Option<String>,
    pub note_end: Option<String>,
}

fn to_json<T: Serialize>(v: &T) -> Value {
    serde_json::to_value(v).unwrap_or(Value::Null)
}

fn opt_int(row: &Row, names: &[&str], plan: &mut Plan, label: &str) -> Option<i32> {
    match row.number(names) {
        Ok(Some(n)) if n >= 0.0 && n <= i32::MAX as f64 => Some(n.round() as i32),
        Ok(Some(n)) => {
            plan.err(format!("{label} hors limites ({n})"));
            None
        }
        Ok(None) => None,
        Err(e) => {
            plan.err(format!("{label} : {e}"));
            None
        }
    }
}

fn opt_date(row: &Row, names: &[&str], plan: &mut Plan, label: &str) -> Option<NaiveDate> {
    match row.date(names) {
        Ok(d) => d,
        Err(e) => {
            plan.err(format!("{label} : {e}"));
            None
        }
    }
}

fn opt_instant(row: &Row, names: &[&str], plan: &mut Plan, label: &str) -> Option<DateTime<Utc>> {
    match row.instant(names) {
        Ok(d) => d,
        Err(e) => {
            plan.err(format!("{label} : {e}"));
            None
        }
    }
}

fn opt_bool(row: &Row, names: &[&str], plan: &mut Plan, label: &str) -> Option<bool> {
    match row.boolean(names) {
        Ok(b) => b,
        Err(e) => {
            plan.err(format!("{label} : {e}"));
            None
        }
    }
}

fn choice(raw: Option<String>, map: fn(&str) -> Option<&'static str>, plan: &mut Plan, label: &str) -> Option<String> {
    let raw = raw?;
    match map(&raw) {
        Some(v) => Some(v.to_string()),
        None => {
            plan.err(format!("{label} « {raw} » non reconnu"));
            None
        }
    }
}

/// Lien « https://…, description » d'une colonne URL SharePoint.
fn url_part(raw: Option<String>) -> Option<String> {
    raw.map(|u| u.split(", ").next().unwrap_or_default().trim().to_string()).filter(|u| !u.is_empty())
}

// ------------------------------------------------------------------ analyse par liste

async fn plan_row(conn: &mut PgConnection, list: List, ctx: &Ctx, row: &Row, source_id: Option<&str>) -> AppResult<Plan> {
    let mut plan = Plan { proposal: "creation", ..Default::default() };
    if row.any_formula() {
        plan.err("cellule contenant une formule ou une erreur : exportez les valeurs");
        plan.proposal = "rejet";
        return Ok(plan);
    }
    let known = match source_id {
        Some(id) => legacy_entity(conn, list.name(), id).await?,
        None => None,
    };
    match list {
        List::Wilayas => plan_wilaya(conn, row, known, &mut plan).await?,
        List::CatalogueFormations => plan_catalog(conn, ctx, row, known, &mut plan).await?,
        List::UtilisateursApp => plan_user(conn, row, &mut plan).await?,
        List::ProfilsPoste => plan_profile(conn, ctx, row, known, &mut plan).await?,
        List::Personnel => plan_person(conn, ctx, row, known, &mut plan).await?,
        List::MaterielIT => plan_asset(conn, row, known, &mut plan).await?,
        List::LignesMobiles => plan_line(conn, row, known, &mut plan).await?,
        List::Affectations => plan_assignment(conn, row, known, &mut plan).await?,
        List::Formations => plan_session(conn, ctx, row, known, &mut plan).await?,
        List::Vehicules => plan_vehicle(conn, row, known, &mut plan).await?,
        List::AffectationsVehicules => plan_vehicle_assignment(conn, row, known, &mut plan).await?,
    }
    if plan.has_error() {
        plan.proposal = "rejet";
    }
    Ok(plan)
}

async fn plan_wilaya(conn: &mut PgConnection, row: &Row, known: Option<Uuid>, plan: &mut Plan) -> AppResult<()> {
    let code = opt_int(row, &["CodeWilaya", "Code"], plan, "CodeWilaya");
    let name = row.text(&["Wilaya", "Libelle", "Libellé", "Title", "Titre"]);
    let (Some(code), Some(name)) = (code, name) else {
        plan.err("CodeWilaya et Wilaya (ou Libellé) obligatoires");
        return Ok(());
    };
    let d = WilayaData { code, name, region: row.text(&["Region", "Région"]), sort_order: opt_int(row, &["OrdreRegion"], plan, "OrdreRegion") };
    let existing: Option<(Uuid, String, Option<String>)> = sqlx::query_as("SELECT id, name, region FROM wilayas WHERE code = $1 OR id = $2")
        .bind(code)
        .bind(known)
        .fetch_optional(&mut *conn)
        .await?;
    if let Some((id, name, region)) = existing {
        plan.target_id = Some(id);
        plan.proposal = if name == d.name && region == d.region { "inchange" } else { "mise_a_jour" };
    }
    plan.resolved = to_json(&d);
    Ok(())
}

async fn plan_catalog(conn: &mut PgConnection, ctx: &Ctx, row: &Row, known: Option<Uuid>, plan: &mut Plan) -> AppResult<()> {
    let Some(name) = row.text(List::CatalogueFormations.title_cols()) else {
        plan.err("Intitulé obligatoire");
        return Ok(());
    };
    let mut d = CatalogData {
        name,
        category: choice(row.text(&["Categorie", "Catégorie"]), legacy::training_category, plan, "Catégorie"),
        duration_hours: row.number(&["DureeHeures", "Durée (heures)"]).unwrap_or_else(|e| {
            plan.err(format!("Durée : {e}"));
            None
        }),
        description: row.text(&["Description"]),
        support_url: url_part(row.text(&["Support"])),
        default_trainer_id: None,
    };
    if let Some(email) = row.text(&["FormateurEmail"]) {
        match ctx.users.get(&email.to_lowercase()) {
            Some((id, true)) => d.default_trainer_id = Some(*id),
            Some((_, false)) => plan.warn(format!("Formateur {email} : compte sans rôle FORMATION actif, intervenant non repris")),
            None => plan.warn(format!("Formateur {email} : aucun compte applicatif (reprenez UtilisateursApp)")),
        }
    }
    let existing = match known {
        Some(id) => Some(id),
        None => ctx.catalog.get(&fold(&d.name)).copied(),
    };
    if let Some(id) = existing {
        plan.target_id = Some(id);
        plan.proposal = "mise_a_jour";
    }
    let _ = conn;
    plan.resolved = to_json(&d);
    Ok(())
}

async fn plan_user(conn: &mut PgConnection, row: &Row, plan: &mut Plan) -> AppResult<()> {
    let name = row.text(List::UtilisateursApp.title_cols());
    let email = row.text(&["Email", "Adresse email"]).map(|e| e.to_lowercase());
    let (Some(name), Some(email)) = (name, email) else {
        plan.err("Nom et Email obligatoires");
        return Ok(());
    };
    if !email.contains('@') {
        plan.err(format!("Email invalide « {email} »"));
        return Ok(());
    }
    let mut roles: Vec<String> = Vec::new();
    for team in row.list(&["Equipe", "Équipe"]) {
        match legacy::team_role(&team) {
            Some(r) => roles.push(r.to_string()),
            None => plan.info(format!("Équipe « {team} » : pas de rôle applicatif correspondant (le manager n'est pas un rôle)")),
        }
    }
    if row.text(&["Role", "Rôle"]).map(|r| fold(&r) == "admin").unwrap_or(false) {
        roles.push("ADMIN".into());
    }
    roles.sort();
    roles.dedup();
    if roles.is_empty() {
        plan.err("aucun rôle applicatif (RH, IT, Graphiste, Formateur ou Admin) : compte non repris");
        return Ok(());
    }
    let active = opt_bool(row, &["Actif"], plan, "Actif").unwrap_or(true);
    let existing: Option<(Uuid, Vec<String>)> = sqlx::query_as(
        "SELECT u.id, coalesce(array_agg(r.role) FILTER (WHERE r.role IS NOT NULL), '{}')
           FROM users u LEFT JOIN user_roles r ON r.user_id = u.id WHERE lower(u.email) = $1 GROUP BY u.id",
    )
    .bind(&email)
    .fetch_optional(&mut *conn)
    .await?;
    if let Some((id, have)) = existing {
        plan.target_id = Some(id);
        let missing: Vec<&String> = roles.iter().filter(|r| !have.contains(r)).collect();
        plan.proposal = if missing.is_empty() { "inchange" } else { "mise_a_jour" };
        if !missing.is_empty() {
            plan.info(format!("Rôles ajoutés : {} (aucun rôle retiré)", missing.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ")));
        }
    } else {
        plan.info("Compte créé sans mot de passe : générez son lien d'activation depuis Administration");
    }
    plan.resolved = to_json(&UserData { email, display_name: name, roles, active });
    Ok(())
}

async fn plan_profile(conn: &mut PgConnection, ctx: &Ctx, row: &Row, known: Option<Uuid>, plan: &mut Plan) -> AppResult<()> {
    let Some(name) = row.text(List::ProfilsPoste.title_cols()) else {
        plan.err("Intitulé obligatoire");
        return Ok(());
    };
    let mut needs: Vec<ProfileNeed> = Vec::new();
    let material = row.list(&["MaterielStandard", "Matériel standard"]);
    let has = |label: &str| material.iter().any(|m| fold(m) == fold(label));
    let mut extras: Vec<String> = material
        .iter()
        .filter(|m| !["pc portable", "pc fixe", "smartphone"].contains(&fold(m).as_str()))
        .cloned()
        .collect();
    extras.sort();
    if has("PC portable") || has("PC fixe") {
        let pref = match (has("PC portable"), has("PC fixe")) {
            (true, false) => Some("portable"),
            (false, true) => Some("fixe"),
            _ => Some("sans_preference"),
        };
        needs.push(ProfileNeed {
            category: "pc".into(),
            preference: pref.map(String::from),
            detail: (!extras.is_empty()).then(|| format!("Matériel standard : {}", extras.join(", "))),
        });
    } else if !extras.is_empty() {
        plan.info(format!("Matériel standard sans PC : {} (noté dans la description)", extras.join(", ")));
    }
    if has("Smartphone") {
        needs.push(ProfileNeed { category: "telephone".into(), ..Default::default() });
    }
    let flags = [
        (&["BesoinEmailPro"][..], "email_pro"),
        (&["BesoinXena"][..], "xena"),
        (&["BesoinIntellix", "BesoinIntelix"][..], "intelix"),
        (&["LigneMobile"][..], "sim"),
        (&["CarteVisite"][..], "carte_visite"),
        (&["Vehicule", "Véhicule"][..], "vehicule"),
    ];
    for (cols, cat) in flags {
        if opt_bool(row, cols, plan, cols[0]) == Some(true) {
            needs.push(ProfileNeed { category: cat.into(), ..Default::default() });
        }
    }
    let mut trainings = Vec::new();
    for t in row.list(&["FormationsRequises", "Formations requises"]) {
        match ctx.catalog.get(&fold(&t)) {
            Some(id) => trainings.push(*id),
            None => plan.warn(format!("Formation « {t} » absente du catalogue (reprenez CatalogueFormations d'abord)")),
        }
    }
    let mut description = row.text(&["Description"]);
    if needs.iter().all(|n| n.category != "pc") && !extras.is_empty() {
        let add = format!("Matériel standard : {}", extras.join(", "));
        description = Some(match description {
            Some(d) => format!("{d}\n{add}"),
            None => add,
        });
    }
    if let Some(m) = row.text(&["MessageriePrevue", "Messagerie prévue"]) {
        plan.info(format!("Messagerie prévue « {m} » : la plateforme reste choisie par l'IT, non reprise"));
    }
    let d = ProfileData {
        name,
        department: row.text(&["Departement", "Département"]),
        description,
        typical_titles: row.text(&["FonctionsTypes", "Fonctions types"]),
        access_notes: row.text(&["AccesACreer", "Accès à créer"]),
        field_based: opt_bool(row, &["Terrain"], plan, "Terrain").unwrap_or(false),
        needs,
        trainings,
    };
    let existing = match known {
        Some(id) => Some(id),
        None => ctx.profiles.get(&fold(&d.name)).copied(),
    };
    if let Some(id) = existing {
        plan.target_id = Some(id);
        plan.proposal = "mise_a_jour";
    }
    let _ = conn;
    plan.resolved = to_json(&d);
    Ok(())
}

async fn plan_person(conn: &mut PgConnection, ctx: &Ctx, row: &Row, known: Option<Uuid>, plan: &mut Plan) -> AppResult<()> {
    let first = row.text(&["Prenom", "Prénom"]);
    let last = row.text(&["NomFamille", "Nom de famille", "Nom"]);
    let (Some(first_name), Some(last_name)) = (first, last) else {
        plan.err("Prénom et NomFamille obligatoires");
        return Ok(());
    };
    let status = choice(row.text(&["Statut"]), legacy::employee_status, plan, "Statut").unwrap_or_else(|| "actif".into());
    if row.text(&["Statut"]).map(|s| fold(&s) == "en depart").unwrap_or(false) {
        plan.info("« En départ » repris comme actif : ouvrez un dossier de départ dans l'application");
    }
    let mut d = PersonData {
        first_name,
        last_name,
        full_name: row.text(List::Personnel.title_cols()),
        matricule: row.text(&["Matricule"]),
        status,
        entry_date: opt_date(row, &["DateEntree", "Date d'entrée"], plan, "DateEntree"),
        exit_date: opt_date(row, &["DateSortie", "Date de sortie"], plan, "DateSortie"),
        job_title: row.text(&["Fonction", "Poste"]),
        department: row.text(&["Departement", "Département"]),
        manager_title: row.text(&["Manager"]),
        manager_source_id: lookup_id(row, "Manager"),
        manager_name: row.text(&["ManagerNom"]),
        contract_type: choice(row.text(&["TypeContrat", "Type de contrat"]), legacy::contract_type, plan, "TypeContrat"),
        landline: row.text(&["PosteFixe", "Poste fixe"]),
        ..Default::default()
    };
    match row.text(&["Site"]) {
        Some(s) => match ctx.site(&s) {
            Some(id) => d.site_id = Some(id),
            None => plan.err(format!("Site inconnu « {s} » : créez-le en Administration (sites : {})", ctx.site_names())),
        },
        None => plan.err("Site obligatoire"),
    }
    if let Some(p) = row.text(&["ProfilPoste", "Profil de poste"]) {
        match ctx.profiles.get(&fold(&p)) {
            Some(id) => d.profile_id = Some(*id),
            None => plan.warn(format!("Profil « {p} » inconnu (reprenez ProfilsPoste d'abord) : non relié")),
        }
    }
    for w in row.list(&["Wilayas"]) {
        match ctx.wilaya(&w) {
            Some(id) => d.wilaya_ids.push(id),
            None => plan.warn(format!("Wilaya « {w} » inconnue (reprenez Wilayas d'abord) : non reliée")),
        }
    }
    if d.status == "sorti" && d.exit_date.is_none() {
        plan.warn("Salarié sorti sans date de sortie");
    }
    if let (Some(a), Some(b)) = (d.entry_date, d.exit_date) {
        if b < a {
            plan.err("Date de sortie antérieure à la date d'entrée");
        }
    }
    if let Some(email) = row.text(&["EmailPro", "Email professionnel"]) {
        let platform = row.text(&["PlateformeMessagerie", "Plateforme messagerie"]).map(|p| fold(&p));
        match platform.as_deref() {
            Some("cpanel") => d.email_platform = Some("cpanel".into()),
            Some("microsoft 365") | Some("m365") | Some("office 365") => d.email_platform = Some("m365".into()),
            _ => plan.warn(format!("Adresse {email} : plateforme inconnue, adresse non reprise (l'IT la consignera)")),
        }
        if d.email_platform.is_some() {
            let taken: Option<String> = sqlx::query_scalar(
                "SELECT e.first_name || ' ' || e.last_name FROM employee_accounts a JOIN employees e ON e.id = a.employee_id
                  WHERE a.system = 'email' AND lower(a.identifier) = lower($1) AND a.status <> 'coupe'",
            )
            .bind(&email)
            .fetch_optional(&mut *conn)
            .await?;
            if let Some(who) = taken {
                plan.info(format!("Adresse {email} déjà consignée pour {who}"));
                d.email_platform = None;
            } else {
                d.email_pro = Some(email.to_lowercase());
            }
        }
    }
    if opt_bool(row, &["M365Operationnel", "M365 opérationnel"], plan, "M365Operationnel") == Some(true) {
        d.m365_verified_at = Some(
            opt_instant(row, &["DateActivationM365"], plan, "DateActivationM365").unwrap_or_else(Utc::now),
        );
    }

    // Identité : ancien ID, puis matricule exact ; jamais le nom seul.
    let matricule_norm = d.matricule.as_deref().and_then(domain::text::normalize_matricule);
    let by_matricule: Option<(Uuid, String)> = match &matricule_norm {
        Some(m) => sqlx::query_as("SELECT id, name_key FROM employees WHERE matricule_norm = $1").bind(m).fetch_optional(&mut *conn).await?,
        None => None,
    };
    let key = domain::text::name_key(&d.first_name, &d.last_name);
    match (known, by_matricule) {
        (Some(id), Some((mid, _))) if id != mid => {
            plan.err("Identifiants contradictoires : l'ancien ID et le matricule désignent deux fiches différentes");
        }
        (Some(id), _) => {
            plan.target_id = Some(id);
            plan.proposal = "mise_a_jour";
        }
        (None, Some((mid, mkey))) => {
            plan.target_id = Some(mid);
            if mkey == key {
                plan.proposal = "mise_a_jour";
            } else {
                plan.proposal = "a_revoir";
                plan.warn("Matricule déjà attribué à une fiche d'un autre nom : rattacher ou exclure");
                plan.candidates = candidates_for(conn, &[mid]).await?;
            }
        }
        (None, None) => {
            let homonyms: Vec<Uuid> = sqlx::query_scalar("SELECT id FROM employees WHERE name_key = $1 LIMIT 10")
                .bind(&key)
                .fetch_all(&mut *conn)
                .await?;
            if !homonyms.is_empty() {
                plan.proposal = "a_revoir";
                plan.warn(format!(
                    "{} fiche(s) du même nom sans matricule commun : « nouvelle personne », « rattacher » ou « exclure »",
                    homonyms.len()
                ));
                plan.candidates = candidates_for(conn, &homonyms).await?;
            }
        }
    }
    if plan.proposal == "creation" && d.job_title.is_none() {
        plan.info("Fonction absente : poste repris « À préciser »");
    }
    plan.resolved = to_json(&d);
    Ok(())
}

async fn candidates_for(conn: &mut PgConnection, ids: &[Uuid]) -> AppResult<Vec<Value>> {
    let rows: Vec<(Uuid, String, Option<String>, Option<String>, Option<String>, String)> = sqlx::query_as(
        "SELECT e.id, e.first_name || ' ' || e.last_name, e.matricule, p.job_title, s.name, e.status
           FROM employees e
           LEFT JOIN LATERAL (SELECT * FROM employee_positions WHERE employee_id = e.id ORDER BY valid_to IS NULL DESC, valid_from DESC LIMIT 1) p ON true
           LEFT JOIN sites s ON s.id = p.site_id
          WHERE e.id = ANY($1)",
    )
    .bind(ids)
    .fetch_all(conn)
    .await?;
    Ok(rows
        .into_iter()
        .map(|(id, n, m, p, s, st)| json!({ "id": id, "nom": n, "matricule": m, "poste": p, "site": s, "statut": st }))
        .collect())
}

async fn plan_asset(conn: &mut PgConnection, row: &Row, known: Option<Uuid>, plan: &mut Plan) -> AppResult<()> {
    let Some(inv) = row.text(List::MaterielIT.title_cols()).map(|s| s.to_uppercase()) else {
        plan.err("Numéro d'inventaire obligatoire");
        return Ok(());
    };
    let Some(kind) = choice(row.text(&["Categorie", "Catégorie"]), legacy::asset_kind, plan, "Catégorie") else {
        if !plan.has_error() {
            plan.err("Catégorie obligatoire");
        }
        return Ok(());
    };
    let brand = row.text(&["Marque"]);
    let model = row.text(&["Modele", "Modèle"]);
    let label = match (&brand, &model) {
        (Some(b), Some(m)) => format!("{b} {m}"),
        (Some(b), None) => b.clone(),
        (None, Some(m)) => m.clone(),
        (None, None) => domain::movements::AssetKind::parse(&kind).map(|k| k.label().to_string()).unwrap_or_else(|| kind.clone()),
    };
    let mut d = AssetData {
        inventory_no: inv,
        label,
        brand,
        model,
        serial_no: row.text(&["NumSerie", "N° de série"]),
        intune_name: row.text(&["NomIntune", "Nom Intune"]),
        purchase_date: opt_date(row, &["DateAchat", "Date d'achat"], plan, "DateAchat"),
        warranty_end: opt_date(row, &["FinGarantie", "Fin de garantie"], plan, "FinGarantie"),
        condition: choice(row.text(&["Etat", "État"]), legacy::asset_condition, plan, "État"),
        status: choice(row.text(&["Statut"]), legacy::asset_status, plan, "Statut").unwrap_or_else(|| "disponible".into()),
        notes: row.text(&["Observations"]),
        kind,
        holder_id: None,
    };
    let existing: Option<(Uuid, bool)> = sqlx::query_as(
        "SELECT a.id, EXISTS (SELECT 1 FROM assignments g WHERE g.asset_id = a.id AND g.ended_at IS NULL)
           FROM assets a WHERE a.id = $1 OR upper(a.inventory_no) = $2 LIMIT 1",
    )
    .bind(known)
    .bind(&d.inventory_no)
    .fetch_optional(&mut *conn)
    .await?;
    let held = existing.map(|e| e.1).unwrap_or(false);
    if let Some((id, _)) = existing {
        plan.target_id = Some(id);
        plan.proposal = "mise_a_jour";
    }
    let (title, who) = resolve_employee(conn, row, "AttribueA").await?;
    if title.is_some() {
        if held {
            plan.info("Actif déjà détenu dans l'application : garde inchangée");
        } else if ["reforme", "reparation"].contains(&d.status.as_str()) {
            plan.warn(format!("Actif « {} » avec détenteur : garde non reprise, à vérifier", d.status));
        } else {
            d.holder_id = plan_employee(plan, "Détenteur", who, title.as_deref(), false);
            if d.holder_id.is_some() {
                plan.info("Garde reprise (mouvement « reprise », sans décharge) ");
            }
        }
    }
    plan.resolved = to_json(&d);
    Ok(())
}

async fn plan_line(conn: &mut PgConnection, row: &Row, known: Option<Uuid>, plan: &mut Plan) -> AppResult<()> {
    let Some(number) = row.text(List::LignesMobiles.title_cols()).map(|n| n.chars().filter(|c| !c.is_whitespace()).collect::<String>()) else {
        plan.err("Numéro obligatoire");
        return Ok(());
    };
    let Some(operator) = row.text(&["Operateur", "Opérateur"]) else {
        plan.err("Opérateur obligatoire");
        return Ok(());
    };
    let mut d = LineData {
        number,
        operator,
        iccid: row.text(&["ICCID"]).map(|s| s.chars().filter(|c| !c.is_whitespace()).collect()),
        plan: row.text(&["Forfait"]),
        line_type: choice(row.text(&["TypeLigne", "Type de ligne"]), legacy::line_type, plan, "TypeLigne"),
        status: choice(row.text(&["Statut"]), legacy::line_status, plan, "Statut").unwrap_or_else(|| "active".into()),
        holder_id: None,
    };
    let existing: Option<Uuid> = sqlx::query_scalar("SELECT id FROM mobile_lines WHERE id = $1 OR number = $2 LIMIT 1")
        .bind(known)
        .bind(&d.number)
        .fetch_optional(&mut *conn)
        .await?;
    if let Some(id) = existing {
        plan.target_id = Some(id);
        plan.proposal = "mise_a_jour";
    }
    if let Some(iccid) = &d.iccid {
        let other: Option<Option<String>> = sqlx::query_scalar(
            "SELECT l.number FROM sim_cards s LEFT JOIN mobile_lines l ON l.id = s.line_id WHERE s.iccid = $1",
        )
        .bind(iccid)
        .fetch_optional(&mut *conn)
        .await?;
        if let Some(n) = other {
            if n.as_deref() != Some(d.number.as_str()) {
                plan.err(format!("ICCID {iccid} déjà rattaché à une autre ligne ({})", n.unwrap_or_else(|| "sans ligne".into())));
            }
        }
    }
    let (title, who) = resolve_employee(conn, row, "AttribueA").await?;
    if title.is_some() {
        if d.iccid.is_none() {
            plan.warn("Titulaire indiqué mais ICCID absent : garde de la SIM non reprise");
        } else if d.status == "resiliee" {
            plan.warn("Ligne résiliée avec titulaire : garde non reprise");
        } else {
            let held: bool = sqlx::query_scalar(
                "SELECT EXISTS (SELECT 1 FROM sim_cards s JOIN assignments a ON a.sim_id = s.id AND a.ended_at IS NULL WHERE s.iccid = $1)",
            )
            .bind(&d.iccid)
            .fetch_one(&mut *conn)
            .await?;
            if held {
                plan.info("SIM déjà détenue dans l'application : garde inchangée");
            } else {
                d.holder_id = plan_employee(plan, "Titulaire", who, title.as_deref(), false);
            }
        }
    }
    plan.resolved = to_json(&d);
    Ok(())
}

async fn plan_assignment(conn: &mut PgConnection, row: &Row, known: Option<Uuid>, plan: &mut Plan) -> AppResult<()> {
    if known.is_some() {
        plan.proposal = "inchange";
        plan.info("Affectation déjà reprise");
        return Ok(());
    }
    let kind = row.text(&["TypeObjet", "Type d'objet"]).map(|k| fold(&k));
    let mut d = AssignmentData::default();
    match kind.as_deref() {
        Some("materiel") => {
            let Some(inv) = row.text(&["Materiel", "Matériel"]).map(|s| s.to_uppercase()) else {
                plan.err("Matériel obligatoire");
                return Ok(());
            };
            let a: Option<(Uuid, String, Option<String>)> =
                sqlx::query_as("SELECT id, label, serial_no FROM assets WHERE upper(inventory_no) = $1")
                    .bind(&inv)
                    .fetch_optional(&mut *conn)
                    .await?;
            match a {
                Some((id, label, _)) => {
                    d.asset_id = Some(id);
                    d.label = format!("{label} ({inv})");
                }
                None => plan.err(format!("Matériel {inv} inconnu : reprenez MaterielIT d'abord")),
            }
        }
        Some("ligne") => {
            let Some(number) = row.text(&["Ligne"]).map(|n| n.chars().filter(|c| !c.is_whitespace()).collect::<String>()) else {
                plan.err("Ligne obligatoire");
                return Ok(());
            };
            let sim: Option<Uuid> = sqlx::query_scalar(
                "SELECT s.id FROM sim_cards s JOIN mobile_lines l ON l.id = s.line_id WHERE l.number = $1 AND s.status <> 'desactivee' LIMIT 1",
            )
            .bind(&number)
            .fetch_optional(&mut *conn)
            .await?;
            match sim {
                Some(id) => {
                    d.sim_id = Some(id);
                    d.label = format!("Carte SIM — ligne {number}");
                }
                None => plan.err(format!("Ligne {number} sans SIM reprise (ICCID) : reprenez LignesMobiles avec l'ICCID")),
            }
        }
        _ => {
            plan.err("TypeObjet « Matériel » ou « Ligne » obligatoire");
            return Ok(());
        }
    }
    d.started_at = opt_instant(row, &["DateAttribution", "Date d'attribution"], plan, "DateAttribution");
    d.ended_at = opt_instant(row, &["DateRestitution", "Date de restitution"], plan, "DateRestitution");
    d.condition = row.text(&["EtatRetour", "État au retour"]);
    d.note = row.text(&["Observations"]);
    if let (Some(a), Some(b)) = (d.started_at, d.ended_at) {
        if b < a {
            plan.err("Date de restitution antérieure à l'attribution");
        }
    }
    let (title, who) = resolve_employee(conn, row, "Employe").await?;
    d.employee_id = plan_employee(plan, "Salarié", who, title.as_deref(), true);
    if plan.has_error() {
        return Ok(());
    }
    if d.ended_at.is_none() {
        // Affectation encore active : contrôle des gardes contradictoires.
        let holder: Option<(Uuid, String)> = sqlx::query_as(
            "SELECT a.employee_id, e.first_name || ' ' || e.last_name FROM assignments a JOIN employees e ON e.id = a.employee_id
              WHERE a.ended_at IS NULL AND (a.asset_id = $1 OR a.sim_id = $2)",
        )
        .bind(d.asset_id)
        .bind(d.sim_id)
        .fetch_optional(&mut *conn)
        .await?;
        if let Some((emp, who)) = holder {
            if Some(emp) == d.employee_id {
                plan.proposal = "inchange";
                plan.info("Déjà détenu par ce salarié dans l'application");
            } else if d.employee_id.is_some() {
                plan.err(format!("Affectation active contradictoire : déjà détenu par {who}"));
            }
        } else {
            plan.info("Garde active reprise (mouvement « reprise », sans décharge)");
        }
    } else {
        plan.info("Affectation terminée : reprise dans l'historique, sans changer l'état du bien");
    }
    plan.resolved = to_json(&d);
    Ok(())
}

async fn plan_session(conn: &mut PgConnection, ctx: &Ctx, row: &Row, known: Option<Uuid>, plan: &mut Plan) -> AppResult<()> {
    if known.is_some() {
        plan.proposal = "inchange";
        plan.info("Session déjà reprise");
        return Ok(());
    }
    let Some(name) = row.text(&["Formation"]) else {
        plan.err("Formation obligatoire");
        return Ok(());
    };
    let Some(catalog_id) = ctx.catalog.get(&fold(&name)).copied() else {
        plan.err(format!("Formation « {name} » absente du catalogue : reprenez CatalogueFormations d'abord"));
        return Ok(());
    };
    let planned = opt_date(row, &["DatePrevue", "Date prévue"], plan, "DatePrevue");
    let done = opt_date(row, &["DateRealisee", "Date réalisée"], plan, "DateRealisee");
    let status = choice(row.text(&["Statut"]), legacy::training_status, plan, "Statut").unwrap_or_else(|| {
        if done.is_some() {
            "realisee".into()
        } else if planned.is_some() {
            "planifiee".into()
        } else {
            "a_planifier".into()
        }
    });
    let mut d = SessionData {
        catalog_id,
        employee_id: None,
        done_date: if status == "realisee" { done.or(planned) } else { None },
        planned_date: planned,
        status,
        evaluation: choice(row.text(&["Evaluation", "Évaluation"]), legacy::training_evaluation, plan, "Évaluation"),
        trainer_id: None,
    };
    if d.status == "realisee" && d.done_date.is_none() {
        plan.warn("Session réalisée sans date : date de réalisation inconnue");
    }
    if let Some(email) = row.text(&["FormateurEmail"]) {
        match ctx.users.get(&email.to_lowercase()) {
            Some((id, true)) => d.trainer_id = Some(*id),
            _ => plan.info(format!("Formateur {email} : pas de compte FORMATION actif, intervenant non relié")),
        }
    }
    let (title, who) = resolve_employee(conn, row, "Employe").await?;
    d.employee_id = plan_employee(plan, "Salarié", who, title.as_deref(), true);
    if !["realisee", "dispensee"].contains(&d.status.as_str()) {
        plan.info("Session ouverte reprise sans tâche : visible dans le suivi des formations");
    }
    plan.resolved = to_json(&d);
    Ok(())
}

async fn plan_vehicle(conn: &mut PgConnection, row: &Row, known: Option<Uuid>, plan: &mut Plan) -> AppResult<()> {
    let Some(plate) = row.text(List::Vehicules.title_cols()) else {
        plan.err("Immatriculation obligatoire");
        return Ok(());
    };
    let Some(kind) = choice(row.text(&["TypeVehicule", "Type de véhicule"]), legacy::vehicle_kind, plan, "TypeVehicule") else {
        if !plan.has_error() {
            plan.err("TypeVehicule (Propriété ou Location) obligatoire");
        }
        return Ok(());
    };
    let mut d = VehicleData {
        plate_norm: crate::routes::vehicles::normalize_plate(&plate),
        plate,
        kind,
        brand: row.text(&["Marque"]),
        model: row.text(&["Modele", "Modèle"]),
        odometer_km: opt_int(row, &["Kilometrage", "Kilométrage"], plan, "Kilométrage"),
        status: choice(row.text(&["Statut"]), legacy::vehicle_status, plan, "Statut").unwrap_or_else(|| "disponible".into()),
        lessor: row.text(&["Loueur"]),
        lease_end: opt_date(row, &["DateFinLocation", "Fin de location"], plan, "DateFinLocation"),
        insurance_until: opt_date(row, &["EcheanceAssurance", "Échéance assurance"], plan, "EcheanceAssurance"),
        inspection_until: opt_date(row, &["EcheanceCT", "Échéance CT"], plan, "EcheanceCT"),
        holder_id: None,
    };
    if d.kind == "location" && d.lessor.is_none() {
        plan.warn("Véhicule loué sans loueur : contrat à compléter");
    }
    if d.kind == "propriete" && (d.lessor.is_some() || d.lease_end.is_some()) {
        plan.warn("Véhicule en propriété avec loueur ou fin de location : informations de location ignorées");
        d.lessor = None;
        d.lease_end = None;
    }
    let existing: Option<(Uuid, bool)> = sqlx::query_as(
        "SELECT v.id, EXISTS (SELECT 1 FROM vehicle_assignments a WHERE a.vehicle_id = v.id AND a.ended_at IS NULL)
           FROM vehicles v WHERE v.id = $1 OR v.plate_norm = $2 LIMIT 1",
    )
    .bind(known)
    .bind(&d.plate_norm)
    .fetch_optional(&mut *conn)
    .await?;
    let held = existing.map(|e| e.1).unwrap_or(false);
    if let Some((id, _)) = existing {
        plan.target_id = Some(id);
        plan.proposal = "mise_a_jour";
    }
    let (title, who) = resolve_employee(conn, row, "AttribueA").await?;
    if title.is_some() {
        if held {
            plan.info("Véhicule déjà confié dans l'application : garde inchangée");
        } else if d.status != "disponible" {
            plan.warn(format!("Véhicule « {} » avec titulaire : garde non reprise, à vérifier", d.status));
        } else {
            d.holder_id = plan_employee(plan, "Titulaire", who, title.as_deref(), false);
        }
    }
    plan.resolved = to_json(&d);
    Ok(())
}

async fn plan_vehicle_assignment(conn: &mut PgConnection, row: &Row, known: Option<Uuid>, plan: &mut Plan) -> AppResult<()> {
    if known.is_some() {
        plan.proposal = "inchange";
        plan.info("Affectation déjà reprise");
        return Ok(());
    }
    let Some(plate) = row.text(&["Vehicule", "Véhicule"]) else {
        plan.err("Véhicule obligatoire");
        return Ok(());
    };
    let plate_norm = crate::routes::vehicles::normalize_plate(&plate);
    let v: Option<Uuid> = sqlx::query_scalar("SELECT id FROM vehicles WHERE plate_norm = $1")
        .bind(&plate_norm)
        .fetch_optional(&mut *conn)
        .await?;
    let Some(vehicle_id) = v else {
        plan.err(format!("Véhicule {plate} inconnu : reprenez Vehicules d'abord"));
        return Ok(());
    };
    let mut d = VehicleAssignmentData {
        vehicle_id,
        plate,
        started_at: opt_instant(row, &["DateRemise", "Date de remise"], plan, "DateRemise"),
        ended_at: opt_instant(row, &["DateRestitution", "Date de restitution"], plan, "DateRestitution"),
        km_start: opt_int(row, &["KmDepart", "Km départ"], plan, "KmDepart"),
        km_end: opt_int(row, &["KmRetour", "Km retour"], plan, "KmRetour"),
        condition_start: row.text(&["EtatDepart", "État au départ"]),
        note_end: row.text(&["ObservationsRetour", "Observations au retour"]),
        employee_id: None,
    };
    if let (Some(a), Some(b)) = (d.km_start, d.km_end) {
        if b < a {
            plan.err("Kilométrage de retour inférieur au départ");
        }
    }
    let (title, who) = resolve_employee(conn, row, "Employe").await?;
    d.employee_id = plan_employee(plan, "Salarié", who, title.as_deref(), true);
    if d.ended_at.is_none() {
        let holder: Option<(Uuid, String)> = sqlx::query_as(
            "SELECT a.employee_id, e.first_name || ' ' || e.last_name FROM vehicle_assignments a JOIN employees e ON e.id = a.employee_id
              WHERE a.vehicle_id = $1 AND a.ended_at IS NULL",
        )
        .bind(vehicle_id)
        .fetch_optional(&mut *conn)
        .await?;
        if let Some((emp, who)) = holder {
            if Some(emp) == d.employee_id {
                plan.proposal = "inchange";
                plan.info("Déjà affecté à ce salarié dans l'application");
            } else if d.employee_id.is_some() {
                plan.err(format!("Affectation active contradictoire : véhicule déjà confié à {who}"));
            }
        }
    }
    plan.resolved = to_json(&d);
    Ok(())
}

// ------------------------------------------------------------------ aperçu

async fn preview(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(list): Path<String>,
    mut multipart: Multipart,
) -> AppResult<Json<Value>> {
    user.require(&[Role::Admin])?;
    let list = List::parse(&list).ok_or_else(|| AppError::invalid("list", "liste inconnue"))?;
    let mut file: Option<(String, Vec<u8>)> = None;
    while let Some(field) = multipart.next_field().await.map_err(|e| AppError::invalid("file", e.to_string()))? {
        if field.name() == Some("file") {
            let name = field.file_name().unwrap_or("export.csv").chars().take(120).collect::<String>();
            let data = field.bytes().await.map_err(|e| AppError::invalid("file", e.to_string()))?;
            file = Some((name, data.to_vec()));
        }
    }
    let (file_name, bytes) = file.ok_or_else(|| AppError::invalid("file", "fichier manquant"))?;
    if bytes.is_empty() || bytes.len() > state.config.max_upload_bytes {
        return Err(AppError::invalid("file", "fichier vide ou trop volumineux"));
    }
    let hash = sha256_hex(&bytes);
    let name_for_read = file_name.clone();
    let table: Table = tokio::task::spawn_blocking(move || tabular::read(&name_for_read, &bytes))
        .await
        .map_err(anyhow::Error::from)?
        .map_err(|e| AppError::invalid("file", e))?;
    let missing: Vec<&str> = list
        .columns()
        .iter()
        .filter(|(c, req)| *req && !table.has(&[*c]) && !(list.title_cols().contains(c) && table.has(list.title_cols())))
        .map(|(c, _)| *c)
        .collect();
    if !missing.is_empty() {
        return Err(AppError::invalid(
            "file",
            format!("colonnes obligatoires absentes pour {} : {} (colonnes lues : {})", list.label(), missing.join(", "), table.headers.join(", ")),
        ));
    }
    if table.rows.is_empty() {
        return Err(AppError::invalid("file", "aucune ligne sous les en-têtes"));
    }
    let has_id = table.has(&["ID"]);

    let mut tx = state.pool.begin().await?;
    let already: Option<DateTime<Utc>> = sqlx::query_scalar(
        "SELECT committed_at FROM migration_batches WHERE source_list = $1 AND file_sha256 = $2 AND status = 'applique'",
    )
    .bind(list.name())
    .bind(&hash)
    .fetch_optional(&mut *tx)
    .await?
    .flatten();
    let ctx = Ctx::load(&mut tx).await?;
    let batch_id: Uuid = sqlx::query_scalar(
        "INSERT INTO migration_batches (source_list, file_name, file_sha256, created_by) VALUES ($1, $2, $3, $4) RETURNING id",
    )
    .bind(list.name())
    .bind(&file_name)
    .bind(&hash)
    .bind(user.id)
    .fetch_one(&mut *tx)
    .await?;
    let mut seen: HashMap<String, u32> = HashMap::new();
    let mut counts: HashMap<&str, i64> = HashMap::new();
    for row in &table.rows {
        let source_id = row.text(&["ID"]).filter(|_| has_id);
        let mut plan = plan_row(&mut tx, list, &ctx, row, source_id.as_deref()).await?;
        // Doublon dans le même fichier (même ancien ID ou même clé).
        let key = source_id.clone().or_else(|| row.text(list.title_cols()).map(|t| fold(&t)));
        if let Some(k) = key.filter(|_| !matches!(list, List::Affectations | List::Formations | List::AffectationsVehicules) || has_id) {
            if let Some(first) = seen.insert(k, row.number) {
                plan.messages.push(msg("erreur", format!("Doublon de la ligne {first} dans ce fichier")));
                plan.proposal = "rejet";
            }
        }
        if !has_id && matches!(list, List::Personnel) && plan.proposal == "creation" {
            plan.messages.push(msg("info", "Export sans colonne ID : les liens (Manager, Affectations) passent par le nom complet"));
        }
        *counts.entry(plan.proposal).or_default() += 1;
        sqlx::query(
            "INSERT INTO migration_rows (batch_id, row_number, source_id, raw, proposal, messages, target_id, candidates, resolved)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
        )
        .bind(batch_id)
        .bind(row.number as i32)
        .bind(&source_id)
        .bind(row.raw_json())
        .bind(plan.proposal)
        .bind(Value::Array(plan.messages))
        .bind(plan.target_id)
        .bind(Value::Array(plan.candidates))
        .bind(&plan.resolved)
        .execute(&mut *tx)
        .await?;
    }
    let summary = json!({ "lignes": table.rows.len(), "propositions": counts, "colonnes": table.headers, "avec_id": has_id });
    sqlx::query("UPDATE migration_batches SET summary = $2 WHERE id = $1")
        .bind(batch_id)
        .bind(&summary)
        .execute(&mut *tx)
        .await?;
    audit::record(
        &mut tx,
        &user,
        Audit { action: "reprise_apercu", entity: "migration_batch", entity_id: Some(batch_id), data: Some(json!({ "liste": list.name(), "fichier": file_name, "sha256": hash })), ..Default::default() },
    )
    .await?;
    tx.commit().await?;
    Ok(Json(json!({ "id": batch_id, "summary": summary, "already_applied_at": already })))
}

async fn batches(State(state): State<AppState>, user: CurrentUser) -> AppResult<Json<Value>> {
    user.require(&[Role::Admin])?;
    let v: Value = sqlx::query_scalar(
        "SELECT coalesce(jsonb_agg(jsonb_build_object('id', b.id, 'source_list', b.source_list, 'file_name', b.file_name,
                    'status', b.status, 'summary', b.summary, 'created_at', b.created_at, 'committed_at', b.committed_at,
                    'created_by', u.display_name) ORDER BY b.created_at DESC), '[]')
           FROM (SELECT * FROM migration_batches ORDER BY created_at DESC LIMIT 100) b JOIN users u ON u.id = b.created_by",
    )
    .fetch_one(&state.pool)
    .await?;
    Ok(Json(v))
}

async fn batch_detail(State(state): State<AppState>, user: CurrentUser, Path(id): Path<Uuid>) -> AppResult<Json<Value>> {
    user.require(&[Role::Admin])?;
    let batch: Option<Value> = sqlx::query_scalar(
        "SELECT jsonb_build_object('id', b.id, 'source_list', b.source_list, 'file_name', b.file_name, 'file_sha256', b.file_sha256,
                'status', b.status, 'summary', b.summary, 'created_at', b.created_at, 'committed_at', b.committed_at,
                'created_by', u.display_name)
           FROM migration_batches b JOIN users u ON u.id = b.created_by WHERE b.id = $1",
    )
    .bind(id)
    .fetch_optional(&state.pool)
    .await?;
    let batch = batch.ok_or_else(|| AppError::not_found("lot de reprise"))?;
    let rows: Value = sqlx::query_scalar(
        "SELECT coalesce(jsonb_agg(jsonb_build_object('row_number', row_number, 'source_id', source_id, 'raw', raw,
                    'proposal', proposal, 'messages', messages, 'target_id', target_id, 'candidates', candidates,
                    'decision', decision, 'decision_target', decision_target, 'result', result, 'result_id', result_id,
                    'result_message', result_message) ORDER BY row_number), '[]')
           FROM migration_rows WHERE batch_id = $1",
    )
    .bind(id)
    .fetch_one(&state.pool)
    .await?;
    Ok(Json(json!({ "batch": batch, "rows": rows })))
}

#[derive(Deserialize)]
struct DecisionInput {
    decision: String,
    target_id: Option<Uuid>,
}

/// Décision nominative sur une ligne à revoir : appliquer (en choisissant
/// au besoin la fiche concernée) ou exclure.
async fn decide(
    State(state): State<AppState>,
    user: CurrentUser,
    Path((id, row)): Path<(Uuid, i32)>,
    Json(i): Json<DecisionInput>,
) -> AppResult<Json<Value>> {
    user.require(&[Role::Admin])?;
    if !["appliquer", "exclure"].contains(&i.decision.as_str()) {
        return Err(AppError::invalid("decision", "appliquer ou exclure"));
    }
    let mut tx = state.pool.begin().await?;
    let status: Option<String> = sqlx::query_scalar("SELECT status FROM migration_batches WHERE id = $1 FOR UPDATE")
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?;
    if status.ok_or_else(|| AppError::not_found("lot de reprise"))? != "apercu" {
        return Err(AppError::conflict("lot déjà appliqué ou abandonné"));
    }
    let r: Option<(String, Value)> = sqlx::query_as("SELECT proposal, candidates FROM migration_rows WHERE batch_id = $1 AND row_number = $2")
        .bind(id)
        .bind(row)
        .fetch_optional(&mut *tx)
        .await?;
    let (proposal, candidates) = r.ok_or_else(|| AppError::not_found("ligne"))?;
    if proposal == "rejet" {
        return Err(AppError::conflict("ligne rejetée : corrigez le fichier et chargez un nouveau lot"));
    }
    if let Some(t) = i.target_id {
        let listed = candidates.as_array().map(|c| c.iter().any(|x| x["id"] == json!(t))).unwrap_or(false);
        if !listed {
            return Err(AppError::invalid("target_id", "choisissez une des fiches proposées"));
        }
    }
    sqlx::query(
        "UPDATE migration_rows SET decision = $3, decision_target = $4, decided_by = $5, decided_at = now()
          WHERE batch_id = $1 AND row_number = $2",
    )
    .bind(id)
    .bind(row)
    .bind(&i.decision)
    .bind(i.target_id)
    .bind(user.id)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(Json(json!({ "ok": true })))
}

async fn abandon(State(state): State<AppState>, user: CurrentUser, Path(id): Path<Uuid>) -> AppResult<Json<Value>> {
    user.require(&[Role::Admin])?;
    let n = sqlx::query("UPDATE migration_batches SET status = 'abandonne' WHERE id = $1 AND status = 'apercu'")
        .bind(id)
        .execute(&state.pool)
        .await?
        .rows_affected();
    if n == 0 {
        return Err(AppError::conflict("lot introuvable, déjà appliqué ou abandonné"));
    }
    Ok(Json(json!({ "ok": true })))
}

// ------------------------------------------------------------------ application

struct PendingRow {
    row_number: i32,
    source_id: Option<String>,
    raw_title: Option<String>,
    proposal: String,
    target_id: Option<Uuid>,
    resolved: Value,
    decision: Option<String>,
    decision_target: Option<Uuid>,
}

async fn commit(State(state): State<AppState>, user: CurrentUser, Path(id): Path<Uuid>) -> AppResult<Json<Value>> {
    user.require(&[Role::Admin])?;
    let mut tx = state.pool.begin().await?;
    let row: Option<(String, String)> = sqlx::query_as("SELECT status, source_list FROM migration_batches WHERE id = $1 FOR UPDATE")
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?;
    let (status, list_name) = row.ok_or_else(|| AppError::not_found("lot de reprise"))?;
    if status != "apercu" {
        return Err(AppError::conflict("lot déjà appliqué ou abandonné"));
    }
    let list = List::parse(&list_name).ok_or_else(|| anyhow::anyhow!("liste inconnue"))?;
    let rows: Vec<(i32, Option<String>, Value, String, Option<Uuid>, Value, Option<String>, Option<Uuid>)> = sqlx::query_as(
        "SELECT row_number, source_id, raw, proposal, target_id, resolved, decision, decision_target
           FROM migration_rows WHERE batch_id = $1 ORDER BY row_number",
    )
    .bind(id)
    .fetch_all(&mut *tx)
    .await?;
    let title_keys: Vec<String> = list.title_cols().iter().map(|c| fold(c)).collect();
    let pending: Vec<PendingRow> = rows
        .into_iter()
        .map(|(n, sid, raw, p, t, r, d, dt)| PendingRow {
            row_number: n,
            source_id: sid,
            raw_title: title_keys.iter().find_map(|k| raw.get(k).and_then(|v| v.as_str()).map(String::from)),
            proposal: p,
            target_id: t,
            resolved: r,
            decision: d,
            decision_target: dt,
        })
        .collect();

    let mut counts: HashMap<&str, i64> = HashMap::new();
    let mut managers: Vec<(Uuid, PersonData)> = Vec::new();
    for r in &pending {
        let (result, result_id, message): (&str, Option<Uuid>, Option<String>) = match (r.proposal.as_str(), r.decision.as_deref()) {
            ("rejet", _) => ("rejete", None, Some("ligne rejetée à l'aperçu".into())),
            ("inchange", _) => ("inchange", r.target_id, None),
            (_, Some("exclure")) => ("ignore", None, Some("exclue par décision".into())),
            ("a_revoir", None) => ("ignore", None, Some("décision requise : ligne non appliquée".into())),
            _ => {
                sqlx::query("SAVEPOINT reprise_ligne").execute(&mut *tx).await?;
                match apply_row(&mut tx, &user, list, r, id).await {
                    Ok((result, entity_id, extra)) => {
                        sqlx::query("RELEASE SAVEPOINT reprise_ligne").execute(&mut *tx).await?;
                        if let Some(p) = extra {
                            managers.push((entity_id, p));
                        }
                        (result, Some(entity_id), None)
                    }
                    Err(e) => {
                        sqlx::query("ROLLBACK TO SAVEPOINT reprise_ligne").execute(&mut *tx).await?;
                        let m = match &e {
                            AppError::Internal(_) => "erreur interne sur cette ligne".to_string(),
                            other => other.to_string(),
                        };
                        if let AppError::Internal(err) = &e {
                            tracing::error!(error = ?err, "reprise : ligne en échec");
                        }
                        ("conflit", None, Some(m))
                    }
                }
            }
        };
        *counts.entry(result).or_default() += 1;
        sqlx::query("UPDATE migration_rows SET result = $3, result_id = $4, result_message = $5 WHERE batch_id = $1 AND row_number = $2")
            .bind(id)
            .bind(r.row_number)
            .bind(result)
            .bind(result_id)
            .bind(&message)
            .execute(&mut *tx)
            .await?;
    }
    // Liens entre fiches du Personnel : le manager est relié après coup.
    let mut linked = 0;
    for (emp, p) in &managers {
        let manager: Option<Uuid> = match (&p.manager_source_id, &p.manager_title) {
            (Some(sid), _) => legacy_entity(&mut tx, "Personnel", sid).await?,
            (None, Some(t)) => {
                let ids: Vec<Uuid> = sqlx::query_scalar(
                    "SELECT entity_id FROM legacy_ids WHERE source_list = 'Personnel' AND lower(source_title) = lower($1)",
                )
                .bind(t)
                .fetch_all(&mut *tx)
                .await?;
                (ids.len() == 1).then(|| ids[0])
            }
            _ => None,
        };
        if let Some(m) = manager.filter(|m| m != emp) {
            sqlx::query(
                "UPDATE employee_positions SET manager_employee_id = $2,
                        manager = coalesce(manager, (SELECT first_name || ' ' || last_name FROM employees WHERE id = $2))
                  WHERE employee_id = $1 AND valid_to IS NULL",
            )
            .bind(emp)
            .bind(m)
            .execute(&mut *tx)
            .await?;
            linked += 1;
        }
    }
    let summary = json!({ "resultats": counts, "managers_relies": linked });
    sqlx::query(
        "UPDATE migration_batches SET status = 'applique', committed_at = now(), committed_by = $2,
                summary = coalesce(summary, '{}'::jsonb) || jsonb_build_object('application', $3::jsonb)
          WHERE id = $1",
    )
    .bind(id)
    .bind(user.id)
    .bind(&summary)
    .execute(&mut *tx)
    .await
    .map_err(|e| match AppError::from(e) {
        AppError::Conflict { .. } => AppError::conflict("ce fichier a déjà été appliqué pour cette liste"),
        other => other,
    })?;
    audit::record(
        &mut tx,
        &user,
        Audit { action: "reprise_appliquee", entity: "migration_batch", entity_id: Some(id), data: Some(json!({ "liste": list.name(), "resultats": summary })), ..Default::default() },
    )
    .await?;
    tx.commit().await?;
    Ok(Json(summary))
}

async fn remember(conn: &mut PgConnection, list: List, r: &PendingRow, entity_id: Uuid, natural: Option<String>, batch: Uuid) -> AppResult<()> {
    let Some(source_id) = r.source_id.clone().or(natural) else { return Ok(()) };
    sqlx::query(
        "INSERT INTO legacy_ids (source_list, source_id, entity, entity_id, source_title, batch_id)
         VALUES ($1, $2, $3, $4, $5, $6)
         ON CONFLICT (source_list, source_id) DO UPDATE SET source_title = coalesce(EXCLUDED.source_title, legacy_ids.source_title)",
    )
    .bind(list.name())
    .bind(source_id)
    .bind(list.entity())
    .bind(entity_id)
    .bind(&r.raw_title)
    .bind(batch)
    .execute(conn)
    .await?;
    Ok(())
}

fn data<T: for<'de> Deserialize<'de>>(v: &Value) -> AppResult<T> {
    serde_json::from_value(v.clone()).map_err(|e| AppError::Internal(anyhow::anyhow!("données de reprise illisibles : {e}")))
}

async fn apply_row(
    conn: &mut PgConnection,
    user: &CurrentUser,
    list: List,
    r: &PendingRow,
    batch: Uuid,
) -> AppResult<(&'static str, Uuid, Option<PersonData>)> {
    // Personnel : la fiche choisie est la cible (rattachement). Autres
    // listes : la fiche choisie est le salarié lié (détenteur, titulaire).
    let (update, target) = if list == List::Personnel {
        (r.proposal == "mise_a_jour" || (r.proposal == "a_revoir" && r.decision_target.is_some()), r.decision_target.or(r.target_id))
    } else {
        (r.proposal == "mise_a_jour", r.target_id)
    };
    let mut extra = None;
    let (result, entity_id): (&'static str, Uuid) = match list {
        List::Wilayas => {
            let d: WilayaData = data(&r.resolved)?;
            let id: Uuid = sqlx::query_scalar(
                "INSERT INTO wilayas (code, name, region, sort_order) VALUES ($1, $2, $3, $4)
                 ON CONFLICT (code) DO UPDATE SET name = EXCLUDED.name, region = coalesce(EXCLUDED.region, wilayas.region),
                        sort_order = coalesce(EXCLUDED.sort_order, wilayas.sort_order)
                 RETURNING id",
            )
            .bind(d.code)
            .bind(&d.name)
            .bind(&d.region)
            .bind(d.sort_order)
            .fetch_one(&mut *conn)
            .await?;
            remember(conn, list, r, id, Some(format!("code:{}", d.code)), batch).await?;
            (if update { "mis_a_jour" } else { "cree" }, id)
        }
        List::CatalogueFormations => {
            let d: CatalogData = data(&r.resolved)?;
            let id: Uuid = match target {
                Some(id) => {
                    sqlx::query(
                        "UPDATE training_catalog SET name = $2, category = coalesce($3, category), duration_hours = coalesce($4, duration_hours),
                                description = coalesce($5, description), support_url = coalesce($6, support_url),
                                default_trainer_id = coalesce($7, default_trainer_id)
                          WHERE id = $1",
                    )
                    .bind(id)
                    .bind(&d.name)
                    .bind(&d.category)
                    .bind(d.duration_hours)
                    .bind(&d.description)
                    .bind(&d.support_url)
                    .bind(d.default_trainer_id)
                    .execute(&mut *conn)
                    .await?;
                    id
                }
                None => {
                    sqlx::query_scalar(
                        "INSERT INTO training_catalog (name, category, duration_hours, description, support_url, default_trainer_id)
                         VALUES ($1, $2, $3, $4, $5, $6) RETURNING id",
                    )
                    .bind(&d.name)
                    .bind(&d.category)
                    .bind(d.duration_hours)
                    .bind(&d.description)
                    .bind(&d.support_url)
                    .bind(d.default_trainer_id)
                    .fetch_one(&mut *conn)
                    .await?
                }
            };
            remember(conn, list, r, id, Some(format!("nom:{}", fold(&d.name))), batch).await?;
            (if target.is_some() { "mis_a_jour" } else { "cree" }, id)
        }
        List::UtilisateursApp => {
            let d: UserData = data(&r.resolved)?;
            let id: Uuid = match target {
                Some(id) => id,
                None => {
                    sqlx::query_scalar("INSERT INTO users (email, display_name, active) VALUES ($1, $2, $3) RETURNING id")
                        .bind(&d.email)
                        .bind(&d.display_name)
                        .bind(d.active)
                        .fetch_one(&mut *conn)
                        .await?
                }
            };
            for role in &d.roles {
                sqlx::query("INSERT INTO user_roles (user_id, role) VALUES ($1, $2) ON CONFLICT DO NOTHING")
                    .bind(id)
                    .bind(role)
                    .execute(&mut *conn)
                    .await?;
            }
            remember(conn, list, r, id, Some(format!("email:{}", d.email)), batch).await?;
            (if target.is_some() { "mis_a_jour" } else { "cree" }, id)
        }
        List::ProfilsPoste => {
            let d: ProfileData = data(&r.resolved)?;
            let id: Uuid = match target {
                Some(id) => {
                    sqlx::query(
                        "UPDATE job_profiles SET name = $2, department = $3, description = $4, typical_titles = $5, access_notes = $6,
                                field_based = $7, version = version + 1, updated_at = now() WHERE id = $1",
                    )
                    .bind(id)
                    .bind(&d.name)
                    .bind(&d.department)
                    .bind(&d.description)
                    .bind(&d.typical_titles)
                    .bind(&d.access_notes)
                    .bind(d.field_based)
                    .execute(&mut *conn)
                    .await?;
                    sqlx::query("DELETE FROM profile_needs WHERE profile_id = $1").bind(id).execute(&mut *conn).await?;
                    sqlx::query("DELETE FROM profile_trainings WHERE profile_id = $1").bind(id).execute(&mut *conn).await?;
                    id
                }
                None => {
                    sqlx::query_scalar(
                        "INSERT INTO job_profiles (name, department, description, typical_titles, access_notes, field_based)
                         VALUES ($1, $2, $3, $4, $5, $6) RETURNING id",
                    )
                    .bind(&d.name)
                    .bind(&d.department)
                    .bind(&d.description)
                    .bind(&d.typical_titles)
                    .bind(&d.access_notes)
                    .bind(d.field_based)
                    .fetch_one(&mut *conn)
                    .await?
                }
            };
            for n in &d.needs {
                sqlx::query("INSERT INTO profile_needs (profile_id, category, preference, detail) VALUES ($1, $2, $3, $4)")
                    .bind(id)
                    .bind(&n.category)
                    .bind(&n.preference)
                    .bind(&n.detail)
                    .execute(&mut *conn)
                    .await?;
            }
            for t in &d.trainings {
                sqlx::query("INSERT INTO profile_trainings (profile_id, catalog_id) VALUES ($1, $2) ON CONFLICT DO NOTHING")
                    .bind(id)
                    .bind(t)
                    .execute(&mut *conn)
                    .await?;
            }
            remember(conn, list, r, id, Some(format!("nom:{}", fold(&d.name))), batch).await?;
            (if target.is_some() { "mis_a_jour" } else { "cree" }, id)
        }
        List::Personnel => {
            let d: PersonData = data(&r.resolved)?;
            let (result, id) = apply_person(conn, user, &d, if update { target } else { None }).await?;
            let natural = d.matricule.as_deref().and_then(domain::text::normalize_matricule).map(|m| format!("matricule:{m}"));
            remember(conn, list, r, id, natural.or_else(|| Some(format!("ligne:{}:{}", batch, r.row_number))), batch).await?;
            if d.manager_title.is_some() || d.manager_source_id.is_some() {
                extra = Some(d);
            }
            (result, id)
        }
        List::MaterielIT => {
            let mut d: AssetData = data(&r.resolved)?;
            if d.holder_id.is_none() {
                d.holder_id = r.decision_target;
            }
            let id = apply_asset(conn, user, &d, target).await?;
            remember(conn, list, r, id, Some(format!("inv:{}", d.inventory_no)), batch).await?;
            (if target.is_some() { "mis_a_jour" } else { "cree" }, id)
        }
        List::LignesMobiles => {
            let mut d: LineData = data(&r.resolved)?;
            if d.holder_id.is_none() {
                d.holder_id = r.decision_target;
            }
            let id = apply_line(conn, user, &d, target).await?;
            remember(conn, list, r, id, Some(format!("num:{}", d.number)), batch).await?;
            (if target.is_some() { "mis_a_jour" } else { "cree" }, id)
        }
        List::Affectations => {
            let mut d: AssignmentData = data(&r.resolved)?;
            if d.employee_id.is_none() {
                d.employee_id = r.decision_target;
            }
            let employee = d.employee_id.ok_or_else(|| AppError::invalid("employe", "salarié à choisir"))?;
            let item = match (d.asset_id, d.sim_id) {
                (Some(a), _) => Item::Asset(a),
                (_, Some(s)) => Item::Sim(s),
                _ => return Err(AppError::invalid("objet", "objet introuvable")),
            };
            let id = reprise_assignment(conn, user, employee, item, &d.label, d.started_at, d.ended_at, d.condition.as_deref(), d.note.as_deref()).await?;
            remember(conn, list, r, id, None, batch).await?;
            ("cree", id)
        }
        List::Formations => {
            let mut d: SessionData = data(&r.resolved)?;
            if d.employee_id.is_none() {
                d.employee_id = r.decision_target;
            }
            let employee = d.employee_id.ok_or_else(|| AppError::invalid("employe", "salarié à choisir"))?;
            let id: Uuid = sqlx::query_scalar(
                "INSERT INTO training_sessions (employee_id, catalog_id, trainer_id, status, planned_date, done_date, evaluation, reason)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, 'Reprise SharePoint') RETURNING id",
            )
            .bind(employee)
            .bind(d.catalog_id)
            .bind(d.trainer_id)
            .bind(&d.status)
            .bind(d.planned_date)
            .bind(d.done_date)
            .bind(&d.evaluation)
            .fetch_one(&mut *conn)
            .await?;
            remember(conn, list, r, id, None, batch).await?;
            ("cree", id)
        }
        List::Vehicules => {
            let mut d: VehicleData = data(&r.resolved)?;
            if d.holder_id.is_none() {
                d.holder_id = r.decision_target;
            }
            let id = crate::routes::vehicles::apply_vehicle_reprise(conn, user, &d, target).await?;
            remember(conn, list, r, id, Some(format!("immat:{}", crate::routes::vehicles::normalize_plate(&d.plate))), batch).await?;
            (if target.is_some() { "mis_a_jour" } else { "cree" }, id)
        }
        List::AffectationsVehicules => {
            let mut d: VehicleAssignmentData = data(&r.resolved)?;
            if d.employee_id.is_none() {
                d.employee_id = r.decision_target;
            }
            let id = crate::routes::vehicles::apply_vehicle_assignment_reprise(conn, user, &d).await?;
            remember(conn, list, r, id, None, batch).await?;
            ("cree", id)
        }
    };
    audit::record(
        conn,
        user,
        Audit {
            action: "reprise_ligne",
            entity: list.entity(),
            entity_id: Some(entity_id),
            employee_id: (list == List::Personnel).then_some(entity_id),
            data: Some(json!({ "liste": list.name(), "ligne": r.row_number, "ancien_id": r.source_id, "resultat": result })),
            ..Default::default()
        },
    )
    .await?;
    Ok((result, entity_id, extra))
}

async fn apply_person(conn: &mut PgConnection, user: &CurrentUser, d: &PersonData, target: Option<Uuid>) -> AppResult<(&'static str, Uuid)> {
    let site_id = d.site_id.ok_or_else(|| AppError::invalid("site", "site obligatoire"))?;
    let (result, id) = match target {
        None => {
            let (planned, actual) = match d.status.as_str() {
                "a_venir" => (d.entry_date, None),
                _ => (None, d.entry_date),
            };
            let id = insert_employee(
                conn,
                &NewEmployee {
                    first_name: d.first_name.clone(),
                    last_name: d.last_name.clone(),
                    matricule: d.matricule.clone(),
                    job_title: d.job_title.clone().unwrap_or_else(|| "À préciser".into()),
                    site_id,
                    department: d.department.clone(),
                    manager: d.manager_name.clone().or_else(|| d.manager_title.clone()),
                    status: "a_venir",
                    planned_arrival: planned,
                    actual_arrival: actual,
                },
            )
            .await?;
            sqlx::query("UPDATE employees SET status = $2 WHERE id = $1")
                .bind(id)
                .bind(if d.status == "sorti" { "actif" } else { d.status.as_str() })
                .execute(&mut *conn)
                .await?;
            ("cree", id)
        }
        Some(id) => {
            // Mise à jour prudente : on complète, on ne réécrit pas le passé.
            let current: Option<(String, Option<NaiveDate>)> =
                sqlx::query_as("SELECT status, arrival_confirmed_at::date FROM employees WHERE id = $1 FOR UPDATE")
                    .bind(id)
                    .fetch_optional(&mut *conn)
                    .await?;
            let (status, confirmed) = current.ok_or_else(|| AppError::not_found("salarié"))?;
            if let Some(m) = d.matricule.as_deref().and_then(domain::text::clean) {
                sqlx::query("UPDATE employees SET matricule = $2, matricule_norm = $3 WHERE id = $1 AND matricule IS NULL")
                    .bind(id)
                    .bind(&m)
                    .bind(domain::text::normalize_matricule(&m))
                    .execute(&mut *conn)
                    .await?;
            }
            if status == "a_venir" && d.status == "actif" && confirmed.is_none() {
                sqlx::query("UPDATE employees SET status = 'actif', actual_arrival = coalesce($2, actual_arrival) WHERE id = $1")
                    .bind(id)
                    .bind(d.entry_date)
                    .execute(&mut *conn)
                    .await?;
            }
            sqlx::query(
                "UPDATE employee_positions SET job_title = coalesce($2, job_title), site_id = $3, department = coalesce($4, department)
                  WHERE employee_id = $1 AND valid_to IS NULL",
            )
            .bind(id)
            .bind(&d.job_title)
            .bind(site_id)
            .bind(&d.department)
            .execute(&mut *conn)
            .await?;
            ("mis_a_jour", id)
        }
    };
    sqlx::query(
        "UPDATE employees SET contract_type = coalesce($2, contract_type), landline = coalesce($3, landline),
                m365_verified_at = coalesce(m365_verified_at, $4), m365_verified_by = CASE WHEN m365_verified_at IS NULL AND $4::timestamptz IS NOT NULL THEN $5 ELSE m365_verified_by END,
                version = version + 1, updated_at = now()
          WHERE id = $1",
    )
    .bind(id)
    .bind(&d.contract_type)
    .bind(&d.landline)
    .bind(d.m365_verified_at)
    .bind(user.id)
    .execute(&mut *conn)
    .await?;
    if let Some(p) = d.profile_id {
        sqlx::query("UPDATE employee_positions SET job_profile_id = $2 WHERE employee_id = $1 AND valid_to IS NULL")
            .bind(id)
            .bind(p)
            .execute(&mut *conn)
            .await?;
    }
    for w in &d.wilaya_ids {
        sqlx::query("INSERT INTO employee_wilayas (employee_id, wilaya_id) VALUES ($1, $2) ON CONFLICT DO NOTHING")
            .bind(id)
            .bind(w)
            .execute(&mut *conn)
            .await?;
    }
    if let (Some(email), Some(platform)) = (&d.email_pro, &d.email_platform) {
        sqlx::query(
            "INSERT INTO employee_accounts (employee_id, system, platform, identifier, created_by, verified_at)
             SELECT $1, 'email', $2, $3, $4, now()
              WHERE NOT EXISTS (SELECT 1 FROM employee_accounts WHERE system = 'email' AND lower(identifier) = lower($3) AND status <> 'coupe')",
        )
        .bind(id)
        .bind(platform)
        .bind(email)
        .bind(user.id)
        .execute(&mut *conn)
        .await?;
    }
    // Sortie : statut et fin de la situation, sans dossier de départ.
    if d.status == "sorti" {
        let is_out: bool = sqlx::query_scalar("SELECT status = 'sorti' FROM employees WHERE id = $1").bind(id).fetch_one(&mut *conn).await?;
        if !is_out {
            sqlx::query("UPDATE employees SET status = 'sorti', exit_date = $2 WHERE id = $1")
                .bind(id)
                .bind(d.exit_date)
                .execute(&mut *conn)
                .await?;
            if let Some(exit) = d.exit_date {
                sqlx::query(
                    "UPDATE employee_positions SET valid_to = GREATEST($2::date + 1, valid_from + 1) WHERE employee_id = $1 AND valid_to IS NULL",
                )
                .bind(id)
                .bind(exit)
                .execute(&mut *conn)
                .await?;
            }
        }
    }
    Ok((result, id))
}

enum Item {
    Asset(Uuid),
    Sim(Uuid),
}

/// Garde reprise : mouvement « reprise » (pas une remise, aucune décharge
/// fabriquée), une ligne et l'affectation correspondante. Une affectation
/// déjà terminée est reprise dans l'historique sans changer l'état du bien.
#[allow(clippy::too_many_arguments)]
async fn reprise_assignment(
    conn: &mut PgConnection,
    user: &CurrentUser,
    employee_id: Uuid,
    item: Item,
    label: &str,
    started_at: Option<DateTime<Utc>>,
    ended_at: Option<DateTime<Utc>>,
    condition: Option<&str>,
    note: Option<&str>,
) -> AppResult<Uuid> {
    let at = started_at.unwrap_or_else(Utc::now);
    let (item_type, asset_id, sim_id) = match item {
        Item::Asset(a) => ("asset", Some(a), None),
        Item::Sim(s) => ("sim", None, Some(s)),
    };
    if ended_at.is_none() {
        let held: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM assignments WHERE ended_at IS NULL AND (asset_id = $1 OR sim_id = $2))",
        )
        .bind(asset_id)
        .bind(sim_id)
        .fetch_one(&mut *conn)
        .await?;
        if held {
            return Err(AppError::conflict("bien déjà détenu dans l'application"));
        }
    }
    let identifier: Option<String> = match item {
        Item::Asset(a) => sqlx::query_scalar("SELECT inventory_no || coalesce(' / ' || serial_no, '') FROM assets WHERE id = $1 FOR UPDATE")
            .bind(a)
            .fetch_optional(&mut *conn)
            .await?,
        Item::Sim(s) => sqlx::query_scalar("SELECT iccid FROM sim_cards WHERE id = $1 FOR UPDATE").bind(s).fetch_optional(&mut *conn).await?,
    };
    let n: i64 = sqlx::query_scalar("SELECT nextval('movement_ref')").fetch_one(&mut *conn).await?;
    let reference = domain::cases::format_movement_reference(n);
    let movement: Uuid = sqlx::query_scalar(
        "INSERT INTO movements (reference, kind, employee_id, operator_id, effective_at, note)
         VALUES ($1, 'reprise', $2, $3, $4, $5) RETURNING id",
    )
    .bind(&reference)
    .bind(employee_id)
    .bind(user.id)
    .bind(at)
    .bind(Some(match note {
        Some(n) => format!("Reprise SharePoint — {n}"),
        None => "Reprise SharePoint".to_string(),
    }))
    .fetch_one(&mut *conn)
    .await?;
    let line: Uuid = sqlx::query_scalar(
        "INSERT INTO movement_lines (movement_id, line_no, item_type, asset_id, sim_id, label, identifier, quantity, condition)
         VALUES ($1, 1, $2, $3, $4, $5, $6, 1, $7) RETURNING id",
    )
    .bind(movement)
    .bind(item_type)
    .bind(asset_id)
    .bind(sim_id)
    .bind(label)
    .bind(&identifier)
    .bind(condition)
    .fetch_one(&mut *conn)
    .await?;
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO assignments (employee_id, handover_line_id, item_type, asset_id, sim_id, quantity, open_quantity, started_at, ended_at)
         VALUES ($1, $2, $3, $4, $5, 1, $6, $7, $8) RETURNING id",
    )
    .bind(employee_id)
    .bind(line)
    .bind(item_type)
    .bind(asset_id)
    .bind(sim_id)
    .bind(if ended_at.is_some() { 0 } else { 1 })
    .bind(at)
    .bind(ended_at)
    .fetch_one(&mut *conn)
    .await?;
    if ended_at.is_none() {
        match item {
            Item::Asset(a) => {
                sqlx::query("UPDATE assets SET status = 'affecte', version = version + 1, updated_at = now() WHERE id = $1")
                    .bind(a)
                    .execute(&mut *conn)
                    .await?;
            }
            Item::Sim(s) => {
                sqlx::query("UPDATE sim_cards SET status = 'affecte' WHERE id = $1").bind(s).execute(&mut *conn).await?;
            }
        }
    }
    Ok(id)
}

async fn apply_asset(conn: &mut PgConnection, user: &CurrentUser, d: &AssetData, target: Option<Uuid>) -> AppResult<Uuid> {
    let id: Uuid = match target {
        Some(id) => {
            sqlx::query(
                "UPDATE assets SET kind = $2, brand = coalesce($3, brand), model = coalesce($4, model), serial_no = coalesce($5, serial_no),
                        intune_name = coalesce($6, intune_name), purchase_date = coalesce($7, purchase_date),
                        warranty_end = coalesce($8, warranty_end), condition = coalesce($9, condition),
                        notes = coalesce($10, notes),
                        status = CASE WHEN status = 'affecte' THEN status ELSE $11 END,
                        version = version + 1, updated_at = now()
                  WHERE id = $1",
            )
            .bind(id)
            .bind(&d.kind)
            .bind(&d.brand)
            .bind(&d.model)
            .bind(&d.serial_no)
            .bind(&d.intune_name)
            .bind(d.purchase_date)
            .bind(d.warranty_end)
            .bind(&d.condition)
            .bind(&d.notes)
            .bind(&d.status)
            .execute(&mut *conn)
            .await?;
            id
        }
        None => {
            sqlx::query_scalar(
                "INSERT INTO assets (inventory_no, qr_token, kind, label, brand, model, serial_no, intune_name, purchase_date,
                                     warranty_end, condition, status, notes)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13) RETURNING id",
            )
            .bind(&d.inventory_no)
            .bind(crate::routes::assets::new_qr_token())
            .bind(&d.kind)
            .bind(&d.label)
            .bind(&d.brand)
            .bind(&d.model)
            .bind(&d.serial_no)
            .bind(&d.intune_name)
            .bind(d.purchase_date)
            .bind(d.warranty_end)
            .bind(&d.condition)
            .bind(&d.status)
            .bind(&d.notes)
            .fetch_one(&mut *conn)
            .await?
        }
    };
    if let Some(holder) = d.holder_id {
        let held: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM assignments WHERE asset_id = $1 AND ended_at IS NULL)")
            .bind(id)
            .fetch_one(&mut *conn)
            .await?;
        if !held {
            let label = format!("{} ({})", d.label, d.inventory_no);
            reprise_assignment(conn, user, holder, Item::Asset(id), &label, None, None, d.condition.as_deref(), None).await?;
        }
    }
    Ok(id)
}

async fn apply_line(conn: &mut PgConnection, user: &CurrentUser, d: &LineData, target: Option<Uuid>) -> AppResult<Uuid> {
    let id: Uuid = match target {
        Some(id) => {
            sqlx::query(
                "UPDATE mobile_lines SET operator = $2, plan = coalesce($3, plan), line_type = coalesce($4, line_type), status = $5 WHERE id = $1",
            )
            .bind(id)
            .bind(&d.operator)
            .bind(&d.plan)
            .bind(&d.line_type)
            .bind(&d.status)
            .execute(&mut *conn)
            .await?;
            id
        }
        None => {
            sqlx::query_scalar("INSERT INTO mobile_lines (number, operator, plan, line_type, status) VALUES ($1, $2, $3, $4, $5) RETURNING id")
                .bind(&d.number)
                .bind(&d.operator)
                .bind(&d.plan)
                .bind(&d.line_type)
                .bind(&d.status)
                .fetch_one(&mut *conn)
                .await?
        }
    };
    let mut sim_id: Option<Uuid> = None;
    if let Some(iccid) = &d.iccid {
        let existing: Option<Uuid> = sqlx::query_scalar("SELECT id FROM sim_cards WHERE iccid = $1").bind(iccid).fetch_optional(&mut *conn).await?;
        sim_id = match existing {
            Some(s) => Some(s),
            None => {
                let active_other: bool = sqlx::query_scalar(
                    "SELECT EXISTS (SELECT 1 FROM sim_cards WHERE line_id = $1 AND status <> 'desactivee')",
                )
                .bind(id)
                .fetch_one(&mut *conn)
                .await?;
                if active_other {
                    return Err(AppError::conflict("la ligne a déjà une autre SIM active"));
                }
                Some(
                    sqlx::query_scalar(
                        "INSERT INTO sim_cards (iccid, line_id, status) VALUES ($1, $2, CASE WHEN $3 = 'resiliee' THEN 'desactivee' ELSE 'disponible' END) RETURNING id",
                    )
                    .bind(iccid)
                    .bind(id)
                    .bind(&d.status)
                    .fetch_one(&mut *conn)
                    .await?,
                )
            }
        };
    }
    if let (Some(holder), Some(sim)) = (d.holder_id, sim_id) {
        let held: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM assignments WHERE sim_id = $1 AND ended_at IS NULL)")
            .bind(sim)
            .fetch_one(&mut *conn)
            .await?;
        if !held {
            reprise_assignment(conn, user, holder, Item::Sim(sim), &format!("Carte SIM — ligne {}", d.number), None, None, None, None).await?;
        }
    }
    Ok(id)
}

