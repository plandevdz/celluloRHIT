//! Worker indépendant de l'API : résout les destinataires des avis,
//! remet les notifications (mode journal en V1) et génère les PDF en
//! rattrapage. Baux SQL : un worker redémarré reprend le travail abandonné.

use std::time::Duration;

use uuid::Uuid;

use crate::config::MailMode;
use crate::documents;
use crate::state::AppState;

const TICK: Duration = Duration::from_secs(3);

pub async fn run(state: AppState) -> anyhow::Result<()> {
    tracing::info!(mode = ?state.config.mail_mode, "worker démarré");
    loop {
        if let Err(e) = tick(&state).await {
            tracing::error!(error = ?e, "cycle du worker en échec");
        }
        tokio::select! {
            _ = tokio::time::sleep(TICK) => {}
            _ = tokio::signal::ctrl_c() => {
                tracing::info!("arrêt du worker");
                return Ok(());
            }
        }
    }
}

pub async fn tick(state: &AppState) -> anyhow::Result<()> {
    // Les échéances passent avant la distribution : un avis créé dans ce
    // cycle est remis dans le même cycle.
    if let Err(e) = check_deadlines(state).await {
        tracing::error!(error = ?e, "contrôle des échéances en échec");
    }
    if let Err(e) = process_transitions(state).await {
        tracing::error!(error = ?e, "mutations ou départs à échéance en échec");
    }
    expand_outbox(state).await?;
    deliver(state).await?;
    generate_documents(state).await?;
    if let Err(e) = archive_documents(state).await {
        tracing::error!(error = ?e, "archivage en échec");
    }
    Ok(())
}

/// Transforme chaque avis en remises individuelles, dédupliquées par adresse.
pub async fn expand_outbox(state: &AppState) -> anyhow::Result<()> {
    let ids: Vec<Uuid> = sqlx::query_scalar(
        "WITH picked AS (
            SELECT id FROM outbox
             WHERE status = 'en_attente' AND (lease_until IS NULL OR lease_until < now())
             ORDER BY created_at LIMIT 20 FOR UPDATE SKIP LOCKED)
         UPDATE outbox o SET lease_until = now() + interval '2 minutes', attempts = o.attempts + 1
           FROM picked WHERE o.id = picked.id RETURNING o.id",
    )
    .fetch_all(&state.pool)
    .await?;
    for id in ids {
        let mut tx = state.pool.begin().await?;
        let recipients: Vec<String> = sqlx::query_scalar(
            "SELECT DISTINCT lower(u.email) FROM users u, outbox o
              WHERE o.id = $1 AND u.active
                AND (u.id = ANY(o.target_user_ids)
                     OR EXISTS (SELECT 1 FROM user_roles r WHERE r.user_id = u.id AND r.role = ANY(o.target_roles)))",
        )
        .bind(id)
        .fetch_all(&mut *tx)
        .await?;
        // Équipes visées sans aucun utilisateur actif : anomalie de routage.
        let missing: Vec<String> = sqlx::query_scalar(
            "SELECT r FROM outbox o, unnest(o.target_roles) r
              WHERE o.id = $1
                AND NOT EXISTS (SELECT 1 FROM user_roles ur JOIN users u ON u.id = ur.user_id
                                 WHERE ur.role = r AND u.active)",
        )
        .bind(id)
        .fetch_all(&mut *tx)
        .await?;
        sqlx::query("UPDATE outbox SET missing_roles = $2 WHERE id = $1")
            .bind(id)
            .bind(&missing)
            .execute(&mut *tx)
            .await?;
        if !missing.is_empty() {
            tracing::warn!(outbox = %id, roles = ?missing, "équipe sans destinataire actif");
        }
        if recipients.is_empty() {
            sqlx::query(
                "UPDATE outbox SET status = 'erreur', last_error = 'aucun destinataire actif : routage à corriger', lease_until = NULL WHERE id = $1",
            )
            .bind(id)
            .execute(&mut *tx)
            .await?;
            tracing::warn!(outbox = %id, "avis sans destinataire actif");
        } else {
            for r in &recipients {
                let sent_to = state.config.mail_test_recipient.clone().unwrap_or_else(|| r.clone());
                sqlx::query(
                    "INSERT INTO deliveries (outbox_id, recipient_email, sent_to) VALUES ($1, $2, $3)
                     ON CONFLICT (outbox_id, recipient_email) DO NOTHING",
                )
                .bind(id)
                .bind(r)
                .bind(sent_to)
                .execute(&mut *tx)
                .await?;
            }
            sqlx::query("UPDATE outbox SET status = 'traite', processed_at = now(), lease_until = NULL WHERE id = $1")
                .bind(id)
                .execute(&mut *tx)
                .await?;
        }
        tx.commit().await?;
    }
    Ok(())
}

pub async fn deliver(state: &AppState) -> anyhow::Result<()> {
    match state.config.mail_mode {
        // Période en mode journal (tests, pilote) : tout avis créé pendant
        // cette période est un avis de test. Une activation antérieure de
        // l'envoi est oubliée : la prochaine sera datée à nouveau.
        MailMode::Journal => {
            sqlx::query(
                "INSERT INTO app_settings (key, at) VALUES ('journal_jusqua', now())
                 ON CONFLICT (key) DO UPDATE SET at = now(), updated_at = now()",
            )
            .execute(&state.pool)
            .await?;
            sqlx::query("DELETE FROM app_settings WHERE key = 'envoi_depuis'").execute(&state.pool).await?;
        }
        // Activation de l'envoi après une période en mode journal : son
        // instant est fixé au premier passage, et aucun avis créé avant ne
        // part, même s'il était encore en attente (pas de rattrapage).
        MailMode::Graph => {
            sqlx::query(
                "INSERT INTO app_settings (key, at)
                 SELECT 'envoi_depuis', now() WHERE EXISTS (SELECT 1 FROM app_settings WHERE key = 'journal_jusqua')
                 ON CONFLICT (key) DO NOTHING",
            )
            .execute(&state.pool)
            .await?;
            let n = sqlx::query(
                "UPDATE deliveries d SET status = 'journalise', last_attempt_at = now(), lease_until = NULL,
                        last_result = 'avis antérieur à l''activation de l''envoi (mode journal) : consigné, jamais envoyé'
                   FROM outbox o, app_settings s
                  WHERE o.id = d.outbox_id AND s.key = 'envoi_depuis'
                    AND d.status = 'en_attente' AND o.created_at < s.at",
            )
            .execute(&state.pool)
            .await?
            .rows_affected();
            if n > 0 {
                tracing::warn!(avis = n, "avis antérieurs à l'activation de l'envoi : consignés, non envoyés");
            }
        }
    }
    let due: Vec<(Uuid, String)> = sqlx::query_as(
        "WITH picked AS (
            SELECT id FROM deliveries
             WHERE status = 'en_attente' AND next_attempt_at <= now() AND (lease_until IS NULL OR lease_until < now())
             ORDER BY next_attempt_at LIMIT 50 FOR UPDATE SKIP LOCKED)
         UPDATE deliveries d SET lease_until = now() + interval '2 minutes'
           FROM picked WHERE d.id = picked.id RETURNING d.id, d.sent_to",
    )
    .fetch_all(&state.pool)
    .await?;
    for (id, sent_to) in due {
        match state.config.mail_mode {
            MailMode::Graph => deliver_graph(state, id, &sent_to).await?,
            MailMode::Journal => {
                sqlx::query(
                    "UPDATE deliveries SET status = 'journalise', attempts = attempts + 1, last_attempt_at = now(),
                            last_result = 'mode journal : consigné, aucun mail envoyé', lease_until = NULL
                      WHERE id = $1",
                )
                .bind(id)
                .execute(&state.pool)
                .await?;
                // Pas de contenu RH dans les journaux : identifiant seulement.
                tracing::info!(delivery = %id, "avis journalisé");
            }
        }
    }
    Ok(())
}

const MAX_SEND_ATTEMPTS: i32 = 10;

/// Envoi réel par Microsoft Graph. 202 = accepté par Microsoft (pas une
/// preuve de livraison) ; panne transitoire ou limitation = nouvel essai
/// (Retry-After respecté, sinon délai croissant) ; refus de droits ou de
/// destinataire = échec visible en Administration ; délai dépassé après
/// soumission = « à vérifier », jamais réexpédié automatiquement.
async fn deliver_graph(state: &AppState, id: Uuid, sent_to: &str) -> anyhow::Result<()> {
    let row: Option<(String, String, Option<String>, i32)> = sqlx::query_as(
        "SELECT o.subject, o.body, o.link_path, d.attempts FROM deliveries d JOIN outbox o ON o.id = d.outbox_id WHERE d.id = $1",
    )
    .bind(id)
    .fetch_optional(&state.pool)
    .await?;
    let Some((subject, body, link, attempts)) = row else { return Ok(()) };
    let finish = |status: &'static str, result: String, next: Option<chrono::DateTime<chrono::Utc>>| {
        let pool = state.pool.clone();
        async move {
            sqlx::query(
                "UPDATE deliveries SET status = $2, attempts = attempts + 1, last_attempt_at = now(), last_result = $3,
                        next_attempt_at = coalesce($4, next_attempt_at), lease_until = NULL
                  WHERE id = $1",
            )
            .bind(id)
            .bind(status)
            .bind(result)
            .bind(next)
            .execute(&pool)
            .await
        }
    };
    if !state.config.recipient_allowed(sent_to) {
        finish("echec", "destinataire hors du périmètre pilote autorisé : aucun envoi".into(), None).await?;
        return Ok(());
    }
    let (Some(client), Some(sender)) = (state.mailer.as_ref(), state.config.mail_sender.as_deref()) else {
        finish("echec", "mode graph sans identité d'envoi configurée".into(), None).await?;
        return Ok(());
    };
    let mut text = body;
    if let Some(l) = link {
        text.push_str(&format!("\nOuvrir dans l'application : {}{}\n", state.config.public_url.trim_end_matches('/'), l));
    }
    text.push_str("\n— Message automatique de l'application Celluloplast | Parcours salarié. Ne pas répondre.\n");
    match client.send_mail(sender, sent_to, &subject, &text).await {
        crate::graph::SendOutcome::Accepted => {
            finish("accepte", "accepté par Exchange (202) — pas une preuve de livraison".into(), None).await?;
            tracing::info!(delivery = %id, "avis accepté par Exchange");
        }
        crate::graph::SendOutcome::Retry { after, reason } => {
            if attempts + 1 >= MAX_SEND_ATTEMPTS {
                finish("echec", format!("{reason} — abandon après {} essais", attempts + 1), None).await?;
            } else {
                let backoff = Duration::from_secs((60u64 << attempts.clamp(0, 6) as u32).min(3600));
                let wait = after.unwrap_or(backoff).max(Duration::from_secs(5));
                let next = chrono::Utc::now() + chrono::Duration::from_std(wait).unwrap_or_else(|_| chrono::Duration::minutes(5));
                finish("en_attente", format!("{reason} — nouvel essai prévu"), Some(next)).await?;
            }
            tracing::warn!(delivery = %id, "envoi différé");
        }
        crate::graph::SendOutcome::Failed(reason) => {
            finish("echec", reason, None).await?;
            tracing::error!(delivery = %id, "envoi refusé par Exchange");
        }
        crate::graph::SendOutcome::Unknown(reason) => {
            finish("a_verifier", reason, None).await?;
            tracing::warn!(delivery = %id, "résultat d'envoi inconnu");
        }
    }
    Ok(())
}

const MAX_ARCHIVE_ATTEMPTS: i32 = 20;

/// Archivage SharePoint des originaux et des scans, repris après une panne
/// ou un redémarrage. La copie locale reste en place ; l'état « archivé »
/// n'est posé qu'après vérification de l'identifiant et de la taille.
pub async fn archive_documents(state: &AppState) -> anyhow::Result<()> {
    let (Some(client), Some(target)) = (state.archiver.as_ref(), state.config.archive_target.as_ref()) else { return Ok(()) };
    let due: Vec<(Uuid, String, i64, i32)> = sqlx::query_as(
        "WITH picked AS (
            SELECT id FROM document_versions
             WHERE archive_status = 'en_attente' AND (archive_next_at IS NULL OR archive_next_at <= now())
             ORDER BY created_at LIMIT 5 FOR UPDATE SKIP LOCKED)
         UPDATE document_versions v SET archive_next_at = now() + interval '10 minutes'
           FROM picked WHERE v.id = picked.id
         RETURNING v.id, v.storage_path, v.size_bytes, v.archive_attempts",
    )
    .fetch_all(&state.pool)
    .await?;
    for (id, path, size, attempts) in due {
        let bytes = match tokio::fs::read(state.config.data_dir.join(&path)).await {
            Ok(b) => b,
            Err(e) => {
                sqlx::query("UPDATE document_versions SET archive_status = 'echec', archive_error = $2 WHERE id = $1")
                    .bind(id)
                    .bind(format!("copie locale illisible : {e}"))
                    .execute(&state.pool)
                    .await?;
                continue;
            }
        };
        let remote = format!("{}/{}", target.root.trim_matches('/'), path);
        match client.upload(&target.site_id, &target.drive_id, &remote, &bytes).await {
            Ok((item, remote_size)) if remote_size == size as u64 || remote_size == 0 && size == 0 => {
                sqlx::query(
                    "UPDATE document_versions SET archive_status = 'archive', archive_id = $2, archived_at = now(), archive_error = NULL,
                            archive_attempts = archive_attempts + 1, archive_next_at = NULL WHERE id = $1",
                )
                .bind(id)
                .bind(item)
                .execute(&state.pool)
                .await?;
                tracing::info!(version = %id, "document archivé");
            }
            other => {
                let reason = match other {
                    Ok((_, s)) => format!("taille distante incohérente ({s} octets au lieu de {size})"),
                    Err(e) => e,
                };
                let failed = attempts + 1 >= MAX_ARCHIVE_ATTEMPTS;
                let wait = chrono::Duration::minutes((5i64 << attempts.clamp(0, 6)).min(720));
                sqlx::query(
                    "UPDATE document_versions SET archive_status = CASE WHEN $3 THEN 'echec' ELSE 'en_attente' END, archive_error = $2,
                            archive_attempts = archive_attempts + 1, archive_next_at = now() + $4 WHERE id = $1",
                )
                .bind(id)
                .bind(&reason)
                .bind(failed)
                .bind(wait)
                .execute(&state.pool)
                .await?;
                tracing::warn!(version = %id, "archivage différé");
            }
        }
    }
    Ok(())
}

/// Mutations à appliquer et départs à échéance. Tout repose sur l'état en
/// base : après un redémarrage, les échéances passées sont traitées.
pub async fn process_transitions(state: &AppState) -> anyhow::Result<()> {
    use crate::config::Module;
    let today = documents::algiers_today();
    // Module fermé (pilote) : aucune mutation appliquée, aucune coupure ni
    // relance de départ déclenchée automatiquement.
    if state.config.module_on(Module::Mutation) {
        crate::routes::transfers::apply_due(&state.pool, today).await?;
    }
    if state.config.module_on(Module::Depart) {
        crate::routes::departures::process_due(&state.pool, today, state.config.escalation_minutes).await?;
    }
    Ok(())
}

/// Avis émis par le serveur lui-même (sans utilisateur), dans l'outbox.
pub async fn enqueue_system(
    conn: &mut sqlx::PgConnection,
    event_type: &str,
    case_id: Option<Uuid>,
    roles: &[&str],
    subject: &str,
    body: &str,
    link_path: Option<&str>,
) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO outbox (event_type, case_id, target_roles, subject, body, link_path, correlation_id)
         VALUES ($1, $2, $3, $4, $5, $6, $7)",
    )
    .bind(event_type)
    .bind(case_id)
    .bind(roles)
    .bind(subject)
    .bind(body)
    .bind(link_path)
    .bind(Uuid::new_v4())
    .execute(conn)
    .await?;
    Ok(())
}

fn fr(d: Option<chrono::NaiveDate>) -> String {
    d.map(|d| d.format("%d/%m/%Y").to_string()).unwrap_or_else(|| "—".into())
}

/// Échéances (pièces et entretien des véhicules, fins de location,
/// abonnements logiciels) : une alerte par élément et par seuil (« proche »
/// puis « dépassée »), jamais répétée, y compris après un redémarrage.
pub async fn check_deadlines(state: &AppState) -> anyhow::Result<()> {
    let today = documents::algiers_today();
    let mut conn = state.pool.acquire().await?;
    // Module fermé (pilote) : aucune alerte d'échéance n'est émise.
    let items = if state.config.module_on(crate::config::Module::Vehicules) {
        crate::routes::vehicles::deadlines_json(&mut conn, today, state.config.alert_days, state.config.alert_km)
            .await
            .map_err(|e| anyhow::anyhow!("échéances véhicules : {e}"))?
    } else {
        serde_json::json!([])
    };
    for it in items.as_array().cloned().unwrap_or_default() {
        let Some(id) = it["id"].as_str().and_then(|s| Uuid::parse_str(s).ok()) else { continue };
        let overdue = it["overdue"].as_bool().unwrap_or(false);
        let due_key = format!(
            "{}:{}:{}",
            if overdue { "depassee" } else { "proche" },
            it["due_date"].as_str().unwrap_or("-"),
            it["due_km"].as_i64().map(|k| k.to_string()).unwrap_or_else(|| "-".into())
        );
        let mut tx = state.pool.begin().await?;
        let fresh = sqlx::query("INSERT INTO alerts_sent (kind, entity_id, due_key) VALUES ('vehicule', $1, $2) ON CONFLICT DO NOTHING")
            .bind(id)
            .bind(&due_key)
            .execute(&mut *tx)
            .await?
            .rows_affected()
            == 1;
        if fresh {
            let plate = it["plate"].as_str().unwrap_or_default();
            let what = match it["kind"].as_str().unwrap_or_default() {
                "assurance" => "Assurance",
                "controle_technique" => "Contrôle technique",
                "carte_grise" => "Carte grise",
                "vignette" => "Vignette",
                "fin_location" => "Fin de location",
                "vidange" => "Vidange",
                "entretien" => "Entretien",
                "pneus" => "Pneus",
                "reparation" => "Réparation",
                _ => "Échéance",
            };
            let due_date = it["due_date"].as_str().and_then(|d| chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d").ok());
            let km = it["due_km"].as_i64().map(|k| format!(" ou {k} km (compteur : {} km)", it["odometer_km"].as_i64().unwrap_or(0))).unwrap_or_default();
            let state_txt = if overdue { "dépassée" } else { "proche" };
            let vehicle = it["vehicle_id"].as_str().unwrap_or_default();
            enqueue_system(
                &mut tx,
                "echeance_vehicule",
                None,
                &["RH"],
                &format!("{what} {state_txt} — véhicule {plate}"),
                &format!("Véhicule {plate} : {what} {state_txt}, échéance le {}{km}.\nRenouvelez la pièce ou consignez l'intervention dans l'application.", fr(due_date)),
                Some(&format!("/vehicules/{vehicle}")),
            )
            .await?;
        }
        tx.commit().await?;
    }
    // Abonnements logiciels : alerte IT avant renouvellement.
    let renewals = if state.config.module_on(crate::config::Module::Logiciels) {
        crate::routes::software::renewals_json(&mut conn, today, state.config.alert_days)
            .await
            .map_err(|e| anyhow::anyhow!("échéances logiciels : {e}"))?
    } else {
        serde_json::json!([])
    };
    for it in renewals.as_array().cloned().unwrap_or_default() {
        let Some(id) = it["id"].as_str().and_then(|s| Uuid::parse_str(s).ok()) else { continue };
        let overdue = it["overdue"].as_bool().unwrap_or(false);
        let end = it["end_date"].as_str().unwrap_or("-").to_string();
        let due_key = format!("{}:{end}", if overdue { "depassee" } else { "proche" });
        let mut tx = state.pool.begin().await?;
        let fresh = sqlx::query("INSERT INTO alerts_sent (kind, entity_id, due_key) VALUES ('logiciel', $1, $2) ON CONFLICT DO NOTHING")
            .bind(id)
            .bind(&due_key)
            .execute(&mut *tx)
            .await?
            .rows_affected()
            == 1;
        if fresh {
            let label = it["label"].as_str().unwrap_or_default();
            let date = chrono::NaiveDate::parse_from_str(&end, "%Y-%m-%d").ok();
            let renew = if it["auto_renew"].as_bool().unwrap_or(false) { " (renouvellement automatique prévu au contrat)" } else { "" };
            enqueue_system(
                &mut tx,
                "echeance_logiciel",
                None,
                &["IT"],
                &format!("{label} — échéance {}", if overdue { "dépassée" } else { "proche" }),
                &format!("Abonnement {label} : échéance le {}{renew}.\nVérifiez le renouvellement et les sièges dans le parc IT.", fr(date)),
                Some("/parc?onglet=logiciels"),
            )
            .await?;
        }
        tx.commit().await?;
    }
    Ok(())
}

pub async fn generate_documents(state: &AppState) -> anyhow::Result<()> {
    let pending: Vec<Uuid> = sqlx::query_scalar(
        "SELECT id FROM documents WHERE status = 'a_generer' AND created_at < now() - interval '5 seconds'
          ORDER BY created_at LIMIT 10",
    )
    .fetch_all(&state.pool)
    .await?;
    for id in pending {
        if let Err(e) = documents::ensure_original(&state.pool, &state.config, id).await {
            tracing::error!(document = %id, error = %e, "génération PDF en échec");
        }
    }
    Ok(())
}
