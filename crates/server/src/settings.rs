//! Réglages d'exploitation modifiables par l'ADMIN depuis l'interface :
//! modules ouverts, envoi Exchange, archivage SharePoint, échéances et
//! sauvegarde chiffrée. Le fichier .env reste la base ; chaque section
//! enregistrée dans `runtime_settings` la remplace entièrement. Aucun secret
//! en base : les clés privées Graph sont des fichiers du serveur
//! (`DATA_DIR/secrets`), générés sur place et jamais affichés.

use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::config::{ArchiveMode, ArchiveTarget, BackupSettings, Config, MailMode, Module};
use crate::graph::GraphConfig;

/// Sections modifiables. L'ordre est celui de l'écran.
pub const SECTIONS: [&str; 5] = ["modules", "mail", "archive", "alerts", "backup"];

/// Identité Graph : envoi des avis ou archivage, chacune son certificat.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Identity {
    Envoi,
    Archive,
}

impl Identity {
    pub fn parse(v: &str) -> Option<Identity> {
        match v {
            "envoi" => Some(Identity::Envoi),
            "archive" => Some(Identity::Archive),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Identity::Envoi => "envoi",
            Identity::Archive => "archive",
        }
    }

    /// Certificat et clé générés depuis l'interface.
    pub fn generated_paths(self, data_dir: &Path) -> (PathBuf, PathBuf) {
        let dir = data_dir.join("secrets");
        (dir.join(format!("graph-{}.crt", self.as_str())), dir.join(format!("graph-{}.key", self.as_str())))
    }

    /// Certificat utilisé : celui de l'interface s'il existe, sinon celui
    /// du fichier .env.
    pub fn cert_paths(self, base: &Config) -> Option<(PathBuf, PathBuf)> {
        let (crt, key) = self.generated_paths(&base.data_dir);
        if crt.is_file() && key.is_file() {
            return Some((crt, key));
        }
        match self {
            Identity::Envoi => base.env_mail_cert.clone(),
            Identity::Archive => base.env_archive_cert.clone(),
        }
    }
}

fn text(v: &Value, key: &str) -> Option<String> {
    v.get(key).and_then(|x| x.as_str()).map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

fn int(v: &Value, key: &str) -> Option<i64> {
    v.get(key).and_then(|x| x.as_i64())
}

fn graph_config(base: &Config, identity: Identity, v: &Value) -> anyhow::Result<GraphConfig> {
    let tenant_id = text(v, "tenant_id").ok_or_else(|| anyhow::anyhow!("identifiant du locataire (tenant) manquant"))?;
    let client_id = text(v, "client_id").ok_or_else(|| anyhow::anyhow!("identifiant de l'application (client) manquant"))?;
    let (cert_file, key_file) = identity
        .cert_paths(base)
        .ok_or_else(|| anyhow::anyhow!("certificat {} absent : générez-le dans cet écran", identity.as_str()))?;
    Ok(GraphConfig {
        tenant_id,
        client_id,
        cert_file,
        key_file,
        authority: base.graph_authority.clone(),
        endpoint: base.graph_endpoint.clone(),
        scope: format!("{}/.default", base.graph_endpoint.trim_end_matches('/')),
    })
}

/// Identité d'envoi décrite par une section « mail » (mode quelconque) :
/// sert au mail de test avant l'activation.
pub fn mail_identity(base: &Config, mail: &Value) -> anyhow::Result<(GraphConfig, String)> {
    let sender = text(mail, "sender").ok_or_else(|| anyhow::anyhow!("boîte d'expédition manquante"))?;
    Ok((graph_config(base, Identity::Envoi, mail)?, sender))
}

/// Identité et cible d'archive décrites par une section « archive ».
pub fn archive_identity(base: &Config, archive: &Value) -> anyhow::Result<(GraphConfig, ArchiveTarget)> {
    let target = ArchiveTarget {
        site_id: text(archive, "site_id").ok_or_else(|| anyhow::anyhow!("identifiant du site SharePoint manquant"))?,
        drive_id: text(archive, "drive_id").ok_or_else(|| anyhow::anyhow!("identifiant de la bibliothèque manquant"))?,
        root: text(archive, "root").unwrap_or_else(|| "Celluloplast".into()),
    };
    Ok((graph_config(base, Identity::Archive, archive)?, target))
}

pub fn parse_modules(v: &Value) -> anyhow::Result<Vec<Module>> {
    let list: Vec<String> = v
        .get("desactives")
        .and_then(|x| x.as_array())
        .map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect())
        .unwrap_or_default();
    Module::parse_list(&list.join(","))
}

pub fn parse_domains(v: &Value) -> Vec<String> {
    v.get("allowed_domains")
        .and_then(|x| x.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_str())
                .map(|d| d.trim().trim_start_matches('@').to_lowercase())
                .filter(|d| !d.is_empty())
                .collect()
        })
        .unwrap_or_default()
}

/// Configuration effective : le .env, puis chaque section de l'interface.
pub fn effective(base: &Config, data: &Value) -> anyhow::Result<Config> {
    let mut c = base.clone();
    c.overridden.clear();
    if let Some(m) = data.get("modules").filter(|v| v.is_object()) {
        c.disabled_modules = parse_modules(m)?;
        c.overridden.push("modules".into());
    }
    if let Some(m) = data.get("mail").filter(|v| v.is_object()) {
        c.mail_test_recipient = text(m, "test_recipient");
        c.mail_allowed_domains = parse_domains(m);
        match text(m, "mode").as_deref().unwrap_or("journal") {
            "graph" => {
                let (graph, sender) = mail_identity(base, m).map_err(|e| anyhow::anyhow!("envoi Exchange : {e}"))?;
                c.mail_mode = MailMode::Graph;
                c.mail_sender = Some(sender);
                c.mail_graph = Some(graph);
            }
            _ => {
                c.mail_mode = MailMode::Journal;
                c.mail_sender = text(m, "sender");
                c.mail_graph = None;
            }
        }
        c.overridden.push("mail".into());
    }
    if let Some(a) = data.get("archive").filter(|v| v.is_object()) {
        match text(a, "mode").as_deref().unwrap_or("desactive") {
            "sharepoint" => {
                let (graph, target) = archive_identity(base, a).map_err(|e| anyhow::anyhow!("archivage SharePoint : {e}"))?;
                c.archive_mode = ArchiveMode::SharePoint;
                c.archive_graph = Some(graph);
                c.archive_target = Some(target);
            }
            _ => {
                c.archive_mode = ArchiveMode::Desactive;
                c.archive_graph = None;
                c.archive_target = None;
            }
        }
        c.overridden.push("archive".into());
    }
    if let Some(a) = data.get("alerts").filter(|v| v.is_object()) {
        if let Some(d) = int(a, "alert_days") {
            c.alert_days = d.clamp(1, 365) as i32;
        }
        if let Some(k) = int(a, "alert_km") {
            c.alert_km = k.clamp(0, 100_000) as i32;
        }
        if let Some(m) = int(a, "escalation_minutes") {
            c.escalation_minutes = m.clamp(5, 24 * 60);
        }
        c.overridden.push("alerts".into());
    }
    if let Some(b) = data.get("backup").filter(|v| v.is_object()) {
        c.backup = match (text(b, "public_key"), text(b, "fingerprint")) {
            (Some(public_key), Some(fingerprint)) => Some(BackupSettings {
                public_key,
                fingerprint,
                uid: text(b, "uid").unwrap_or_default(),
                hour: int(b, "hour").unwrap_or(2).clamp(0, 23) as u32,
                keep: int(b, "keep").unwrap_or(14).clamp(1, 90) as u32,
            }),
            _ => None,
        };
        c.overridden.push("backup".into());
    }
    Ok(c)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn base() -> Config {
        let mut c = crate::config::Config::from_env_for_tests();
        c.data_dir = std::env::temp_dir().join("cellulo-settings-test-inexistant");
        c
    }

    #[test]
    fn sans_reglage_le_env_s_applique() {
        let b = base();
        let c = effective(&b, &json!({})).unwrap();
        assert_eq!(c.disabled_modules, b.disabled_modules);
        assert_eq!(c.mail_mode, MailMode::Journal);
        assert!(c.overridden.is_empty());
    }

    #[test]
    fn modules_et_echeances() {
        let c = effective(&base(), &json!({"modules": {"desactives": ["pilote"]}, "alerts": {"alert_days": 10}})).unwrap();
        assert_eq!(c.disabled_modules.len(), 6);
        assert_eq!(c.alert_days, 10);
        assert!(effective(&base(), &json!({"modules": {"desactives": ["vehicule"]}})).is_err(), "faute de frappe refusée");
    }

    #[test]
    fn envoi_graph_exige_un_certificat() {
        let r = effective(&base(), &json!({"mail": {"mode": "graph", "sender": "avis@exemple.test", "tenant_id": "t", "client_id": "c"}}));
        assert!(r.unwrap_err().to_string().contains("certificat"));
        let c = effective(&base(), &json!({"mail": {"mode": "journal", "allowed_domains": ["@Exemple.test"]}})).unwrap();
        assert_eq!(c.mail_mode, MailMode::Journal);
        assert_eq!(c.mail_allowed_domains, vec!["exemple.test".to_string()]);
    }
}
