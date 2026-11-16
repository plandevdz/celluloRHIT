//! Règles provisoires d'arbitrage (16/11/2026) : intervenant de formation
//! sans compte, auteur de l'enregistrement distinct, anciennes cases
//! « Carte de visite » et « Besoin email » conservées en texte seulement,
//! anciens profils Manager / Moyens généraux sans droit. Données fictives.

mod common;

use axum::http::{Method, StatusCode};
use common::{Session, TestApp};
use serde_json::{json, Value};
use sqlx::PgPool;
use uuid::Uuid;

async fn reprise(app: &TestApp, s: &Session, list: &str, csv: &str) -> Value {
    let (status, v) = app.upload(s, &format!("/reprise/{list}/preview"), &format!("{list}.csv"), csv.as_bytes()).await;
    assert_eq!(status, StatusCode::OK, "{v}");
    let id = v["id"].as_str().unwrap().to_string();
    let (status, done) = app.call(s, Method::POST, &format!("/reprise/batches/{id}/commit"), None).await;
    assert_eq!(status, StatusCode::OK, "{done}");
    v
}

#[sqlx::test(migrator = "server::MIGRATOR")]
async fn formation_animee_par_une_personne_sans_compte(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let rh = app.user("Rh", &["RH"]).await;
    let formation = app.user("Formatrice", &["FORMATION"]).await;
    let site = app.site("CHERAGA").await;
    // Catalogue : intervenant par défaut sans compte.
    let (status, v) = app
        .call(&formation, Method::POST, "/trainings/catalog", Some(json!({
            "name": "Sécurité (fictif)", "category": "qhse", "default_trainer_name": "Cabinet Externe (fictif)"
        })))
        .await;
    assert_eq!(status, StatusCode::OK, "{v}");
    let catalog = v["id"].as_str().unwrap().to_string();
    let (_, cat) = app.call(&rh, Method::GET, "/trainings/catalog", None).await;
    assert_eq!(cat[0]["default_trainer_name"], "Cabinet Externe (fictif)");
    assert!(cat[0]["default_trainer_id"].is_null());

    let onb = app.announce(&rh, site, "Lina", "Saïdi", json!({ "trainings": [{ "catalog_id": catalog }] })).await;
    let case_id = onb["case_id"].as_str().unwrap().to_string();
    let session: Uuid = sqlx::query_scalar("SELECT id FROM training_sessions WHERE case_id = $1::uuid").bind(&case_id).fetch_one(app.pool()).await.unwrap();
    let name: Option<String> = sqlx::query_scalar("SELECT trainer_name FROM training_sessions WHERE id = $1").bind(session).fetch_one(app.pool()).await.unwrap();
    assert_eq!(name.as_deref(), Some("Cabinet Externe (fictif)"), "intervenant par défaut repris dans la session");

    // La Formation enregistre : animée par une autre personne, sans compte.
    let (status, v) = app
        .call(&formation, Method::POST, &format!("/trainings/sessions/{session}"), Some(json!({
            "action": "realiser", "trainer_name": "M. Hamidi (intervenant fictif)"
        })))
        .await;
    assert_eq!(status, StatusCode::OK, "{v}");
    let (trainer_id, trainer_name, recorded_by): (Option<Uuid>, Option<String>, Option<Uuid>) =
        sqlx::query_as("SELECT trainer_id, trainer_name, recorded_by FROM training_sessions WHERE id = $1").bind(session).fetch_one(app.pool()).await.unwrap();
    assert_eq!(trainer_id, None, "l'enregistreur ne devient pas l'animateur");
    assert_eq!(trainer_name.as_deref(), Some("M. Hamidi (intervenant fictif)"));
    assert_eq!(recorded_by, Some(formation.id));
    assert_eq!(app.count("SELECT count(*) FROM users").await, 2, "aucun compte créé pour l'intervenant");
    let (_, sessions) = app.call(&formation, Method::GET, "/trainings/sessions?status=done", None).await;
    assert_eq!(sessions[0]["trainer"], "M. Hamidi (intervenant fictif)");
    assert_eq!(sessions[0]["recorded_by"], "Formatrice");
    let emp = onb["employee_id"].as_str().unwrap();
    let (_, fiche) = app.call(&rh, Method::GET, &format!("/employees/{emp}"), None).await;
    assert_eq!(fiche["trainings"][0]["trainer"], "M. Hamidi (intervenant fictif)");
    assert_eq!(fiche["trainings"][0]["recorded_by"], "Formatrice");
}

#[sqlx::test(migrator = "server::MIGRATOR")]
async fn reprise_sans_preuve_ni_droit_automatique(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let admin = app.user("Admin", &["ADMIN"]).await;
    app.site("Cheraga").await;

    // Anciens profils : Manager et Moyens généraux n'ouvrent aucun accès.
    let users = "NomPrenom;Email;Equipe;Role;Actif\n\
Responsable Fictif;resp@exemple.test;Manager;Responsable;Oui\n\
Moyens Fictifs;mg@exemple.test;Moyens généraux;Utilisateur;Oui\n";
    reprise(&app, &admin, "UtilisateursApp", users).await;
    assert_eq!(app.count("SELECT count(*) FROM users WHERE email IN ('resp@exemple.test', 'mg@exemple.test')").await, 0);

    // Fiche : le manager est désigné sans obtenir de compte ; cartes et besoin email en texte.
    let personnel = "ID;NomComplet;Prenom;NomFamille;Matricule;Statut;DateEntree;Site;Fonction;Manager;CarteVisite;BesoinEmailPro;PosteFixe\n\
1;Responsable Fictif;Responsable;Fictif;00010;Actif;01/01/2018;Cheraga;Chef de zone;;;;\n\
2;Yanis Mebarki;Yanis;Mebarki;00011;Actif;01/02/2021;Cheraga;Délégué;Responsable Fictif;Oui;Oui;Fixe 2\n";
    reprise(&app, &admin, "Personnel", personnel).await;
    let (note, poste_fixe): (Option<String>, Option<String>) =
        sqlx::query_as("SELECT import_note, poste_fixe FROM employees WHERE matricule = '00011'").fetch_one(app.pool()).await.unwrap();
    let note = note.unwrap();
    assert!(note.contains("Carte de visite") && note.contains("n'atteste ni impression ni remise"), "{note}");
    assert!(note.contains("Besoin email pro"), "{note}");
    assert_eq!(poste_fixe.as_deref(), Some("Fixe 2"), "valeur conservée telle quelle");
    assert_eq!(
        app.count("SELECT count(*) FROM employee_positions p JOIN employees m ON m.id = p.manager_employee_id WHERE m.matricule = '00010' AND p.valid_to IS NULL").await,
        1
    );
    assert_eq!(app.count("SELECT count(*) FROM users").await, 1, "le manager n'a pas de compte");
    // Ni tâche, ni carte, ni dossier, ni avis.
    assert_eq!(app.count("SELECT count(*) FROM cases").await, 0);
    assert_eq!(app.count("SELECT count(*) FROM tasks").await, 0);
    assert_eq!(app.count("SELECT count(*) FROM outbox").await, 0);

    // Formation reprise : animateur sans compte conservé par son nom.
    reprise(&app, &admin, "CatalogueFormations", "Title;Categorie;FormateurNom\nAccueil (fictif);QHSE;Mme Bensalem (externe)\n").await;
    let formations = "Formation;Employe;Statut;DateRealisee;FormateurNom;FormateurEmail\n\
Accueil (fictif);Yanis Mebarki;Réalisée;10/03/2021;Mme Bensalem (externe);bensalem@exemple.test\n";
    reprise(&app, &admin, "Formations", formations).await;
    let (status, trainer_name, recorded_by): (String, Option<String>, Option<Uuid>) =
        sqlx::query_as("SELECT status, trainer_name, recorded_by FROM training_sessions").fetch_one(app.pool()).await.unwrap();
    assert_eq!(status, "realisee");
    assert_eq!(trainer_name.as_deref(), Some("Mme Bensalem (externe)"));
    assert_eq!(recorded_by, Some(admin.id), "l'auteur de la reprise est tracé à part");
    let default_name: Option<String> = sqlx::query_scalar("SELECT default_trainer_name FROM training_catalog").fetch_one(app.pool()).await.unwrap();
    assert_eq!(default_name.as_deref(), Some("Mme Bensalem (externe)"));
    assert_eq!(app.count("SELECT count(*) FROM users").await, 1);
}

#[sqlx::test(migrator = "server::MIGRATOR")]
async fn salarie_deja_present_repris_sans_doublon(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let admin = app.user("Admin", &["ADMIN"]).await;
    app.site("Cheraga").await;
    let personnel = "ID;NomComplet;Prenom;NomFamille;Matricule;Statut;DateEntree;Site;Fonction;LignePrincipale
502;Nadia Cherif;Nadia;Cherif;00123;Actif;02/05/2022;Cheraga;Déléguée;0661 00 00 02
";
    reprise(&app, &admin, "Personnel", personnel).await;
    reprise(&app, &admin, "MaterielIT", "NumInventaire;Categorie;Statut;AttribueA
PC-201;PC portable;Attribué;Nadia Cherif
").await;
    reprise(&app, &admin, "LignesMobiles", "Numero;Operateur;ICCID;Statut;AttribueA
0661000002;Mobilis;8921300000000000002;Attribuée;Nadia Cherif
").await;
    // Ligne principale rattrapée après la reprise des lignes.
    assert_eq!(
        app.count("SELECT count(*) FROM employees e JOIN mobile_lines l ON l.id = e.primary_line_id WHERE e.matricule = '00123'").await,
        1
    );
    let affectations = "ID;Title;TypeObjet;Materiel;Ligne;Employe;DateAttribution;Observations;AttribueParNom;Dossier
701;AFF-0045;Matériel;PC-201;;Nadia Cherif;03/05/2022;Avec sacoche;Karim (ancien);ONB-2022-031
702;AFF-0046;Ligne;;0661000002;Nadia Cherif;03/05/2022;;Karim (ancien);ONB-2022-031
";
    let p = reprise(&app, &admin, "Affectations", affectations).await;
    assert_eq!(p["summary"]["propositions"]["mise_a_jour"], 2, "{p}");
    // Une seule garde par bien, datée de l'attribution réelle, observation complétée.
    assert_eq!(app.count("SELECT count(*) FROM assignments").await, 2);
    assert_eq!(app.count("SELECT count(*) FROM assignments WHERE (started_at AT TIME ZONE 'Africa/Algiers')::date = '2022-05-03'").await, 2);
    let note: String = sqlx::query_scalar(
        "SELECT m.note FROM movements m JOIN movement_lines l ON l.movement_id = m.id JOIN assets x ON x.id = l.asset_id WHERE x.inventory_no = 'PC-201'",
    )
    .fetch_one(app.pool())
    .await
    .unwrap();
    assert!(note.contains("Avec sacoche") && note.contains("attribué par Karim (ancien)") && note.contains("réf. SharePoint AFF-0045") && note.contains("dossier SharePoint ONB-2022-031"), "{note}");
    // Aucune décharge pour une remise ancienne, aucun dossier, aucun avis.
    assert_eq!(app.count("SELECT count(*) FROM documents").await, 0);
    assert_eq!(app.count("SELECT count(*) FROM movements WHERE kind <> 'reprise'").await, 0);
    assert_eq!(app.count("SELECT count(*) FROM cases").await, 0);
    assert_eq!(app.count("SELECT count(*) FROM outbox").await, 0);
}
