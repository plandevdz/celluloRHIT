//! Administration depuis l'interface (16/11/2026) : modules, envoi Exchange
//! (certificat généré sur le serveur, aucun appel réseau dans les tests),
//! modèles de tâches, modification des listes par chaque équipe, wilayas
//! cibles d'une mutation et sauvegarde chiffrée. Données fictives.

mod common;

use axum::http::{Method, StatusCode};
use common::TestApp;
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

#[sqlx::test(migrator = "server::MIGRATOR")]
async fn modules_et_envoi_regles_dans_l_interface(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let admin = app.user("Admin Fictif", &["ADMIN"]).await;
    let rh = app.user("Rh Fictif", &["RH"]).await;

    // Réservé à l'ADMIN.
    let (status, _) = app.call(&rh, Method::GET, "/admin/config", None).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, cfg) = app.call(&admin, Method::GET, "/admin/config", None).await;
    assert_eq!(status, StatusCode::OK, "{cfg}");
    assert_eq!(cfg["effective"]["mail_mode"], "journal");
    assert_eq!(cfg["overridden"], json!([]));

    // Fermer un module : effet immédiat sur l'API, sans redémarrage.
    let (status, _) = app.call(&rh, Method::GET, "/vehicles", None).await;
    assert_eq!(status, StatusCode::OK);
    let (status, v) = app.call(&admin, Method::PUT, "/admin/config/modules", Some(json!({ "desactives": ["vehicules", "etats"] }))).await;
    assert_eq!(status, StatusCode::OK, "{v}");
    let (status, v) = app.call(&rh, Method::GET, "/vehicles", None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(v["error"], "module_desactive");
    let (_, h) = app.call(&rh, Method::GET, "/health", None).await;
    assert_eq!(h["modules_desactives"], json!(["etats", "vehicules"]));
    // Faute de frappe refusée : rien n'est enregistré.
    let (status, _) = app.call(&admin, Method::PUT, "/admin/config/modules", Some(json!({ "desactives": ["vehicule"] }))).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    // Retour aux valeurs du .env (ici : tout ouvert).
    let (status, _) = app.call(&admin, Method::DELETE, "/admin/config/modules", None).await;
    assert_eq!(status, StatusCode::OK);
    let (status, _) = app.call(&rh, Method::GET, "/vehicles", None).await;
    assert_eq!(status, StatusCode::OK);

    // Envoi Exchange : refusé sans certificat.
    let mail = json!({ "mode": "graph", "sender": "avis@exemple.test", "tenant_id": "tenant-fictif", "client_id": "client-fictif",
                       "allowed_domains": ["@Exemple.test"] });
    let (status, v) = app.call(&admin, Method::PUT, "/admin/config/mail", Some(mail.clone())).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(v["message"].as_str().unwrap().contains("certificat"), "{v}");
    // Certificat généré sur le serveur ; seule la partie publique se télécharge.
    let (status, c) = app.call(&admin, Method::POST, "/admin/config/certificates/envoi", Some(json!({}))).await;
    assert_eq!(status, StatusCode::OK, "{c}");
    assert_eq!(c["source"], "interface");
    assert_eq!(c["details"]["subject"], "CN=celluloplast-envoi");
    let (status, _) = app.call(&admin, Method::POST, "/admin/config/certificates/envoi", Some(json!({}))).await;
    assert_eq!(status, StatusCode::CONFLICT, "remplacement à confirmer");
    let req = axum::http::Request::builder()
        .uri("/api/v1/admin/config/certificates/envoi")
        .header("cookie", &admin.cookie)
        .body(axum::body::Body::empty())
        .unwrap();
    let (status, bytes) = app.raw(req).await;
    assert_eq!(status, StatusCode::OK);
    let pem = String::from_utf8(bytes).unwrap();
    assert!(pem.contains("BEGIN CERTIFICATE") && !pem.contains("PRIVATE KEY"));
    // Activation : le client Graph est construit (aucun appel réseau).
    let (status, v) = app.call(&admin, Method::PUT, "/admin/config/mail", Some(mail)).await;
    assert_eq!(status, StatusCode::OK, "{v}");
    let (_, cfg) = app.call(&admin, Method::GET, "/admin/config", None).await;
    assert_eq!(cfg["effective"]["mail_mode"], "graph");
    assert_eq!(cfg["effective"]["mail_allowed_domains"], json!(["exemple.test"]));
    assert_eq!(cfg["overridden"], json!(["mail"]));
    assert!(app.state.mailer().is_some());
    // Retour au mode journal.
    let (status, _) = app.call(&admin, Method::PUT, "/admin/config/mail", Some(json!({ "mode": "journal" }))).await;
    assert_eq!(status, StatusCode::OK);
    assert!(app.state.mailer().is_none());
    assert_eq!(app.count("SELECT count(*) FROM audit_events WHERE action = 'parametres_modifies'").await, 3);

    // Échéances bornées.
    let (status, _) = app.call(&admin, Method::PUT, "/admin/config/alerts", Some(json!({ "alert_days": 0, "alert_km": 1000, "escalation_minutes": 60 }))).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    let (status, _) = app.call(&admin, Method::PUT, "/admin/config/alerts", Some(json!({ "alert_days": 15, "alert_km": 500, "escalation_minutes": 30 }))).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(app.state.cfg().alert_days, 15);
}

#[sqlx::test(migrator = "server::MIGRATOR")]
async fn modeles_de_taches_appliques_a_la_creation(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let admin = app.user("Admin Fictif", &["ADMIN"]).await;
    let rh = app.user("Rh Fictif", &["RH"]).await;
    let it = app.user("It Fictif", &["IT"]).await;
    let site = app.site("CHERAGA").await;

    let (status, list) = app.call(&it, Method::GET, "/task-templates", None).await;
    assert_eq!(status, StatusCode::OK);
    assert!(list.as_array().unwrap().iter().any(|t| t["task_key"] == "need:pc"));
    let (status, _) = app.call(&rh, Method::PUT, "/admin/task-templates/need:pc", Some(json!({ "title": "x" }))).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, _) = app.call(&admin, Method::PUT, "/admin/task-templates/inconnu", Some(json!({ "title": "x" }))).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, v) = app
        .call(&admin, Method::PUT, "/admin/task-templates/need:pc", Some(json!({
            "title": "Préparer le poste de travail", "instructions": "Installer l'antivirus et le VPN.", "delay_days": -3, "display_order": 5
        })))
        .await;
    assert_eq!(status, StatusCode::OK, "{v}");

    app.announce(&rh, site, "Lina", "Exemple", json!({ "needs": [{ "category": "pc", "answer": "oui" }, { "category": "email_pro", "answer": "oui" }] }))
        .await;
    let (title, due, instructions, order): (String, chrono::NaiveDate, Option<String>, i32) =
        sqlx::query_as("SELECT title, due_date, instructions, display_order FROM tasks WHERE gen_key = 'need:pc'").fetch_one(app.pool()).await.unwrap();
    assert_eq!(title, "Préparer le poste de travail");
    assert_eq!(due, chrono::NaiveDate::from_ymd_opt(2026, 11, 28).unwrap(), "trois jours avant l'arrivée du 01/12");
    assert_eq!(instructions.as_deref(), Some("Installer l'antivirus et le VPN."));
    assert_eq!(order, 5);
    // Sans modèle : comportement inchangé.
    let (due2, ins2): (chrono::NaiveDate, Option<String>) =
        sqlx::query_as("SELECT due_date, instructions FROM tasks WHERE gen_key = 'need:email_pro'").fetch_one(app.pool()).await.unwrap();
    assert_eq!(due2, chrono::NaiveDate::from_ymd_opt(2026, 12, 1).unwrap());
    assert!(ins2.is_none());
    // Consignes visibles dans « Mon travail ».
    let (_, tasks) = app.call(&it, Method::GET, "/tasks", None).await;
    let pc = tasks.as_array().unwrap().iter().find(|t| t["title"] == "Préparer le poste de travail").unwrap().clone();
    assert_eq!(pc["instructions"], "Installer l'antivirus et le VPN.");
    // Modifier le modèle ne réécrit pas la tâche existante.
    let (status, _) = app.call(&admin, Method::DELETE, "/admin/task-templates/need:pc", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(app.count("SELECT count(*) FROM tasks WHERE title = 'Préparer le poste de travail'").await, 1);
}

#[sqlx::test(migrator = "server::MIGRATOR")]
async fn chaque_equipe_modifie_ses_listes(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let admin = app.user("Admin Fictif", &["ADMIN"]).await;
    let rh = app.user("Rh Fictif", &["RH"]).await;
    let it = app.user("It Fictif", &["IT"]).await;
    let site = app.site("CHERAGA").await;
    let oran = app.site("ORAN").await;

    // RH : poste et site corrigés directement (fiche importée incomplète).
    let onb = app.announce(&rh, site, "Sami", "Fictif", json!({})).await;
    let emp = onb["employee_id"].as_str().unwrap().to_string();
    let (_, e) = app.call(&rh, Method::GET, &format!("/employees/{emp}"), None).await;
    let version = e["employee"]["version"].as_i64().unwrap();
    let (status, _) = app
        .call(&it, Method::PATCH, &format!("/employees/{emp}"), Some(json!({ "version": version, "job_title": "Magasinier" })))
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "l'IT ne modifie pas la fiche RH");
    let (status, v) = app
        .call(&rh, Method::PATCH, &format!("/employees/{emp}"), Some(json!({ "version": version, "job_title": "Magasinier", "site_id": oran })))
        .await;
    assert_eq!(status, StatusCode::OK, "{v}");
    let (_, e) = app.call(&rh, Method::GET, &format!("/employees/{emp}"), None).await;
    assert_eq!(e["employee"]["job_title"], "Magasinier");
    assert_eq!(e["employee"]["site_id"], json!(oran));
    assert_eq!(
        app.count("SELECT count(*) FROM audit_events WHERE action = 'modification_fiche' AND data->'situation'->>'poste_apres' = 'Magasinier'").await,
        1
    );

    // RH : wilayas (référentiel métier).
    let (status, w) = app.call(&rh, Method::POST, "/wilayas", Some(json!({ "code": 16, "name": "Alger" }))).await;
    assert_eq!(status, StatusCode::OK, "{w}");

    // IT : accessoire, SIM, ligne.
    let (status, acc) = app.call(&it, Method::POST, "/accessories", Some(json!({ "label": "Souris", "quantity": 3 }))).await;
    assert_eq!(status, StatusCode::OK, "{acc}");
    let acc = acc["id"].as_str().unwrap().to_string();
    let (status, v) = app.call(&it, Method::PATCH, &format!("/accessories/{acc}"), Some(json!({ "label": "Souris sans fil", "site_id": site }))).await;
    assert_eq!(status, StatusCode::OK, "{v}");
    assert_eq!(app.count("SELECT count(*) FROM accessories WHERE label = 'Souris sans fil' AND quantity_on_hand = 3 AND site_id IS NOT NULL").await, 1);
    let (_, sim) = app
        .call(&it, Method::POST, "/sims", Some(json!({ "iccid": "8921300000000000001", "line_number": "0550000001", "operator": "Djezzy" })))
        .await;
    let sim = sim["id"].as_str().unwrap().to_string();
    let (status, _) = app.call(&it, Method::PATCH, &format!("/sims/{sim}"), Some(json!({ "iccid": "8921 3000 0000 0000 002" }))).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(app.count("SELECT count(*) FROM sim_cards WHERE iccid = '8921300000000000002'").await, 1);
    let line: Uuid = sqlx::query_scalar("SELECT id FROM mobile_lines WHERE number = '0550000001'").fetch_one(app.pool()).await.unwrap();
    let (status, _) = app.call(&it, Method::PATCH, &format!("/lines/{line}"), Some(json!({ "operator": "Mobilis" }))).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(app.count("SELECT count(*) FROM mobile_lines WHERE operator = 'Mobilis'").await, 1);
    let (status, _) = app.call(&rh, Method::PATCH, &format!("/sims/{sim}"), Some(json!({ "iccid": "1" }))).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "le RH ne modifie pas le parc IT");

    // Parc : filtre « À compléter ».
    let (status, a) = app
        .call(&it, Method::POST, "/assets", Some(json!({ "inventory_no": "PCP-0001", "kind": "pc_portable", "label": "Portable" })))
        .await;
    assert_eq!(status, StatusCode::OK, "{a}");
    let (_, list) = app.call(&it, Method::GET, "/assets?incomplet=true", None).await;
    assert_eq!(list.as_array().unwrap().len(), 1);
    assert!(list[0]["missing"].to_string().contains("n° de série"));

    // ADMIN : site renommé puis fermé (jamais supprimé).
    let (status, _) = app.call(&admin, Method::PATCH, &format!("/admin/sites/{oran}"), Some(json!({ "name": "Oran — Usine", "active": false }))).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(app.count("SELECT count(*) FROM sites WHERE name = 'Oran — Usine' AND NOT active").await, 1);
    let (status, _) = app.call(&rh, Method::PATCH, &format!("/admin/sites/{oran}"), Some(json!({ "active": true }))).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    // Un site fermé n'est plus proposé pour une correction.
    let (_, e) = app.call(&rh, Method::GET, &format!("/employees/{emp}"), None).await;
    let (status, _) = app
        .call(&rh, Method::PATCH, &format!("/employees/{emp}"), Some(json!({ "version": e["employee"]["version"], "site_id": oran })))
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
}

#[sqlx::test(migrator = "server::MIGRATOR")]
async fn mutation_avec_wilayas_cibles(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let rh = app.user("Rh Fictif", &["RH"]).await;
    let site = app.site("CHERAGA").await;
    let (_, w1) = app.call(&rh, Method::POST, "/wilayas", Some(json!({ "code": 16, "name": "Alger" }))).await;
    let (_, w2) = app.call(&rh, Method::POST, "/wilayas", Some(json!({ "code": 31, "name": "Oran" }))).await;
    let (w1, w2) = (w1["id"].as_str().unwrap().to_string(), w2["id"].as_str().unwrap().to_string());
    let onb = app.announce(&rh, site, "Nora", "Fictive", json!({})).await;
    let emp = onb["employee_id"].as_str().unwrap().to_string();
    let (status, _) = app.call(&rh, Method::POST, &format!("/employees/{emp}/arrival"), Some(json!({ "actual_arrival": "2026-11-01" }))).await;
    assert_eq!(status, StatusCode::OK);
    let (_, e) = app.call(&rh, Method::GET, &format!("/employees/{emp}"), None).await;
    let version = e["employee"]["version"].as_i64().unwrap();
    let (status, _) = app.call(&rh, Method::PATCH, &format!("/employees/{emp}"), Some(json!({ "version": version, "wilaya_ids": [w1] }))).await;
    assert_eq!(status, StatusCode::OK);

    let (status, _) = app
        .call(&rh, Method::POST, "/transfers", Some(json!({ "employee_id": emp, "job_title": "Superviseur", "effective_date": "2026-11-10", "wilaya_ids": [Uuid::new_v4()] })))
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "wilaya inconnue");
    let (status, m) = app
        .call(&rh, Method::POST, "/transfers", Some(json!({ "employee_id": emp, "job_title": "Superviseur", "effective_date": "2026-11-10", "wilaya_ids": [w2] })))
        .await;
    assert_eq!(status, StatusCode::CREATED, "{m}");
    let reference = m["reference"].as_str().unwrap().to_string();
    let (_, c) = app.call(&rh, Method::GET, &format!("/cases/{reference}"), None).await;
    assert_eq!(c["transfer"]["target"]["wilayas"], json!(["31 Oran"]), "{}", c["transfer"]);
    // Rien ne change avant la date d'effet ; application par le worker.
    assert_eq!(app.count(&format!("SELECT count(*) FROM employee_wilayas WHERE employee_id = '{emp}' AND wilaya_id = '{w1}'")).await, 1);
    sqlx::query("UPDATE transfers SET status = 'confirmee'").execute(app.pool()).await.unwrap();
    let applied = server::routes::transfers::apply_due(app.pool(), chrono::NaiveDate::from_ymd_opt(2026, 11, 10).unwrap()).await.unwrap();
    assert_eq!(applied, 1);
    assert_eq!(app.count(&format!("SELECT count(*) FROM employee_wilayas WHERE employee_id = '{emp}'")).await, 1);
    assert_eq!(app.count(&format!("SELECT count(*) FROM employee_wilayas WHERE employee_id = '{emp}' AND wilaya_id = '{w2}'")).await, 1);
}

/// Sauvegarde réelle (gpg et pg_dump) : jouée si les outils sont présents
/// (`GPG_BIN`, `PG_DUMP_BIN`), sinon signalée et ignorée.
#[sqlx::test(migrator = "server::MIGRATOR")]
async fn sauvegarde_chiffree_depuis_l_interface(pool: PgPool) {
    let gpg = std::env::var("GPG_BIN").unwrap_or_else(|_| "gpg".into());
    let pg_dump = std::env::var("PG_DUMP_BIN").unwrap_or_else(|_| "pg_dump".into());
    let ok = |b: &str| std::process::Command::new(b).arg("--version").output().map(|o| o.status.success()).unwrap_or(false);
    if !ok(&gpg) || !ok(&pg_dump) {
        eprintln!("sauvegarde : gpg ou pg_dump absent, essai ignoré");
        return;
    }
    let db: String = sqlx::query_scalar("SELECT current_database()").fetch_one(&pool).await.unwrap();
    let base_url = std::env::var("DATABASE_URL").unwrap();
    let url = format!("{}/{db}", &base_url[..base_url.rfind('/').unwrap()]);
    let app = TestApp::with_config(pool, |c| {
        c.gpg_bin = gpg.clone();
        c.pg_dump_bin = pg_dump.clone();
        c.database_url = url.clone();
    })
    .await;
    let admin = app.user("Admin Fictif", &["ADMIN"]).await;

    // Paire de clés de test (clé privée gardée par le test, hors du serveur).
    let home = std::env::temp_dir().join(format!("cellulo-gpg-{}", Uuid::new_v4().simple()));
    std::fs::create_dir_all(&home).unwrap();
    // gpg de Git pour Windows (MSYS) : l'agent exige un chemin « /c/… ».
    let home_arg = {
        let h = home.to_string_lossy().replace('\\', "/");
        match (cfg!(windows), h.chars().nth(1)) {
            (true, Some(':')) => format!("/{}{}", h[..1].to_lowercase(), &h[2..]),
            _ => h,
        }
    };
    // Arrête l'agent de la clé de test, même en cas d'échec.
    struct Agent(String, String);
    impl Drop for Agent {
        fn drop(&mut self) {
            let _ = std::process::Command::new(&self.0).arg("--homedir").arg(&self.1).args(["--kill", "all"]).output();
        }
    }
    let _agent = Agent(gpg.replace("gpg.exe", "gpgconf.exe").replace("/gpg", "/gpgconf"), home_arg.clone());
    let run = |args: &[&str]| {
        let out = std::process::Command::new(&gpg).arg("--homedir").arg(&home_arg).args(["--batch", "--pinentry-mode", "loopback", "--passphrase", ""]).args(args).output().unwrap();
        assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
        out.stdout
    };
    run(&["--quick-generate-key", "Sauvegarde fictive <sauvegarde@exemple.test>", "rsa2048", "encr", "never"]);
    let public = String::from_utf8(run(&["--armor", "--export", "sauvegarde@exemple.test"])).unwrap();
    let secret = String::from_utf8(run(&["--armor", "--export-secret-keys", "sauvegarde@exemple.test"])).unwrap();

    // Refus d'une clé privée, puis configuration.
    let (status, v) = app.call(&admin, Method::PUT, "/admin/config/backup", Some(json!({ "public_key": secret, "hour": 2, "keep": 3 }))).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{v}");
    let (status, v) = app.call(&admin, Method::POST, "/admin/backups", Some(json!({}))).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{v}");
    let (status, v) = app.call(&admin, Method::PUT, "/admin/config/backup", Some(json!({ "public_key": public, "hour": 23, "keep": 3 }))).await;
    assert_eq!(status, StatusCode::OK, "{v}");
    let (_, cfg) = app.call(&admin, Method::GET, "/admin/config", None).await;
    assert_eq!(cfg["backup"]["configured"], true);
    assert!(cfg["saved"]["backup"]["fingerprint"].as_str().unwrap().len() >= 40);

    // Un document pour vérifier l'archive des documents.
    std::fs::create_dir_all(app.state.cfg().data_dir.join("dossiers/ONB-00001")).unwrap();
    std::fs::write(app.state.cfg().data_dir.join("dossiers/ONB-00001/decharge.pdf"), b"%PDF-1.4 fictif").unwrap();

    let (status, r) = app.call(&admin, Method::POST, "/admin/backups", Some(json!({}))).await;
    assert_eq!(status, StatusCode::OK, "{r}");
    let (status, _) = app.call(&admin, Method::POST, "/admin/backups", Some(json!({}))).await;
    assert_eq!(status, StatusCode::CONFLICT, "une seule à la fois");
    server::backup::tick(&app.state).await.unwrap();
    let mut status = String::new();
    for _ in 0..120 {
        status = sqlx::query_scalar("SELECT status FROM backup_runs ORDER BY requested_at DESC LIMIT 1").fetch_one(app.pool()).await.unwrap();
        if status != "en_cours" && status != "demande" {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    }
    let (_, runs) = app.call(&admin, Method::GET, "/admin/backups", None).await;
    assert_eq!(status, "reussie", "{runs}");
    let files = runs[0]["files"].as_array().unwrap().clone();
    assert_eq!(files.len(), 4);
    let id = runs[0]["id"].as_str().unwrap();
    let dump = files.iter().find(|f| f["name"].as_str().unwrap().starts_with("cellulo-")).unwrap()["name"].as_str().unwrap().to_string();

    // Téléchargement (chiffré), puis déchiffrement avec la seule clé privée.
    let req = axum::http::Request::builder()
        .uri(format!("/api/v1/admin/backups/{id}/files/{dump}"))
        .header("cookie", &admin.cookie)
        .body(axum::body::Body::empty())
        .unwrap();
    let (code, bytes) = app.raw(req).await;
    assert_eq!(code, StatusCode::OK);
    assert!(!bytes.starts_with(b"PGDMP"), "jamais en clair");
    let enc = home.join("dump.gpg");
    std::fs::write(&enc, &bytes).unwrap();
    let plain = run(&["--decrypt", enc.to_str().unwrap()]);
    assert!(plain.starts_with(b"PGDMP"), "dump PostgreSQL au format custom");
    let docs = files.iter().find(|f| f["name"].as_str().unwrap().starts_with("documents-")).unwrap()["name"].as_str().unwrap().to_string();
    let enc_docs = app.state.cfg().backup_dir.join(&docs);
    let tgz = run(&["--decrypt", enc_docs.to_str().unwrap()]);
    let mut archive = tar::Archive::new(flate2::read::GzDecoder::new(&tgz[..]));
    let names: Vec<String> = archive.entries().unwrap().map(|e| e.unwrap().path().unwrap().to_string_lossy().replace('\\', "/")).collect();
    assert!(names.iter().any(|n| n.ends_with("dossiers/ONB-00001/decharge.pdf")), "{names:?}");
    assert!(!names.iter().any(|n| n.contains("_sauvegardes")), "les sauvegardes ne s'incluent pas elles-mêmes : {names:?}");
    // Nom de fichier hors liste refusé.
    let (code, _) = app.call(&admin, Method::GET, &format!("/admin/backups/{id}/files/..%2F.env"), None).await;
    assert_eq!(code, StatusCode::NOT_FOUND);
    let _ = std::fs::remove_dir_all(&home);
}
