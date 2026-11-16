//! Mode pilote (16/11/2026) : seules les fonctions utilisables sont ouvertes,
//! aucune tâche impossible, aucun ancien avis de test expédié, anciennes
//! preuves identifiables sans PDF régénéré, adresse reprise jamais réputée
//! vérifiée. Données fictives.

mod common;

use axum::http::{Method, StatusCode};
use common::{Session, TestApp};
use serde_json::{json, Value};
use server::config::{MailMode, Module};
use sqlx::PgPool;
use uuid::Uuid;

fn pilote(c: &mut server::config::Config) {
    c.disabled_modules = Module::ALL.to_vec();
}

async fn reprise(app: &TestApp, s: &Session, list: &str, csv: &str) {
    let (status, v) = app.upload(s, &format!("/reprise/{list}/preview"), &format!("{list}.csv"), csv.as_bytes()).await;
    assert_eq!(status, StatusCode::OK, "{v}");
    let id = v["id"].as_str().unwrap().to_string();
    let (status, done) = app.call(s, Method::POST, &format!("/reprise/batches/{id}/commit"), None).await;
    assert_eq!(status, StatusCode::OK, "{done}");
}

#[sqlx::test(migrator = "server::MIGRATOR")]
async fn modules_fermes_routes_taches_et_traitements(pool: PgPool) {
    // Même base : une instance complète (pour créer une échéance véhicule
    // ancienne) et l'instance pilote.
    let complet = TestApp::new(pool.clone()).await;
    let app = TestApp::with_config(pool, pilote).await;
    let rh = app.user("Rh", &["RH"]).await;
    let admin = app.user("Admin", &["ADMIN"]).await;
    let it = app.user("Tech", &["IT"]).await;
    app.user("Graphiste", &["GRAPHISTE"]).await;
    let site = app.site("CHERAGA").await;

    let (status, h) = app.call(&rh, Method::GET, "/health", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(h["modules_desactives"].as_array().unwrap().len(), 6, "{h}");

    // Routes des modules fermés : refusées quel que soit le rôle, avec un message clair.
    let emp = app.announce(&rh, site, "Lina", "Saïdi", json!({})).await;
    let emp_id = emp["employee_id"].as_str().unwrap().to_string();
    for (s, method, uri) in [
        (&rh, Method::GET, "/vehicles".to_string()),
        (&admin, Method::GET, format!("/employees/{emp_id}/vehicles")),
        (&it, Method::GET, "/software".to_string()),
        (&it, Method::GET, format!("/employees/{emp_id}/licenses")),
        (&it, Method::GET, "/inventories".to_string()),
        (&it, Method::GET, "/assets/labels.pdf?ids=x".to_string()),
        (&rh, Method::POST, "/transfers".to_string()),
        (&rh, Method::POST, format!("/employees/{emp_id}/positions")),
        (&rh, Method::POST, "/departures".to_string()),
        (&it, Method::POST, format!("/exit-controls/{}/confirm", Uuid::new_v4())),
        (&admin, Method::GET, "/reports/export.xlsx".to_string()),
    ] {
        let body = (method == Method::POST).then(|| json!({}));
        let (status, v) = app.call(s, method.clone(), &uri, body).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{uri} : {v}");
        assert_eq!(v["error"], "module_desactive", "{uri} : {v}");
    }
    // Le cœur reste ouvert : accueil RH (signatures), parc, scan d'un bien.
    let (status, _) = app.call(&rh, Method::GET, "/reports/overview", None).await;
    assert_eq!(status, StatusCode::OK);
    let (status, _) = app.call(&it, Method::GET, "/assets", None).await;
    assert_eq!(status, StatusCode::OK);
    let (_, v) = app.call(&it, Method::GET, "/assets/lookup/INCONNU-1", None).await;
    assert_ne!(v["error"], "module_desactive", "le scan d'un bien reste disponible");

    // Profil prévoyant un véhicule : aucune tâche RH impossible.
    let (status, p) = app
        .call(&rh, Method::POST, "/job-profiles", Some(json!({
            "name": "Commercial itinérant (fictif)", "needs": [{ "category": "pc" }, { "category": "vehicule" }], "trainings": []
        })))
        .await;
    assert_eq!(status, StatusCode::OK, "{p}");
    let refused = app
        .call_with(&rh, Method::POST, "/arrivals", Some(json!({
            "first_name": "Amel", "last_name": "Terrain", "job_title": "Commerciale", "site_id": site, "planned_arrival": "2026-12-01",
            "needs": [{ "category": "vehicule", "answer": "oui", "origin": "profil" }]
        })), &[("idempotency-key", &Uuid::new_v4().to_string())])
        .await;
    assert_eq!(refused.0, StatusCode::UNPROCESSABLE_ENTITY, "{}", refused.1);
    assert!(refused.1["message"].as_str().unwrap().contains("pilote"));
    // L'interface n'envoie pas le véhicule du profil : arrivée créée, PC seul.
    let onb = app
        .announce(&rh, site, "Amel", "Terrain", json!({
            "job_profile_id": p["id"], "needs": [{ "category": "pc", "answer": "oui", "origin": "profil" }]
        }))
        .await;
    let case_id = onb["case_id"].as_str().unwrap().to_string();
    assert_eq!(app.count(&format!("SELECT count(*) FROM tasks WHERE case_id = '{case_id}' AND gen_key = 'need:vehicule'")).await, 0);
    assert_eq!(app.count(&format!("SELECT count(*) FROM tasks WHERE case_id = '{case_id}' AND role = 'RH'")).await, 0);
    // Révision et demande ponctuelle : véhicule refusé aussi.
    let (_, c) = app.call(&rh, Method::GET, &format!("/cases/{}", onb["reference"].as_str().unwrap()), None).await;
    let (status, _) = app
        .call(&rh, Method::POST, &format!("/cases/{case_id}/needs"), Some(json!({
            "version": c["case"]["version"], "needs": [{ "category": "vehicule", "answer": "oui" }]
        })))
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    let (status, _) = app
        .call(&rh, Method::POST, &format!("/employees/{emp_id}/requests"), Some(json!({ "category": "vehicule" })))
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    let (status, v) = app
        .call(&rh, Method::POST, &format!("/employees/{emp_id}/requests"), Some(json!({ "category": "carte_visite" })))
        .await;
    assert!(status.is_success(), "{v}");

    // Traitements automatiques : aucune alerte véhicule émise par le pilote.
    let (_, v) = complet.call(&rh, Method::POST, "/vehicles", Some(json!({ "plate": "AB-123-CD", "kind": "propriete" }))).await;
    let soon = (chrono::Utc::now().date_naive() + chrono::Duration::days(5)).to_string();
    let (status, _) = complet
        .call(&rh, Method::POST, &format!("/vehicles/{}/documents", v["id"].as_str().unwrap()), Some(json!({ "kind": "assurance", "reference": "POL-1", "valid_until": soon })))
        .await;
    assert_eq!(status, StatusCode::OK);
    server::worker::check_deadlines(&app.state).await.unwrap();
    server::worker::process_transitions(&app.state).await.unwrap();
    assert_eq!(app.count("SELECT count(*) FROM outbox WHERE event_type LIKE 'echeance%'").await, 0);
    // Contre-épreuve : la même échéance serait signalée module ouvert.
    server::worker::check_deadlines(&complet.state).await.unwrap();
    assert_eq!(app.count("SELECT count(*) FROM outbox WHERE event_type = 'echeance_vehicule'").await, 1);
}

#[sqlx::test(migrator = "server::MIGRATOR")]
async fn anciens_avis_de_test_jamais_envoyes(pool: PgPool) {
    let journal = TestApp::with_config(pool.clone(), pilote).await;
    let rh = journal.user("Rh", &["RH"]).await;
    journal.user("Tech", &["IT"]).await;
    journal.user("Formatrice", &["FORMATION"]).await;
    let site = journal.site("CHERAGA").await;
    // Période de test en mode journal : un avis consigné, un autre créé après
    // le dernier passage du worker (resté en attente).
    journal.announce(&rh, site, "Test", "Un", json!({})).await;
    server::worker::tick(&journal.state).await.unwrap();
    journal.announce(&rh, site, "Test", "Deux", json!({})).await;
    assert_eq!(journal.count("SELECT count(*) FROM outbox WHERE status = 'en_attente'").await, 1);

    // Activation de l'envoi : l'avis de test en attente ne part pas.
    let graph = TestApp::with_config(pool, |c| {
        pilote(c);
        c.mail_mode = MailMode::Graph;
    })
    .await;
    server::worker::tick(&graph.state).await.unwrap();
    assert_eq!(graph.count("SELECT count(*) FROM deliveries WHERE status IN ('accepte', 'echec', 'a_verifier')").await, 0);
    assert!(
        graph.count("SELECT count(*) FROM deliveries WHERE last_result LIKE 'avis antérieur à l''activation%'").await >= 1,
        "avis de test consignés, jamais envoyés"
    );
    // Un avis créé après l'activation est bien traité par l'envoi (ici sans
    // identité configurée : échec visible, preuve qu'il n'est pas filtré).
    graph.announce(&rh, site, "Pilote", "Trois", json!({})).await;
    server::worker::tick(&graph.state).await.unwrap();
    assert!(graph.count("SELECT count(*) FROM deliveries WHERE status = 'echec'").await >= 1);
    // Retour en mode journal puis nouvelle activation : nouveau repère.
    server::worker::tick(&journal.state).await.unwrap();
    assert_eq!(journal.count("SELECT count(*) FROM app_settings WHERE key = 'envoi_depuis'").await, 0);
}

#[sqlx::test(migrator = "server::MIGRATOR")]
async fn preuves_anciennes_restitution_et_adresse_reprise(pool: PgPool) {
    let app = TestApp::with_config(pool, pilote).await;
    let admin = app.user("Admin", &["ADMIN"]).await;
    let rh = app.user("Rh", &["RH"]).await;
    let it = app.user("Tech", &["IT"]).await;
    let graphiste = app.user("Graphiste", &["GRAPHISTE"]).await;
    app.site("Cheraga").await;
    reprise(&app, &admin, "Personnel", "ID;NomComplet;Prenom;NomFamille;Matricule;Statut;DateEntree;Site;Fonction;EmailPro;PlateformeMessagerie;M365Operationnel\n\
502;Nadia Cherif;Nadia;Cherif;00123;Actif;02/05/2022;Cheraga;Déléguée;n.cherif@exemple.test;cPanel;Oui\n").await;
    reprise(&app, &admin, "MaterielIT", "NumInventaire;Categorie;Statut\nPC-1;PC portable;En stock\nPC-2;PC portable;En stock\nPC-3;PC portable;En stock\n").await;
    reprise(&app, &admin, "Affectations", "ID;Title;TypeObjet;Materiel;Employe;DateAttribution;Attachments;LienJustificatif\n\
701;AFF-1;Matériel;PC-1;Nadia Cherif;03/05/2022;Oui;\n\
702;AFF-2;Matériel;PC-2;Nadia Cherif;03/05/2022;;https://exemple-fictif.sharepoint.com/sites/rh/Documents/AFF-2.pdf\n\
703;AFF-3;Matériel;PC-3;Nadia Cherif;03/05/2022;;http://exemple.test/faux.pdf\n").await;
    let emp: Uuid = sqlx::query_scalar("SELECT id FROM employees WHERE matricule = '00123'").fetch_one(app.pool()).await.unwrap();

    // Adresse reprise : ni date de vérification inventée, ni M365 activé.
    let (_, accounts) = app.call(&it, Method::GET, &format!("/employees/{emp}/accounts"), None).await;
    assert_eq!(accounts[0]["imported"], true, "{accounts}");
    assert!(accounts[0]["verified_at"].is_null(), "{accounts}");
    let (m365, note): (Option<chrono::DateTime<chrono::Utc>>, Option<String>) =
        sqlx::query_as("SELECT m365_verified_at, import_note FROM employees WHERE id = $1").bind(emp).fetch_one(app.pool()).await.unwrap();
    assert!(m365.is_none(), "« Oui » sans date : rien n'est activé");
    assert!(note.unwrap().contains("M365"));
    // « À vérifier » ne date aucune vérification ; « Actif (vérifié) » la date.
    let account = accounts[0]["id"].as_str().unwrap().to_string();
    let (status, _) = app.call(&it, Method::PATCH, &format!("/employees/{emp}/accounts/{account}"), Some(json!({ "status": "a_verifier" }))).await;
    assert_eq!(status, StatusCode::OK);
    let (_, accounts) = app.call(&it, Method::GET, &format!("/employees/{emp}/accounts"), None).await;
    assert!(accounts[0]["verified_at"].is_null(), "{accounts}");
    let (status, _) = app.call(&it, Method::PATCH, &format!("/employees/{emp}/accounts/{account}"), Some(json!({ "status": "actif" }))).await;
    assert_eq!(status, StatusCode::OK);
    let (_, accounts) = app.call(&it, Method::GET, &format!("/employees/{emp}/accounts"), None).await;
    assert!(!accounts[0]["verified_at"].is_null(), "{accounts}");

    // Justificatifs identifiables (RH, IT) ; lien non SharePoint ignoré.
    let (_, h) = app.call(&rh, Method::GET, &format!("/employees/{emp}/holdings"), None).await;
    let by = |inv: &str| h.as_array().unwrap().iter().find(|x| x["identifier"].as_str().unwrap_or("").starts_with(inv)).unwrap().clone();
    assert_eq!(by("PC-1")["legacy_proof_ref"], "pièce jointe de l'élément n° 701 de la liste Affectations (SharePoint)");
    assert_eq!(by("PC-2")["legacy_proof_url"], "https://exemple-fictif.sharepoint.com/sites/rh/Documents/AFF-2.pdf");
    assert!(by("PC-3")["legacy_proof_url"].is_null() && by("PC-3")["legacy_proof_ref"].is_null());
    // Le Graphiste ne voit ni les biens ni ces références.
    let (_, fiche) = app.call(&graphiste, Method::GET, &format!("/employees/{emp}"), None).await;
    let text = fiche.to_string();
    assert!(!text.contains("sharepoint.com") && !text.contains("pièce jointe"), "{text}");
    let (status, _) = app.call(&graphiste, Method::GET, &format!("/employees/{emp}/holdings"), None).await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    // Restitution d'un bien repris : la restitution a sa décharge, la remise
    // ancienne n'en reçoit aucune.
    let line = by("PC-1")["origin_line_id"].as_str().unwrap().to_string();
    let (status, v) = app.call(&it, Method::POST, "/returns", Some(json!({ "employee_id": emp, "lines": [{ "origin_line_id": line }] }))).await;
    assert_eq!(status, StatusCode::CREATED, "{v}");
    assert_eq!(app.count("SELECT count(*) FROM documents d JOIN movements m ON m.id = d.movement_id WHERE m.kind = 'reprise'").await, 0);
    assert_eq!(app.count("SELECT count(*) FROM documents d JOIN movements m ON m.id = d.movement_id WHERE m.kind = 'restitution'").await, 1);
    assert_eq!(app.count("SELECT count(*) FROM assignments WHERE ended_at IS NULL").await, 2);
    let _: &Value = &v;
}
