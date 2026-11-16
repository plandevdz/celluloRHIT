use std::sync::{Arc, RwLock};

use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::PgPool;

use crate::config::Config;
use crate::graph::GraphClient;

/// Configuration effective et clients Graph correspondants.
struct Live {
    /// Horodatage des réglages appliqués (`runtime_settings.updated_at`).
    stamp: Option<DateTime<Utc>>,
    config: Arc<Config>,
    mailer: Option<Arc<GraphClient>>,
    archiver: Option<Arc<GraphClient>>,
    /// Réglages enregistrés mais inapplicables (certificat disparu…) : la
    /// configuration du .env reste en vigueur et l'écran l'indique.
    problem: Option<String>,
}

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    /// Configuration de démarrage (fichier .env).
    pub base: Arc<Config>,
    live: Arc<RwLock<Live>>,
}

fn clients(config: &Config) -> anyhow::Result<(Option<Arc<GraphClient>>, Option<Arc<GraphClient>>)> {
    let mailer = match &config.mail_graph {
        Some(c) => Some(Arc::new(GraphClient::new(c.clone()).map_err(|e| anyhow::anyhow!("identité d'envoi Graph : {e}"))?)),
        None => None,
    };
    let archiver = match &config.archive_graph {
        Some(c) => Some(Arc::new(GraphClient::new(c.clone()).map_err(|e| anyhow::anyhow!("identité d'archive Graph : {e}"))?)),
        None => None,
    };
    Ok((mailer, archiver))
}

impl AppState {
    /// Construit l'état ; une configuration Graph illisible (certificat,
    /// clé) est une erreur de démarrage, jamais un envoi silencieux perdu.
    pub fn try_new(pool: PgPool, config: Config) -> anyhow::Result<Self> {
        let (mailer, archiver) = clients(&config)?;
        let config = Arc::new(config);
        Ok(AppState {
            pool,
            base: config.clone(),
            live: Arc::new(RwLock::new(Live { stamp: None, config, mailer, archiver, problem: None })),
        })
    }

    /// Mode journal et archive désactivée (tests, outils) : aucun client.
    pub fn new(pool: PgPool, config: Config) -> Self {
        let config = Arc::new(config);
        AppState {
            pool,
            base: config.clone(),
            live: Arc::new(RwLock::new(Live { stamp: None, config, mailer: None, archiver: None, problem: None })),
        }
    }

    /// Configuration effective (.env + réglages de l'interface).
    pub fn cfg(&self) -> Arc<Config> {
        self.live.read().expect("verrou de configuration").config.clone()
    }

    pub fn mailer(&self) -> Option<Arc<GraphClient>> {
        self.live.read().expect("verrou de configuration").mailer.clone()
    }

    pub fn archiver(&self) -> Option<Arc<GraphClient>> {
        self.live.read().expect("verrou de configuration").archiver.clone()
    }

    pub fn settings_problem(&self) -> Option<String> {
        self.live.read().expect("verrou de configuration").problem.clone()
    }

    /// Relit les réglages de l'interface s'ils ont changé. Appelé au
    /// démarrage, après chaque enregistrement et à chaque cycle du worker.
    pub async fn refresh(&self) -> anyhow::Result<()> {
        let row: Option<(Value, DateTime<Utc>)> =
            sqlx::query_as("SELECT data, updated_at FROM runtime_settings WHERE id").fetch_optional(&self.pool).await?;
        let Some((data, stamp)) = row else { return Ok(()) };
        if self.live.read().expect("verrou de configuration").stamp == Some(stamp) {
            return Ok(());
        }
        self.apply(&data, stamp);
        Ok(())
    }

    fn apply(&self, data: &Value, stamp: DateTime<Utc>) {
        let built = crate::settings::effective(&self.base, data).and_then(|c| {
            let (m, a) = clients(&c)?;
            Ok((c, m, a))
        });
        let mut live = self.live.write().expect("verrou de configuration");
        live.stamp = Some(stamp);
        match built {
            Ok((config, mailer, archiver)) => {
                live.config = Arc::new(config);
                live.mailer = mailer;
                live.archiver = archiver;
                live.problem = None;
            }
            Err(e) => {
                tracing::error!(error = %e, "réglages de l'interface inapplicables : configuration du .env conservée");
                let (mailer, archiver) = clients(&self.base).unwrap_or((None, None));
                live.config = self.base.clone();
                live.mailer = mailer;
                live.archiver = archiver;
                live.problem = Some(e.to_string());
            }
        }
    }
}
