//! Mutation (R09) et démission (R10). Données fictives ; nécessite
//! DATABASE_URL.

mod common;

use axum::http::{Method, StatusCode};
use common::{Session, TestApp};
use serde_json::{json, Value};
use sqlx::PgPool;
use uuid::Uuid;

async fn case(app: &TestApp, s: &Session, reference: &str) -> Value {
    let (status, v) = app.call(s, Method::GET, &format!("/cases/{reference}"), None).await;
    assert_eq!(status, StatusCode::OK, "{v}");
    v
}

async fn employee_version(app: &TestApp, id: &str) -> i32 {
    sqlx::query_scalar("SELECT version FROM employees WHERE id = $1::uuid").bind(id).fetch_one(app.pool()).await.unwrap()
}

#[sqlx::test(migrator = "server::MIGRATOR")]
async fn r09_mutation_xena_conserve_vehicule_repris(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let rh = app.user("Rh Mutation", &["RH"]).await;
    let it = app.user("Tech", &["IT"]).await;
    let oran = app.site("ORAN").await;
    let alger = app.site("ALGER").await;

    // Profil cible sédentaire : Xena et PC, pas de véhicule.
    let (status, p) = app
        .call(&rh, Method::POST, "/job-profiles", Some(json!({
            "name": "Assistant commercial sédentaire",
            "needs": [{ "category": "xena" }, { "category": "pc", "preference": "portable" }]
        })))
        .await;
    assert_eq!(status, StatusCode::OK, "{p}");
    let profile = p["id"].as_str().unwrap().to_string();

    // Commercial terrain avec Xena et un véhicule.
    let onb = app
        .announce(&rh, oran, "Rafik", "Terrain", json!({ "needs": [{ "category": "xena", "answer": "oui" }] }))
        .await;
    let emp = onb["employee_id"].as_str().unwrap().to_string();
    let (status, _) = app.call(&rh, Method::POST, &format!("/employees/{emp}/arrival"), Some(json!({ "actual_arrival": "2026-11-01" }))).await;
    assert_eq!(status, StatusCode::OK);
    let (status, _) = app.call(&it, Method::POST, &format!("/employees/{emp}/accounts"), Some(json!({ "system": "xena", "identifier": "rterrain" }))).await;
    assert_eq!(status, StatusCode::OK);
    let (_, v) = app.call(&rh, Method::POST, "/vehicles", Some(json!({ "plate": "00001 116 31", "kind": "propriete", "odometer_km": 5000 }))).await;
    let vid = v["id"].as_str().unwrap().to_string();
    let (status, _) = app.call(&rh, Method::POST, &format!("/vehicles/{vid}/handover"), Some(json!({ "employee_id": emp, "odometer_km": 5000 }))).await;
    assert_eq!(status, StatusCode::CREATED);

    let (status, m) = app
        .call(&rh, Method::POST, "/transfers", Some(json!({
            "employee_id": emp, "job_title": "Assistant commercial", "site_id": alger,
            "job_profile_id": profile, "effective_date": "2026-11-10"
        })))
        .await;
    assert_eq!(status, StatusCode::CREATED, "{m}");
    let reference = m["reference"].as_str().unwrap().to_string();
    let transfer = m["transfer_id"].as_str().unwrap().to_string();
    let c = case(&app, &rh, &reference).await;
    let deltas = c["transfer"]["deltas"].as_array().unwrap();
    let by_kind = |k: &str| deltas.iter().find(|d| d["object_kind"] == k).unwrap_or_else(|| panic!("{k} absent : {deltas:?}"));
    assert_eq!(by_kind("compte")["proposal"], "conserver", "Xena conservé");
    assert_eq!(by_kind("vehicule")["proposal"], "reprendre", "véhicule à restituer : tâche RH");
    assert_eq!(by_kind("besoin")["proposal"], "ajouter");

    // Rien n'est appliqué avant confirmation, même la date passée.
    server::worker::process_transitions(&app.state).await.unwrap();
    let title: String = sqlx::query_scalar("SELECT job_title FROM employee_positions WHERE employee_id = $1::uuid AND valid_to IS NULL")
        .bind(&emp)
        .fetch_one(app.pool())
        .await
        .unwrap();
    assert_eq!(title, "Commercial", "aucun changement de poste anticipé");

    // Conflit de version.
    let version = c["transfer"]["version"].as_i64().unwrap();
    let (status, _) = app.call(&rh, Method::POST, &format!("/transfers/{transfer}/confirm"), Some(json!({ "version": version - 1 }))).await;
    assert_eq!(status, StatusCode::PRECONDITION_FAILED);
    let (status, v) = app.call(&rh, Method::POST, &format!("/transfers/{transfer}/confirm"), Some(json!({ "version": version }))).await;
    assert_eq!(status, StatusCode::OK, "{v}");
    let roles: Vec<String> = sqlx::query_scalar("SELECT role || ':' || title FROM tasks t JOIN cases c ON c.id = t.case_id WHERE c.reference = $1 ORDER BY role")
        .bind(&reference)
        .fetch_all(app.pool())
        .await
        .unwrap();
    assert_eq!(roles.len(), 2, "{roles:?}");
    assert!(roles.iter().any(|r| r.starts_with("RH:Reprendre : Véhicule")));
    assert!(roles.iter().any(|r| r.starts_with("IT:")), "PC à ajouter");
    assert!(!roles.iter().any(|r| r.contains("Xena")), "Xena conservé : aucune tâche");

    // Report : seules les actions futures sont décalées.
    let c = case(&app, &rh, &reference).await;
    let (status, v) = app
        .call(&rh, Method::POST, &format!("/transfers/{transfer}/reschedule"), Some(json!({
            "version": c["transfer"]["version"], "effective_date": "2026-11-12", "reason": "prise de poste décalée"
        })))
        .await;
    assert_eq!(status, StatusCode::OK, "{v}");
    assert_eq!(v["shifted_tasks"], 2);

    // Le poste change entre-temps (correction datée) : application suspendue.
    let (status, v) = app
        .call(&rh, Method::POST, &format!("/employees/{emp}/positions"), Some(json!({
            "version": employee_version(&app, &emp).await, "job_title": "Commercial senior", "site_id": oran,
            "valid_from": "2026-11-05", "reason": "promotion antérieure non saisie"
        })))
        .await;
    assert_eq!(status, StatusCode::OK, "{v}");
    server::worker::process_transitions(&app.state).await.unwrap();
    let c = case(&app, &rh, &reference).await;
    assert_eq!(c["transfer"]["status"], "suspendue");
    assert!(c["transfer"]["conflict"]["motif"].as_str().unwrap().contains("poste a changé"));
    assert_eq!(app.count("SELECT count(*) FROM outbox WHERE event_type = 'mutation_suspendue'").await, 1);

    // Recalcul puis confirmation : une seule application à échéance.
    let (status, _) = app.call(&rh, Method::POST, &format!("/transfers/{transfer}/refresh"), Some(json!({ "version": c["transfer"]["version"] }))).await;
    assert_eq!(status, StatusCode::OK);
    let c = case(&app, &rh, &reference).await;
    let (status, _) = app.call(&rh, Method::POST, &format!("/transfers/{transfer}/confirm"), Some(json!({ "version": c["transfer"]["version"] }))).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(app.count("SELECT count(*) FROM tasks t JOIN cases c ON c.id = t.case_id WHERE c.kind = 'MUT'").await, 2, "pas de tâche en double au recalcul");
    server::worker::process_transitions(&app.state).await.unwrap();
    server::worker::process_transitions(&app.state).await.unwrap();
    let positions: Vec<(String, Option<chrono::NaiveDate>)> =
        sqlx::query_as("SELECT job_title, valid_to FROM employee_positions WHERE employee_id = $1::uuid ORDER BY valid_from")
            .bind(&emp)
            .fetch_all(app.pool())
            .await
            .unwrap();
    assert_eq!(positions.len(), 3, "{positions:?}");
    assert_eq!(positions[2].0, "Assistant commercial");
    assert!(positions[2].1.is_none());
    assert_eq!(app.count("SELECT count(*) FROM audit_events WHERE action = 'mutation_appliquee'").await, 1);
    let c = case(&app, &rh, &reference).await;
    assert_eq!(c["transfer"]["status"], "appliquee");
    // Annulation après application : correction datée exigée.
    let (status, _) = app.call(&rh, Method::POST, &format!("/transfers/{transfer}/cancel"), Some(json!({ "version": c["transfer"]["version"], "reason": "erreur" }))).await;
    assert_eq!(status, StatusCode::CONFLICT);

    // Clôture : la reprise du véhicule doit être faite (ou motivée).
    let case_id = c["case"]["id"].as_str().unwrap().to_string();
    let (status, v) = app.call(&rh, Method::POST, &format!("/cases/{case_id}/close"), Some(json!({ "version": c["case"]["version"] }))).await;
    assert_eq!(status, StatusCode::CONFLICT, "{v}");
    let (status, _) = app.call(&rh, Method::POST, &format!("/vehicles/{vid}/return"), Some(json!({ "odometer_km": 5400 }))).await;
    assert_eq!(status, StatusCode::CREATED);
    let task: Uuid = sqlx::query_scalar("SELECT t.id FROM tasks t JOIN cases c ON c.id = t.case_id WHERE c.reference = $1 AND t.role = 'RH'")
        .bind(&reference)
        .fetch_one(app.pool())
        .await
        .unwrap();
    let (status, _) = app.call(&rh, Method::POST, &format!("/tasks/{task}/claim"), None).await;
    assert_eq!(status, StatusCode::OK);
    let (status, v) = app.call(&rh, Method::POST, &format!("/tasks/{task}/complete"), Some(json!({}))).await;
    assert_eq!(status, StatusCode::OK, "{v}");
}

#[sqlx::test(migrator = "server::MIGRATOR")]
async fn mutation_annulee_avant_effet(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let rh = app.user("Rh", &["RH"]).await;
    let site = app.site("ORAN").await;
    let emp = app.announce(&rh, site, "Sami", "Bureau", json!({})).await["employee_id"].as_str().unwrap().to_string();
    let (status, m) = app
        .call(&rh, Method::POST, "/transfers", Some(json!({ "employee_id": emp, "job_title": "Chef d'équipe", "effective_date": "2027-03-01" })))
        .await;
    assert_eq!(status, StatusCode::CREATED, "{m}");
    // Une seconde mutation pour le même salarié est refusée.
    let (status, _) = app
        .call(&rh, Method::POST, "/transfers", Some(json!({ "employee_id": emp, "job_title": "Autre", "effective_date": "2027-04-01" })))
        .await;
    assert_eq!(status, StatusCode::CONFLICT);
    let c = case(&app, &rh, m["reference"].as_str().unwrap()).await;
    let (status, v) = app
        .call(&rh, Method::POST, &format!("/transfers/{}/cancel", m["transfer_id"].as_str().unwrap()), Some(json!({ "version": c["transfer"]["version"], "reason": "projet abandonné" })))
        .await;
    assert_eq!(status, StatusCode::OK, "{v}");
    let c = case(&app, &rh, m["reference"].as_str().unwrap()).await;
    assert_eq!(c["case"]["status"], "annule");
    assert_eq!(c["transfer"]["status"], "annulee");
}

#[sqlx::test(migrator = "server::MIGRATOR")]
async fn r10_demission_materiel_non_rendu(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let rh = app.user("Rh Depart", &["RH"]).await;
    let it = app.user("Tech Depart", &["IT"]).await;
    let admin = app.user("Admin", &["ADMIN"]).await;
    let graphiste = app.user("Graphiste", &["GRAPHISTE"]).await;
    let site = app.site("ROUIBA").await;
    let emp = app.announce(&rh, site, "Nadia", "Sortante", json!({})).await["employee_id"].as_str().unwrap().to_string();
    let (status, _) = app.call(&rh, Method::POST, &format!("/employees/{emp}/arrival"), Some(json!({ "actual_arrival": "2026-10-01" }))).await;
    assert_eq!(status, StatusCode::OK);
    let (_, acc) = app
        .call(&it, Method::POST, &format!("/employees/{emp}/accounts"), Some(json!({ "system": "email", "platform": "cpanel", "identifier": "n.sortante@exemple.test" })))
        .await;
    let account = acc["id"].as_str().unwrap().to_string();
    let (_, pc) = app.call(&it, Method::POST, "/assets", Some(json!({ "inventory_no": "PC-900", "kind": "pc_portable", "label": "Portable" }))).await;
    let (_, sim) = app.call(&it, Method::POST, "/sims", Some(json!({ "iccid": "8921300000000000900", "line_number": "0550000900", "operator": "Mobilis" }))).await;
    let (status, h) = app
        .call(&it, Method::POST, "/handovers", Some(json!({
            "employee_id": emp, "lines": [{ "type": "asset", "asset_id": pc["id"] }, { "type": "sim", "sim_id": sim["id"] }]
        })))
        .await;
    assert_eq!(status, StatusCode::CREATED, "{h}");

    // Dernier jour passé, coupure convenue il y a deux heures.
    let last_day = (chrono::Utc::now() + chrono::Duration::hours(1)).date_naive() - chrono::Duration::days(1);
    let cutoff = chrono::Utc::now() - chrono::Duration::hours(2);
    let (status, d) = app
        .call(&rh, Method::POST, "/departures", Some(json!({
            "employee_id": emp, "reason": "demission", "private_note": "Motif personnel confidentiel",
            "last_day": last_day, "cutoff_at": cutoff, "hr_contact": "rh@exemple.test"
        })))
        .await;
    assert_eq!(status, StatusCode::CREATED, "{d}");
    assert_eq!(d["controls"], 3);
    let reference = d["reference"].as_str().unwrap().to_string();

    // Motif privé : RH seulement. GRAPHISTE : aucun accès, même direct.
    let c = case(&app, &rh, &reference).await;
    assert_eq!(c["departure"]["private_note"], "Motif personnel confidentiel");
    let c_it = case(&app, &it, &reference).await;
    assert!(c_it["departure"]["private_note"].is_null());
    assert!(!c_it.to_string().contains("confidentiel"), "aucune fuite du motif dans la vue IT");
    let (status, _) = app.call(&graphiste, Method::GET, &format!("/cases/{reference}"), None).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let outbox_body: String = sqlx::query_scalar("SELECT body FROM outbox WHERE event_type = 'depart_planifie'").fetch_one(app.pool()).await.unwrap();
    assert!(!outbox_body.contains("confidentiel"));

    // Redémarrage simulé à l'heure de coupure : échéances reprises une fois.
    server::worker::process_transitions(&app.state).await.unwrap();
    server::worker::process_transitions(&app.state).await.unwrap();
    assert_eq!(app.count("SELECT count(*) FROM outbox WHERE event_type = 'coupure_exigible'").await, 1);
    assert_eq!(app.count("SELECT count(*) FROM outbox WHERE event_type = 'coupure_a_verifier'").await, 1);
    // Aucun compte réputé coupé sans confirmation de l'IT.
    let st: String = sqlx::query_scalar("SELECT status FROM employee_accounts WHERE id = $1::uuid").bind(&account).fetch_one(app.pool()).await.unwrap();
    assert_eq!(st, "actif");
    // Sortie administrative effective, dossier encore ouvert pour les biens.
    let (status, e) = app.call(&rh, Method::GET, &format!("/employees/{emp}"), None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(e["employee"]["status"], "sorti");
    assert_eq!(e["employee"]["exit_date"], json!(last_day));
    let c = case(&app, &rh, &reference).await;
    assert_eq!(c["case"]["status"], "ouvert");
    assert_eq!(c["departure"]["status"], "sortie_effective");

    // Clôture refusée tant que l'accès n'est pas confirmé coupé.
    let case_id = c["case"]["id"].as_str().unwrap().to_string();
    let (status, v) = app.call(&admin, Method::POST, &format!("/cases/{case_id}/close"), Some(json!({ "version": c["case"]["version"], "exception_reason": "forcer" }))).await;
    assert_eq!(status, StatusCode::CONFLICT, "{v}");

    let controls = c["departure"]["controls"].as_array().unwrap().clone();
    let ctrl = |k: &str, label: &str| -> String {
        controls.iter().find(|x| x["kind"] == k && x["label"].as_str().unwrap().contains(label)).unwrap()["id"].as_str().unwrap().to_string()
    };
    // La tâche de coupure ne se termine pas sans le contrôle.
    let task: Uuid = sqlx::query_scalar("SELECT id FROM tasks WHERE gen_key LIKE 'off:compte:%'").fetch_one(app.pool()).await.unwrap();
    let (status, _) = app.call(&it, Method::POST, &format!("/tasks/{task}/claim"), None).await;
    assert_eq!(status, StatusCode::OK);
    let (status, _) = app.call(&it, Method::POST, &format!("/tasks/{task}/complete"), Some(json!({}))).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    let done_at = chrono::Utc::now() - chrono::Duration::minutes(30);
    let (status, v) = app
        .call(&it, Method::POST, &format!("/exit-controls/{}/confirm", ctrl("compte", "cPanel")), Some(json!({ "done_at": done_at, "system": "cPanel" })))
        .await;
    assert_eq!(status, StatusCode::OK, "{v}");
    let st: String = sqlx::query_scalar("SELECT status FROM employee_accounts WHERE id = $1::uuid").bind(&account).fetch_one(app.pool()).await.unwrap();
    assert_eq!(st, "coupe");

    // Le PC revient (restitution ordinaire) : son contrôle est soldé.
    let pc_line: Uuid = sqlx::query_scalar("SELECT g.handover_line_id FROM assignments g WHERE g.asset_id = $1::uuid")
        .bind(pc["id"].as_str().unwrap())
        .fetch_one(app.pool())
        .await
        .unwrap();
    let (status, v) = app.call(&it, Method::POST, "/returns", Some(json!({ "employee_id": emp, "lines": [{ "origin_line_id": pc_line }] }))).await;
    assert_eq!(status, StatusCode::CREATED, "{v}");
    let c = case(&app, &rh, &reference).await;
    let state_of = |k: &str| c["departure"]["controls"].as_array().unwrap().iter().find(|x| x["kind"] == k && x["label"].as_str().unwrap().contains(if k == "bien" { "Portable" } else { "" })).unwrap()["state"].clone();
    assert_eq!(state_of("bien"), "fait");

    // SIM conservée : écart signalé, visible avec sa date de suivi.
    let (status, _) = app
        .call(&rh, Method::POST, &format!("/exit-controls/{}/gap", ctrl("bien", "SIM")), Some(json!({ "note": "SIM non rendue, relance prévue", "follow_up_date": "2026-12-01" })))
        .await;
    assert_eq!(status, StatusCode::OK);
    let c = case(&app, &rh, &reference).await;
    let (status, v) = app.call(&rh, Method::POST, &format!("/cases/{case_id}/close"), Some(json!({ "version": c["case"]["version"] }))).await;
    assert_eq!(status, StatusCode::CONFLICT, "écart non arbitré : {v}");
    let (status, _) = app
        .call(&rh, Method::POST, &format!("/exit-controls/{}/loss", ctrl("bien", "SIM")), Some(json!({ "reason": "déclarée perdue par la salariée" })))
        .await;
    assert_eq!(status, StatusCode::OK);
    let c = case(&app, &rh, &reference).await;
    let (status, v) = app.call(&rh, Method::POST, &format!("/cases/{case_id}/close"), Some(json!({ "version": c["case"]["version"] }))).await;
    assert_eq!(status, StatusCode::OK, "{v}");
}
