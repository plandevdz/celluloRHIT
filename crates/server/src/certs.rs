//! Certificats des identités Microsoft Graph, générés sur le serveur : la
//! clé privée ne quitte jamais la machine ; seul le certificat public est
//! téléchargé pour être déposé dans l'application Entra.

use std::path::Path;
use std::str::FromStr;
use std::time::Duration;

use rsa::pkcs1v15::SigningKey;
use rsa::pkcs8::{EncodePrivateKey, LineEnding};
use rsa::RsaPrivateKey;
use sha2::{Digest, Sha256};
use x509_cert::builder::{Builder, CertificateBuilder, Profile};
use x509_cert::der::{Decode, DecodePem, Encode, EncodePem};
use x509_cert::name::Name;
use x509_cert::serial_number::SerialNumber;
use x509_cert::spki::SubjectPublicKeyInfoOwned;
use x509_cert::time::Validity;
use x509_cert::Certificate;

/// Paire RSA 2048 et certificat autosigné (PEM), valable `days` jours.
pub fn generate(common_name: &str, days: u64) -> anyhow::Result<(String, String)> {
    let mut rng = rand::thread_rng();
    let key = RsaPrivateKey::new(&mut rng, 2048)?;
    let signer = SigningKey::<Sha256>::new(key.clone());
    let spki = SubjectPublicKeyInfoOwned::from_key(key.to_public_key()).map_err(|e| anyhow::anyhow!("clé publique : {e}"))?;
    let mut serial = [0u8; 16];
    rand::RngCore::fill_bytes(&mut rng, &mut serial);
    serial[0] &= 0x7f;
    serial[0] |= 0x01;
    let serial = SerialNumber::new(&serial).map_err(|e| anyhow::anyhow!("numéro de série : {e}"))?;
    let validity = Validity::from_now(Duration::from_secs(days * 86_400)).map_err(|e| anyhow::anyhow!("validité : {e}"))?;
    let subject = Name::from_str(&format!("CN={common_name}")).map_err(|e| anyhow::anyhow!("sujet : {e}"))?;
    let builder = CertificateBuilder::new(Profile::Root, serial, validity, subject, spki, &signer)
        .map_err(|e| anyhow::anyhow!("certificat : {e}"))?;
    let cert = builder.build::<rsa::pkcs1v15::Signature>().map_err(|e| anyhow::anyhow!("signature du certificat : {e}"))?;
    let cert_pem = cert.to_pem(LineEnding::LF).map_err(|e| anyhow::anyhow!("encodage du certificat : {e}"))?;
    let key_pem = key.to_pkcs8_pem(LineEnding::LF).map_err(|e| anyhow::anyhow!("encodage de la clé : {e}"))?.to_string();
    Ok((cert_pem, key_pem))
}

/// Écrit la paire : clé lisible par le seul compte du service.
pub fn write_pair(crt: &Path, key: &Path, cert_pem: &str, key_pem: &str) -> anyhow::Result<()> {
    if let Some(dir) = crt.parent() {
        std::fs::create_dir_all(dir)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))?;
        }
    }
    let tmp_key = key.with_extension("key.partiel");
    std::fs::write(&tmp_key, key_pem)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&tmp_key, std::fs::Permissions::from_mode(0o600))?;
    }
    std::fs::write(crt.with_extension("crt.partiel"), cert_pem)?;
    std::fs::rename(&tmp_key, key)?;
    std::fs::rename(crt.with_extension("crt.partiel"), crt)?;
    Ok(())
}

/// Résumé affichable d'un certificat : empreintes, sujet, fin de validité.
pub fn describe(cert_pem: &str) -> anyhow::Result<serde_json::Value> {
    let cert = Certificate::from_pem(cert_pem.as_bytes()).map_err(|e| anyhow::anyhow!("certificat illisible : {e}"))?;
    let der = cert.to_der().map_err(|e| anyhow::anyhow!("certificat : {e}"))?;
    let _ = Certificate::from_der(&der);
    let sha1 = hex::encode_upper(sha1::Sha1::digest(&der));
    let sha256 = hex::encode_upper(Sha256::digest(&der));
    let tbs = &cert.tbs_certificate;
    let not_after = tbs.validity.not_after.to_unix_duration().as_secs() as i64;
    let not_before = tbs.validity.not_before.to_unix_duration().as_secs() as i64;
    Ok(serde_json::json!({
        "subject": tbs.subject.to_string(),
        "thumbprint_sha1": sha1,
        "thumbprint_sha256": sha256,
        "not_before": chrono::DateTime::from_timestamp(not_before, 0),
        "not_after": chrono::DateTime::from_timestamp(not_after, 0),
    }))
}

#[cfg(test)]
mod tests {
    #[test]
    fn certificat_autosigne_lisible_par_le_client_graph() {
        let (cert, key) = super::generate("celluloplast-envoi", 730).unwrap();
        assert!(cert.starts_with("-----BEGIN CERTIFICATE-----"));
        assert!(key.starts_with("-----BEGIN PRIVATE KEY-----"));
        let d = super::describe(&cert).unwrap();
        assert_eq!(d["subject"], "CN=celluloplast-envoi");
        assert_eq!(d["thumbprint_sha1"].as_str().unwrap().len(), 40);
        let dir = std::env::temp_dir().join(format!("cellulo-cert-{}", uuid::Uuid::new_v4()));
        let (crt_path, key_path) = (dir.join("a.crt"), dir.join("a.key"));
        super::write_pair(&crt_path, &key_path, &cert, &key).unwrap();
        let client = crate::graph::GraphClient::new(crate::graph::GraphConfig {
            tenant_id: "t".into(),
            client_id: "c".into(),
            cert_file: crt_path,
            key_file: key_path,
            authority: "https://login.microsoftonline.com".into(),
            endpoint: "https://graph.microsoft.com".into(),
            scope: "https://graph.microsoft.com/.default".into(),
        });
        assert!(client.is_ok());
        let _ = std::fs::remove_dir_all(dir);
    }
}
