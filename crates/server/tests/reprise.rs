//! Reprise des listes SharePoint (T08) : aperçu sans écriture, décisions
//! nominatives, liens préservés, aucun parcours ni avis, rejeu refusé.
//! Données fictives uniquement ; nécessite DATABASE_URL.

mod common;

use axum::http::{Method, StatusCode};
use common::{Session, TestApp};
use serde_json::{json, Value};
use sqlx::PgPool;

fn row<'a>(v: &'a Value, n: i64) -> &'a Value {
    v["rows"].as_array().unwrap().iter().find(|r| r["row_number"] == n).unwrap_or_else(|| panic!("ligne {n} absente : {v}"))
}

async fn preview(app: &TestApp, s: &Session, list: &str, csv: &str) -> (String, Value) {
    let (status, v) = app.upload(s, &format!("/reprise/{list}/preview"), &format!("{list}.csv"), csv.as_bytes()).await;
    assert_eq!(status, StatusCode::OK, "{v}");
    let id = v["id"].as_str().unwrap().to_string();
    let (status, detail) = app.call(s, Method::GET, &format!("/reprise/batches/{id}"), None).await;
    assert_eq!(status, StatusCode::OK);
    (id, detail)
}

async fn commit(app: &TestApp, s: &Session, id: &str) -> Value {
    let (status, v) = app.call(s, Method::POST, &format!("/reprise/batches/{id}/commit"), None).await;
    assert_eq!(status, StatusCode::OK, "{v}");
    let (_, detail) = app.call(s, Method::GET, &format!("/reprise/batches/{id}"), None).await;
    detail
}

#[sqlx::test(migrator = "server::MIGRATOR")]
async fn reprise_complete_des_listes(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let admin = app.user("Admin Reprise", &["ADMIN"]).await;
    let rh = app.user("Rh Reprise", &["RH"]).await;
    let site = app.site("Cheraga").await;

    // Le GRAPHISTE ou le RH n'accèdent pas à la reprise (ADMIN seul).
    let (status, _) = app.upload(&rh, "/reprise/Wilayas/preview", "w.csv", b"CodeWilaya;Wilaya\n16;Alger\n").await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    // Wilayas.
    let (id, _) = preview(&app, &admin, "Wilayas", "CodeWilaya;Wilaya;Region\n16;Alger;Centre\n31;Oran;Ouest\n").await;
    let done = commit(&app, &admin, &id).await;
    assert_eq!(row(&done, 2)["result"], "cree");
    assert_eq!(app.count("SELECT count(*) FROM wilayas").await, 2);

    // Une homonyme déjà connue, sans matricule commun.
    let (status, _) = app
        .call(&rh, Method::POST, "/employees", Some(json!({
            "first_name": "Sara", "last_name": "Haddad", "job_title": "Assistante", "site_id": site,
            "arrival_date": "2021-01-01", "already_present": true
        })))
        .await;
    assert_eq!(status, StatusCode::OK);

    let personnel = "ID;NomComplet;Prenom;NomFamille;Matricule;Statut;DateEntree;Site;Fonction;Departement;Manager;TypeContrat;Wilayas;EmailPro;PlateformeMessagerie\n\
1;Amine Benali;Amine;Benali;00042;Actif;01/02/2020;Cheraga;Directeur commercial;Commercial;;CDI;Alger;a.benali@exemple.test;cPanel\n\
2;Sara Haddad;Sara;Haddad;;Actif;15/03/2022;Cheraga;Commerciale;Commercial;Amine Benali;CDD;16;;\n\
3;Karim Zerrouki;Karim;Zerrouki;;Sorti;01/01/2019;Cheraga;Technicien;IT;;;;;\n\
4;Nadia Bouzid;Nadia;Bouzid;;Actif;;Oran;Poste;;;;;;\n";
    let (id, p) = preview(&app, &admin, "Personnel", personnel).await;
    assert_eq!(row(&p, 2)["proposal"], "creation");
    assert_eq!(row(&p, 3)["proposal"], "a_revoir", "homonyme : jamais rapproché sur le nom");
    assert_eq!(row(&p, 5)["proposal"], "rejet", "site inconnu");
    // Aperçu sans écriture métier.
    assert_eq!(app.count("SELECT count(*) FROM employees").await, 1);

    // Décision nominative : nouvelle personne (pas l'homonyme).
    let (status, v) = app
        .call(&admin, Method::POST, &format!("/reprise/batches/{id}/rows/3/decision"), Some(json!({ "decision": "appliquer" })))
        .await;
    assert_eq!(status, StatusCode::OK, "{v}");
    let done = commit(&app, &admin, &id).await;
    assert_eq!(row(&done, 2)["result"], "cree");
    assert_eq!(row(&done, 3)["result"], "cree");
    assert_eq!(row(&done, 4)["result"], "cree");
    assert_eq!(row(&done, 5)["result"], "rejete");
    assert_eq!(app.count("SELECT count(*) FROM employees").await, 4);
    // Zéros initiaux, contrat, wilaya, adresse et manager repris.
    let (mat, contract): (Option<String>, Option<String>) =
        sqlx::query_as("SELECT matricule, contract_type FROM employees WHERE last_name = 'Benali'").fetch_one(app.pool()).await.unwrap();
    assert_eq!(mat.as_deref(), Some("00042"));
    assert_eq!(contract.as_deref(), Some("cdi"));
    assert_eq!(app.count("SELECT count(*) FROM employee_accounts WHERE platform = 'cpanel' AND identifier = 'a.benali@exemple.test'").await, 1);
    assert_eq!(
        app.count(
            "SELECT count(*) FROM employee_positions p JOIN employees e ON e.id = p.employee_id
              JOIN employees m ON m.id = p.manager_employee_id
              WHERE e.last_name = 'Haddad' AND e.contract_type = 'cdd' AND m.last_name = 'Benali'"
        )
        .await,
        1,
        "le manager est relié à la nouvelle fiche, pas à l'homonyme"
    );
    assert_eq!(app.count("SELECT count(*) FROM employee_wilayas").await, 2);
    assert_eq!(app.count("SELECT count(*) FROM employees WHERE last_name = 'Zerrouki' AND status = 'sorti'").await, 1);
    // Aucun parcours, aucune tâche, aucun avis.
    assert_eq!(app.count("SELECT count(*) FROM cases").await, 0);
    assert_eq!(app.count("SELECT count(*) FROM tasks").await, 0);
    assert_eq!(app.count("SELECT count(*) FROM outbox").await, 0);

    // Rejeu du même fichier : signalé à l'aperçu, refusé à l'application.
    let (status, v) = app.upload(&admin, "/reprise/Personnel/preview", "Personnel.csv", personnel.as_bytes()).await;
    assert_eq!(status, StatusCode::OK);
    assert!(!v["already_applied_at"].is_null());
    let replay = v["id"].as_str().unwrap().to_string();
    let (status, _) = app.call(&admin, Method::POST, &format!("/reprise/batches/{replay}/commit"), None).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(app.count("SELECT count(*) FROM employees").await, 4);

    // Matériel : garde reprise depuis « AttribueA », matériel partagé en service.
    let materiel = "ID;NumInventaire;Categorie;Marque;Modele;NumSerie;Statut;AttribueA;DateAchat\n\
10;PC-001;PC portable;Dell;Latitude 5440;SN1;Attribué;Amine Benali;15/03/2024\n\
11;PC-002;Smartphone;Samsung;A54;SN2;En stock;;\n\
12;SRV-01;Serveur;HP;ML30;;En service;;\n\
13;GP-01;Grille-pain;;;;En stock;;\n";
    let (id, p) = preview(&app, &admin, "MaterielIT", materiel).await;
    assert_eq!(row(&p, 5)["proposal"], "rejet", "catégorie inconnue jamais devinée");
    commit(&app, &admin, &id).await;
    assert_eq!(app.count("SELECT count(*) FROM assets WHERE inventory_no = 'PC-001' AND status = 'affecte' AND brand = 'Dell'").await, 1);
    assert_eq!(app.count("SELECT count(*) FROM assets WHERE inventory_no = 'SRV-01' AND status = 'en_service'").await, 1);
    assert_eq!(app.count("SELECT count(*) FROM movements WHERE kind = 'reprise'").await, 1);
    assert_eq!(app.count("SELECT count(*) FROM documents").await, 0, "aucune décharge fabriquée pour le passé");

    // Lignes : SIM créée avec l'ICCID, garde reprise.
    let lignes = "Numero;Operateur;ICCID;Forfait;TypeLigne;Statut;AttribueA\n0550000001;Mobilis;8921300000000000001;Pro 2000;Voix+Data;Attribuée;Sara Haddad\n";
    let (id, _) = preview(&app, &admin, "LignesMobiles", lignes).await;
    commit(&app, &admin, &id).await;
    assert_eq!(
        app.count(
            "SELECT count(*) FROM assignments a JOIN sim_cards s ON s.id = a.sim_id JOIN employees e ON e.id = a.employee_id
              WHERE a.ended_at IS NULL AND e.contract_type = 'cdd' AND s.status = 'affecte'"
        )
        .await,
        1
    );

    // Affectations : déjà détenue (inchangée), historique, contradiction.
    let affectations = "ID;TypeObjet;Materiel;Ligne;Employe;DateAttribution;DateRestitution\n\
100;Matériel;PC-001;;Amine Benali;01/04/2024;\n\
101;Matériel;PC-002;;Amine Benali;01/01/2023;01/06/2023\n\
102;Matériel;PC-001;;Karim Zerrouki;01/01/2024;\n";
    let (id, p) = preview(&app, &admin, "Affectations", affectations).await;
    assert_eq!(row(&p, 2)["proposal"], "inchange");
    assert_eq!(row(&p, 3)["proposal"], "creation");
    assert_eq!(row(&p, 4)["proposal"], "rejet", "affectation active contradictoire");
    commit(&app, &admin, &id).await;
    assert_eq!(app.count("SELECT count(*) FROM assets WHERE inventory_no = 'PC-002' AND status = 'disponible'").await, 1);
    assert_eq!(app.count("SELECT count(*) FROM assignments WHERE ended_at IS NOT NULL").await, 1);

    // La garde reprise se restitue normalement, ligne d'origine à l'appui.
    let (emp, line): (uuid::Uuid, uuid::Uuid) = sqlx::query_as(
        "SELECT a.employee_id, a.handover_line_id FROM assignments a JOIN assets x ON x.id = a.asset_id
          WHERE x.inventory_no = 'PC-001' AND a.ended_at IS NULL",
    )
    .fetch_one(app.pool())
    .await
    .unwrap();
    let (status, v) = app
        .call(&admin, Method::POST, "/returns", Some(json!({ "employee_id": emp, "lines": [{ "origin_line_id": line }] })))
        .await;
    assert_eq!(status, StatusCode::CREATED, "{v}");
    assert_eq!(app.count("SELECT count(*) FROM assets WHERE inventory_no = 'PC-001' AND status = 'controle'").await, 1);
}

#[sqlx::test(migrator = "server::MIGRATOR")]
async fn detenteur_rapproche_par_le_nom_sur_decision(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let admin = app.user("Admin", &["ADMIN"]).await;
    let rh = app.user("Rh", &["RH"]).await;
    let site = app.site("ORAN").await;
    let emp = app.announce(&rh, site, "Amel", "Terrain", json!({})).await["employee_id"].as_str().unwrap().to_string();
    let (id, p) = preview(&app, &admin, "MaterielIT", "NumInventaire;Categorie;AttribueA
PC-902;PC fixe;Amel Terrain
").await;
    assert_eq!(row(&p, 2)["proposal"], "a_revoir", "jamais de rapprochement automatique par le nom");
    let (status, v) = app
        .call(&admin, Method::POST, &format!("/reprise/batches/{id}/rows/2/decision"), Some(json!({ "decision": "appliquer", "target_id": emp })))
        .await;
    assert_eq!(status, StatusCode::OK, "{v}");
    let done = commit(&app, &admin, &id).await;
    assert_eq!(row(&done, 2)["result"], "cree", "{done}");
    assert_eq!(
        app.count("SELECT count(*) FROM assignments g JOIN assets a ON a.id = g.asset_id WHERE a.inventory_no = 'PC-902' AND g.ended_at IS NULL").await,
        1
    );
}

#[sqlx::test(migrator = "server::MIGRATOR")]
async fn colonnes_obligatoires_et_formules(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let admin = app.user("Admin", &["ADMIN"]).await;
    let (status, v) = app.upload(&admin, "/reprise/MaterielIT/preview", "m.csv", b"Marque;Modele\nDell;X\n").await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{v}");
    assert!(v["message"].as_str().unwrap().contains("NumInventaire"));
    let (id, p) = preview(&app, &admin, "MaterielIT", "NumInventaire;Categorie\n=1+1;PC fixe\nPC-9;PC fixe\nPC-9;PC fixe\n").await;
    assert_eq!(row(&p, 2)["proposal"], "rejet");
    assert_eq!(row(&p, 3)["proposal"], "creation");
    assert_eq!(row(&p, 4)["proposal"], "rejet", "doublon dans le fichier");
    let done = commit(&app, &admin, &id).await;
    assert_eq!(row(&done, 3)["result"], "cree");
    assert_eq!(app.count("SELECT count(*) FROM assets").await, 1);
}
