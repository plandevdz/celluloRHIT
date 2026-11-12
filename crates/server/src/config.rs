use std::path::PathBuf;

/// Mode d'envoi des notifications.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MailMode {
    /// Aucun envoi : les avis sont journalisés et visibles dans la console Admin.
    /// L'adaptateur Microsoft Graph (02-Technique T05) sera ajouté au
    /// raccordement Exchange Online.
    Journal,
}

impl MailMode {
    pub fn as_str(self) -> &'static str {
        match self {
            MailMode::Journal => "journal",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Config {
    pub database_url: String,
    pub bind_addr: String,
    /// URL publique utilisée dans les liens des mails (ex. https://10.0.0.50).
    pub public_url: String,
    /// Répertoire de l'interface web compilée.
    pub web_dir: PathBuf,
    /// Répertoire des documents (originaux PDF, scans, spool d'archive).
    pub data_dir: PathBuf,
    pub cookie_secure: bool,
    pub session_idle_minutes: i64,
    pub session_max_hours: i64,
    pub mail_mode: MailMode,
    /// En mode test, tous les mails partent vers cette adresse autorisée.
    pub mail_test_recipient: Option<String>,
    pub pdf_font: PathBuf,
    pub pdf_font_bold: PathBuf,
    pub max_upload_bytes: usize,
}

fn var(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.trim().is_empty())
}

impl Config {
    pub fn from_env() -> anyhow::Result<Self> {
        let database_url = var("DATABASE_URL")
            .ok_or_else(|| anyhow::anyhow!("DATABASE_URL est obligatoire"))?;
        let mail_mode = match var("MAIL_MODE").as_deref().unwrap_or("journal") {
            "journal" => MailMode::Journal,
            "graph" => anyhow::bail!("MAIL_MODE=graph n'est pas encore disponible dans cette version"),
            other => anyhow::bail!("MAIL_MODE inconnu : {other} (journal)"),
        };
        let (default_font, default_bold) = if cfg!(windows) {
            ("C:/Windows/Fonts/arial.ttf", "C:/Windows/Fonts/arialbd.ttf")
        } else {
            (
                "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
                "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf",
            )
        };
        Ok(Config {
            database_url,
            bind_addr: var("BIND_ADDR").unwrap_or_else(|| "0.0.0.0:8080".into()),
            public_url: var("PUBLIC_URL").unwrap_or_else(|| "https://localhost".into()),
            web_dir: var("WEB_DIR").unwrap_or_else(|| "web/dist".into()).into(),
            data_dir: var("DATA_DIR").unwrap_or_else(|| "data".into()).into(),
            cookie_secure: var("COOKIE_SECURE").map(|v| v != "false").unwrap_or(true),
            session_idle_minutes: parse_or("SESSION_IDLE_MINUTES", 60)?,
            session_max_hours: parse_or("SESSION_MAX_HOURS", 10)?,
            mail_mode,
            mail_test_recipient: var("MAIL_TEST_RECIPIENT"),
            pdf_font: var("PDF_FONT").unwrap_or_else(|| default_font.into()).into(),
            pdf_font_bold: var("PDF_FONT_BOLD").unwrap_or_else(|| default_bold.into()).into(),
            max_upload_bytes: parse_or("MAX_UPLOAD_MB", 15)? as usize * 1024 * 1024,
        })
    }
}

fn parse_or(name: &str, default: i64) -> anyhow::Result<i64> {
    match var(name) {
        None => Ok(default),
        Some(v) => v.parse().map_err(|_| anyhow::anyhow!("{name} doit être un entier")),
    }
}
