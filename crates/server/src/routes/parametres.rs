//! Administration → Paramètres, Sauvegardes et Modèles de tâches : ce que
//! l'ADMIN règle lui-même, sans éditer le fichier .env ni se connecter au
//! serveur. Aucun secret n'est affiché : les clés privées restent des
//! fichiers du serveur, seuls les certificats publics se téléchargent.

use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::header;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post, put};
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use domain::Role;
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::audit::{self, Audit};
use crate::auth::CurrentUser;
use crate::config::{Config, Module};
use crate::error::{AppError, AppResult};
use crate::graph::{GraphClient, SendOutcome};
use crate::settings::{self, Identity, SECTIONS};
use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/admin/config", get(overview))
        .route("/admin/config/{section}", put(save_section).delete(reset_section))
        .route("/admin/config/certificates/{identity}", get(download_certificate).post(generate_certificate))
        .route("/admin/config/mail-test", post(mail_test))
        .route("/admin/config/archive-test", post(archive_test))
        .route("/admin/backups", get(list_backups).post(request_backup))
        .route("/admin/backups/{id}/files/{name}", get(download_backup))
        .route("/task-templates", get(list_templates))
        .route("/admin/task-templates/{key}", put(save_template).delete(reset_template))
}

fn effective_json(c: &Config) -> Value {
    json!({
        "mail_mode": c.mail_mode.as_str(),
        "mail_sender": c.mail_sender,
        "mail_test_recipient": c.mail_test_recipient,
        "mail_allowed_domains": c.mail_allowed_domains,
        "mail_tenant_id": c.mail_graph.as_ref().map(|g| g.tenant_id.clone()),
        "mail_client_id": c.mail_graph.as_ref().map(|g| g.client_id.clone()),
        "archive_mode": c.archive_mode.as_str(),
        "archive_tenant_id": c.archive_graph.as_ref().map(|g| g.tenant_id.clone()),
        "archive_client_id": c.archive_graph.as_ref().map(|g| g.client_id.clone()),
        "archive_site_id": c.archive_target.as_ref().map(|t| t.site_id.clone()),
        "archive_drive_id": c.archive_target.as_ref().map(|t| t.drive_id.clone()),
        "archive_root": c.archive_target.as_ref().map(|t| t.root.clone()),
        "alert_days": c.alert_days,
        "alert_km": c.alert_km,
        "escalation_minutes": c.escalation_minutes,
        "modules_desactives": c.disabled_modules.iter().map(|m| m.as_str()).collect::<Vec<_>>(),
    })
}

fn certificate_json(base: &Config, identity: Identity) -> Value {
    let (crt, key) = identity.generated_paths(&base.data_dir);
    let (source, path) = if crt.is_file() && key.is_file() {
        ("interface", Some(crt))
    } else {
        match identity.cert_paths(base) {
            Some((c, _)) => ("env", Some(c)),
            None => ("aucun", None),
        }
    };
    let details = path
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|pem| crate::certs::describe(&pem).ok())
        .unwrap_or(Value::Null);
    json!({ "source": source, "details": details })
}

fn tool_ok(bin: &str) -> bool {
    std::process::Command::new(bin).arg("--version").output().map(|o| o.status.success()).unwrap_or(false)
}

async fn saved_data(state: &AppState) -> AppResult<(Value, i32, DateTime<Utc>, Option<String>)> {
    let row: (Value, i32, DateTime<Utc>, Option<String>) = sqlx::query_as(
        "SELECT s.data, s.version, s.updated_at, u.display_name FROM runtime_settings s LEFT JOIN users u ON u.id = s.updated_by WHERE s.id",
    )
    .fetch_one(&state.pool)
    .await?;
    Ok(row)
}

async fn overview(State(state): State<AppState>, user: CurrentUser) -> AppResult<Json<Value>> {
    user.require(&[Role::Admin])?;
    let _ = state.refresh().await;
    let (data, version, updated_at, updated_by) = saved_data(&state).await?;
    let cfg = state.cfg();
    let base = state.base.clone();
    let (gpg_ok, pg_ok) = {
        let (g, p) = (cfg.gpg_bin.clone(), cfg.pg_dump_bin.clone());
        tokio::task::spawn_blocking(move || (tool_ok(&g), tool_ok(&p))).await.unwrap_or((false, false))
    };
    let last: Option<(String, Option<DateTime<Utc>>, Option<String>)> = sqlx::query_as(
        "SELECT status, finished_at, error FROM backup_runs ORDER BY requested_at DESC LIMIT 1",
    )
    .fetch_optional(&state.pool)
    .await?;
    let last_success: Option<DateTime<Utc>> =
        sqlx::query_scalar("SELECT max(finished_at) FROM backup_runs WHERE status = 'reussie'").fetch_one(&state.pool).await?;
    let backup = cfg.backup.as_ref();
    Ok(Json(json!({
        "effective": effective_json(&cfg),
        "env": effective_json(&base),
        "saved": data,
        "version": version,
        "updated_at": updated_at,
        "updated_by": updated_by,
        "overridden": cfg.overridden,
        "problem": state.settings_problem(),
        "sections": SECTIONS,
        "modules": Module::ALL.iter().map(|m| json!({ "name": m.as_str(), "label": m.label() })).collect::<Vec<_>>(),
        "certificates": {
            "envoi": certificate_json(&base, Identity::Envoi),
            "archive": certificate_json(&base, Identity::Archive),
        },
        "backup": {
            "configured": backup.is_some(),
            "fingerprint": backup.map(|b| b.fingerprint.clone()),
            "uid": backup.map(|b| b.uid.clone()),
            "hour": backup.map(|b| b.hour).unwrap_or(2),
            "keep": backup.map(|b| b.keep).unwrap_or(14),
            "gpg_ok": gpg_ok,
            "pg_dump_ok": pg_ok,
            "caddy_mounted": cfg.caddy_data_dir.as_ref().map(|d| d.is_dir()).unwrap_or(false),
            "last_status": last.as_ref().map(|l| l.0.clone()),
            "last_error": last.as_ref().and_then(|l| l.2.clone()),
            "last_success_at": last_success,
        },
        "public_url": cfg.public_url,
        "app_version": crate::VERSION,
    })))
}

fn text(v: &Value, key: &str) -> Option<String> {
    v.get(key).and_then(|x| x.as_str()).map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

fn email_like(v: &str) -> bool {
    let mut parts = v.split('@');
    matches!((parts.next(), parts.next(), parts.next()), (Some(a), Some(b), None) if !a.is_empty() && b.contains('.') && !v.contains(' '))
}

/// Normalise et contrôle une section avant enregistrement.
async fn clean_section(state: &AppState, section: &str, input: &Value, previous: Option<&Value>) -> AppResult<Value> {
    let base = &state.base;
    match section {
        "modules" => {
            let list = settings::parse_modules(input).map_err(|e| AppError::invalid("desactives", e.to_string()))?;
            Ok(json!({ "desactives": list.iter().map(|m| m.as_str()).collect::<Vec<_>>() }))
        }
        "mail" => {
            let mode = text(input, "mode").unwrap_or_else(|| "journal".into());
            if !["journal", "graph"].contains(&mode.as_str()) {
                return Err(AppError::invalid("mode", "mode d'envoi inconnu (journal ou graph)"));
            }
            for key in ["sender", "test_recipient"] {
                if let Some(v) = text(input, key) {
                    if !email_like(&v) {
                        return Err(AppError::invalid(key, "adresse email invalide"));
                    }
                }
            }
            let out = json!({
                "mode": mode,
                "sender": text(input, "sender"),
                "test_recipient": text(input, "test_recipient"),
                "allowed_domains": settings::parse_domains(input),
                "tenant_id": text(input, "tenant_id"),
                "client_id": text(input, "client_id"),
            });
            if mode == "graph" {
                let (g, _) = settings::mail_identity(base, &out).map_err(|e| AppError::invalid("mode", format!("envoi impossible : {e}")))?;
                GraphClient::new(g).map_err(|e| AppError::invalid("mode", format!("identité d'envoi inutilisable : {e}")))?;
            }
            Ok(out)
        }
        "archive" => {
            let mode = text(input, "mode").unwrap_or_else(|| "desactive".into());
            if !["desactive", "sharepoint"].contains(&mode.as_str()) {
                return Err(AppError::invalid("mode", "mode d'archivage inconnu (desactive ou sharepoint)"));
            }
            let out = json!({
                "mode": mode,
                "tenant_id": text(input, "tenant_id"),
                "client_id": text(input, "client_id"),
                "site_id": text(input, "site_id"),
                "drive_id": text(input, "drive_id"),
                "root": text(input, "root").unwrap_or_else(|| "Celluloplast".into()),
            });
            if mode == "sharepoint" {
                let (g, _) = settings::archive_identity(base, &out).map_err(|e| AppError::invalid("mode", format!("archivage impossible : {e}")))?;
                GraphClient::new(g).map_err(|e| AppError::invalid("mode", format!("identité d'archive inutilisable : {e}")))?;
            }
            Ok(out)
        }
        "alerts" => {
            let get = |k: &str, min: i64, max: i64| -> AppResult<i64> {
                let v = input.get(k).and_then(|x| x.as_i64()).ok_or_else(|| AppError::invalid(k, "nombre entier attendu"))?;
                if !(min..=max).contains(&v) {
                    return Err(AppError::invalid(k, format!("valeur entre {min} et {max}")));
                }
                Ok(v)
            };
            Ok(json!({
                "alert_days": get("alert_days", 1, 365)?,
                "alert_km": get("alert_km", 0, 100_000)?,
                "escalation_minutes": get("escalation_minutes", 5, 1440)?,
            }))
        }
        "backup" => {
            let hour = input.get("hour").and_then(|x| x.as_i64()).unwrap_or(2);
            let keep = input.get("keep").and_then(|x| x.as_i64()).unwrap_or(14);
            if !(0..=23).contains(&hour) {
                return Err(AppError::invalid("hour", "heure entre 0 et 23"));
            }
            if !(1..=90).contains(&keep) {
                return Err(AppError::invalid("keep", "entre 1 et 90 sauvegardes conservées"));
            }
            let (key, fpr, uid) = match text(input, "public_key") {
                Some(k) => {
                    let cfg = state.cfg();
                    let k2 = k.clone();
                    let (fpr, uid) = tokio::task::spawn_blocking(move || crate::backup::inspect_public_key(&cfg, &k2))
                        .await
                        .map_err(anyhow::Error::from)?
                        .map_err(|e| AppError::invalid("public_key", e.to_string()))?;
                    (k, fpr, uid)
                }
                None => match previous {
                    Some(p) if text(p, "public_key").is_some() => (
                        text(p, "public_key").unwrap_or_default(),
                        text(p, "fingerprint").unwrap_or_default(),
                        text(p, "uid").unwrap_or_default(),
                    ),
                    _ => return Err(AppError::invalid("public_key", "collez la clé publique de récupération")),
                },
            };
            Ok(json!({ "public_key": key, "fingerprint": fpr, "uid": uid, "hour": hour, "keep": keep }))
        }
        _ => Err(AppError::not_found("section")),
    }
}

/// Aperçu non secret d'une section pour le journal d'audit.
fn audit_view(section: &str, v: &Value) -> Value {
    let mut v = v.clone();
    if section == "backup" {
        if let Some(o) = v.as_object_mut() {
            o.remove("public_key");
        }
    }
    v
}

async fn store(state: &AppState, user: &CurrentUser, section: &str, value: Option<Value>) -> AppResult<Json<Value>> {
    let mut tx = state.pool.begin().await?;
    let (mut data,): (Value,) = sqlx::query_as("SELECT data FROM runtime_settings WHERE id FOR UPDATE").fetch_one(&mut *tx).await?;
    if !data.is_object() {
        data = json!({});
    }
    let obj = data.as_object_mut().expect("objet");
    match &value {
        Some(v) => {
            obj.insert(section.to_string(), v.clone());
        }
        None => {
            obj.remove(section);
        }
    }
    // L'ensemble doit rester applicable (sinon rien n'est enregistré).
    settings::effective(&state.base, &data).map_err(|e| AppError::invalid(section, e.to_string()))?;
    sqlx::query("UPDATE runtime_settings SET data = $1, version = version + 1, updated_at = now(), updated_by = $2 WHERE id")
        .bind(&data)
        .bind(user.id)
        .execute(&mut *tx)
        .await?;
    audit::record(
        &mut tx,
        user,
        Audit {
            action: if value.is_some() { "parametres_modifies" } else { "parametres_retour_env" },
            entity: "runtime_settings",
            data: Some(json!({ "section": section, "valeur": value.as_ref().map(|v| audit_view(section, v)) })),
            ..Default::default()
        },
    )
    .await?;
    tx.commit().await?;
    state.refresh().await?;
    Ok(Json(json!({ "section": section, "saved": value.is_some(), "problem": state.settings_problem() })))
}

async fn save_section(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(section): Path<String>,
    Json(input): Json<Value>,
) -> AppResult<Json<Value>> {
    user.require(&[Role::Admin])?;
    if !SECTIONS.contains(&section.as_str()) {
        return Err(AppError::not_found("section"));
    }
    let (data, ..) = saved_data(&state).await?;
    let value = clean_section(&state, &section, &input, data.get(&section)).await?;
    store(&state, &user, &section, Some(value)).await
}

async fn reset_section(State(state): State<AppState>, user: CurrentUser, Path(section): Path<String>) -> AppResult<Json<Value>> {
    user.require(&[Role::Admin])?;
    if !SECTIONS.contains(&section.as_str()) {
        return Err(AppError::not_found("section"));
    }
    store(&state, &user, &section, None).await
}

// ------------------------------------------------------------------ certificats

#[derive(Deserialize)]
struct GenerateInput {
    #[serde(default)]
    replace: bool,
}

async fn generate_certificate(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(identity): Path<String>,
    Json(input): Json<GenerateInput>,
) -> AppResult<Json<Value>> {
    user.require(&[Role::Admin])?;
    let identity = Identity::parse(&identity).ok_or_else(|| AppError::not_found("identité"))?;
    let (crt, key) = identity.generated_paths(&state.base.data_dir);
    if crt.is_file() && !input.replace {
        return Err(AppError::conflict(
            "un certificat existe déjà : confirmez son remplacement (l'ancien devra être retiré de l'application Entra)",
        ));
    }
    let cn = format!("celluloplast-{}", identity.as_str());
    let (crt2, key2) = (crt.clone(), key.clone());
    tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
        let (cert_pem, key_pem) = crate::certs::generate(&cn, 730)?;
        crate::certs::write_pair(&crt2, &key2, &cert_pem, &key_pem)
    })
    .await
    .map_err(anyhow::Error::from)??;
    let mut tx = state.pool.begin().await?;
    audit::record(
        &mut tx,
        &user,
        Audit { action: "certificat_genere", entity: "graph_identity", data: Some(json!({ "identite": identity.as_str(), "remplace": input.replace })), ..Default::default() },
    )
    .await?;
    // Les clients Graph relisent la clé au prochain rafraîchissement.
    sqlx::query("UPDATE runtime_settings SET updated_at = now() WHERE id").execute(&mut *tx).await?;
    tx.commit().await?;
    state.refresh().await?;
    Ok(Json(certificate_json(&state.base, identity)))
}

async fn download_certificate(State(state): State<AppState>, user: CurrentUser, Path(identity): Path<String>) -> AppResult<Response> {
    user.require(&[Role::Admin])?;
    let identity = Identity::parse(identity.trim_end_matches(".crt")).ok_or_else(|| AppError::not_found("identité"))?;
    let (crt, _) = identity.cert_paths(&state.base).ok_or_else(|| AppError::not_found("certificat"))?;
    let pem = tokio::fs::read(&crt).await.map_err(|_| AppError::not_found("certificat"))?;
    Ok((
        [
            (header::CONTENT_TYPE, "application/x-x509-ca-cert".to_string()),
            (header::CONTENT_DISPOSITION, format!("attachment; filename=\"celluloplast-{}.crt\"", identity.as_str())),
        ],
        Body::from(pem),
    )
        .into_response())
}

// ------------------------------------------------------------------ essais de connexion

#[derive(Deserialize)]
struct MailTestInput {
    to: String,
}

async fn mail_test(State(state): State<AppState>, user: CurrentUser, Json(input): Json<MailTestInput>) -> AppResult<Json<Value>> {
    user.require(&[Role::Admin])?;
    let to = input.to.trim().to_string();
    if !email_like(&to) {
        return Err(AppError::invalid("to", "adresse email invalide"));
    }
    let (data, ..) = saved_data(&state).await?;
    let cfg = state.cfg();
    let (graph, sender) = match data.get("mail").filter(|v| v.is_object()) {
        Some(m) => settings::mail_identity(&state.base, m).map_err(|e| AppError::invalid("mail", e.to_string()))?,
        None => match (&cfg.mail_graph, &cfg.mail_sender) {
            (Some(g), Some(s)) => (g.clone(), s.clone()),
            _ => return Err(AppError::invalid("mail", "renseignez et enregistrez d'abord l'identité d'envoi")),
        },
    };
    let domains = match data.get("mail") {
        Some(m) => settings::parse_domains(m),
        None => cfg.mail_allowed_domains.clone(),
    };
    let domain = to.rsplit('@').next().unwrap_or_default().to_lowercase();
    if !domains.is_empty() && !domains.contains(&domain) {
        return Err(AppError::invalid("to", format!("domaine hors des domaines autorisés ({})", domains.join(", "))));
    }
    let client = GraphClient::new(graph).map_err(|e| AppError::invalid("mail", e.to_string()))?;
    let body = "Message de test envoyé depuis Administration → Paramètres de l'application Celluloplast | Parcours salarié.\n\
                Aucune action n'est attendue.\n";
    let outcome = client.send_mail(&sender, &to, "Celluloplast — test d'envoi", body).await;
    let (status, detail) = match outcome {
        SendOutcome::Accepted => ("accepte", "Accepté par Exchange (202) : vérifiez la réception dans la boîte du destinataire.".to_string()),
        SendOutcome::Retry { reason, .. } => ("echec", reason),
        SendOutcome::Failed(r) => ("echec", r),
        SendOutcome::Unknown(r) => ("inconnu", r),
    };
    let mut tx = state.pool.begin().await?;
    audit::record(
        &mut tx,
        &user,
        Audit { action: "test_envoi", entity: "graph_identity", data: Some(json!({ "destinataire": to, "resultat": status, "detail": detail })), ..Default::default() },
    )
    .await?;
    tx.commit().await?;
    Ok(Json(json!({ "status": status, "detail": detail })))
}

async fn archive_test(State(state): State<AppState>, user: CurrentUser) -> AppResult<Json<Value>> {
    user.require(&[Role::Admin])?;
    let (data, ..) = saved_data(&state).await?;
    let cfg = state.cfg();
    let (graph, target) = match data.get("archive").filter(|v| v.is_object()) {
        Some(a) => settings::archive_identity(&state.base, a).map_err(|e| AppError::invalid("archive", e.to_string()))?,
        None => match (&cfg.archive_graph, &cfg.archive_target) {
            (Some(g), Some(t)) => (g.clone(), t.clone()),
            _ => return Err(AppError::invalid("archive", "renseignez et enregistrez d'abord l'identité d'archive")),
        },
    };
    let client = GraphClient::new(graph).map_err(|e| AppError::invalid("archive", e.to_string()))?;
    let stamp = Utc::now().format("%Y%m%d-%H%M%S");
    let path = format!("{}/_test-connexion/test-{stamp}.txt", target.root.trim_matches('/'));
    let result = client.upload(&target.site_id, &target.drive_id, &path, b"Test de connexion Celluloplast | Parcours salarie\n").await;
    let (status, detail) = match &result {
        Ok((id, size)) => ("accepte", format!("Fichier déposé ({size} octets, identifiant {id}) : {path}")),
        Err(e) => ("echec", e.clone()),
    };
    let mut tx = state.pool.begin().await?;
    audit::record(
        &mut tx,
        &user,
        Audit { action: "test_archive", entity: "graph_identity", data: Some(json!({ "resultat": status, "detail": detail })), ..Default::default() },
    )
    .await?;
    tx.commit().await?;
    Ok(Json(json!({ "status": status, "detail": detail })))
}

// ------------------------------------------------------------------ sauvegardes

async fn list_backups(State(state): State<AppState>, user: CurrentUser) -> AppResult<Json<Value>> {
    user.require(&[Role::Admin])?;
    let v: Value = sqlx::query_scalar(
        "SELECT coalesce(jsonb_agg(jsonb_build_object('id', b.id, 'trigger', b.trigger, 'status', b.status, 'stamp', b.stamp,
                    'files', b.files, 'warnings', b.warnings, 'error', b.error, 'fingerprint', b.fingerprint,
                    'requested_by', u.display_name, 'requested_at', b.requested_at, 'started_at', b.started_at,
                    'finished_at', b.finished_at, 'purged_at', b.purged_at) ORDER BY b.requested_at DESC), '[]')
           FROM (SELECT * FROM backup_runs ORDER BY requested_at DESC LIMIT 60) b LEFT JOIN users u ON u.id = b.requested_by",
    )
    .fetch_one(&state.pool)
    .await?;
    Ok(Json(v))
}

async fn request_backup(State(state): State<AppState>, user: CurrentUser) -> AppResult<Json<Value>> {
    user.require(&[Role::Admin])?;
    if state.cfg().backup.is_none() {
        return Err(AppError::invalid("public_key", "configurez d'abord la clé publique de récupération"));
    }
    let mut tx = state.pool.begin().await?;
    let id: Uuid = sqlx::query_scalar("INSERT INTO backup_runs (trigger, requested_by) VALUES ('manuel', $1) RETURNING id")
        .bind(user.id)
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| match &e {
            sqlx::Error::Database(d) if d.code().as_deref() == Some("23505") => AppError::conflict("une sauvegarde est déjà demandée ou en cours"),
            _ => AppError::from(e),
        })?;
    audit::record(&mut tx, &user, Audit { action: "sauvegarde_demandee", entity: "backup_run", entity_id: Some(id), ..Default::default() }).await?;
    tx.commit().await?;
    Ok(Json(json!({ "id": id, "status": "demande" })))
}

async fn download_backup(State(state): State<AppState>, user: CurrentUser, Path((id, name)): Path<(Uuid, String)>) -> AppResult<Response> {
    user.require(&[Role::Admin])?;
    let files: Option<(Value, Option<DateTime<Utc>>)> =
        sqlx::query_as("SELECT files, purged_at FROM backup_runs WHERE id = $1 AND status = 'reussie'").bind(id).fetch_optional(&state.pool).await?;
    let (files, purged) = files.ok_or_else(|| AppError::not_found("sauvegarde"))?;
    if purged.is_some() {
        return Err(AppError::not_found("sauvegarde (supprimée du serveur par la rotation)"));
    }
    let listed = files.as_array().map(|a| a.iter().any(|f| f["name"].as_str() == Some(name.as_str()))).unwrap_or(false);
    if !listed || !crate::backup::is_backup_file(&name) {
        return Err(AppError::not_found("fichier"));
    }
    let path = state.cfg().backup_dir.join(&name);
    let file = tokio::fs::File::open(&path).await.map_err(|_| AppError::not_found("fichier"))?;
    let len = file.metadata().await.map(|m| m.len()).unwrap_or(0);
    let mut tx = state.pool.begin().await?;
    audit::record(
        &mut tx,
        &user,
        Audit { action: "sauvegarde_telechargee", entity: "backup_run", entity_id: Some(id), data: Some(json!({ "fichier": name })), ..Default::default() },
    )
    .await?;
    tx.commit().await?;
    let stream = tokio_util_stream(file);
    let mime = if name.ends_with(".txt") { "text/plain; charset=utf-8" } else { "application/octet-stream" };
    Ok((
        [
            (header::CONTENT_TYPE, mime.to_string()),
            (header::CONTENT_LENGTH, len.to_string()),
            (header::CONTENT_DISPOSITION, format!("attachment; filename=\"{name}\"")),
        ],
        stream,
    )
        .into_response())
}

/// Corps de réponse lu par morceaux (les sauvegardes peuvent être grosses).
fn tokio_util_stream(file: tokio::fs::File) -> Body {
    Body::from_stream(tokio_util::io::ReaderStream::with_capacity(file, 256 * 1024))
}

// ------------------------------------------------------------------ modèles de tâches

/// Types de tâches paramétrables : un par besoin produisant du travail,
/// plus les sessions de formation.
fn template_keys() -> Vec<(String, String, String)> {
    let mut out: Vec<(String, String, String)> = domain::needs::NeedCategory::ALL
        .iter()
        .filter(|c| **c != domain::needs::NeedCategory::Formation)
        .map(|c| (c.task_key(), c.task_title().to_string(), c.responsible_role().as_str().to_string()))
        .collect();
    out.push(("formation".into(), "Formation : <intitulé>".into(), Role::Formation.as_str().to_string()));
    out
}

async fn list_templates(State(state): State<AppState>, user: CurrentUser) -> AppResult<Json<Value>> {
    user.require(&[Role::Admin, Role::Rh, Role::It, Role::Formation, Role::Graphiste])?;
    let rows: Vec<(String, Option<String>, Option<String>, Option<i32>, i32, DateTime<Utc>)> =
        sqlx::query_as("SELECT task_key, title, instructions, delay_days, display_order, updated_at FROM task_templates").fetch_all(&state.pool).await?;
    let out: Vec<Value> = template_keys()
        .into_iter()
        .map(|(key, default_title, role)| {
            let t = rows.iter().find(|r| r.0 == key);
            json!({
                "task_key": key,
                "default_title": default_title,
                "role": role,
                "title": t.and_then(|r| r.1.clone()),
                "instructions": t.and_then(|r| r.2.clone()),
                "delay_days": t.and_then(|r| r.3),
                "display_order": t.map(|r| r.4).unwrap_or(100),
                "customized": t.is_some(),
                "updated_at": t.map(|r| r.5),
            })
        })
        .collect();
    Ok(Json(json!(out)))
}

#[derive(Deserialize)]
struct TemplateInput {
    title: Option<String>,
    instructions: Option<String>,
    delay_days: Option<i32>,
    display_order: Option<i32>,
}

async fn save_template(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(key): Path<String>,
    Json(input): Json<TemplateInput>,
) -> AppResult<Json<Value>> {
    user.require(&[Role::Admin])?;
    if !template_keys().iter().any(|(k, ..)| *k == key) {
        return Err(AppError::not_found("type de tâche"));
    }
    if let Some(d) = input.delay_days {
        if !(-365..=365).contains(&d) {
            return Err(AppError::invalid("delay_days", "délai entre -365 et 365 jours"));
        }
    }
    let title = input.title.map(|t| t.trim().to_string()).filter(|t| !t.is_empty());
    if title.as_ref().map(|t| t.chars().count() > 120).unwrap_or(false) {
        return Err(AppError::invalid("title", "120 caractères au plus"));
    }
    let instructions = input.instructions.map(|t| t.trim().to_string()).filter(|t| !t.is_empty());
    if instructions.as_ref().map(|t| t.chars().count() > 4000).unwrap_or(false) {
        return Err(AppError::invalid("instructions", "4000 caractères au plus"));
    }
    let order = input.display_order.unwrap_or(100).clamp(0, 9999);
    let mut tx = state.pool.begin().await?;
    sqlx::query(
        "INSERT INTO task_templates (task_key, title, instructions, delay_days, display_order, updated_by)
         VALUES ($1, $2, $3, $4, $5, $6)
         ON CONFLICT (task_key) DO UPDATE SET title = $2, instructions = $3, delay_days = $4, display_order = $5,
                updated_by = $6, updated_at = now()",
    )
    .bind(&key)
    .bind(&title)
    .bind(&instructions)
    .bind(input.delay_days)
    .bind(order)
    .bind(user.id)
    .execute(&mut *tx)
    .await?;
    audit::record(
        &mut tx,
        &user,
        Audit {
            action: "modele_tache_modifie",
            entity: "task_template",
            data: Some(json!({ "type": key, "titre": title, "delai_jours": input.delay_days, "ordre": order })),
            ..Default::default()
        },
    )
    .await?;
    tx.commit().await?;
    Ok(Json(json!({ "task_key": key })))
}

async fn reset_template(State(state): State<AppState>, user: CurrentUser, Path(key): Path<String>) -> AppResult<Json<Value>> {
    user.require(&[Role::Admin])?;
    let mut tx = state.pool.begin().await?;
    sqlx::query("DELETE FROM task_templates WHERE task_key = $1").bind(&key).execute(&mut *tx).await?;
    audit::record(&mut tx, &user, Audit { action: "modele_tache_retabli", entity: "task_template", data: Some(json!({ "type": key })), ..Default::default() }).await?;
    tx.commit().await?;
    Ok(Json(json!({ "task_key": key })))
}
