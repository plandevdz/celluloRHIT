//! Inventaire QR (R07) : doublon neutralisé, écarts explicites, rapport
//! exportable, aucune affectation changée, réimpression identique.
//! Nécessite DATABASE_URL.

mod common;

use axum::body::Body;
use axum::http::{header, Method, Request, StatusCode};
use common::TestApp;
use serde_json::{json, Value};
use sqlx::PgPool;
use uuid::Uuid;

#[sqlx::test(migrator = "server::MIGRATOR")]
async fn r07_campagne_inventaire(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let it = app.user("Tech Inventaire", &["IT"]).await;
    let site = app.site("CHERAGA").await;
    let bureau: Uuid = sqlx::query_scalar("INSERT INTO locations (site_id, name) VALUES ($1, 'Bureau 1') RETURNING id")
        .bind(site)
        .fetch_one(app.pool())
        .await
        .unwrap();
    let atelier: Uuid = sqlx::query_scalar("INSERT INTO locations (site_id, name) VALUES ($1, 'Atelier') RETURNING id")
        .bind(site)
        .fetch_one(app.pool())
        .await
        .unwrap();
    let mut assets = Vec::new();
    for (inv, loc) in [("PC-101", bureau), ("PC-102", bureau), ("ECR-01", atelier)] {
        let (status, v) = app
            .call(&it, Method::POST, "/assets", Some(json!({
                "inventory_no": inv, "kind": "pc_portable", "label": "Portable", "site_id": site, "location_id": loc
            })))
            .await;
        assert_eq!(status, StatusCode::OK, "{v}");
        assets.push((v["id"].as_str().unwrap().to_string(), v["qr_token"].as_str().unwrap().to_string()));
    }

    // Étiquettes : planche A4 et étiquette unitaire ; même jeton à chaque fois.
    let ids = assets.iter().map(|a| a.0.clone()).collect::<Vec<_>>().join(",");
    for fmt in ["a4-24", "a4-65", "unitaire"] {
        let req = Request::builder()
            .uri(format!("/api/v1/assets/labels.pdf?ids={ids}&format={fmt}&start=3"))
            .header(header::COOKIE, &it.cookie)
            .body(Body::empty())
            .unwrap();
        let (status, bytes) = app.raw(req).await;
        assert_eq!(status, StatusCode::OK);
        assert!(bytes.starts_with(b"%PDF"));
    }
    let (_, again) = app.call(&it, Method::GET, &format!("/assets/{}", assets[0].0), None).await;
    assert_eq!(again["asset"]["qr_token"], assets[0].1, "le jeton QR ne change jamais");

    let (status, c) = app.call(&it, Method::POST, "/inventories", Some(json!({ "name": "Inventaire annuel", "site_id": site }))).await;
    assert_eq!(status, StatusCode::OK, "{c}");
    assert_eq!(c["expected"], 3);
    let camp = c["id"].as_str().unwrap().to_string();
    let scan = |code: String, loc: Uuid, key: &str| {
        let body = json!({ "code": code, "location_id": loc, "command_key": key });
        let uri = format!("/inventories/{camp}/scan");
        let app = &app;
        let it = &it;
        async move { app.call(it, Method::POST, &uri, Some(body)).await }
    };

    // Vu au bon endroit, puis doublon au même local.
    let (_, r) = scan(assets[0].1.clone(), bureau, "k1").await;
    assert_eq!(r["result"], "vu", "{r}");
    let (_, r) = scan("pc-101".into(), bureau, "k2").await;
    assert_eq!(r["result"], "doublon");
    assert_eq!(r["message"], "Déjà pointé dans ce local");
    // Rejeu de la même commande (réseau coupé) : rien de plus.
    let (_, r) = scan(assets[0].1.clone(), bureau, "k1").await;
    assert_eq!(r["replayed"], true);
    // Vu ailleurs que l'emplacement attendu.
    let (_, r) = scan(assets[2].1.clone(), bureau, "k3").await;
    assert_eq!(r["result"], "ailleurs");
    // Code inconnu : aucun lien, aucun actif.
    let (status, r) = scan("HTTP://EXEMPLE.TEST/X".into(), atelier, "k4").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(r["result"], "inconnu");
    assert!(r["asset_id"].is_null());

    let (_, rep) = app.call(&it, Method::GET, &format!("/inventories/{camp}/report"), None).await;
    let n = |k: &str| rep[k].as_array().unwrap().len();
    assert_eq!(n("vus"), 1);
    assert_eq!(n("ailleurs"), 1);
    assert_eq!(n("non_vus"), 1, "PC-102 non vu : à vérifier, pas perdu");
    assert_eq!(n("inconnus"), 1);
    assert_eq!(rep["counters"]["duplicates"], 1);
    assert_eq!(rep["non_vus"][0]["inventory_no"], "PC-102");

    // Aucune affectation ni emplacement modifiés par les pointages.
    assert_eq!(app.count("SELECT count(*) FROM assignments").await, 0);
    let loc: Option<Uuid> = sqlx::query_scalar("SELECT location_id FROM assets WHERE inventory_no = 'ECR-01'").fetch_one(app.pool()).await.unwrap();
    assert_eq!(loc, Some(atelier));

    // Rapport Excel exportable.
    let req = Request::builder()
        .uri(format!("/api/v1/inventories/{camp}/report.xlsx"))
        .header(header::COOKIE, &it.cookie)
        .body(Body::empty())
        .unwrap();
    let (status, bytes) = app.raw(req).await;
    assert_eq!(status, StatusCode::OK);
    assert!(bytes.starts_with(b"PK"));

    // Arbitrage puis rectification explicite (commande distincte, tracée).
    let (status, _) = app
        .call(&it, Method::POST, &format!("/inventories/{camp}/arbitrations"), Some(json!({
            "asset_id": assets[2].0, "decision": "parc_rectifie", "note": "écran déplacé au bureau 1"
        })))
        .await;
    assert_eq!(status, StatusCode::OK);
    let (_, d) = app.call(&it, Method::GET, &format!("/assets/{}", assets[2].0), None).await;
    let (status, _) = app
        .call(&it, Method::PATCH, &format!("/assets/{}", assets[2].0), Some(json!({
            "version": d["asset"]["version"], "location_id": bureau, "reason": "inventaire INV : vu au bureau 1"
        })))
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(app.count("SELECT count(*) FROM audit_events WHERE action = 'rectification_emplacement'").await, 1);

    let (status, _) = app.call(&it, Method::POST, &format!("/inventories/{camp}/close"), Some(json!({ "note": "fin" }))).await;
    assert_eq!(status, StatusCode::OK);
    let (status, _) = scan(assets[1].1.clone(), bureau, "k9").await;
    assert_eq!(status, StatusCode::CONFLICT, "plus de pointage après clôture");
    let _: Value = rep;
}
