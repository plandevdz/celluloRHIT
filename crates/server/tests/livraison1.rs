//! Première livraison (prompt de cadrage) : fiche personnel complète,
//! profils de poste, arrivée préparée par profil avec plusieurs
//! équipements, Excel sans perte. Scénarios 1 à 9 ; données fictives.

mod common;

use std::io::Cursor;

use axum::http::{Method, StatusCode};
use calamine::{open_workbook_from_rs, Data, Reader, Xlsx};
use common::{Session, TestApp};
use rust_xlsxwriter::{ExcelDateTime, Format, Workbook};
use serde_json::{json, Value};
use sqlx::PgPool;
use uuid::Uuid;

async fn task_id(app: &TestApp, case_id: &str, gen_key: &str) -> Uuid {
    sqlx::query_scalar("SELECT id FROM tasks WHERE case_id = $1::uuid AND gen_key = $2")
        .bind(case_id)
        .bind(gen_key)
        .fetch_one(app.pool())
        .await
        .unwrap_or_else(|_| panic!("tâche {gen_key} absente"))
}

async fn claim_complete(app: &TestApp, s: &Session, task: Uuid, body: Value) -> (StatusCode, Value) {
    let (status, v) = app.call(s, Method::POST, &format!("/tasks/{task}/claim"), None).await;
    assert!(status == StatusCode::OK || status == StatusCode::CONFLICT, "{v}");
    app.call(s, Method::POST, &format!("/tasks/{task}/complete"), Some(body)).await
}

async fn version(app: &TestApp, id: &str) -> i32 {
    sqlx::query_scalar("SELECT version FROM employees WHERE id = $1::uuid").bind(id).fetch_one(app.pool()).await.unwrap()
}

async fn wilaya(app: &TestApp, code: i32, name: &str) -> String {
    let id: Uuid = sqlx::query_scalar("INSERT INTO wilayas (code, name, region) VALUES ($1, $2, 'Région fictive') RETURNING id")
        .bind(code)
        .bind(name)
        .fetch_one(app.pool())
        .await
        .unwrap();
    id.to_string()
}

fn ids(v: &Value) -> Vec<String> {
    v["items"].as_array().unwrap().iter().map(|e| e["id"].as_str().unwrap().to_string()).collect()
}

#[sqlx::test(migrator = "server::MIGRATOR")]
async fn s1_s2_arrivee_minimale_et_fiche_terrain(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let rh = app.user("Rh", &["RH"]).await;
    let it = app.user("Tech", &["IT"]).await;
    let formation = app.user("Formatrice", &["FORMATION"]).await;
    let cheraga = app.site("CHERAGA").await;
    let oran = app.site("ORAN").await;

    // 1. Cinq champs, sans matricule ni profil : IT et Formation voient le
    // même ONB ; aucune tâche inventée.
    let onb = app.announce(&rh, cheraga, "Lina", "Minimale", json!({})).await;
    assert_eq!(onb["tasks_created"], 0);
    let reference = onb["reference"].as_str().unwrap();
    for s in [&it, &formation] {
        let (status, c) = app.call(s, Method::GET, &format!("/cases/{reference}"), None).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(c["tasks"].as_array().unwrap().len(), 0);
    }
    assert_eq!(app.count("SELECT count(*) FROM tasks").await, 0);
    let roles: Vec<String> = sqlx::query_scalar("SELECT unnest(target_roles) FROM outbox ORDER BY 1").fetch_all(app.pool()).await.unwrap();
    assert_eq!(roles, vec!["FORMATION", "IT"]);

    // 2. Déléguée rattachée à Chéraga, un manager identifié, trois wilayas.
    let manager = app.announce(&rh, cheraga, "Karim", "Responsable", json!({ "matricule": "00007" })).await["employee_id"].as_str().unwrap().to_string();
    let delegate = app.announce(&rh, cheraga, "Sara", "Déléguée", json!({})).await["employee_id"].as_str().unwrap().to_string();
    let other = app.announce(&rh, oran, "Omar", "Ailleurs", json!({})).await["employee_id"].as_str().unwrap().to_string();
    let w16 = wilaya(&app, 16, "Alger").await;
    let w9 = wilaya(&app, 9, "Blida").await;
    let w35 = wilaya(&app, 35, "Boumerdès").await;
    let w31 = wilaya(&app, 31, "Oran").await;
    let (status, v) = app
        .call(&rh, Method::PATCH, &format!("/employees/{delegate}"), Some(json!({
            "version": version(&app, &delegate).await, "contract_type": "cdi", "wilaya_ids": [w16, w9, w35],
            "manager_employee_id": manager, "department": "Commercial"
        })))
        .await;
    assert_eq!(status, StatusCode::OK, "{v}");
    let (_, v) = app.call(&rh, Method::PATCH, &format!("/employees/{other}"), Some(json!({ "version": version(&app, &other).await, "wilaya_ids": [w31] }))).await;
    assert!(v["version"].is_number());
    // Un manager ne peut pas être son propre manager.
    let (status, _) = app
        .call(&rh, Method::PATCH, &format!("/employees/{delegate}"), Some(json!({ "version": version(&app, &delegate).await, "manager_employee_id": delegate })))
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    // Chacune des trois wilayas retrouve la même personne.
    for w in [&w16, &w9, &w35] {
        let (_, list) = app.call(&rh, Method::GET, &format!("/employees?wilaya_id={w}"), None).await;
        assert_eq!(ids(&list), vec![delegate.clone()], "{list}");
    }
    let (_, list) = app.call(&rh, Method::GET, &format!("/employees?wilaya_id={w31}"), None).await;
    assert_eq!(ids(&list), vec![other.clone()]);
    let (_, list) = app.call(&rh, Method::GET, "/employees?department=commercial", None).await;
    assert_eq!(ids(&list), vec![delegate.clone()]);
    // Le site reste Chéraga ; le manager est une fiche, pas un texte.
    let (_, e) = app.call(&rh, Method::GET, &format!("/employees/{delegate}"), None).await;
    assert_eq!(e["employee"]["site_name"], "CHERAGA");
    assert_eq!(e["employee"]["manager_employee_id"], json!(manager));
    assert_eq!(e["employee"]["manager_employee_name"], "Karim Responsable");
    assert_eq!(e["employee"]["wilayas"].as_array().unwrap().len(), 3);

    // « En départ » : présent, pas sorti ; filtrable.
    let (status, _) = app
        .call(&rh, Method::PATCH, &format!("/employees/{other}"), Some(json!({ "version": version(&app, &other).await, "leaving": true, "planned_exit_date": "2027-01-31" })))
        .await;
    assert_eq!(status, StatusCode::OK);
    let (_, list) = app.call(&rh, Method::GET, "/employees?status=en_depart", None).await;
    assert_eq!(ids(&list), vec![other.clone()]);
    let st: String = sqlx::query_scalar("SELECT status FROM employees WHERE id = $1::uuid").bind(&other).fetch_one(app.pool()).await.unwrap();
    assert_ne!(st, "sorti");
}

#[sqlx::test(migrator = "server::MIGRATOR")]
async fn s3_a_s7_profil_equipements_remise_partielle_et_confidentialite(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let rh = app.user("Rh", &["RH"]).await;
    let it = app.user("Tech", &["IT"]).await;
    let formation = app.user("Formatrice", &["FORMATION"]).await;
    let it_formation = app.user("Tech Formateur", &["IT", "FORMATION"]).await;
    let graphiste = app.user("Graphiste", &["GRAPHISTE"]).await;
    let site = app.site("CHERAGA").await;
    let mut catalog = Vec::new();
    for name in ["Produits (fictif)", "Outils digitaux (fictif)"] {
        let (_, v) = app.call(&formation, Method::POST, "/trainings/catalog", Some(json!({ "name": name, "category": "produit", "duration_hours": 2 }))).await;
        catalog.push(v["id"].as_str().unwrap().to_string());
    }

    // 3. Profil fictif : PC fixe, SIM, email pro, Xena, deux formations.
    let (status, p) = app
        .call(&rh, Method::POST, "/job-profiles", Some(json!({
            "name": "Délégué médical (fictif)", "department": "Commercial", "field_based": true,
            "needs": [{ "category": "pc", "preference": "fixe" }, { "category": "sim" }, { "category": "email_pro" }, { "category": "xena" }],
            "trainings": catalog
        })))
        .await;
    assert_eq!(status, StatusCode::OK, "{p}");
    let profile = p["id"].as_str().unwrap().to_string();
    // Le RH retire Xena, garde une formation et ajoute deux écrans.
    let onb = app
        .announce(&rh, site, "Amel", "Terrain", json!({
            "job_profile_id": profile, "contract_type": "cdd",
            "needs": [
                { "category": "pc", "answer": "oui", "preference": "fixe", "origin": "profil" },
                { "category": "sim", "answer": "oui", "origin": "profil" },
                { "category": "email_pro", "answer": "oui", "origin": "profil" },
                { "category": "xena", "answer": "non" },
                { "category": "ecran", "answer": "oui", "quantity": 2 }
            ],
            "trainings": [{ "catalog_id": catalog[0] }]
        }))
        .await;
    let case_id = onb["case_id"].as_str().unwrap().to_string();
    let emp = onb["employee_id"].as_str().unwrap().to_string();
    let reference = onb["reference"].as_str().unwrap().to_string();
    let keys: Vec<String> = sqlx::query_scalar("SELECT gen_key FROM tasks WHERE case_id = $1::uuid ORDER BY gen_key").bind(&case_id).fetch_all(app.pool()).await.unwrap();
    assert_eq!(keys, vec![format!("formation:{}", catalog[0]), "need:ecran".into(), "need:email_pro".into(), "need:pc".into(), "need:sim".into()]);
    // Modifier le profil le lendemain ne change rien au dossier.
    let (_, pv) = app.call(&rh, Method::GET, &format!("/job-profiles/{profile}"), None).await;
    let (status, _) = app
        .call(&rh, Method::PUT, &format!("/job-profiles/{profile}"), Some(json!({
            "version": pv["version"], "name": "Délégué médical (fictif)", "needs": [{ "category": "telephone" }], "trainings": []
        })))
        .await;
    assert_eq!(status, StatusCode::OK);
    let (_, c) = app.call(&rh, Method::GET, &format!("/cases/{reference}"), None).await;
    let need = |cat: &str| c["needs"].as_array().unwrap().iter().find(|n| n["category"] == cat).unwrap().clone();
    assert_eq!(need("pc")["origin"], "profil");
    assert_eq!(need("pc")["preference"], "fixe");
    assert_eq!(need("ecran")["quantity"], 2);
    assert_eq!(need("xena")["answer"], "non");
    assert_eq!(need("telephone")["answer"], "a_preciser");

    // 4. Formation réalisée avant toute remise ; adresse cPanel, M365 indépendant.
    let session: Uuid = sqlx::query_scalar("SELECT id FROM training_sessions WHERE case_id = $1::uuid").bind(&case_id).fetch_one(app.pool()).await.unwrap();
    let (status, v) = app.call(&formation, Method::POST, &format!("/trainings/sessions/{session}"), Some(json!({ "action": "realiser", "evaluation": "acquis" }))).await;
    assert_eq!(status, StatusCode::OK, "{v}");
    let (status, _) = app
        .call(&it, Method::POST, &format!("/employees/{emp}/accounts"), Some(json!({ "system": "email", "platform": "cpanel", "identifier": "a.terrain@exemple.test" })))
        .await;
    assert_eq!(status, StatusCode::OK);
    let m365: Option<chrono::DateTime<chrono::Utc>> = sqlx::query_scalar("SELECT m365_verified_at FROM employees WHERE id = $1::uuid").bind(&emp).fetch_one(app.pool()).await.unwrap();
    assert!(m365.is_none(), "l'adresse créée ne coche pas « M365 opérationnel »");

    // 5. Portable à la place du fixe (motif), une SIM, un seul écran sur deux.
    let mut assets = Vec::new();
    for (inv, kind) in [("PC-501", "pc_portable"), ("ECR-501", "ecran"), ("ECR-502", "ecran")] {
        let (_, a) = app.call(&it, Method::POST, "/assets", Some(json!({ "inventory_no": inv, "kind": kind, "label": format!("{kind} fictif") }))).await;
        assets.push(a["id"].as_str().unwrap().to_string());
    }
    let (_, sim) = app.call(&it, Method::POST, "/sims", Some(json!({ "iccid": "8921300000000000501", "line_number": "0550000501", "operator": "Mobilis" }))).await;
    let lines = json!([{ "type": "asset", "asset_id": assets[0] }, { "type": "sim", "sim_id": sim["id"] }, { "type": "asset", "asset_id": assets[1] }]);
    let (status, _) = app.call(&it, Method::POST, "/handovers", Some(json!({ "employee_id": emp, "case_id": case_id, "lines": lines }))).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "substitution sans motif refusée");
    let (status, h) = app
        .call(&it, Method::POST, "/handovers", Some(json!({ "employee_id": emp, "case_id": case_id, "lines": lines, "substitution_reason": "plus de fixe en stock" })))
        .await;
    assert_eq!(status, StatusCode::CREATED, "{h}");
    let pc_task = task_id(&app, &case_id, "need:pc").await;
    let (status, v) = claim_complete(&app, &it, pc_task, json!({})).await;
    assert_eq!(status, StatusCode::OK, "{v}");
    let ecran_task = task_id(&app, &case_id, "need:ecran").await;
    let (status, v) = claim_complete(&app, &it, ecran_task, json!({})).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "un écran ne solde pas une demande de deux : {v}");
    assert!(v["message"].as_str().unwrap().contains("1/2"));
    let sim_task = task_id(&app, &case_id, "need:sim").await;
    let (status, _) = claim_complete(&app, &it, sim_task, json!({})).await;
    assert_eq!(status, StatusCode::OK);
    let (_, c) = app.call(&rh, Method::GET, &format!("/cases/{reference}"), None).await;
    let need = |cat: &str| c["needs"].as_array().unwrap().iter().find(|n| n["category"] == cat).unwrap().clone();
    assert_eq!(need("ecran")["delivered"], 1);
    assert_eq!(need("pc")["delivered_kinds"], json!(["pc_portable"]));
    assert_eq!(need("pc")["preference"], "fixe", "préférence initiale conservée");
    assert_eq!(need("sim")["delivered"], 1);
    assert_eq!(c["movements"][0]["substitution_reason"], "plus de fixe en stock");
    assert_eq!(c["movements"][0]["lines"].as_array().unwrap().len(), 3, "la décharge ne liste que les articles remis");

    // 6. Présence confirmée alors qu'un écran manque ; après résolution et
    // clôture, une carte demandée depuis la fiche ne va qu'au Graphiste.
    let (status, _) = app.call(&rh, Method::POST, &format!("/employees/{emp}/arrival"), Some(json!({ "actual_arrival": "2026-11-02" }))).await;
    assert_eq!(status, StatusCode::OK);
    let (status, _) = app.call(&it, Method::POST, "/handovers", Some(json!({ "employee_id": emp, "case_id": case_id, "lines": [{ "type": "asset", "asset_id": assets[2] }] }))).await;
    assert_eq!(status, StatusCode::CREATED);
    let (status, v) = app.call(&it, Method::POST, &format!("/tasks/{ecran_task}/complete"), Some(json!({}))).await;
    assert_eq!(status, StatusCode::OK, "{v}");
    let email_task = task_id(&app, &case_id, "need:email_pro").await;
    let (status, _) = claim_complete(&app, &it, email_task, json!({})).await;
    assert_eq!(status, StatusCode::OK);
    // Décharges signées : scan déposé pour chaque remise (sinon clôture refusée).
    let docs: Vec<Uuid> = sqlx::query_scalar("SELECT d.id FROM documents d JOIN movements m ON m.id = d.movement_id WHERE m.case_id = $1::uuid")
        .bind(&case_id)
        .fetch_all(app.pool())
        .await
        .unwrap();
    for d in &docs {
        server::documents::ensure_original(app.pool(), &app.state.cfg(), *d).await.unwrap();
        let (status, v) = app.upload(&it, &format!("/documents/{d}/scan"), "scan.pdf", b"%PDF-1.4 scan fictif").await;
        assert_eq!(status, StatusCode::OK, "{v}");
    }
    let (_, c) = app.call(&rh, Method::GET, &format!("/cases/{reference}"), None).await;
    let (status, v) = app.call(&rh, Method::POST, &format!("/cases/{case_id}/close"), Some(json!({ "version": c["case"]["version"] }))).await;
    assert_eq!(status, StatusCode::OK, "{v}");
    let before = app.count("SELECT count(*) FROM outbox").await;
    let (status, dem) = app
        .call(&rh, Method::POST, &format!("/employees/{emp}/requests"), Some(json!({ "category": "carte_visite", "parent_case_id": case_id })))
        .await;
    assert_eq!(status, StatusCode::CREATED, "{dem}");
    assert_eq!(dem["visible_to"], json!(["GRAPHISTE"]));
    assert_eq!(app.count("SELECT count(*) FROM outbox").await, before + 1);
    let st: String = sqlx::query_scalar("SELECT status FROM cases WHERE id = $1::uuid").bind(&case_id).fetch_one(app.pool()).await.unwrap();
    assert_eq!(st, "clos", "l'arrivée reste close");

    // 7. Le Graphiste voit les coordonnées utiles, rien d'autre.
    let (status, g) = app.call(&graphiste, Method::GET, &format!("/employees/{emp}"), None).await;
    assert_eq!(status, StatusCode::OK);
    let text = g.to_string();
    assert_eq!(g["contact"]["email_pro"], "a.terrain@exemple.test");
    assert_eq!(g["contact"]["mobile"], "0550000501");
    // Valeurs entre guillemets : un identifiant aléatoire peut contenir « cdd ».
    for forbidden in ["contract_type", "\"cdd\"", "wilayas", "import_note", "poste_fixe", "plus de fixe", "PC-501"] {
        assert!(!text.contains(forbidden), "le Graphiste ne doit pas voir « {forbidden} » : {text}");
    }
    assert_eq!(g["positions"], json!([]));
    assert_eq!(g["holdings"], json!([]));
    let (status, _) = app.call(&graphiste, Method::GET, &format!("/cases/{reference}"), None).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "ni l'arrivée ni ses décharges");
    // L'IT voit la fiche de travail mais pas le contrat.
    let (_, e) = app.call(&it, Method::GET, &format!("/employees/{emp}"), None).await;
    assert!(e["employee"]["contract_type"].is_null());
    // IT + FORMATION : ses deux files.
    let (_, tasks) = app.call(&it_formation, Method::GET, "/tasks?status=done", None).await;
    let roles: std::collections::BTreeSet<String> = tasks.as_array().unwrap().iter().map(|t| t["role"].as_str().unwrap().to_string()).collect();
    assert!(roles.contains("IT") && roles.contains("FORMATION"), "{roles:?}");

    // Ligne principale : choisie parmi les lignes réellement détenues.
    let line: Uuid = sqlx::query_scalar("SELECT id FROM mobile_lines WHERE number = '0550000501'").fetch_one(app.pool()).await.unwrap();
    let (status, _) = app.call(&rh, Method::POST, &format!("/employees/{emp}/primary-line"), Some(json!({ "line_id": line }))).await;
    assert_eq!(status, StatusCode::OK);
    let other: Uuid = sqlx::query_scalar("INSERT INTO mobile_lines (number, operator) VALUES ('0550999999', 'Djezzy') RETURNING id").fetch_one(app.pool()).await.unwrap();
    let (status, _) = app.call(&rh, Method::POST, &format!("/employees/{emp}/primary-line"), Some(json!({ "line_id": other }))).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "jamais une ligne non détenue");
}

// ------------------------------------------------------------------ Excel

const ALL: [&str; 14] = [
    "ID salarié",
    "Matricule",
    "Prénom",
    "Nom",
    "Poste",
    "Site",
    "Date d’arrivée",
    "Département",
    "Contrat",
    "Profil de poste",
    "Manager (matricule)",
    "Manager (ID salarié)",
    "Manager (nom, pour information)",
    "Wilayas (codes)",
];

fn workbook(headers: &[&str], rows: &[Vec<&str>]) -> Vec<u8> {
    let mut wb = Workbook::new();
    let ws = wb.add_worksheet();
    ws.set_name("Personnel").unwrap();
    let text = Format::new().set_num_format("@");
    let date = Format::new().set_num_format("dd/mm/yyyy");
    for (c, h) in headers.iter().enumerate() {
        ws.write_string(0, c as u16, *h).unwrap();
    }
    for (r, row) in rows.iter().enumerate() {
        for (c, v) in row.iter().enumerate() {
            if v.is_empty() {
                continue;
            }
            if headers[c] == "Date d’arrivée" {
                let p: Vec<u16> = v.split('-').map(|x| x.parse().unwrap()).collect();
                ws.write_datetime_with_format(r as u32 + 1, c as u16, &ExcelDateTime::from_ymd(p[0], p[1] as u8, p[2] as u8).unwrap(), &date).unwrap();
            } else {
                ws.write_string_with_format(r as u32 + 1, c as u16, *v, &text).unwrap();
            }
        }
    }
    wb.save_to_buffer().unwrap()
}

fn row<'a>(v: &'a Value, n: i64) -> &'a Value {
    v["rows"].as_array().unwrap().iter().find(|r| r["row_number"] == n).unwrap_or_else(|| panic!("ligne {n} absente : {v}"))
}

#[sqlx::test(migrator = "server::MIGRATOR")]
async fn s8_s9_excel_sans_perte_et_fiches_anterieures(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let rh = app.user("Rh", &["RH"]).await;
    let site = app.site("CHERAGA").await;
    let w = [wilaya(&app, 16, "Alger").await, wilaya(&app, 9, "Blida").await, wilaya(&app, 35, "Boumerdès").await];
    let (_, p) = app.call(&rh, Method::POST, "/job-profiles", Some(json!({ "name": "Délégué (fictif)" }))).await;
    let profile = p["id"].as_str().unwrap().to_string();
    let mk = |first: &str, last: &str, mat: Option<&str>| {
        json!({ "first_name": first, "last_name": last, "matricule": mat, "job_title": "Délégué", "site_id": site,
                "arrival_date": "2022-01-10", "already_present": true })
    };
    let (_, m) = app.call(&rh, Method::POST, "/employees", Some(mk("Karim", "Chef", Some("00007")))).await;
    let manager = m["id"].as_str().unwrap().to_string();
    // 9. Fiche antérieure à la livraison : rien d'inventé.
    let (_, old) = app.call(&rh, Method::POST, "/employees", Some(mk("Ancien", "Salarie", None))).await;
    let old = old["id"].as_str().unwrap().to_string();
    let (status, e) = app.call(&rh, Method::GET, &format!("/employees/{old}"), None).await;
    assert_eq!(status, StatusCode::OK);
    for field in ["contract_type", "job_profile_id", "manager_employee_id", "planned_exit_date"] {
        assert!(e["employee"][field].is_null(), "{field} inventé : {e}");
    }
    assert_eq!(e["employee"]["wilayas"], json!([]));

    let (_, s) = app.call(&rh, Method::POST, "/employees", Some(mk("Sara", "Deleguee", Some("00042")))).await;
    let sara = s["id"].as_str().unwrap().to_string();
    let (status, v) = app
        .call(&rh, Method::PATCH, &format!("/employees/{sara}"), Some(json!({
            "version": version(&app, &sara).await, "contract_type": "cdi", "job_profile_id": profile,
            "manager_employee_id": manager, "wilaya_ids": w
        })))
        .await;
    assert_eq!(status, StatusCode::OK, "{v}");

    // 8a. Export puis réimport du même classeur : rien de perdu ni dupliqué.
    let req = axum::http::Request::builder()
        .uri("/api/v1/personnel/export.xlsx")
        .header(axum::http::header::COOKIE, &rh.cookie)
        .body(axum::body::Body::empty())
        .unwrap();
    let (status, bytes) = app.raw(req).await;
    assert_eq!(status, StatusCode::OK);
    let mut wb: Xlsx<_> = open_workbook_from_rs(Cursor::new(bytes.clone())).unwrap();
    let range = wb.worksheet_range("Personnel").unwrap();
    let sara_row: Vec<String> = range
        .rows()
        .find(|r| r.get(1).map(|c| c == &Data::String("00042".into())).unwrap_or(false))
        .unwrap()
        .iter()
        .map(|c| c.to_string())
        .collect();
    assert_eq!(sara_row[8], "CDI");
    assert_eq!(sara_row[9], "Délégué (fictif)");
    assert_eq!(sara_row[10], "00007");
    assert_eq!(sara_row[11], manager);
    assert_eq!(sara_row[13], "9 Blida; 16 Alger; 35 Boumerdès");
    let employees_before = app.count("SELECT count(*) FROM employees").await;
    let (status, p) = app.upload(&rh, "/personnel-imports", "Personnel-export.xlsx", &bytes).await;
    assert_eq!(status, StatusCode::OK, "{p}");
    assert!(p["rows"].as_array().unwrap().iter().all(|r| r["proposal"] == "inchange"), "{p}");
    let id = p["batch"]["id"].as_str().unwrap().to_string();
    let (status, _) = app.call(&rh, Method::POST, &format!("/personnel-imports/{id}/commit"), None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(app.count("SELECT count(*) FROM employees").await, employees_before);
    assert_eq!(app.count("SELECT count(*) FROM employee_wilayas").await, 3);

    // 8b. Deux homonymes, manager ambigu, ligne incomplète, wilaya inconnue.
    let (_, _h1) = app.call(&rh, Method::POST, "/employees", Some(mk("Nadia", "Homonyme", None))).await;
    let (_, _h2) = app.call(&rh, Method::POST, "/employees", Some(mk("Nadia", "Homonyme", Some("555")))).await;
    let bytes = workbook(
        &ALL,
        &[
            // 2 : nouvelle personne, manager par matricule, deux wilayas
            vec!["", "01234", "Yanis", "Nouveau", "Délégué", "CHERAGA", "2025-02-01", "Commercial", "CDD", "Délégué (fictif)", "00007", "", "", "16;35"],
            // 3 : manager donné par son seul nom, porté par deux fiches
            vec![&sara, "", "", "", "", "", "", "", "", "", "", "", "Nadia Homonyme", ""],
            // 4 : ligne incomplète (nouvelle fiche sans site)
            vec!["", "", "Ines", "Incomplete", "Agent", "", "2025-02-01", "", "", "", "", "", "", ""],
            // 5 : wilaya inconnue et contrat inconnu
            vec![&sara, "", "", "", "", "", "", "", "CDX", "", "", "", "", "99"],
            // 6 : homonyme d'une fiche existante, sans matricule
            vec!["", "", "Nadia", "Homonyme", "Agent", "CHERAGA", "2025-02-01", "", "", "", "", "", "", ""],
        ],
    );
    let (status, p) = app.upload(&rh, "/personnel-imports", "Personnel-v2.xlsx", &bytes).await;
    assert_eq!(status, StatusCode::OK, "{p}");
    assert_eq!(row(&p, 2)["proposal"], "creation", "{p}");
    assert_eq!(row(&p, 3)["proposal"], "inchange", "le nom seul ne change pas le manager");
    assert!(row(&p, 3)["messages"].to_string().contains("seul nom"), "{p}");
    assert_eq!(row(&p, 4)["proposal"], "rejet");
    assert_eq!(row(&p, 5)["proposal"], "rejet");
    assert!(row(&p, 5)["messages"].to_string().contains("Wilaya « 99 »"));
    assert!(row(&p, 5)["messages"].to_string().contains("Contrat « CDX »"));
    assert_eq!(row(&p, 6)["proposal"], "a_revoir", "jamais d'identité déduite du nom");
    let id = p["batch"]["id"].as_str().unwrap().to_string();
    let (status, _) = app.call(&rh, Method::POST, &format!("/personnel-imports/{id}/commit"), None).await;
    assert_eq!(status, StatusCode::OK);
    let (manager_of, contract, wilayas): (Option<Uuid>, Option<String>, i64) = sqlx::query_as(
        "SELECT p.manager_employee_id, e.contract_type, (SELECT count(*) FROM employee_wilayas w WHERE w.employee_id = e.id)
           FROM employees e JOIN employee_positions p ON p.employee_id = e.id AND p.valid_to IS NULL WHERE e.matricule = '01234'",
    )
    .fetch_one(app.pool())
    .await
    .unwrap();
    assert_eq!(manager_of.map(|m| m.to_string()), Some(manager.clone()));
    assert_eq!(contract.as_deref(), Some("cdd"));
    assert_eq!(wilayas, 2);

    // 8c. Ancien modèle à 8 colonnes : accepté, rien d'effacé.
    let bytes = workbook(&ALL[..8], &[vec![&sara, "", "", "", "Déléguée senior", "", "", ""]]);
    let (status, p) = app.upload(&rh, "/personnel-imports", "Personnel-ancien.xlsx", &bytes).await;
    assert_eq!(status, StatusCode::OK, "{p}");
    assert_eq!(row(&p, 2)["proposal"], "mise_a_jour");
    let id = p["batch"]["id"].as_str().unwrap().to_string();
    let (status, _) = app.call(&rh, Method::POST, &format!("/personnel-imports/{id}/commit"), None).await;
    assert_eq!(status, StatusCode::OK);
    let (_, e) = app.call(&rh, Method::GET, &format!("/employees/{sara}"), None).await;
    assert_eq!(e["employee"]["job_title"], "Déléguée senior");
    assert_eq!(e["employee"]["contract_type"], "cdi");
    assert_eq!(e["employee"]["job_profile_id"], json!(profile));
    assert_eq!(e["employee"]["manager_employee_id"], json!(manager));
    assert_eq!(e["employee"]["wilayas"].as_array().unwrap().len(), 3);

    // Aucun onboarding, aucune tâche, aucun avis par l'import.
    assert_eq!(app.count("SELECT count(*) FROM cases").await, 0);
    assert_eq!(app.count("SELECT count(*) FROM tasks").await, 0);
    assert_eq!(app.count("SELECT count(*) FROM outbox").await, 0);
}
