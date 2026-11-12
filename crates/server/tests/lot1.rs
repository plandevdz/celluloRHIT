//! Lot 1 — fiabilisation de l'arrivée : confidentialité par périmètre,
//! confirmation de présence, preuves de fin de tâche, avis honnêtes.
//! Données fictives uniquement ; nécessite DATABASE_URL.

mod common;

use axum::http::{Method, StatusCode};
use common::TestApp;
use serde_json::{json, Value};
use sqlx::PgPool;
use uuid::Uuid;

fn task_id(tasks: &Value, role: &str, title_part: &str) -> String {
    tasks
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["role"] == role && t["title"].as_str().unwrap().contains(title_part))
        .unwrap_or_else(|| panic!("tâche {role} « {title_part} » absente : {tasks}"))["id"]
        .as_str()
        .unwrap()
        .to_string()
}

async fn new_asset(app: &TestApp, inv: &str, kind: &str) -> Uuid {
    sqlx::query_scalar("INSERT INTO assets (inventory_no, qr_token, kind, label) VALUES ($1, 'QR-' || $1, $2, $1) RETURNING id")
        .bind(inv)
        .bind(kind)
        .fetch_one(app.pool())
        .await
        .unwrap()
}

/// Accès AUTORISÉ du Graphiste à un dossier qui contient sa carte de visite,
/// mais aussi une note RH, une observation IT et un motif de blocage IT.
#[sqlx::test(migrator = "server::MIGRATOR")]
async fn graphiste_autorise_ne_recoit_que_sa_mission(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let rh = app.user("Rh Conf", &["RH"]).await;
    let it = app.user("It Conf", &["IT"]).await;
    let graph = app.user("Graph Conf", &["GRAPHISTE"]).await;
    let form = app.user("Form Conf", &["FORMATION"]).await;
    let multi = app.user("It Form Conf", &["IT", "FORMATION"]).await;
    let site = app.site("ORAN").await;
    let catalog: Uuid = sqlx::query_scalar("INSERT INTO training_catalog (name) VALUES ('Accueil sécurité') RETURNING id")
        .fetch_one(app.pool())
        .await
        .unwrap();
    let v = app
        .announce(&rh, site, "Nadia", "Kerroum", json!({
            "matricule": "M-SECRET-77",
            "note": "NOTE-RH-CONFIDENTIELLE",
            "needs": [
                { "category": "carte_visite", "answer": "oui", "detail": "50 cartes" },
                { "category": "pc", "answer": "oui", "preference": "fixe" },
                { "category": "xena", "answer": "oui" }
            ],
            "trainings": [{ "catalog_id": catalog }]
        }))
        .await;
    let reference = v["reference"].as_str().unwrap().to_string();
    let emp = v["employee_id"].as_str().unwrap().to_string();

    // L'IT travaille : un blocage motivé et une exception avec observation.
    let (_, rh_view) = app.call(&rh, Method::GET, &format!("/cases/{reference}"), None).await;
    let pc = task_id(&rh_view["tasks"], "IT", "PC");
    let xena = task_id(&rh_view["tasks"], "IT", "Xena");
    app.call(&it, Method::POST, &format!("/tasks/{pc}/claim"), None).await;
    let (s, _) = app.call(&it, Method::POST, &format!("/tasks/{pc}/block"), Some(json!({ "reason": "BLOCAGE-IT-CONFIDENTIEL" }))).await;
    assert_eq!(s, StatusCode::OK);
    app.call(&it, Method::POST, &format!("/tasks/{xena}/claim"), None).await;
    let (s, _) = app
        .call(&it, Method::POST, &format!("/tasks/{xena}/complete"), Some(json!({ "resolution": "exception", "note": "OBS-IT-CONFIDENTIELLE" })))
        .await;
    assert_eq!(s, StatusCode::OK);
    app.call(&it, Method::POST, &format!("/employees/{emp}/accounts"), Some(json!({ "system": "email", "platform": "cpanel", "identifier": "n.kerroum@exemple.test" }))).await;

    // Le Graphiste ouvre son dossier : accès autorisé…
    let (s, g) = app.call(&graph, Method::GET, &format!("/cases/{reference}"), None).await;
    assert_eq!(s, StatusCode::OK, "{g}");
    assert_eq!(g["scope"], "equipe");
    let tasks = g["tasks"].as_array().unwrap();
    assert_eq!(tasks.len(), 1, "seule sa tâche : {tasks:?}");
    assert_eq!(tasks[0]["role"], "GRAPHISTE");
    assert_eq!(g["needs"].as_array().unwrap().len(), 1);
    assert_eq!(g["needs"][0]["category"], "carte_visite");
    assert_eq!(g["needs"][0]["detail"], "50 cartes");
    // … mais aucune donnée étrangère à sa mission dans la réponse JSON.
    let raw = g.to_string();
    for secret in ["NOTE-RH-CONFIDENTIELLE", "BLOCAGE-IT-CONFIDENTIEL", "OBS-IT-CONFIDENTIELLE", "M-SECRET-77", "Accueil sécurité"] {
        assert!(!raw.contains(secret), "« {secret} » ne doit pas être renvoyé au Graphiste : {raw}");
    }
    for empty in ["movements", "children", "notifications", "trainings"] {
        assert_eq!(g[empty], json!([]), "{empty} doit être vide pour le Graphiste");
    }
    assert!(g["case"]["matricule"].is_null());

    // Fiche salarié : coordonnées utiles, pas d'historique ni d'autres dossiers.
    let (s, ge) = app.call(&graph, Method::GET, &format!("/employees/{emp}"), None).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(ge["positions"], json!([]));
    assert_eq!(ge["holdings"], json!([]));
    assert_eq!(ge["trainings"], json!([]));
    assert!(ge["employee"]["matricule"].is_null());
    assert_eq!(ge["contact"]["email_pro"], "n.kerroum@exemple.test");
    let ge_raw = ge.to_string();
    assert!(!ge_raw.contains("M-SECRET-77"));
    // Une demande ultérieure sans tâche graphiste n'apparaît pas.
    let (s, _) = app.call(&rh, Method::POST, &format!("/employees/{emp}/requests"), Some(json!({ "category": "telephone" }))).await;
    assert_eq!(s, StatusCode::CREATED);
    let (_, ge) = app.call(&graph, Method::GET, &format!("/employees/{emp}"), None).await;
    let refs: Vec<&str> = ge["cases"].as_array().unwrap().iter().map(|c| c["reference"].as_str().unwrap()).collect();
    assert_eq!(refs, vec![reference.as_str()], "autres dossiers masqués");

    // Accès directs par identifiant : refusés.
    for path in [format!("/employees/{emp}/accounts"), format!("/employees/{emp}/holdings"), "/employees".to_string()] {
        let (s, _) = app.call(&graph, Method::GET, &path, None).await;
        assert_eq!(s, StatusCode::FORBIDDEN, "{path}");
    }

    // FORMATION : sa session, sans les observations IT.
    let (s, f) = app.call(&form, Method::GET, &format!("/cases/{reference}"), None).await;
    assert_eq!(s, StatusCode::OK);
    let fraw = f.to_string();
    assert!(fraw.contains("Accueil sécurité"));
    for secret in ["NOTE-RH-CONFIDENTIELLE", "BLOCAGE-IT-CONFIDENTIEL", "OBS-IT-CONFIDENTIELLE", "50 cartes"] {
        assert!(!fraw.contains(secret), "« {secret} » ne doit pas être renvoyé à la Formation");
    }
    assert!(f["tasks"].as_array().unwrap().iter().all(|t| t["role"] == "FORMATION"));

    // IT + FORMATION : droits cumulés des deux équipes, sans la note RH.
    let (_, m) = app.call(&multi, Method::GET, &format!("/cases/{reference}"), None).await;
    let mraw = m.to_string();
    assert!(mraw.contains("BLOCAGE-IT-CONFIDENTIEL") && mraw.contains("Accueil sécurité"));
    assert!(!mraw.contains("NOTE-RH-CONFIDENTIELLE") && !mraw.contains("50 cartes"));

    // Le RH voit tout.
    let (_, r) = app.call(&rh, Method::GET, &format!("/cases/{reference}"), None).await;
    let rraw = r.to_string();
    assert!(rraw.contains("NOTE-RH-CONFIDENTIELLE") && rraw.contains("OBS-IT-CONFIDENTIELLE"));
}

/// Présence lundi, formation mardi, PC mercredi : actif dès lundi.
#[sqlx::test(migrator = "server::MIGRATOR")]
async fn confirmer_arrivee_distincte_de_la_cloture(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let rh = app.user("Rh Arr", &["RH"]).await;
    let it = app.user("It Arr", &["IT"]).await;
    let graph = app.user("Graph Arr", &["GRAPHISTE"]).await;
    let site = app.site("ALGER").await;
    let today = server::documents::algiers_today();
    let monday = today - chrono::Duration::days(2);
    let v = app
        .announce(&rh, site, "Sami", "Haddad", json!({ "planned_arrival": monday, "needs": [{ "category": "pc", "answer": "oui" }] }))
        .await;
    let emp = v["employee_id"].as_str().unwrap().to_string();
    let case_id = v["case_id"].as_str().unwrap().to_string();

    // Avant confirmation : signalé au RH, pas compté actif.
    let (_, check) = app.call(&rh, Method::GET, "/employees/arrivals-to-check", None).await;
    assert_eq!(check.as_array().unwrap().len(), 1);

    let uri = format!("/employees/{emp}/arrival");
    let (s, _) = app.call(&graph, Method::POST, &uri, Some(json!({ "actual_arrival": monday }))).await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let (s, _) = app.call(&rh, Method::POST, &uri, Some(json!({ "actual_arrival": today + chrono::Duration::days(1) }))).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "pas de présence future");

    // Double clic : un seul événement.
    let body = json!({ "actual_arrival": monday });
    let (a, b) = tokio::join!(app.call(&rh, Method::POST, &uri, Some(body.clone())), app.call(&rh, Method::POST, &uri, Some(body.clone())));
    assert_eq!((a.0, b.0), (StatusCode::OK, StatusCode::OK));
    let changed = [a.1["changed"].as_bool().unwrap(), b.1["changed"].as_bool().unwrap()];
    assert!(changed.contains(&true) && changed.contains(&false));
    let events: i64 = sqlx::query_scalar("SELECT count(*) FROM audit_events WHERE action = 'arrivee_confirmee' AND entity_id = $1::uuid")
        .bind(&emp)
        .fetch_one(app.pool())
        .await
        .unwrap();
    assert_eq!(events, 1);
    let (status, actual): (String, chrono::NaiveDate) =
        sqlx::query_as("SELECT status, actual_arrival FROM employees WHERE id = $1::uuid").bind(&emp).fetch_one(app.pool()).await.unwrap();
    assert_eq!((status.as_str(), actual), ("actif", monday));
    // Le dossier reste ouvert : le PC n'est pas encore remis.
    let cs: String = sqlx::query_scalar("SELECT status FROM cases WHERE id = $1::uuid").bind(&case_id).fetch_one(app.pool()).await.unwrap();
    assert_eq!(cs, "ouvert");
    // La présence confirmée figure dans l'historique de l'arrivée (vue RH).
    let (_, detail) = app.call(&rh, Method::GET, &format!("/cases/{}", v["reference"].as_str().unwrap()), None).await;
    assert!(detail["history"].to_string().contains("arrivee_confirmee"), "{}", detail["history"]);
    let (_, actifs) = app.call(&rh, Method::GET, "/employees?status=actif", None).await;
    assert_eq!(actifs["total"], 1, "compté actif dès sa présence");

    // Mercredi : remise du PC, le statut ne bouge pas.
    let asset = new_asset(&app, "PC-ARR", "pc_portable").await;
    let (s, _) = app
        .call(&it, Method::POST, "/handovers", Some(json!({ "employee_id": emp, "case_id": case_id, "lines": [{ "type": "asset", "asset_id": asset }] })))
        .await;
    assert_eq!(s, StatusCode::CREATED);

    // Correction de date : explicite et motivée.
    let (s, _) = app.call(&rh, Method::POST, &uri, Some(json!({ "actual_arrival": today }))).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);
    let (s, _) = app.call(&rh, Method::POST, &uri, Some(json!({ "actual_arrival": today, "correction_reason": "erreur de saisie" }))).await;
    assert_eq!(s, StatusCode::OK);
    let corr: i64 = sqlx::query_scalar("SELECT count(*) FROM audit_events WHERE action = 'date_arrivee_corrigee'")
        .fetch_one(app.pool())
        .await
        .unwrap();
    assert_eq!(corr, 1);

    // La clôture ne confirme pas la présence d'une autre recrue.
    let other = app.announce(&rh, site, "Lyna", "Brahimi", json!({})).await;
    let (s, _) = app.call(&rh, Method::POST, &format!("/cases/{}/close", other["case_id"].as_str().unwrap()), Some(json!({ "version": 1 }))).await;
    assert_eq!(s, StatusCode::OK);
    let st: String = sqlx::query_scalar("SELECT status FROM employees WHERE id = $1::uuid")
        .bind(other["employee_id"].as_str().unwrap())
        .fetch_one(app.pool())
        .await
        .unwrap();
    assert_eq!(st, "a_venir");
    let (_, check) = app.call(&rh, Method::GET, "/employees/arrivals-to-check", None).await;
    assert!(check.to_string().contains("Brahimi"), "dossier clos sans présence confirmée : à vérifier");
}

/// Une SIM ne prouve pas un PC ; la substitution est contrôlée sur toutes
/// les lignes ; « déjà détenu » et « exception » sont distingués de « remis ».
#[sqlx::test(migrator = "server::MIGRATOR")]
async fn preuve_de_remise_par_categorie(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let rh = app.user("Rh Preuve", &["RH"]).await;
    let it = app.user("It Preuve", &["IT"]).await;
    let site = app.site("ORAN").await;
    let v = app
        .announce(&rh, site, "Rania", "Saadi", json!({ "needs": [
            { "category": "pc", "answer": "oui", "preference": "fixe" },
            { "category": "sim", "answer": "oui" },
            { "category": "telephone", "answer": "oui" }
        ]}))
        .await;
    let emp = v["employee_id"].as_str().unwrap().to_string();
    let case_id = v["case_id"].as_str().unwrap().to_string();
    let (_, view) = app.call(&rh, Method::GET, &format!("/cases/{}", v["reference"].as_str().unwrap()), None).await;
    let pc = task_id(&view["tasks"], "IT", "PC");
    let sim_task = task_id(&view["tasks"], "IT", "SIM");
    let tel_task = task_id(&view["tasks"], "IT", "téléphone");
    for t in [&pc, &sim_task, &tel_task] {
        app.call(&it, Method::POST, &format!("/tasks/{t}/claim"), None).await;
    }

    // Remise d'une SIM rattachée à la tâche PC : ne prouve pas le PC.
    let sim: Uuid = sqlx::query_scalar("INSERT INTO sim_cards (iccid) VALUES ('8921300000000000077') RETURNING id").fetch_one(app.pool()).await.unwrap();
    let (s, _) = app
        .call(&it, Method::POST, "/handovers", Some(json!({ "employee_id": emp, "case_id": case_id, "task_id": pc, "lines": [{ "type": "sim", "sim_id": sim }] })))
        .await;
    assert_eq!(s, StatusCode::CREATED);
    let (s, err) = app.call(&it, Method::POST, &format!("/tasks/{pc}/complete"), Some(json!({}))).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{err}");
    assert!(err["message"].as_str().unwrap().contains("0/1"));
    // La même remise prouve bien la SIM.
    let (s, done) = app.call(&it, Method::POST, &format!("/tasks/{sim_task}/complete"), Some(json!({}))).await;
    assert_eq!(s, StatusCode::OK, "{done}");
    assert_eq!(done["resolution"], "remis");

    // Téléphone en première ligne, portable ensuite, préférence fixe : motif exigé.
    let phone = new_asset(&app, "TEL-P1", "telephone").await;
    let laptop = new_asset(&app, "PC-P1", "pc_portable").await;
    let lines = json!([{ "type": "asset", "asset_id": phone }, { "type": "asset", "asset_id": laptop }]);
    let (s, err) = app
        .call(&it, Method::POST, "/handovers", Some(json!({ "employee_id": emp, "case_id": case_id, "task_id": tel_task, "lines": lines })))
        .await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{err}");
    assert_eq!(err["field"], "substitution_reason");
    let (s, _) = app
        .call(&it, Method::POST, "/handovers", Some(json!({ "employee_id": emp, "case_id": case_id, "task_id": tel_task, "lines": lines, "substitution_reason": "Plus de fixe" })))
        .await;
    assert_eq!(s, StatusCode::CREATED);
    let (s, done) = app.call(&it, Method::POST, &format!("/tasks/{pc}/complete"), Some(json!({}))).await;
    assert_eq!(s, StatusCode::OK, "{done}");
    assert_eq!(done["resolution"], "remis");
    assert_eq!(done["evidence"]["quantite"], 1);

    // « Déjà détenu » : seule une affectation active du bon type est acceptée.
    let v2 = app.announce(&rh, site, "Yanis", "Ouali", json!({ "needs": [{ "category": "pc", "answer": "oui" }, { "category": "telephone", "answer": "oui" }] })).await;
    let emp2 = v2["employee_id"].as_str().unwrap().to_string();
    let old_pc = new_asset(&app, "PC-ANCIEN", "pc_fixe").await;
    let sim2: Uuid = sqlx::query_scalar("INSERT INTO sim_cards (iccid) VALUES ('8921300000000000078') RETURNING id").fetch_one(app.pool()).await.unwrap();
    // Remise hors dossier (prêt direct antérieur) : toujours possible.
    let (s, _) = app
        .call(&it, Method::POST, "/handovers", Some(json!({ "employee_id": emp2, "lines": [{ "type": "asset", "asset_id": old_pc }, { "type": "sim", "sim_id": sim2 }] })))
        .await;
    assert_eq!(s, StatusCode::CREATED);
    let (_, held) = app.call(&it, Method::GET, &format!("/employees/{emp2}/holdings"), None).await;
    let assignment = |t: &str| held.as_array().unwrap().iter().find(|h| h["item_type"] == t && (t != "asset" || true)).unwrap()["assignment_id"].clone();
    let (_, view2) = app.call(&rh, Method::GET, &format!("/cases/{}", v2["reference"].as_str().unwrap()), None).await;
    let pc2 = task_id(&view2["tasks"], "IT", "PC");
    let tel2 = task_id(&view2["tasks"], "IT", "téléphone");
    app.call(&it, Method::POST, &format!("/tasks/{pc2}/claim"), None).await;
    app.call(&it, Method::POST, &format!("/tasks/{tel2}/claim"), None).await;
    let (s, _) = app
        .call(&it, Method::POST, &format!("/tasks/{pc2}/complete"), Some(json!({ "resolution": "deja_detenu", "assignment_ids": [assignment("sim")] })))
        .await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "une SIM détenue ne vaut pas un PC");
    let (s, done) = app
        .call(&it, Method::POST, &format!("/tasks/{pc2}/complete"), Some(json!({ "resolution": "deja_detenu", "assignment_ids": [assignment("asset")] })))
        .await;
    assert_eq!(s, StatusCode::OK, "{done}");
    assert_eq!(done["resolution"], "deja_detenu");

    // Exception : motif obligatoire, jamais comptée comme remise.
    let (s, _) = app.call(&it, Method::POST, &format!("/tasks/{tel2}/complete"), Some(json!({ "resolution": "exception" }))).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);
    let (s, done) = app
        .call(&it, Method::POST, &format!("/tasks/{tel2}/complete"), Some(json!({ "resolution": "exception", "note": "Utilise son téléphone personnel (accord direction)" })))
        .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(done["resolution"], "exception");
    let phones_out: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM assignments a JOIN assets x ON x.id = a.asset_id WHERE a.employee_id = $1::uuid AND x.kind = 'telephone'",
    )
    .bind(&emp2)
    .fetch_one(app.pool())
    .await
    .unwrap();
    assert_eq!(phones_out, 0, "aucune affectation fictive créée par une exception");
}

/// Une remise unique « PC + SIM » lancée depuis la tâche SIM solde aussi
/// la tâche PC du même dossier grâce à la ligne PC (et seulement elle).
/// Inverse : une SIM seule, ou un PC remis dans un autre dossier, ne
/// permettent pas de déclarer ce PC « remis » ; le résultat correct pour un
/// PC déjà détenu est « déjà détenu ».
#[sqlx::test(migrator = "server::MIGRATOR")]
async fn remise_unique_pc_sim_et_cas_inverses(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let rh = app.user("Rh Unique", &["RH"]).await;
    let it = app.user("It Unique", &["IT"]).await;
    let site = app.site("ORAN").await;
    let needs = json!({ "needs": [{ "category": "pc", "answer": "oui" }, { "category": "sim", "answer": "oui" }] });
    let new_sim = |iccid: &'static str| {
        let pool = app.pool().clone();
        async move {
            sqlx::query_scalar::<_, Uuid>("INSERT INTO sim_cards (iccid) VALUES ($1) RETURNING id")
                .bind(iccid)
                .fetch_one(&pool)
                .await
                .unwrap()
        }
    };
    let line_of = |movement_ref: String, asset: Option<Uuid>, sim: Option<Uuid>| {
        let pool = app.pool().clone();
        async move {
            sqlx::query_scalar::<_, Uuid>(
                "SELECT ml.id FROM movement_lines ml JOIN movements m ON m.id = ml.movement_id
                  WHERE m.reference = $1 AND (ml.asset_id = $2 OR ml.sim_id = $3)",
            )
            .bind(movement_ref)
            .bind(asset)
            .bind(sim)
            .fetch_one(&pool)
            .await
            .unwrap()
        }
    };

    // ---- Cas nominal : une seule remise « SIM + PC » depuis la tâche SIM.
    let a = app.announce(&rh, site, "Adel", "Bouzid", needs.clone()).await;
    let (emp_a, case_a) = (a["employee_id"].as_str().unwrap().to_string(), a["case_id"].as_str().unwrap().to_string());
    let (_, view) = app.call(&rh, Method::GET, &format!("/cases/{}", a["reference"].as_str().unwrap()), None).await;
    let pc_a = task_id(&view["tasks"], "IT", "PC");
    let sim_task_a = task_id(&view["tasks"], "IT", "SIM");
    app.call(&it, Method::POST, &format!("/tasks/{pc_a}/claim"), None).await;
    app.call(&it, Method::POST, &format!("/tasks/{sim_task_a}/claim"), None).await;
    let pc_asset = new_asset(&app, "PC-UNIQUE", "pc_fixe").await;
    let sim_a = new_sim("8921300000000000101").await;
    let (s, remise) = app
        .call(&it, Method::POST, "/handovers", Some(json!({
            "employee_id": emp_a, "case_id": case_a, "task_id": sim_task_a,
            "lines": [{ "type": "sim", "sim_id": sim_a }, { "type": "asset", "asset_id": pc_asset }]
        })))
        .await;
    assert_eq!(s, StatusCode::CREATED, "{remise}");
    let mref = remise["reference"].as_str().unwrap().to_string();
    let pc_line = line_of(mref.clone(), Some(pc_asset), None).await;
    let sim_line = line_of(mref.clone(), None, Some(sim_a)).await;

    let (s, done) = app.call(&it, Method::POST, &format!("/tasks/{pc_a}/complete"), Some(json!({}))).await;
    assert_eq!(s, StatusCode::OK, "la tâche PC est soldée par la remise lancée depuis la tâche SIM : {done}");
    assert_eq!(done["resolution"], "remis");
    assert_eq!(done["evidence"]["movement_line_ids"], json!([pc_line]), "preuve = la ligne PC, pas la ligne SIM");
    assert_eq!(done["evidence"]["movements"], json!([mref]));
    let (s, done) = app.call(&it, Method::POST, &format!("/tasks/{sim_task_a}/complete"), Some(json!({}))).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(done["evidence"]["movement_line_ids"], json!([sim_line]), "preuve SIM = la ligne SIM");

    // ---- Inverse 1 : une SIM seule ne prouve pas le PC.
    let b = app.announce(&rh, site, "Farah", "Meziane", needs.clone()).await;
    let (emp_b, case_b) = (b["employee_id"].as_str().unwrap().to_string(), b["case_id"].as_str().unwrap().to_string());
    let (_, view) = app.call(&rh, Method::GET, &format!("/cases/{}", b["reference"].as_str().unwrap()), None).await;
    let pc_b = task_id(&view["tasks"], "IT", "PC");
    let sim_task_b = task_id(&view["tasks"], "IT", "SIM");
    app.call(&it, Method::POST, &format!("/tasks/{pc_b}/claim"), None).await;
    app.call(&it, Method::POST, &format!("/tasks/{sim_task_b}/claim"), None).await;
    let sim_b = new_sim("8921300000000000102").await;
    let (s, _) = app
        .call(&it, Method::POST, "/handovers", Some(json!({
            "employee_id": emp_b, "case_id": case_b, "task_id": sim_task_b, "lines": [{ "type": "sim", "sim_id": sim_b }]
        })))
        .await;
    assert_eq!(s, StatusCode::CREATED);
    let (s, err) = app.call(&it, Method::POST, &format!("/tasks/{pc_b}/complete"), Some(json!({}))).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "SIM seule : PC non remis — {err}");
    assert!(err["message"].as_str().unwrap().contains("0/1"));

    // ---- Inverse 2 : PC remis dans un AUTRE dossier (demande ponctuelle).
    let (s, dem) = app
        .call(&rh, Method::POST, &format!("/employees/{emp_b}/requests"), Some(json!({ "category": "pc", "force": true })))
        .await;
    assert_eq!(s, StatusCode::CREATED, "{dem}");
    let other_pc = new_asset(&app, "PC-AUTRE-DOSSIER", "pc_portable").await;
    let (s, remise_dem) = app
        .call(&it, Method::POST, "/handovers", Some(json!({
            "employee_id": emp_b, "case_id": dem["case_id"], "lines": [{ "type": "asset", "asset_id": other_pc }]
        })))
        .await;
    assert_eq!(s, StatusCode::CREATED, "{remise_dem}");
    let (s, err) = app.call(&it, Method::POST, &format!("/tasks/{pc_b}/complete"), Some(json!({ "resolution": "remis" }))).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "un PC remis dans un autre dossier ne prouve pas ce PC : {err}");

    // Le salarié possède déjà un PC : le résultat correct est « déjà détenu ».
    let (_, held) = app.call(&it, Method::GET, &format!("/employees/{emp_b}/holdings"), None).await;
    let pc_assignment = held
        .as_array()
        .unwrap()
        .iter()
        .find(|h| h["asset_kind"] == "pc_portable")
        .expect("PC détenu")["assignment_id"]
        .clone();
    let (s, done) = app
        .call(&it, Method::POST, &format!("/tasks/{pc_b}/complete"), Some(json!({ "resolution": "deja_detenu", "assignment_ids": [pc_assignment] })))
        .await;
    assert_eq!(s, StatusCode::OK, "{done}");
    assert_eq!(done["resolution"], "deja_detenu");
    assert_eq!(done["evidence"]["remises_d_origine"], json!([remise_dem["reference"]]));
}

/// Mode journal : visibilité ≠ envoi ; équipe sans destinataire signalée
/// sans annuler l'arrivée ; un seul avis pour un IT + FORMATION.
#[sqlx::test(migrator = "server::MIGRATOR")]
async fn avis_honnetes_en_mode_journal(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let rh = app.user("Rh Avis", &["RH"]).await;
    let admin = app.user("Admin Avis", &["ADMIN"]).await;
    let multi = app.user("It Form Avis", &["IT", "FORMATION"]).await;
    let site = app.site("ORAN").await;
    // Aucun Graphiste actif.
    let v = app.announce(&rh, site, "Ines", "Mokrani", json!({ "needs": [{ "category": "carte_visite", "answer": "oui" }] })).await;
    assert_eq!(v["mail_mode"], "journal");
    assert_eq!(v["visible_to"], json!(["IT", "FORMATION", "GRAPHISTE"]));
    assert_eq!(v["teams_without_recipient"], json!(["GRAPHISTE"]));
    assert!(v.get("notified_roles").is_none(), "plus de « équipes prévenues » trompeur");

    server::worker::tick(&app.state).await.unwrap();
    let missing: Vec<String> = sqlx::query_scalar("SELECT unnest(missing_roles) FROM outbox").fetch_all(app.pool()).await.unwrap();
    assert_eq!(missing, vec!["GRAPHISTE".to_string()]);
    let mine: i64 = sqlx::query_scalar("SELECT count(*) FROM deliveries WHERE recipient_email = $1")
        .bind(&multi.email)
        .fetch_one(app.pool())
        .await
        .unwrap();
    assert_eq!(mine, 1);
    let (_, anomalies) = app.call(&admin, Method::GET, "/admin/notifications?status=anomalie", None).await;
    assert_eq!(anomalies.as_array().unwrap().len(), 1);
    assert_eq!(anomalies[0]["missing_roles"], json!(["GRAPHISTE"]));

    let (_, detail) = app.call(&rh, Method::GET, &format!("/cases/{}", v["reference"].as_str().unwrap()), None).await;
    let n = &detail["notifications"][0];
    // Seul destinataire actif : l'utilisateur IT + FORMATION, une fois.
    assert_eq!(n["recipients"], 1, "{n}");
    assert_eq!(n["logged"], 1, "{n}");
    assert_eq!(n["accepted"], 0, "consigné n'est pas envoyé");
    assert!(n.get("delivered").is_none());
    let (_, health) = app.call(&rh, Method::GET, "/health", None).await;
    assert_eq!(health["mail_mode"], "journal");
}
