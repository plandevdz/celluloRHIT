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
