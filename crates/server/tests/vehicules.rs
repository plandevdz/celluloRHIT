//! Parc véhicules RH (R11, R12 partie véhicule). Données fictives ;
//! nécessite DATABASE_URL.

mod common;

use axum::http::{Method, StatusCode};
use common::TestApp;
use serde_json::{json, Value};
use sqlx::PgPool;

#[sqlx::test(migrator = "server::MIGRATOR")]
async fn r11_location_restituee_puis_rendue_au_loueur(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let rh = app.user("Rh Vehicules", &["RH"]).await;
    let graphiste = app.user("Graphiste", &["GRAPHISTE"]).await;
    let site = app.site("ORAN").await;

    let (status, v) = app
        .call(&rh, Method::POST, "/vehicles", Some(json!({
            "plate": "12345 116 31", "kind": "location", "brand": "Renault", "model": "Clio", "site_id": site,
            "odometer_km": 10000, "lease": { "lessor": "Loueur Fictif", "contract_no": "LOC-1", "end_date": "2027-06-30" }
        })))
        .await;
    assert_eq!(status, StatusCode::OK, "{v}");
    let vid = v["id"].as_str().unwrap().to_string();
    // Doublon d'immatriculation (espaces différents) refusé.
    let (status, _) = app.call(&rh, Method::POST, "/vehicles", Some(json!({ "plate": "12345116 31", "kind": "propriete" }))).await;
    assert_eq!(status, StatusCode::CONFLICT);

    // Arrivée avec besoin véhicule : tâche RH, soldée par la remise.
    let onb = app
        .announce(&rh, site, "Yacine", "Terrain", json!({ "needs": [{ "category": "vehicule", "answer": "oui" }] }))
        .await;
    let case_id = onb["case_id"].as_str().unwrap().to_string();
    let employee = onb["employee_id"].as_str().unwrap().to_string();
    let task: uuid::Uuid = sqlx::query_scalar("SELECT id FROM tasks WHERE role = 'RH'").fetch_one(app.pool()).await.unwrap();
    let (status, _) = app.call(&rh, Method::POST, &format!("/tasks/{task}/claim"), None).await;
    assert_eq!(status, StatusCode::OK);
    let (status, v) = app.call(&rh, Method::POST, &format!("/tasks/{task}/complete"), Some(json!({ "resolution": "remis" }))).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "pas de remise, pas de fin : {v}");

    // Kilométrage incohérent refusé sans justification.
    let (status, v) = app
        .call(&rh, Method::POST, &format!("/vehicles/{vid}/handover"), Some(json!({
            "employee_id": employee, "case_id": case_id, "task_id": task, "odometer_km": 9000
        })))
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{v}");
    let (status, v) = app
        .call_with(&rh, Method::POST, &format!("/vehicles/{vid}/handover"), Some(json!({
            "employee_id": employee, "case_id": case_id, "task_id": task, "odometer_km": 10020,
            "condition": "Rayure portière arrière", "keys_count": 2, "fuel_level": "1/2"
        })), &[("idempotency-key", "veh-remise-1")])
        .await;
    assert_eq!(status, StatusCode::CREATED, "{v}");
    let (status, again) = app
        .call_with(&rh, Method::POST, &format!("/vehicles/{vid}/handover"), Some(json!({
            "employee_id": employee, "case_id": case_id, "task_id": task, "odometer_km": 10020,
            "condition": "Rayure portière arrière", "keys_count": 2, "fuel_level": "1/2"
        })), &[("idempotency-key", "veh-remise-1")])
        .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(again["reference"], v["reference"], "rejeu : même remise");
    let (status, v) = app.call(&rh, Method::POST, &format!("/tasks/{task}/complete"), Some(json!({ "resolution": "remis" }))).await;
    assert_eq!(status, StatusCode::OK, "{v}");
    assert_eq!(v["evidence"]["immatriculation"], "12345 116 31");

    // Retour au loueur impossible tant que le salarié détient le véhicule.
    let (status, _) = app.call(&rh, Method::POST, &format!("/vehicles/{vid}/lessor-return"), Some(json!({}))).await;
    assert_eq!(status, StatusCode::CONFLICT);
    // Restitution chez Celluloplast, puis retour au loueur : deux mouvements.
    let (status, v) = app
        .call(&rh, Method::POST, &format!("/vehicles/{vid}/return"), Some(json!({ "odometer_km": 11250, "condition": "Bon", "keys_count": 2 })))
        .await;
    assert_eq!(status, StatusCode::CREATED, "{v}");
    let (status, v) = app
        .call(&rh, Method::POST, &format!("/vehicles/{vid}/lessor-return"), Some(json!({ "odometer_km": 11262, "condition": "Remis au loueur" })))
        .await;
    assert_eq!(status, StatusCode::OK, "{v}");
    let (status, d) = app.call(&rh, Method::GET, &format!("/vehicles/{vid}"), None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(d["vehicle"]["status"], "rendu_loueur");
    assert_eq!(d["vehicle"]["odometer_km"], 11262);
    let kinds: Vec<&str> = d["movements"].as_array().unwrap().iter().map(|m| m["kind"].as_str().unwrap()).collect();
    assert_eq!(kinds, vec!["retour_loueur", "restitution", "remise"]);
    assert_eq!(d["leases"][0]["status"], "restitue");
    // Non affectable après remise au loueur.
    let (status, _) = app
        .call(&rh, Method::POST, &format!("/vehicles/{vid}/handover"), Some(json!({ "employee_id": employee, "odometer_km": 11300 })))
        .await;
    assert_eq!(status, StatusCode::CONFLICT);

    // Le parc véhicules n'est pas visible du GRAPHISTE, même en appel direct.
    let (status, _) = app.call(&graphiste, Method::GET, "/vehicles", None).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, _) = app.call(&graphiste, Method::GET, &format!("/vehicles/{vid}"), None).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[sqlx::test(migrator = "server::MIGRATOR")]
async fn r12_echeances_et_renouvellement_conserve(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let rh = app.user("Rh", &["RH"]).await;
    let (_, v) = app.call(&rh, Method::POST, "/vehicles", Some(json!({ "plate": "AB-123-CD", "kind": "propriete", "odometer_km": 14200 }))).await;
    let vid = v["id"].as_str().unwrap().to_string();
    let soon = (chrono::Utc::now().date_naive() + chrono::Duration::days(10)).to_string();
    let later = (chrono::Utc::now().date_naive() + chrono::Duration::days(400)).to_string();
    let (status, _) = app
        .call(&rh, Method::POST, &format!("/vehicles/{vid}/documents"), Some(json!({ "kind": "assurance", "reference": "POL-1", "valid_until": soon })))
        .await;
    assert_eq!(status, StatusCode::OK);
    let (status, _) = app
        .call(&rh, Method::POST, &format!("/vehicles/{vid}/maintenance"), Some(json!({
            "kind": "vidange", "done_on": "2026-01-10", "odometer_km": 14200, "next_due_km": 15000
        })))
        .await;
    assert_eq!(status, StatusCode::OK);
    let (_, d) = app.call(&rh, Method::GET, "/vehicles/deadlines", None).await;
    let kinds: Vec<&str> = d.as_array().unwrap().iter().map(|x| x["kind"].as_str().unwrap()).collect();
    assert!(kinds.contains(&"assurance") && kinds.contains(&"vidange"), "{d}");

    // Worker : alerte RH une seule fois, même après plusieurs passages.
    server::worker::check_deadlines(&app.state).await.unwrap();
    server::worker::check_deadlines(&app.state).await.unwrap();
    let alerts = app.count("SELECT count(*) FROM outbox WHERE event_type = 'echeance_vehicule' AND 'RH' = ANY(target_roles)").await;
    assert_eq!(alerts, 2, "une alerte assurance et une alerte vidange");

    // Renouvellement : nouvelle pièce, l'ancienne est conservée.
    let (status, _) = app
        .call(&rh, Method::POST, &format!("/vehicles/{vid}/documents"), Some(json!({ "kind": "assurance", "reference": "POL-2", "valid_until": later })))
        .await;
    assert_eq!(status, StatusCode::OK);
    let (_, d) = app.call(&rh, Method::GET, "/vehicles/deadlines", None).await;
    let kinds: Vec<Value> = d.as_array().unwrap().iter().map(|x| x["kind"].clone()).collect();
    assert!(!kinds.contains(&json!("assurance")), "l'assurance renouvelée n'est plus en échéance : {d}");
    let (_, detail) = app.call(&rh, Method::GET, &format!("/vehicles/{vid}"), None).await;
    let docs = detail["documents"].as_array().unwrap();
    assert_eq!(docs.len(), 2, "historique conservé");
    assert_eq!(docs.iter().filter(|d| d["current"] == true).count(), 1);
}
