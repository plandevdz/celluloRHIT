//! Logiciels (R12) : dernier siège attribué en parallèle, libération,
//! licence perpétuelle sans fausse échéance. Nécessite DATABASE_URL.

mod common;

use axum::http::{Method, StatusCode};
use common::TestApp;
use serde_json::json;
use sqlx::PgPool;

#[sqlx::test(migrator = "server::MIGRATOR")]
async fn r12_dernier_siege_en_parallele(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let it1 = app.user("Tech Un", &["IT"]).await;
    let it2 = app.user("Tech Deux", &["IT"]).await;
    let rh = app.user("Rh", &["RH"]).await;
    let graphiste = app.user("Graphiste", &["GRAPHISTE"]).await;
    let site = app.site("ALGER").await;
    let a = app.announce(&rh, site, "Lina", "Ait", json!({})).await["employee_id"].as_str().unwrap().to_string();
    let b = app.announce(&rh, site, "Omar", "Saidi", json!({})).await["employee_id"].as_str().unwrap().to_string();

    // Licence perpétuelle : pas de fausse date de renouvellement.
    let (status, _) = app
        .call(&it1, Method::POST, "/software", Some(json!({
            "vendor": "Éditeur", "product": "Outil", "license_kind": "perpetuelle", "end_date": "2027-01-01", "seats": 3
        })))
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);

    let (status, v) = app
        .call(&it1, Method::POST, "/software", Some(json!({
            "vendor": "Éditeur Fictif", "product": "CAO", "license_kind": "abonnement", "start_date": "2026-01-01",
            "end_date": "2027-12-31", "seats": 1, "seat_release": "immediat"
        })))
        .await;
    assert_eq!(status, StatusCode::OK, "{v}");
    let sub = v["id"].as_str().unwrap().to_string();

    let uri = format!("/software/{sub}/allocations");
    let (r1, r2) = tokio::join!(
        app.call(&it1, Method::POST, &uri, Some(json!({ "employee_id": a }))),
        app.call(&it2, Method::POST, &uri, Some(json!({ "employee_id": b })))
    );
    let mut statuses = vec![r1.0, r2.0];
    statuses.sort();
    assert_eq!(statuses, vec![StatusCode::CREATED, StatusCode::CONFLICT], "une seule attribution initiale : {r1:?} {r2:?}");
    assert_eq!(app.count("SELECT count(*) FROM license_allocations WHERE ended_at IS NULL").await, 1);
    let winner = if r1.0 == StatusCode::CREATED { r1.1 } else { r2.1 };
    let loser_emp = if r1.0 == StatusCode::CREATED { &b } else { &a };

    // Capacité non réductible sous le nombre occupé.
    let (_, d) = app.call(&it1, Method::GET, &format!("/software/{sub}"), None).await;
    let version = d["subscription"]["version"].as_i64().unwrap();
    let (status, _) = app
        .call(&it1, Method::PATCH, &format!("/software/{sub}"), Some(json!({
            "version": version, "vendor": "Éditeur Fictif", "product": "CAO", "license_kind": "abonnement",
            "start_date": "2026-01-01", "end_date": "2027-12-31", "seats": 0
        })))
        .await;
    assert_eq!(status, StatusCode::CONFLICT);

    // Libération : le siège redevient disponible.
    let (status, v) = app
        .call(&it1, Method::POST, &format!("/license-allocations/{}/release", winner["id"].as_str().unwrap()), Some(json!({ "note": "fin de mission" })))
        .await;
    assert_eq!(status, StatusCode::OK, "{v}");
    let (status, v) = app.call(&it2, Method::POST, &uri, Some(json!({ "employee_id": loser_emp }))).await;
    assert_eq!(status, StatusCode::CREATED, "{v}");

    // Fiche salarié : licence visible pour l'IT, jamais pour le GRAPHISTE.
    let (_, e) = app.call(&it1, Method::GET, &format!("/employees/{loser_emp}"), None).await;
    assert_eq!(e["licenses"].as_array().unwrap().len(), 1);
    let (status, _) = app.call(&graphiste, Method::GET, "/software", None).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[sqlx::test(migrator = "server::MIGRATOR")]
async fn siege_libere_en_fin_de_periode(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let it = app.user("Tech", &["IT"]).await;
    let rh = app.user("Rh", &["RH"]).await;
    let site = app.site("ALGER").await;
    let a = app.announce(&rh, site, "Lina", "Ait", json!({})).await["employee_id"].as_str().unwrap().to_string();
    let b = app.announce(&rh, site, "Omar", "Saidi", json!({})).await["employee_id"].as_str().unwrap().to_string();
    let end = (chrono::Utc::now().date_naive() + chrono::Duration::days(20)).to_string();
    let (_, v) = app
        .call(&it, Method::POST, "/software", Some(json!({
            "vendor": "Éditeur", "product": "Suite", "license_kind": "abonnement", "end_date": end, "seats": 1, "seat_release": "fin_periode"
        })))
        .await;
    let sub = v["id"].as_str().unwrap().to_string();
    let (_, alloc) = app.call(&it, Method::POST, &format!("/software/{sub}/allocations"), Some(json!({ "employee_id": a }))).await;
    let (status, _) = app.call(&it, Method::POST, &format!("/license-allocations/{}/release", alloc["id"].as_str().unwrap()), None).await;
    assert_eq!(status, StatusCode::OK);
    // Le siège reste occupé jusqu'à la fin de la période payée.
    let (status, _) = app.call(&it, Method::POST, &format!("/software/{sub}/allocations"), Some(json!({ "employee_id": b }))).await;
    assert_eq!(status, StatusCode::CONFLICT);
    // Alerte de renouvellement à l'IT, une seule fois.
    server::worker::check_deadlines(&app.state).await.unwrap();
    server::worker::check_deadlines(&app.state).await.unwrap();
    assert_eq!(app.count("SELECT count(*) FROM outbox WHERE event_type = 'echeance_logiciel' AND 'IT' = ANY(target_roles)").await, 1);
}
