use std::sync::Arc;

use sqlx::PgPool;

use crate::config::Config;
use crate::graph::GraphClient;

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub config: Arc<Config>,
    /// Client d'envoi (mode graph) ; `None` en mode journal.
    pub mailer: Option<Arc<GraphClient>>,
    /// Client d'archive SharePoint ; `None` si l'archivage est désactivé.
    pub archiver: Option<Arc<GraphClient>>,
}

impl AppState {
    /// Construit l'état ; une configuration Graph illisible (certificat,
    /// clé) est une erreur de démarrage, jamais un envoi silencieux perdu.
    pub fn try_new(pool: PgPool, config: Config) -> anyhow::Result<Self> {
        let mailer = match &config.mail_graph {
            Some(c) => Some(Arc::new(GraphClient::new(c.clone()).map_err(|e| anyhow::anyhow!("identité d'envoi Graph : {e}"))?)),
            None => None,
        };
        let archiver = match &config.archive_graph {
            Some(c) => Some(Arc::new(GraphClient::new(c.clone()).map_err(|e| anyhow::anyhow!("identité d'archive Graph : {e}"))?)),
            None => None,
        };
        Ok(AppState { pool, config: Arc::new(config), mailer, archiver })
    }

    /// Mode journal et archive désactivée (tests, outils) : aucun client.
    pub fn new(pool: PgPool, config: Config) -> Self {
        AppState { pool, config: Arc::new(config), mailer: None, archiver: None }
    }
}
