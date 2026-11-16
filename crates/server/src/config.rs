use std::path::PathBuf;

use crate::graph::GraphConfig;

/// Mode d'envoi des notifications.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MailMode {
    /// Aucun envoi : les avis sont journalisés et visibles dans la console Admin.
    Journal,
    /// Envoi par Microsoft Graph depuis la boîte dédiée (Exchange Online).
    Graph,
}

impl MailMode {
    pub fn as_str(self) -> &'static str {
        match self {
            MailMode::Journal => "journal",
            MailMode::Graph => "graph",
        }
    }
}

/// Archivage des originaux PDF et des scans.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArchiveMode {
    /// Copie locale seulement (volume protégé et sauvegardé).
    Desactive,
    /// Dépôt dans une bibliothèque SharePoint, copie locale conservée.
    SharePoint,
}

impl ArchiveMode {
    pub fn as_str(self) -> &'static str {
        match self {
            ArchiveMode::Desactive => "desactive",
            ArchiveMode::SharePoint => "sharepoint",
        }
    }

    /// État initial d'une nouvelle version de document.
    pub fn initial_status(self) -> &'static str {
        match self {
            ArchiveMode::Desactive => "desactive",
            ArchiveMode::SharePoint => "en_attente",
        }
    }
}

/// Modules facultatifs, désactivables (pilote) : menus masqués, routes
/// refusées, traitements automatiques arrêtés. Le cœur (personnel, arrivées,
/// travail, formations, demandes, parc IT, remises, restitutions, décharges,
/// Excel, reprise) n'est jamais désactivable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Module {
    Vehicules,
    Logiciels,
    Inventaire,
    Mutation,
    Depart,
    Etats,
}

impl Module {
    pub const ALL: [Module; 6] = [Module::Vehicules, Module::Logiciels, Module::Inventaire, Module::Mutation, Module::Depart, Module::Etats];

    pub fn as_str(self) -> &'static str {
        match self {
            Module::Vehicules => "vehicules",
            Module::Logiciels => "logiciels",
            Module::Inventaire => "inventaire",
            Module::Mutation => "mutation",
            Module::Depart => "depart",
            Module::Etats => "etats",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Module::Vehicules => "Véhicules",
            Module::Logiciels => "Logiciels et licences",
            Module::Inventaire => "Étiquettes et inventaire QR",
            Module::Mutation => "Mutation",
            Module::Depart => "Départ",
            Module::Etats => "États des lieux",
        }
    }

    /// `MODULES_DESACTIVES` : liste séparée par des virgules, ou « pilote »
    /// (tous les modules facultatifs). Une valeur inconnue empêche le
    /// démarrage : un module ne doit jamais rester ouvert par une faute de frappe.
    pub fn parse_list(v: &str) -> anyhow::Result<Vec<Module>> {
        let mut out = Vec::new();
        for item in v.split(',').map(|s| s.trim().to_lowercase()).filter(|s| !s.is_empty()) {
            if item == "pilote" {
                out.extend(Module::ALL);
                continue;
            }
            match Module::ALL.iter().find(|m| m.as_str() == item) {
                Some(m) => out.push(*m),
                None => anyhow::bail!(
                    "MODULES_DESACTIVES : module inconnu « {item} » (pilote, {})",
                    Module::ALL.map(|m| m.as_str()).join(", ")
                ),
            }
        }
        out.sort_by_key(|m| m.as_str());
        out.dedup();
        Ok(out)
    }
}

#[derive(Debug, Clone)]
pub struct ArchiveTarget {
    pub site_id: String,
    pub drive_id: String,
    /// Dossier racine dans la bibliothèque.
    pub root: String,
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
    /// Boîte d'expédition (mode graph).
    pub mail_sender: Option<String>,
    /// Identité applicative d'envoi (mode graph).
    pub mail_graph: Option<GraphConfig>,
    /// Domaines autorisés pendant le pilote : tout autre destinataire est
    /// refusé sans envoi. Vide : aucun filtre.
    pub mail_allowed_domains: Vec<String>,
    pub archive_mode: ArchiveMode,
    /// Identité applicative d'archive, distincte de celle d'envoi.
    pub archive_graph: Option<GraphConfig>,
    pub archive_target: Option<ArchiveTarget>,
    pub pdf_font: PathBuf,
    pub pdf_font_bold: PathBuf,
    pub max_upload_bytes: usize,
    /// Délai d'alerte (jours) avant une échéance : pièces de véhicule,
    /// location, abonnements logiciels.
    pub alert_days: i32,
    /// Seuil d'alerte kilométrique avant un entretien.
    pub alert_km: i32,
    /// Délai après l'heure de coupure avant relance « accès à vérifier ».
    pub escalation_minutes: i64,
    /// Modules facultatifs désactivés (pilote).
    pub disabled_modules: Vec<Module>,
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
            "graph" => MailMode::Graph,
            other => anyhow::bail!("MAIL_MODE inconnu : {other} (journal ou graph)"),
        };
        let (mail_sender, mail_graph) = match mail_mode {
            MailMode::Journal => (None, None),
            MailMode::Graph => (
                Some(var("MAIL_SENDER").ok_or_else(|| anyhow::anyhow!("MAIL_SENDER (boîte d'expédition) est obligatoire en mode graph"))?),
                Some(GraphConfig::from_env("GRAPH")?),
            ),
        };
        let archive_mode = match var("ARCHIVE_MODE").as_deref().unwrap_or("desactive") {
            "desactive" => ArchiveMode::Desactive,
            "sharepoint" => ArchiveMode::SharePoint,
            other => anyhow::bail!("ARCHIVE_MODE inconnu : {other} (desactive ou sharepoint)"),
        };
        let (archive_graph, archive_target) = match archive_mode {
            ArchiveMode::Desactive => (None, None),
            ArchiveMode::SharePoint => (
                Some(GraphConfig::from_env("ARCHIVE")?),
                Some(ArchiveTarget {
                    site_id: var("ARCHIVE_SITE_ID").ok_or_else(|| anyhow::anyhow!("ARCHIVE_SITE_ID est obligatoire"))?,
                    drive_id: var("ARCHIVE_DRIVE_ID").ok_or_else(|| anyhow::anyhow!("ARCHIVE_DRIVE_ID est obligatoire"))?,
                    root: var("ARCHIVE_ROOT").unwrap_or_else(|| "Celluloplast".into()),
                }),
            ),
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
            mail_sender,
            mail_graph,
            mail_allowed_domains: var("MAIL_ALLOWED_DOMAINS")
                .map(|v| v.split(',').map(|d| d.trim().trim_start_matches('@').to_lowercase()).filter(|d| !d.is_empty()).collect())
                .unwrap_or_default(),
            archive_mode,
            archive_graph,
            archive_target,
            pdf_font: var("PDF_FONT").unwrap_or_else(|| default_font.into()).into(),
            pdf_font_bold: var("PDF_FONT_BOLD").unwrap_or_else(|| default_bold.into()).into(),
            max_upload_bytes: parse_or("MAX_UPLOAD_MB", 15)? as usize * 1024 * 1024,
            alert_days: parse_or("ALERT_DAYS", 30)? as i32,
            alert_km: parse_or("ALERT_KM", 1000)? as i32,
            escalation_minutes: parse_or("ESCALATION_MINUTES", 60)?,
            disabled_modules: Module::parse_list(&var("MODULES_DESACTIVES").unwrap_or_default())?,
        })
    }

    pub fn module_on(&self, m: Module) -> bool {
        !self.disabled_modules.contains(&m)
    }

    /// Destinataire autorisé pendant le pilote ?
    pub fn recipient_allowed(&self, address: &str) -> bool {
        if self.mail_allowed_domains.is_empty() {
            return true;
        }
        let domain = address.rsplit('@').next().unwrap_or_default().to_lowercase();
        self.mail_allowed_domains.iter().any(|d| *d == domain)
    }
}

fn parse_or(name: &str, default: i64) -> anyhow::Result<i64> {
    match var(name) {
        None => Ok(default),
        Some(v) => v.parse().map_err(|_| anyhow::anyhow!("{name} doit être un entier")),
    }
}
