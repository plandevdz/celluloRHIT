//! Lot 2 — import/export Excel du personnel. Classeurs fictifs construits
//! dans le test ; nécessite DATABASE_URL.

mod common;

use std::io::Cursor;

use axum::body::Body;
use axum::http::{header, Method, Request, StatusCode};
use calamine::{open_workbook_from_rs, Data, Reader, Xlsx};
use common::{Session, TestApp};
use rust_xlsxwriter::{ExcelDateTime, Format, Formula, Workbook};
use serde_json::{json, Value};
use sqlx::PgPool;

/// Valeur de cellule dans un classeur de test.
enum V<'a> {
    T(&'a str),
    N(f64),
    D(u16, u8, u8),
    F(&'a str),
    E,
}

const HEADERS: [&str; 8] = ["ID salarié", "Matricule", "Prénom", "Nom", "Poste", "Site", "Date d’arrivée", "Département"];

fn workbook(rows: &[[V; 8]]) -> Vec<u8> {
    let mut wb = Workbook::new();
    let ws = wb.add_worksheet();
    ws.set_name("Personnel").unwrap();
    let date = Format::new().set_num_format("dd/mm/yyyy");
    for (c, h) in HEADERS.iter().enumerate() {
        ws.write_string(0, c as u16, *h).unwrap();
    }
    for (r, row) in rows.iter().enumerate() {
        let r = r as u32 + 1;
        for (c, v) in row.iter().enumerate() {
            let c = c as u16;
            match v {
                V::T(s) => {
                    ws.write_string(r, c, *s).unwrap();
                }
                V::N(n) => {
                    ws.write_number(r, c, *n).unwrap();
                }
                V::D(y, m, d) => {
                    ws.write_datetime_with_format(r, c, &ExcelDateTime::from_ymd(*y, *m, *d).unwrap(), &date).unwrap();
                }
                V::F(f) => {
                    ws.write_formula(r, c, Formula::new(*f)).unwrap();
                }
                V::E => {}
            }
        }
    }
    wb.save_to_buffer().unwrap()
}

async fn upload(app: &TestApp, s: &Session, bytes: &[u8]) -> (StatusCode, Value) {
    let boundary = "XLSXBOUNDARY";
    let mut body = format!(
        "--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"Personnel-test.xlsx\"\r\nContent-Type: application/vnd.openxmlformats-officedocument.spreadsheetml.sheet\r\n\r\n"
    )
    .into_bytes();
    body.extend_from_slice(bytes);
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    let req = Request::builder()
        .method(Method::POST)
        .uri("/api/v1/personnel-imports")
        .header(header::COOKIE, &s.cookie)
        .header("x-cellulo-client", "web")
        .header(header::CONTENT_TYPE, format!("multipart/form-data; boundary={boundary}"))
        .body(Body::from(body))
        .unwrap();
    let (status, bytes) = app.raw(req).await;
    (status, serde_json::from_slice(&bytes).unwrap_or(json!({ "raw": String::from_utf8_lossy(&bytes) })))
}

fn row<'a>(v: &'a Value, n: i64) -> &'a Value {
    v["rows"].as_array().unwrap().iter().find(|r| r["row_number"] == n).unwrap_or_else(|| panic!("ligne {n} absente"))
}

async fn count(app: &TestApp, sql: &str) -> i64 {
    sqlx::query_scalar(sql).fetch_one(app.pool()).await.unwrap()
}

#[sqlx::test(migrator = "server::MIGRATOR")]
async fn import_apercu_decisions_application_et_rejeu(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let rh = app.user("Rh Import", &["RH"]).await;
    let site = app.site("ORAN").await;
    // Fiches existantes (fictives).
    let mk = |first: &str, last: &str, mat: &str| {
        json!({ "first_name": first, "last_name": last, "matricule": mat, "job_title": "Agent", "site_id": site,
                "arrival_date": "2022-01-10", "already_present": true })
    };
    let (_, a) = app.call(&rh, Method::POST, "/employees", Some(mk("Karim", "Saidi", "777"))).await;
    let (_, b) = app.call(&rh, Method::POST, "/employees", Some(mk("Lina", "Haddad", "888"))).await;
    let a_id = a["id"].as_str().unwrap().to_string();
    let b_id = b["id"].as_str().unwrap().to_string();

    let bytes = workbook(&[
        // 2, 3 : deux homonymes sans matricule
        [V::E, V::E, V::T("Amine"), V::T("Benali"), V::T("Commercial"), V::T("ORAN"), V::D(2024, 3, 1), V::E],
        [V::E, V::E, V::T("Amine"), V::T("Benali"), V::T("Magasinier"), V::T("oran"), V::D(2024, 3, 2), V::E],
        // 4 : matricule texte avec zéros initiaux, nom arabe, date en texte
        [V::E, V::T("00042"), V::T("سارة"), V::T("Ouali"), V::T("Comptable"), V::T("ORAN"), V::T("05/01/2023"), V::T("Finance")],
        // 5 : ID de Karim + matricule de Lina → contradiction
        [V::T(&a_id), V::T("888"), V::E, V::E, V::E, V::E, V::E, V::E],
        // 6 : matricule saisi comme nombre
        [V::E, V::N(42.0), V::T("Nour"), V::T("Kaci"), V::T("Agent"), V::T("ORAN"), V::D(2024, 1, 1), V::E],
        // 7 : poste manquant pour une nouvelle fiche
        [V::E, V::E, V::T("Nour"), V::T("Kaci"), V::E, V::T("ORAN"), V::D(2024, 1, 1), V::E],
        // 8 : formule
        [V::E, V::E, V::T("Ali"), V::F("=\"Ferhat\""), V::T("Agent"), V::T("ORAN"), V::D(2024, 1, 1), V::E],
        // 9 : mise à jour de Lina par son ID (poste) ; vide = conserver
        [V::T(&b_id), V::E, V::E, V::E, V::T("Cheffe d'équipe"), V::E, V::E, V::E],
        // 10 : site inconnu
        [V::E, V::E, V::T("Yacine"), V::T("Mebarki"), V::T("Agent"), V::T("Tlemcen"), V::D(2024, 1, 1), V::E],
    ]);

    let (s, p) = upload(&app, &rh, &bytes).await;
    assert_eq!(s, StatusCode::OK, "{p}");
    let proposals: Vec<(i64, String)> = p["rows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| (r["row_number"].as_i64().unwrap(), r["proposal"].as_str().unwrap().to_string()))
        .collect();
    let expect = [
        (2, "a_revoir"), (3, "a_revoir"), (4, "creation"), (5, "rejet"), (6, "rejet"),
        (7, "rejet"), (8, "rejet"), (9, "mise_a_jour"), (10, "rejet"),
    ];
    assert_eq!(proposals, expect.iter().map(|(n, s)| (*n, s.to_string())).collect::<Vec<_>>(), "{p}");
    assert!(row(&p, 5)["messages"].to_string().contains("contradictoires"));
    assert!(row(&p, 6)["messages"].to_string().contains("zéros initiaux"));
    assert!(row(&p, 8)["messages"].to_string().contains("formules"));
    assert!(row(&p, 2)["messages"].to_string().contains("Homonyme dans le fichier"));
    assert_eq!(row(&p, 9)["changes"]["Poste"]["apres"], "Cheffe d'équipe");
    // Aperçu : aucune écriture métier.
    assert_eq!(count(&app, "SELECT count(*) FROM employees").await, 2);

    let batch = p["batch"]["id"].as_str().unwrap().to_string();
    // Décision impossible sur une ligne rejetée ; « rattacher » limité aux candidats.
    let (s, _) = app.call(&rh, Method::POST, &format!("/personnel-imports/{batch}/rows/5/decision"), Some(json!({ "decision": "exclure" }))).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);
    let (s, _) = app
        .call(&rh, Method::POST, &format!("/personnel-imports/{batch}/rows/2/decision"), Some(json!({ "decision": "rattacher", "employee_id": a_id })))
        .await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);
    // Le RH décide : ligne 2 = nouvelle personne ; ligne 3 laissée sans décision.
    let (s, _) = app.call(&rh, Method::POST, &format!("/personnel-imports/{batch}/rows/2/decision"), Some(json!({ "decision": "creer" }))).await;
    assert_eq!(s, StatusCode::OK);

    let uri = format!("/personnel-imports/{batch}/commit");
    let (a1, a2) = tokio::join!(app.call(&rh, Method::POST, &uri, Some(json!({}))), app.call(&rh, Method::POST, &uri, Some(json!({}))));
    assert_eq!((a1.0, a2.0), (StatusCode::OK, StatusCode::OK), "double clic : {} / {}", a1.1, a2.1);
    let done = a1.1;
    assert_eq!(done["batch"]["status"], "applique");
    assert_eq!(row(&done, 2)["result"], "cree");
    assert_eq!(row(&done, 3)["result"], "ignore", "ambiguë sans décision : jamais appliquée");
    assert_eq!(row(&done, 4)["result"], "cree");
    assert_eq!(row(&done, 9)["result"], "mis_a_jour");
    assert_eq!(count(&app, "SELECT count(*) FROM employees").await, 4, "2 existants + Amine (ligne 2) + Sara");
    assert_eq!(count(&app, "SELECT count(*) FROM employees WHERE last_name = 'Benali'").await, 1);

    // Valeurs en base.
    let (mat, first, status): (String, String, String) =
        sqlx::query_as("SELECT matricule, first_name, status FROM employees WHERE last_name = 'Ouali'").fetch_one(app.pool()).await.unwrap();
    assert_eq!((mat.as_str(), first.as_str(), status.as_str()), ("00042", "سارة", "actif"));
    let job: String = sqlx::query_scalar("SELECT job_title FROM employee_positions WHERE employee_id = $1::uuid AND valid_to IS NULL")
        .bind(&b_id)
        .fetch_one(app.pool())
        .await
        .unwrap();
    assert_eq!(job, "Cheffe d'équipe");
    let b_mat: String = sqlx::query_scalar("SELECT matricule FROM employees WHERE id = $1::uuid").bind(&b_id).fetch_one(app.pool()).await.unwrap();
    assert_eq!(b_mat, "888", "valeur vide = conserver");
    // Aucune arrivée, tâche ni avis.
    assert_eq!(count(&app, "SELECT count(*) FROM cases").await, 0);
    assert_eq!(count(&app, "SELECT count(*) FROM tasks").await, 0);
    assert_eq!(count(&app, "SELECT count(*) FROM outbox").await, 0);
    assert_eq!(count(&app, "SELECT count(*) FROM audit_events WHERE action = 'import_personnel_applique'").await, 1);

    // Rejeu du même classeur : signalé et refusé, rien n'est dupliqué.
    let (s, again) = upload(&app, &rh, &bytes).await;
    assert_eq!(s, StatusCode::OK);
    assert!(again["batch"]["already_applied"].is_object(), "{}", again["batch"]);
    let batch2 = again["batch"]["id"].as_str().unwrap().to_string();
    app.call(&rh, Method::POST, &format!("/personnel-imports/{batch2}/rows/2/decision"), Some(json!({ "decision": "creer" }))).await;
    let (s, err) = app.call(&rh, Method::POST, &format!("/personnel-imports/{batch2}/commit"), Some(json!({}))).await;
    assert_eq!(s, StatusCode::CONFLICT, "{err}");
    assert_eq!(count(&app, "SELECT count(*) FROM employees").await, 4);
}

/// La date d'arrivée réelle confirmée est protégée : identique acceptée,
/// différente refusée (aperçu ET application), date réelle future refusée,
/// les autres lignes du lot restent traitables.
#[sqlx::test(migrator = "server::MIGRATOR")]
async fn import_protege_la_date_reelle_confirmee(pool: PgPool) {
    use chrono::Datelike;
    let app = TestApp::new(pool).await;
    let rh = app.user("Rh Dates", &["RH"]).await;
    let site = app.site("ORAN").await;
    let create = |first: &'static str, last: &'static str, date: &'static str| {
        json!({ "first_name": first, "last_name": last, "job_title": "Agent", "site_id": site, "arrival_date": date, "already_present": true })
    };
    let mut ids = Vec::new();
    for (f, l, d) in [("Amel", "Saadi", "2022-01-10"), ("Kamel", "Rahmani", "2021-05-03"), ("Djamel", "Ferroukhi", "2020-06-01"), ("Rania", "Lounis", "2020-02-02")] {
        let (s, v) = app.call(&rh, Method::POST, "/employees", Some(create(f, l, d))).await;
        assert_eq!(s, StatusCode::OK, "{v}");
        ids.push(v["id"].as_str().unwrap().to_string());
    }
    let (a, c, d, h) = (ids[0].clone(), ids[1].clone(), ids[2].clone(), ids[3].clone());
    // Fiche historique active dont la date réelle n'a jamais été saisie.
    sqlx::query("UPDATE employees SET actual_arrival = NULL WHERE id = $1::uuid").bind(&d).execute(app.pool()).await.unwrap();

    let future = server::documents::algiers_today() + chrono::Duration::days(40);
    let fut = V::D(future.year() as u16, future.month() as u8, future.day() as u8);
    let bytes = workbook(&[
        // 2 : même date confirmée + changement de poste → accepté
        [V::T(&a), V::E, V::E, V::E, V::T("Chef de quai"), V::E, V::D(2022, 1, 10), V::E],
        // 3 : autre date que la date confirmée → refusé
        [V::T(&c), V::E, V::E, V::E, V::E, V::E, V::D(2022, 2, 1), V::E],
        // 4 : date réelle future pour un salarié déjà présent → refusée
        [V::T(&d), V::E, V::E, V::E, V::E, V::E, fut, V::E],
        // 5 : salarié historique à créer → accepté
        [V::E, V::E, V::T("Yanis"), V::T("Ait"), V::T("Magasinier"), V::T("ORAN"), V::D(2023, 3, 1), V::E],
        // 6 : homonyme de Rania Lounis (date différente) : à revoir, puis rattaché
        [V::E, V::E, V::T("Rania"), V::T("Lounis"), V::T("Agent"), V::T("ORAN"), V::D(2024, 9, 15), V::E],
    ]);
    let (s, p) = upload(&app, &rh, &bytes).await;
    assert_eq!(s, StatusCode::OK, "{p}");
    assert_eq!(row(&p, 2)["proposal"], "mise_a_jour", "date identique acceptée : {}", row(&p, 2));
    assert!(row(&p, 2)["changes"].get("Date d’arrivée").is_none());
    assert_eq!(row(&p, 3)["proposal"], "rejet");
    assert!(row(&p, 3)["messages"].to_string().contains("Corriger la date d’arrivée"), "{}", row(&p, 3));
    assert_eq!(row(&p, 4)["proposal"], "rejet");
    assert!(row(&p, 4)["messages"].to_string().contains("future"), "{}", row(&p, 4));
    assert_eq!(row(&p, 5)["proposal"], "creation");
    assert_eq!(row(&p, 6)["proposal"], "a_revoir");

    // Le RH rattache la ligne 6 à la fiche existante : refus à l'application.
    let batch = p["batch"]["id"].as_str().unwrap().to_string();
    let (s, _) = app
        .call(&rh, Method::POST, &format!("/personnel-imports/{batch}/rows/6/decision"), Some(json!({ "decision": "rattacher", "employee_id": h })))
        .await;
    assert_eq!(s, StatusCode::OK);
    let (s, done) = app.call(&rh, Method::POST, &format!("/personnel-imports/{batch}/commit"), Some(json!({}))).await;
    assert_eq!(s, StatusCode::OK, "{done}");
    assert_eq!(row(&done, 2)["result"], "mis_a_jour");
    assert_eq!(row(&done, 3)["result"], "rejete");
    assert_eq!(row(&done, 4)["result"], "rejete");
    assert_eq!(row(&done, 5)["result"], "cree", "l'autre ligne valide reste traitable");
    assert_eq!(row(&done, 6)["result"], "conflit", "règle appliquée aussi au commit : {}", row(&done, 6));
    assert!(row(&done, 6)["result_message"].as_str().unwrap().contains("Corriger la date d’arrivée"));

    // Base : dates confirmées intactes, modification autorisée appliquée.
    let date_of = |id: String| {
        let pool = app.pool().clone();
        async move {
            sqlx::query_scalar::<_, Option<chrono::NaiveDate>>("SELECT actual_arrival FROM employees WHERE id = $1::uuid")
                .bind(id)
                .fetch_one(&pool)
                .await
                .unwrap()
        }
    };
    assert_eq!(date_of(a.clone()).await, chrono::NaiveDate::from_ymd_opt(2022, 1, 10));
    assert_eq!(date_of(c.clone()).await, chrono::NaiveDate::from_ymd_opt(2021, 5, 3));
    assert_eq!(date_of(d.clone()).await, None, "aucune date future enregistrée");
    assert_eq!(date_of(h.clone()).await, chrono::NaiveDate::from_ymd_opt(2020, 2, 2));
    let job: String = sqlx::query_scalar("SELECT job_title FROM employee_positions WHERE employee_id = $1::uuid AND valid_to IS NULL")
        .bind(&a)
        .fetch_one(app.pool())
        .await
        .unwrap();
    assert_eq!(job, "Chef de quai");
    let (st, actual): (String, Option<chrono::NaiveDate>) =
        sqlx::query_as("SELECT status, actual_arrival FROM employees WHERE last_name = 'Ait'").fetch_one(app.pool()).await.unwrap();
    assert_eq!((st.as_str(), actual), ("actif", chrono::NaiveDate::from_ymd_opt(2023, 3, 1)), "salarié historique créé");
    assert_eq!(count(&app, "SELECT count(*) FROM audit_events WHERE action = 'date_arrivee_corrigee'").await, 0, "l'import ne corrige jamais une date");
}

#[sqlx::test(migrator = "server::MIGRATOR")]
async fn export_droits_et_contenu_reel(pool: PgPool) {
    let app = TestApp::new(pool).await;
    let rh = app.user("Rh Export", &["RH"]).await;
    let it = app.user("It Export", &["IT"]).await;
    let graph = app.user("Graph Export", &["GRAPHISTE"]).await;
    let site = app.site("ALGER").await;
    for (first, last, mat, job) in [("محمد", "Benaïssa", "00042", "=1+1"), ("Hélène", "Aït-Ahmed", "", "Comptable")] {
        let (s, _) = app
            .call(&rh, Method::POST, "/employees", Some(json!({ "first_name": first, "last_name": last, "matricule": mat, "job_title": job,
                    "site_id": site, "arrival_date": "2021-09-01", "already_present": true })))
            .await;
        assert_eq!(s, StatusCode::OK);
    }

    // Comptes non autorisés, y compris en appel direct.
    for s in [&it, &graph] {
        for path in ["/personnel/export.xlsx", "/personnel/template.xlsx", "/personnel-imports"] {
            let (st, _) = app.call(s, Method::GET, path, None).await;
            assert_eq!(st, StatusCode::FORBIDDEN, "{path}");
        }
        let (st, _) = upload(&app, s, &workbook(&[])).await;
        assert_eq!(st, StatusCode::FORBIDDEN);
        let (st, _) = app.call(s, Method::POST, &format!("/personnel-imports/{}/commit", uuid::Uuid::new_v4()), Some(json!({}))).await;
        assert_eq!(st, StatusCode::FORBIDDEN);
    }

    let req = Request::builder().uri("/api/v1/personnel/export.xlsx").header(header::COOKIE, &rh.cookie).body(Body::empty()).unwrap();
    let (s, bytes) = app.raw(req).await;
    assert_eq!(s, StatusCode::OK);
    let mut wb: Xlsx<_> = open_workbook_from_rs(Cursor::new(bytes.clone())).unwrap();
    let range = wb.worksheet_range("Personnel").unwrap();
    let formulas = wb.worksheet_formula("Personnel").unwrap();
    assert!(formulas.used_cells().all(|(_, _, f)| f.is_empty()), "aucune formule dans l'export");
    let rows: Vec<Vec<Data>> = range.rows().map(|r| r.to_vec()).collect();
    assert_eq!(rows[0][0], Data::String("ID salarié".into()));
    assert_eq!(rows.len(), 3);
    let arabic = rows.iter().find(|r| r[3] == Data::String("Benaïssa".into())).unwrap();
    assert_eq!(arabic[1], Data::String("00042".into()), "matricule exporté en texte");
    assert_eq!(arabic[2], Data::String("محمد".into()));
    assert_eq!(arabic[4], Data::String("=1+1".into()), "contenu exporté comme texte, pas comme formule");
    let accent = rows.iter().find(|r| r[3] == Data::String("Aït-Ahmed".into())).unwrap();
    assert_eq!(accent[2], Data::String("Hélène".into()));
    assert_eq!(count(&app, "SELECT count(*) FROM audit_events WHERE action = 'export_personnel'").await, 1);

    // Réimport de l'export : reconnu par l'ID ; la seule anomalie est le
    // « =1+1 » qui ressemble à une formule et doit être revu.
    let (s, p) = upload(&app, &rh, &bytes).await;
    assert_eq!(s, StatusCode::OK, "{p}");
    let props: Vec<&str> = p["rows"].as_array().unwrap().iter().map(|r| r["proposal"].as_str().unwrap()).collect();
    assert!(props.contains(&"inchange"), "{props:?}");
    assert_eq!(p["counts"]["creation"], Value::Null, "aucune création à partir d'un export");

    // Modèle téléchargeable.
    let req = Request::builder().uri("/api/v1/personnel/template.xlsx").header(header::COOKIE, &rh.cookie).body(Body::empty()).unwrap();
    let (s, tpl) = app.raw(req).await;
    assert_eq!(s, StatusCode::OK);
    let mut wb: Xlsx<_> = open_workbook_from_rs(Cursor::new(tpl)).unwrap();
    let r = wb.worksheet_range("Personnel").unwrap();
    let headers: Vec<String> = r.rows().next().unwrap().iter().map(|c| c.to_string()).collect();
    assert_eq!(headers, HEADERS.iter().map(|h| h.to_string()).collect::<Vec<_>>());
}
