//! Client Microsoft Graph (02-Technique T05, T06) : OAuth2 « client
//! credentials » par certificat (assertion JWT RS256), envoi par
//! `POST /users/{boîte}/sendMail` et dépôt de fichiers dans une
//! bibliothèque SharePoint. Aucun mot de passe de boîte ni droit de lecture
//! du courrier. Deux identités distinctes : envoi et archive.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use base64::Engine;
use rsa::pkcs1::DecodeRsaPrivateKey;
use rsa::pkcs1v15::SigningKey;
use rsa::pkcs8::DecodePrivateKey;
use rsa::signature::{SignatureEncoding, Signer};
use rsa::RsaPrivateKey;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tokio::sync::Mutex;

#[derive(Debug, Clone)]
pub struct GraphConfig {
    pub tenant_id: String,
    pub client_id: String,
    pub cert_file: PathBuf,
    pub key_file: PathBuf,
    /// https://login.microsoftonline.com (remplaçable pour les tests).
    pub authority: String,
    /// https://graph.microsoft.com (remplaçable pour les tests).
    pub endpoint: String,
    pub scope: String,
}

impl GraphConfig {
    /// Lit `{PREFIX}_TENANT_ID`, `_CLIENT_ID`, `_CERT_FILE`, `_KEY_FILE`.
    pub fn from_env(prefix: &str) -> anyhow::Result<GraphConfig> {
        let get = |n: &str| -> anyhow::Result<String> {
            std::env::var(format!("{prefix}_{n}"))
                .ok()
                .filter(|v| !v.trim().is_empty())
                .ok_or_else(|| anyhow::anyhow!("{prefix}_{n} est obligatoire"))
        };
        let opt = |n: &str, d: &str| std::env::var(format!("{prefix}_{n}")).ok().filter(|v| !v.trim().is_empty()).unwrap_or_else(|| d.to_string());
        Ok(GraphConfig {
            tenant_id: get("TENANT_ID")?,
            client_id: get("CLIENT_ID")?,
            cert_file: get("CERT_FILE")?.into(),
            key_file: get("KEY_FILE")?.into(),
            authority: opt("AUTHORITY", "https://login.microsoftonline.com"),
            endpoint: opt("ENDPOINT", "https://graph.microsoft.com"),
            scope: opt("SCOPE", "https://graph.microsoft.com/.default"),
        })
    }
}

/// Résultat d'un envoi, sans jamais confondre « accepté » et « livré ».
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SendOutcome {
    /// HTTP 202 : accepté par Microsoft (pas une preuve de livraison).
    Accepted,
    /// Panne transitoire ou limitation : nouvel essai après le délai.
    Retry { after: Option<Duration>, reason: String },
    /// Erreur définitive (droits, destinataire, requête) : à corriger.
    Failed(String),
    /// Délai dépassé après soumission : résultat inconnu, ne pas réexpédier.
    Unknown(String),
}

pub struct GraphClient {
    http: reqwest::Client,
    cfg: GraphConfig,
    key: RsaPrivateKey,
    x5t: String,
    x5t_s256: String,
    token: Mutex<Option<(String, Instant)>>,
}

fn pem_blocks(text: &str, label: &str) -> Vec<Vec<u8>> {
    let begin = format!("-----BEGIN {label}-----");
    let end = format!("-----END {label}-----");
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find(&begin) {
        let after = &rest[start + begin.len()..];
        let Some(stop) = after.find(&end) else { break };
        let b64: String = after[..stop].chars().filter(|c| !c.is_whitespace()).collect();
        if let Ok(der) = STANDARD.decode(b64) {
            out.push(der);
        }
        rest = &after[stop + end.len()..];
    }
    out
}

impl GraphClient {
    pub fn new(cfg: GraphConfig) -> anyhow::Result<GraphClient> {
        let cert = std::fs::read_to_string(&cfg.cert_file).map_err(|e| anyhow::anyhow!("certificat {:?} illisible : {e}", cfg.cert_file))?;
        let der = pem_blocks(&cert, "CERTIFICATE").into_iter().next().ok_or_else(|| anyhow::anyhow!("aucun certificat PEM dans {:?}", cfg.cert_file))?;
        let key_pem = std::fs::read_to_string(&cfg.key_file).map_err(|e| anyhow::anyhow!("clé {:?} illisible : {e}", cfg.key_file))?;
        let key = RsaPrivateKey::from_pkcs8_pem(&key_pem)
            .or_else(|_| RsaPrivateKey::from_pkcs1_pem(&key_pem))
            .map_err(|_| anyhow::anyhow!("clé privée RSA illisible (PKCS#8 ou PKCS#1 attendu)"))?;
        let x5t = URL_SAFE_NO_PAD.encode(sha1::Sha1::digest(&der));
        let x5t_s256 = URL_SAFE_NO_PAD.encode(Sha256::digest(&der));
        let http = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(60))
            .user_agent("celluloplast-parcours")
            .build()?;
        Ok(GraphClient { http, cfg, key, x5t, x5t_s256, token: Mutex::new(None) })
    }

    fn token_url(&self) -> String {
        format!("{}/{}/oauth2/v2.0/token", self.cfg.authority.trim_end_matches('/'), self.cfg.tenant_id)
    }

    /// Assertion signée par la clé privée du certificat (jamais de secret partagé).
    fn assertion(&self) -> anyhow::Result<String> {
        let now = chrono::Utc::now().timestamp();
        let header = json!({ "alg": "RS256", "typ": "JWT", "x5t": self.x5t, "x5t#S256": self.x5t_s256 });
        let claims = json!({
            "aud": self.token_url(), "iss": self.cfg.client_id, "sub": self.cfg.client_id,
            "jti": uuid::Uuid::new_v4().to_string(), "nbf": now - 30, "iat": now, "exp": now + 600,
        });
        let signing_input = format!("{}.{}", URL_SAFE_NO_PAD.encode(header.to_string()), URL_SAFE_NO_PAD.encode(claims.to_string()));
        let signer = SigningKey::<Sha256>::new(self.key.clone());
        let sig = signer.try_sign(signing_input.as_bytes()).map_err(|e| anyhow::anyhow!("signature de l'assertion : {e}"))?;
        Ok(format!("{signing_input}.{}", URL_SAFE_NO_PAD.encode(sig.to_bytes())))
    }

    /// Jeton d'accès mis en cache jusqu'à cinq minutes avant son expiration.
    pub async fn token(&self) -> Result<String, SendOutcome> {
        let mut guard = self.token.lock().await;
        if let Some((t, until)) = guard.as_ref() {
            if Instant::now() < *until {
                return Ok(t.clone());
            }
        }
        let assertion = self.assertion().map_err(|e| SendOutcome::Failed(e.to_string()))?;
        let resp = self
            .http
            .post(self.token_url())
            .form(&[
                ("client_id", self.cfg.client_id.as_str()),
                ("scope", self.cfg.scope.as_str()),
                ("client_assertion_type", "urn:ietf:params:oauth:client-assertion-type:jwt-bearer"),
                ("client_assertion", assertion.as_str()),
                ("grant_type", "client_credentials"),
            ])
            .send()
            .await
            .map_err(|e| SendOutcome::Retry { after: None, reason: format!("jeton : serveur d'identité injoignable ({})", short(&e)) })?;
        let status = resp.status();
        let body: Value = resp.json().await.unwrap_or(Value::Null);
        if !status.is_success() {
            let code = body["error"].as_str().unwrap_or("erreur").to_string();
            let reason = format!("jeton refusé ({status}, {code})");
            return Err(if status.is_server_error() { SendOutcome::Retry { after: None, reason } } else { SendOutcome::Failed(reason) });
        }
        let token = body["access_token"].as_str().ok_or_else(|| SendOutcome::Failed("réponse de jeton sans access_token".into()))?.to_string();
        let ttl = body["expires_in"].as_u64().unwrap_or(3600).saturating_sub(300).max(60);
        *guard = Some((token.clone(), Instant::now() + Duration::from_secs(ttl)));
        Ok(token)
    }

    async fn invalidate(&self) {
        *self.token.lock().await = None;
    }

    /// Envoi d'un message texte depuis la boîte dédiée. Pas de copie dans
    /// les éléments envoyés : aucun droit de lecture n'est nécessaire.
    pub async fn send_mail(&self, sender: &str, to: &str, subject: &str, body: &str) -> SendOutcome {
        let token = match self.token().await {
            Ok(t) => t,
            Err(o) => return o,
        };
        let url = format!("{}/v1.0/users/{}/sendMail", self.cfg.endpoint.trim_end_matches('/'), urlencode(sender));
        let payload = json!({
            "message": {
                "subject": subject,
                "body": { "contentType": "Text", "content": body },
                "toRecipients": [{ "emailAddress": { "address": to } }],
            },
            "saveToSentItems": false,
        });
        match self.http.post(url).bearer_auth(token).json(&payload).send().await {
            Ok(resp) => {
                let status = resp.status();
                let retry_after = resp
                    .headers()
                    .get("retry-after")
                    .and_then(|v| v.to_str().ok())
                    .and_then(|v| v.trim().parse::<u64>().ok())
                    .map(Duration::from_secs);
                let text = resp.text().await.unwrap_or_default();
                let code = serde_json::from_str::<Value>(&text).ok().and_then(|v| v["error"]["code"].as_str().map(String::from)).unwrap_or_default();
                match status.as_u16() {
                    202 | 200 => SendOutcome::Accepted,
                    401 => {
                        self.invalidate().await;
                        SendOutcome::Retry { after: None, reason: "jeton expiré ou révoqué (401)".into() }
                    }
                    429 | 500 | 502 | 503 | 504 => SendOutcome::Retry { after: retry_after, reason: format!("Exchange indisponible ou limitation ({status} {code})") },
                    _ => SendOutcome::Failed(format!("refusé par Exchange ({status} {code}) : vérifier les droits de la boîte ou l'adresse")),
                }
            }
            Err(e) if e.is_connect() => SendOutcome::Retry { after: None, reason: format!("Exchange injoignable ({})", short(&e)) },
            Err(e) => SendOutcome::Unknown(format!("résultat inconnu après soumission ({}) : vérifier avant tout nouvel envoi", short(&e))),
        }
    }

    /// Dépose un fichier dans la bibliothèque ; au-delà de 4 Mo, par une
    /// session d'envoi en morceaux. Renvoie (identifiant, taille) distants.
    pub async fn upload(&self, site_id: &str, drive_id: &str, path: &str, bytes: &[u8]) -> Result<(String, u64), String> {
        let token = self.token().await.map_err(|o| format!("{o:?}"))?;
        let base = format!(
            "{}/v1.0/sites/{}/drives/{}/root:/{}:",
            self.cfg.endpoint.trim_end_matches('/'),
            urlencode(site_id),
            urlencode(drive_id),
            path.split('/').map(urlencode).collect::<Vec<_>>().join("/")
        );
        let item: Value = if bytes.len() <= 4 * 1024 * 1024 {
            let resp = self.http.put(format!("{base}/content")).bearer_auth(&token).body(bytes.to_vec()).send().await.map_err(|e| short(&e))?;
            let status = resp.status();
            if !status.is_success() {
                if status.as_u16() == 401 {
                    self.invalidate().await;
                }
                return Err(format!("dépôt refusé ({status})"));
            }
            resp.json().await.map_err(|e| short(&e))?
        } else {
            let session: Value = self
                .http
                .post(format!("{base}/createUploadSession"))
                .bearer_auth(&token)
                .json(&json!({ "item": { "@microsoft.graph.conflictBehavior": "replace" } }))
                .send()
                .await
                .map_err(|e| short(&e))?
                .json()
                .await
                .map_err(|e| short(&e))?;
            let upload_url = session["uploadUrl"].as_str().ok_or("session d'envoi sans uploadUrl")?.to_string();
            let chunk = 5 * 320 * 1024;
            let total = bytes.len();
            let mut last = Value::Null;
            let mut start = 0;
            while start < total {
                let end = (start + chunk).min(total);
                let resp = self
                    .http
                    .put(&upload_url)
                    .header("Content-Range", format!("bytes {start}-{}/{total}", end - 1))
                    .body(bytes[start..end].to_vec())
                    .send()
                    .await
                    .map_err(|e| short(&e))?;
                if !resp.status().is_success() {
                    return Err(format!("morceau refusé ({})", resp.status()));
                }
                last = resp.json().await.unwrap_or(Value::Null);
                start = end;
            }
            last
        };
        let id = item["id"].as_str().ok_or("réponse sans identifiant d'élément")?.to_string();
        let size = item["size"].as_u64().unwrap_or(0);
        Ok((id, size))
    }
}

fn short(e: &reqwest::Error) -> String {
    let s = e.to_string();
    s.chars().take(200).collect()
}

fn urlencode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'@' | b',' => out.push(b as char),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodage_des_chemins() {
        assert_eq!(urlencode("avis@exemple.test"), "avis@exemple.test");
        assert_eq!(urlencode("ONB-00007 v1.pdf"), "ONB-00007%20v1.pdf");
        assert_eq!(urlencode("é"), "%C3%A9");
    }

    #[test]
    fn blocs_pem() {
        let text = "x\n-----BEGIN CERTIFICATE-----\nAAEC\n-----END CERTIFICATE-----\n";
        assert_eq!(pem_blocks(text, "CERTIFICATE"), vec![vec![0u8, 1, 2]]);
    }
}
