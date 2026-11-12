//! Tests d'intégration des scénarios de recette (05-Recette.md) sur une
//! vraie base isolée. Nécessitent DATABASE_URL vers un PostgreSQL de test.

mod common;

use axum::body::Body;
use axum::http::{header, Method, Request, StatusCode};
use common::TestApp;
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

/// R01 — deux annonces, rejeu idempotent, avis IT + FORMATION sans doublon
/// pour l'utilisateur multi-rôles.
#[sqlx::test(migrator = "server::MIGRATOR")]
async fn r01_annonce_idempotente_et_avis_sans_doublon(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let rh = app.user("Rh Un", &["RH"]).await;
    let admin = app.user("Admin Un", &["ADMIN"]).await;
    let multi = app.user("It Formation", &["IT", "FORMATION"]).await;
    let site = app.site("ORAN").await;

    let body = json!({ "first_name": "Amine", "last_name": "Benali", "job_title": "Commercial",
                       "site_id": site, "planned_arrival": "2026-12-01" });
    let key = [("idempotency-key", "cle-r01")];
    let (s1, v1) = app.call_with(&rh, Method::POST, "/arrivals", Some(body.clone()), &key).await;
    assert_eq!(s1, StatusCode::CREATED, "{v1}");
    assert_eq!(v1["reference"], "ONB-00001");
    // Rejeu : même résultat, pas de second dossier.
    let (s2, v2) = app.call_with(&rh, Method::POST, "/arrivals", Some(body.clone()), &key).await;
    assert_eq!(s2, StatusCode::CREATED);
    assert_eq!(v1, v2);
    // Même clé, autre contenu : refus.
    let mut other = body.clone();
    other["first_name"] = json!("Autre");
    let (s3, _) = app.call_with(&rh, Method::POST, "/arrivals", Some(other), &key).await;
    assert_eq!(s3, StatusCode::UNPROCESSABLE_ENTITY);
    // Sans clé : refus.
    let (s4, _) = app.call(&rh, Method::POST, "/arrivals", Some(body.clone())).await;
    assert_eq!(s4, StatusCode::UNPROCESSABLE_ENTITY);

    app.announce(&admin, site, "Lina", "Haddad", json!({})).await;
    let cases: i64 = sqlx::query_scalar("SELECT count(*) FROM cases WHERE kind = 'ONB'").fetch_one(app.pool()).await.unwrap();
    assert_eq!(cases, 2);

    // Aucun besoin précis : aucune tâche inventée, mais IT et FORMATION avisés.
    let tasks: i64 = sqlx::query_scalar("SELECT count(*) FROM tasks").fetch_one(app.pool()).await.unwrap();
    assert_eq!(tasks, 0);
    let roles: Vec<Vec<String>> = sqlx::query_scalar("SELECT target_roles FROM outbox ORDER BY created_at")
        .fetch_all(app.pool())
        .await
        .unwrap();
    assert_eq!(roles, vec![vec!["IT".to_string(), "FORMATION".to_string()]; 2]);

    server::worker::tick(&app.state).await.unwrap();
    let per_event: Vec<i64> = sqlx::query_scalar(
        "SELECT count(*) FROM deliveries WHERE recipient_email = $1 GROUP BY outbox_id",
    )
    .bind(&multi.email)
    .fetch_all(app.pool())
    .await
    .unwrap();
    assert_eq!(per_event, vec![1, 1], "un seul mail par avis pour l'utilisateur IT + FORMATION");
    let journalised: i64 = sqlx::query_scalar("SELECT count(*) FROM deliveries WHERE status = 'journalise'")
        .fetch_one(app.pool())
        .await
        .unwrap();
    assert_eq!(journalised, 2);
}

/// R02 + R06 — préférence fixe, remise d'un portable avec chargeur et SIM,
/// restitution partielle, PDF et scan.
#[sqlx::test(migrator = "server::MIGRATOR")]
async fn r02_r06_remise_substituee_et_restitution_partielle(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let rh = app.user("Rh Deux", &["RH"]).await;
    let it = app.user("It Deux", &["IT"]).await;
    let site = app.site("ORAN").await;
    let v = app
        .announce(&rh, site, "Karim", "Saidi", json!({ "needs": [{ "category": "pc", "answer": "oui", "preference": "fixe" }] }))
        .await;
    let case_id: Uuid = serde_json::from_value(v["case_id"].clone()).unwrap();
    let employee_id: Uuid = serde_json::from_value(v["employee_id"].clone()).unwrap();

    let (asset, _): (Uuid, String) = sqlx::query_as(
        "INSERT INTO assets (inventory_no, qr_token, kind, label, serial_no) VALUES ('PC-042', 'CP-T1', 'pc_portable', 'Portable', 'SN1') RETURNING id, qr_token",
    )
    .fetch_one(app.pool())
    .await
    .unwrap();
    let charger: Uuid = sqlx::query_scalar("INSERT INTO accessories (label, quantity_on_hand) VALUES ('Chargeur', 3) RETURNING id")
        .fetch_one(app.pool())
        .await
        .unwrap();
    let sim: Uuid = sqlx::query_scalar("INSERT INTO sim_cards (iccid) VALUES ('8921300000000000009') RETURNING id")
        .fetch_one(app.pool())
        .await
        .unwrap();

    let (_, tasks) = app.call(&it, Method::GET, "/tasks", None).await;
    let task_id = tasks[0]["id"].as_str().unwrap().to_string();
    assert_eq!(tasks[0]["need_preference"], "fixe");
    let (s, _) = app.call(&it, Method::POST, &format!("/tasks/{task_id}/claim"), None).await;
    assert_eq!(s, StatusCode::OK);
    // Terminer sans remise ni observation : refusé (préparé ≠ remis).
    let (s, _) = app.call(&it, Method::POST, &format!("/tasks/{task_id}/complete"), Some(json!({}))).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);

    let lines = json!([
        { "type": "asset", "asset_id": asset, "condition": "Neuf" },
        { "type": "accessoire", "accessory_id": charger, "quantity": 1 },
        { "type": "sim", "sim_id": sim }
    ]);
    let handover = json!({ "employee_id": employee_id, "case_id": case_id, "task_id": task_id, "lines": lines });
    // Portable au lieu du fixe préféré : raison obligatoire.
    let (s, err) = app.call(&it, Method::POST, "/handovers", Some(handover.clone())).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{err}");
    assert_eq!(err["field"], "substitution_reason");
    let mut with_reason = handover.clone();
    with_reason["substitution_reason"] = json!("Plus de PC fixe en stock");
    let (s, remise) = app.call(&it, Method::POST, "/handovers", Some(with_reason)).await;
    assert_eq!(s, StatusCode::CREATED, "{remise}");

    // La préférence RH reste intacte.
    let pref: Option<String> = sqlx::query_scalar("SELECT preference FROM case_needs WHERE case_id = $1 AND category = 'pc'")
        .bind(case_id)
        .fetch_one(app.pool())
        .await
        .unwrap();
    assert_eq!(pref.as_deref(), Some("fixe"));
    // Le même actif ne peut pas être remis deux fois.
    let other = app.announce(&rh, site, "Autre", "Personne", json!({})).await;
    let (s, _) = app
        .call(&it, Method::POST, "/handovers", Some(json!({ "employee_id": other["employee_id"], "lines": [{ "type": "asset", "asset_id": asset }] })))
        .await;
    assert_eq!(s, StatusCode::CONFLICT);

    let (s, _) = app.call(&it, Method::POST, &format!("/tasks/{task_id}/complete"), Some(json!({}))).await;
    assert_eq!(s, StatusCode::OK);

    // PDF d'affectation à trois lignes.
    let doc_id: Uuid = serde_json::from_value(remise["document_id"].clone()).unwrap();
    server::documents::ensure_original(app.pool(), &app.state.config, doc_id).await.unwrap();
    let req = Request::builder()
        .uri(format!("/api/v1/documents/{doc_id}/original"))
        .header(header::COOKIE, &it.cookie)
        .body(Body::empty())
        .unwrap();
    let (s, pdf) = app.raw(req).await;
    assert_eq!(s, StatusCode::OK);
    assert!(pdf.starts_with(b"%PDF-"));
    let n_lines: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM movement_lines ml JOIN documents d ON d.movement_id = ml.movement_id WHERE d.id = $1",
    )
    .bind(doc_id)
    .fetch_one(app.pool())
    .await
    .unwrap();
    assert_eq!(n_lines, 3);

    // Restitution du portable et du chargeur seulement.
    let (_, held) = app.call(&it, Method::GET, &format!("/employees/{employee_id}/holdings"), None).await;
    assert_eq!(held.as_array().unwrap().len(), 3);
    let origin = |label: &str| {
        held.as_array().unwrap().iter().find(|h| h["label"].as_str().unwrap().starts_with(label)).unwrap()["origin_line_id"].clone()
    };
    let too_much = json!({ "employee_id": employee_id, "lines": [{ "origin_line_id": origin("Chargeur"), "quantity": 2 }] });
    let (s, _) = app.call(&it, Method::POST, "/returns", Some(too_much)).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);
    let ret = json!({ "employee_id": employee_id, "case_id": case_id, "lines": [
        { "origin_line_id": origin("Portable"), "condition": "Bon état" },
        { "origin_line_id": origin("Chargeur"), "quantity": 1 }
    ]});
    let (s, restit) = app.call(&it, Method::POST, "/returns", Some(ret)).await;
    assert_eq!(s, StatusCode::CREATED, "{restit}");
    let (_, held) = app.call(&it, Method::GET, &format!("/employees/{employee_id}/holdings"), None).await;
    let held = held.as_array().unwrap();
    assert_eq!(held.len(), 1);
    assert_eq!(held[0]["item_type"], "sim", "la SIM reste attribuée");
    let status: String = sqlx::query_scalar("SELECT status FROM assets WHERE id = $1").bind(asset).fetch_one(app.pool()).await.unwrap();
    assert_eq!(status, "controle");

    // Restitution reliée à la remise d'origine ; signé seulement avec le scan.
    let rdoc: Uuid = serde_json::from_value(restit["document_id"].clone()).unwrap();
    let origin_ref: Option<String> = sqlx::query_scalar(
        "SELECT om.reference FROM documents d JOIN movement_lines ml ON ml.movement_id = d.movement_id
           JOIN movement_lines ol ON ol.id = ml.origin_line_id JOIN movements om ON om.id = ol.movement_id
          WHERE d.id = $1 LIMIT 1",
    )
    .bind(rdoc)
    .fetch_one(app.pool())
    .await
    .unwrap();
    assert_eq!(origin_ref, remise["reference"].as_str().map(String::from));
    server::documents::ensure_original(app.pool(), &app.state.config, rdoc).await.unwrap();
    let doc_status: String = sqlx::query_scalar("SELECT status FROM documents WHERE id = $1").bind(rdoc).fetch_one(app.pool()).await.unwrap();
    assert_eq!(doc_status, "genere", "imprimé n'est pas signé");

    let boundary = "XBOUNDARY";
    let multipart = format!(
        "--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"scan.pdf\"\r\nContent-Type: application/pdf\r\n\r\n%PDF-1.4 scan signé\r\n--{boundary}--\r\n"
    );
    let req = Request::builder()
        .method(Method::POST)
        .uri(format!("/api/v1/documents/{rdoc}/scan"))
        .header(header::COOKIE, &it.cookie)
        .header("x-cellulo-client", "web")
        .header(header::CONTENT_TYPE, format!("multipart/form-data; boundary={boundary}"))
        .body(Body::from(multipart))
        .unwrap();
    let (s, body) = app.raw(req).await;
    assert_eq!(s, StatusCode::OK, "{}", String::from_utf8_lossy(&body));
    let doc_status: String = sqlx::query_scalar("SELECT status FROM documents WHERE id = $1").bind(rdoc).fetch_one(app.pool()).await.unwrap();
    assert_eq!(doc_status, "signe");

    // Un exécutable déguisé est refusé.
    let fake = format!(
        "--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"scan.pdf\"\r\n\r\nMZ\x00\x00\r\n--{boundary}--\r\n"
    );
    let req = Request::builder()
        .method(Method::POST)
        .uri(format!("/api/v1/documents/{rdoc}/scan"))
        .header(header::COOKIE, &it.cookie)
        .header("x-cellulo-client", "web")
        .header(header::CONTENT_TYPE, format!("multipart/form-data; boundary={boundary}"))
        .body(Body::from(fake))
        .unwrap();
    let (s, _) = app.raw(req).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);
}

/// R03 — deux techniciens prennent la même tâche en même temps ; la
/// formation avance sans attendre le PC.
#[sqlx::test(migrator = "server::MIGRATOR")]
async fn r03_prise_concurrente_et_formation_independante(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let rh = app.user("Rh Trois", &["RH"]).await;
    let it1 = app.user("Tech Un", &["IT"]).await;
    let it2 = app.user("Tech Deux", &["IT"]).await;
    let multi = app.user("Tech Formateur", &["IT", "FORMATION"]).await;
    let site = app.site("ALGER").await;
    let catalog: Uuid = sqlx::query_scalar("INSERT INTO training_catalog (name) VALUES ('Sécurité') RETURNING id")
        .fetch_one(app.pool())
        .await
        .unwrap();
    app.announce(&rh, site, "Nora", "Kaci", json!({
        "needs": [{ "category": "pc", "answer": "oui" }],
        "trainings": [{ "catalog_id": catalog }]
    }))
    .await;

    let (_, tasks) = app.call(&it1, Method::GET, "/tasks", None).await;
    let pc_task = tasks.as_array().unwrap().iter().find(|t| t["role"] == "IT").unwrap()["id"].as_str().unwrap().to_string();
    let uri = format!("/tasks/{pc_task}/claim");
    let (a, b) = tokio::join!(
        app.call(&it1, Method::POST, &uri, None),
        app.call(&it2, Method::POST, &uri, None)
    );
    let statuses = [a.0, b.0];
    assert!(statuses.contains(&StatusCode::OK));
    assert!(statuses.contains(&StatusCode::CONFLICT));
    let loser = if a.0 == StatusCode::CONFLICT { a.1 } else { b.1 };
    assert!(loser["message"].as_str().unwrap().starts_with("déjà prise en charge par Tech"), "{loser}");
    let owners: i64 = sqlx::query_scalar("SELECT count(*) FROM tasks WHERE id = $1::uuid AND owner_id IS NOT NULL")
        .bind(&pc_task)
        .fetch_one(app.pool())
        .await
        .unwrap();
    assert_eq!(owners, 1);

    // L'utilisateur IT + FORMATION voit les deux files, sans doublon.
    let (_, mine) = app.call(&multi, Method::GET, "/tasks", None).await;
    let roles: Vec<&str> = mine.as_array().unwrap().iter().map(|t| t["role"].as_str().unwrap()).collect();
    assert_eq!(roles.len(), 2);
    assert!(roles.contains(&"IT") && roles.contains(&"FORMATION"));

    let (_, sessions) = app.call(&multi, Method::GET, "/trainings/sessions", None).await;
    let session = sessions[0]["id"].as_str().unwrap().to_string();
    let (s, v) = app
        .call(&multi, Method::POST, &format!("/trainings/sessions/{session}"), Some(json!({ "action": "realiser", "date": "2026-12-02" })))
        .await;
    assert_eq!(s, StatusCode::OK, "{v}");
    let done: String = sqlx::query_scalar("SELECT status FROM tasks WHERE role = 'FORMATION'").fetch_one(app.pool()).await.unwrap();
    assert_eq!(done, "termine");
    let pc_status: String = sqlx::query_scalar("SELECT status FROM tasks WHERE role = 'IT'").fetch_one(app.pool()).await.unwrap();
    assert_eq!(pc_status, "en_cours", "le PC n'est pas encore remis");
}

/// R04 — adresse cPanel puis accès M365 confirmé séparément.
#[sqlx::test(migrator = "server::MIGRATOR")]
async fn r04_email_et_m365_independants(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let rh = app.user("Rh Quatre", &["RH"]).await;
    let it = app.user("It Quatre", &["IT"]).await;
    let site = app.site("ORAN").await;
    let first = app.announce(&rh, site, "Sara", "Mansouri", json!({ "needs": [{ "category": "email_pro", "answer": "oui" }] })).await;
    let second = app.announce(&rh, site, "Ali", "Ferhat", json!({ "needs": [{ "category": "email_pro", "answer": "non" }] })).await;
    let emp = first["employee_id"].as_str().unwrap();

    let (s, _) = app
        .call(&it, Method::POST, &format!("/employees/{emp}/accounts"), Some(json!({ "system": "email", "identifier": "s.mansouri@exemple.test" })))
        .await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "plateforme obligatoire");
    let (s, _) = app
        .call(&it, Method::POST, &format!("/employees/{emp}/accounts"), Some(json!({ "system": "email", "platform": "cpanel", "identifier": "s.mansouri@exemple.test" })))
        .await;
    assert_eq!(s, StatusCode::OK);
    let (s, _) = app.call(&it, Method::POST, &format!("/employees/{emp}/m365"), Some(json!({ "verified": true }))).await;
    assert_eq!(s, StatusCode::OK);
    let (platform, m365): (String, bool) = sqlx::query_as(
        "SELECT a.platform, e.m365_verified_at IS NOT NULL FROM employee_accounts a JOIN employees e ON e.id = a.employee_id WHERE e.id = $1::uuid",
    )
    .bind(emp)
    .fetch_one(app.pool())
    .await
    .unwrap();
    assert_eq!((platform.as_str(), m365), ("cpanel", true));
    let second_tasks: i64 = sqlx::query_scalar("SELECT count(*) FROM tasks WHERE case_id = $1::uuid")
        .bind(second["case_id"].as_str().unwrap())
        .fetch_one(app.pool())
        .await
        .unwrap();
    assert_eq!(second_tasks, 0, "aucune tâche mail imposée");
}

/// R05 — clôture contrôlée puis carte de visite en demande autonome.
#[sqlx::test(migrator = "server::MIGRATOR")]
async fn r05_demande_ponctuelle_apres_cloture(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let rh = app.user("Rh Cinq", &["RH"]).await;
    let graphiste = app.user("Graph Cinq", &["GRAPHISTE"]).await;
    let site = app.site("ORAN").await;
    let v = app.announce(&rh, site, "Yasmine", "Toumi", json!({ "needs": [{ "category": "xena", "answer": "oui" }] })).await;
    let case_id = v["case_id"].as_str().unwrap();
    let emp = v["employee_id"].as_str().unwrap();

    let (s, err) = app.call(&rh, Method::POST, &format!("/cases/{case_id}/close"), Some(json!({ "version": 1 }))).await;
    assert_eq!(s, StatusCode::CONFLICT);
    assert_eq!(err["detail"]["unresolved"].as_array().unwrap().len(), 1);
    sqlx::query("UPDATE tasks SET status = 'annule' WHERE case_id = $1::uuid").bind(case_id).execute(app.pool()).await.unwrap();
    let (s, _) = app.call(&rh, Method::POST, &format!("/cases/{case_id}/close"), Some(json!({ "version": 1 }))).await;
    assert_eq!(s, StatusCode::OK);

    let before: i64 = sqlx::query_scalar("SELECT count(*) FROM outbox").fetch_one(app.pool()).await.unwrap();
    let (s, dem) = app
        .call(&rh, Method::POST, &format!("/employees/{emp}/requests"), Some(json!({ "category": "carte_visite", "parent_case_id": case_id })))
        .await;
    assert_eq!(s, StatusCode::CREATED, "{dem}");
    assert!(dem["reference"].as_str().unwrap().starts_with("DEM-"));
    let roles: Vec<String> = sqlx::query_scalar("SELECT target_roles FROM outbox ORDER BY created_at DESC LIMIT 1")
        .fetch_one(app.pool())
        .await
        .unwrap();
    assert_eq!(roles, vec!["GRAPHISTE".to_string()]);
    let after: i64 = sqlx::query_scalar("SELECT count(*) FROM outbox").fetch_one(app.pool()).await.unwrap();
    assert_eq!(after, before + 1);
    let onb_status: String = sqlx::query_scalar("SELECT status FROM cases WHERE id = $1::uuid").bind(case_id).fetch_one(app.pool()).await.unwrap();
    assert_eq!(onb_status, "clos", "l'ONB n'est pas rouvert");

    // Le graphiste voit sa tâche et le dossier DEM, pas l'ONB ni les notes RH.
    let (_, tasks) = app.call(&graphiste, Method::GET, "/tasks", None).await;
    assert_eq!(tasks.as_array().unwrap().len(), 1);
    let (s, _) = app.call(&graphiste, Method::GET, &format!("/cases/{}", dem["reference"].as_str().unwrap()), None).await;
    assert_eq!(s, StatusCode::OK);
    let onb_ref = v["reference"].as_str().unwrap();
    let (s, _) = app.call(&graphiste, Method::GET, &format!("/cases/{onb_ref}"), None).await;
    assert_eq!(s, StatusCode::FORBIDDEN);
}

/// R14 (partiel) — refus côté API, y compris en appel direct.
#[sqlx::test(migrator = "server::MIGRATOR")]
async fn r14_refus_hors_perimetre(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let graphiste = app.user("Graph Quatorze", &["GRAPHISTE"]).await;
    let rh = app.user("Rh Quatorze", &["RH"]).await;
    let (s, _) = app.call(&graphiste, Method::GET, "/employees", None).await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let (s, _) = app.call(&graphiste, Method::GET, &format!("/documents/{}/original", Uuid::new_v4()), None).await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let (s, _) = app.call(&graphiste, Method::GET, "/admin/users", None).await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let (s, _) = app.call(&rh, Method::POST, "/handovers", Some(json!({ "employee_id": Uuid::new_v4(), "lines": [] }))).await;
    assert_eq!(s, StatusCode::FORBIDDEN);

    // Commande sans l'en-tête client : refusée (protection CSRF).
    let req = Request::builder()
        .method(Method::POST)
        .uri("/api/v1/arrivals")
        .header(header::COOKIE, &rh.cookie)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from("{}"))
        .unwrap();
    let (s, _) = app.raw(req).await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    // Sans session : 401.
    let req = Request::builder().uri("/api/v1/tasks").body(Body::empty()).unwrap();
    let (s, _) = app.raw(req).await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
}

/// Homonymes et matricule déjà attribué (M01).
#[sqlx::test(migrator = "server::MIGRATOR")]
async fn homonymes_proposes_et_matricule_unique(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let rh = app.user("Rh Six", &["RH"]).await;
    let site = app.site("ORAN").await;
    app.announce(&rh, site, "Amine", "Benali", json!({ "matricule": "00042" })).await;
    let (_, cands) = app.call(&rh, Method::GET, "/employees/similar?first_name=AMINE&last_name=benali", None).await;
    assert_eq!(cands.as_array().unwrap().len(), 1);
    // Deux homonymes peuvent coexister (sans matricule identique).
    app.announce(&rh, site, "Amine", "Benali", json!({})).await;
    // Un matricule déjà attribué ne crée pas un second salarié.
    let (s, err) = app
        .call_with(
            &rh,
            Method::POST,
            "/arrivals",
            Some(json!({ "first_name": "X", "last_name": "Y", "job_title": "Z", "site_id": site,
                          "planned_arrival": "2026-12-01", "matricule": " 000 42" })),
            &[("idempotency-key", "k-mat")],
        )
        .await;
    assert_eq!(s, StatusCode::CONFLICT, "{err}");
    let kept: String = sqlx::query_scalar("SELECT matricule FROM employees WHERE matricule IS NOT NULL").fetch_one(app.pool()).await.unwrap();
    assert_eq!(kept, "00042", "zéros initiaux préservés");
}
