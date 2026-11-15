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
    expand_outbox(state).await?;
    deliver(state).await?;
    generate_documents(state).await?;
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
    for (id, _sent_to) in due {
        match state.config.mail_mode {
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
    let items = crate::routes::vehicles::deadlines_json(&mut conn, today, state.config.alert_days, state.config.alert_km)
        .await
        .map_err(|e| anyhow::anyhow!("échéances véhicules : {e}"))?;
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
    let renewals = crate::routes::software::renewals_json(&mut conn, today, state.config.alert_days)
        .await
        .map_err(|e| anyhow::anyhow!("échéances logiciels : {e}"))?;
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
