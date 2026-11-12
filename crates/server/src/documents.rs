//! Stockage des décharges : original PDF versionné, scans signés liés.
//! Le PDF est produit hors de la transaction métier (API puis worker en
//! rattrapage) et n'est jamais écrasé.

use std::path::{Path, PathBuf};

use chrono::{DateTime, FixedOffset, Utc};
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use uuid::Uuid;

use crate::config::Config;
use crate::pdf::{self, DocData, DocKind, DocLine};

/// Africa/Algiers : UTC+1 toute l'année (pas d'heure d'été).
pub fn to_algiers(t: DateTime<Utc>) -> DateTime<FixedOffset> {
    t.with_timezone(&FixedOffset::east_opt(3600).expect("décalage valide"))
}

pub fn fr_datetime(t: DateTime<Utc>) -> String {
    to_algiers(t).format("%d/%m/%Y %H:%M").to_string()
}

/// Nom de fichier sûr (référence, sans séparateur de chemin).
fn safe(s: &str) -> String {
    s.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' { c } else { '_' }).collect()
}

pub fn movement_dir(case_ref: Option<&str>, kind: &str, movement_ref: &str) -> PathBuf {
    let folder = if kind == "remise" { "remises" } else { "restitutions" };
    PathBuf::from("dossiers")
        .join(safe(case_ref.unwrap_or("hors-dossier")))
        .join(folder)
        .join(safe(movement_ref))
}

/// Écriture atomique : fichier temporaire puis renommage.
pub async fn write_file(root: &Path, relative: &Path, bytes: &[u8]) -> anyhow::Result<()> {
    let full = root.join(relative);
    if let Some(parent) = full.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let tmp = full.with_extension(format!("tmp-{}", Uuid::new_v4()));
    tokio::fs::write(&tmp, bytes).await?;
    tokio::fs::rename(&tmp, &full).await?;
    Ok(())
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

type MovementHead = (String, String, DateTime<Utc>, Option<String>, Option<String>, String, Option<String>, Option<String>, String, String);

/// Génère l'original v1 s'il n'existe pas encore. Idempotent et sûr en
/// concurrence (verrou sur la ligne du document).
pub async fn ensure_original(pool: &PgPool, config: &Config, document_id: Uuid) -> anyhow::Result<()> {
    let mut tx = pool.begin().await?;
    let doc: Option<(Uuid, String, String)> = sqlx::query_as(
        "SELECT movement_id, kind, template_version FROM documents WHERE id = $1 FOR UPDATE",
    )
    .bind(document_id)
    .fetch_optional(&mut *tx)
    .await?;
    let Some((movement_id, kind, template_version)) = doc else {
        anyhow::bail!("document {document_id} introuvable");
    };
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM document_versions WHERE document_id = $1 AND file_kind = 'original')",
    )
    .bind(document_id)
    .fetch_one(&mut *tx)
    .await?;
    if exists {
        tx.commit().await?;
        return Ok(());
    }
    let head: MovementHead = sqlx::query_as(
        "SELECT m.reference, m.kind, m.effective_at, m.substitution_reason, m.note,
                e.first_name || ' ' || e.last_name, e.matricule, s.name, u.display_name,
                coalesce(c.reference, '')
           FROM movements m
           JOIN employees e ON e.id = m.employee_id
           JOIN users u ON u.id = m.operator_id
           LEFT JOIN cases c ON c.id = m.case_id
           -- Situation en vigueur à la date du mouvement ; à défaut (remise
           -- avant l'arrivée effective), la situation prévue.
           LEFT JOIN LATERAL (
                SELECT site_id FROM employee_positions ep
                 WHERE ep.employee_id = e.id AND (ep.valid_to IS NULL OR ep.valid_to > m.effective_at::date)
                 ORDER BY (ep.valid_from <= m.effective_at::date) DESC, ep.valid_from
                 LIMIT 1) p ON true
           LEFT JOIN sites s ON s.id = p.site_id
          WHERE m.id = $1",
    )
    .bind(movement_id)
    .fetch_one(&mut *tx)
    .await?;
    let (mref, mkind, effective_at, substitution, note, employee_name, matricule, site, operator, case_ref) = head;
    let lines: Vec<(String, Option<String>, i32, Option<String>, Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT ml.label, ml.identifier, ml.quantity, ml.condition, ml.note, om.reference
           FROM movement_lines ml
           LEFT JOIN movement_lines ol ON ol.id = ml.origin_line_id
           LEFT JOIN movements om ON om.id = ol.movement_id
          WHERE ml.movement_id = $1 ORDER BY ml.line_no",
    )
    .bind(movement_id)
    .fetch_all(&mut *tx)
    .await?;

    let mut observations = Vec::new();
    if let Some(s) = substitution {
        observations.push(format!("Substitution : {s}"));
    }
    for (label, _, _, _, line_note, _) in &lines {
        if let Some(n) = line_note {
            observations.push(format!("{label} : {n}"));
        }
    }
    if let Some(n) = note {
        observations.push(n);
    }
    let data = DocData {
        kind: if kind == "affectation" { DocKind::Affectation } else { DocKind::Restitution },
        template_version: template_version.clone(),
        movement_reference: mref.clone(),
        version: 1,
        case_reference: (!case_ref.is_empty()).then(|| case_ref.clone()),
        effective_local: fr_datetime(effective_at),
        employee_name,
        matricule,
        site,
        operator,
        lines: lines
            .into_iter()
            .map(|(label, identifier, quantity, condition, _, origin)| DocLine { label, identifier, quantity, origin, condition })
            .collect(),
        observations,
        generated_local: fr_datetime(Utc::now()),
    };
    let (font, bold) = (config.pdf_font.clone(), config.pdf_font_bold.clone());
    let bytes = tokio::task::spawn_blocking(move || pdf::render(&data, &font, &bold)).await??;
    let relative = movement_dir((!case_ref.is_empty()).then_some(case_ref.as_str()), &mkind, &mref).join("v1-original.pdf");
    write_file(&config.data_dir, &relative, &bytes).await?;
    sqlx::query(
        "INSERT INTO document_versions (document_id, version, file_kind, storage_path, sha256, size_bytes, mime_type, archive_status)
         VALUES ($1, 1, 'original', $2, $3, $4, 'application/pdf', 'desactive')",
    )
    .bind(document_id)
    .bind(relative.to_string_lossy().replace('\\', "/"))
    .bind(sha256_hex(&bytes))
    .bind(bytes.len() as i64)
    .execute(&mut *tx)
    .await?;
    sqlx::query("UPDATE documents SET status = 'genere', updated_at = now() WHERE id = $1 AND status = 'a_generer'")
        .bind(document_id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    tracing::info!(document = %document_id, movement = %mref, "décharge générée");
    Ok(())
}

/// Type de fichier accepté pour un scan, d'après sa signature binaire
/// (jamais d'après le nom ou l'en-tête déclaré).
pub fn sniff_scan(bytes: &[u8]) -> Option<(&'static str, &'static str)> {
    if bytes.starts_with(b"%PDF-") {
        Some(("application/pdf", "pdf"))
    } else if bytes.starts_with(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]) {
        Some(("image/png", "png"))
    } else if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some(("image/jpeg", "jpg"))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signature_des_scans() {
        assert_eq!(sniff_scan(b"%PDF-1.7 ...").map(|s| s.1), Some("pdf"));
        assert_eq!(sniff_scan(&[0xFF, 0xD8, 0xFF, 0xE0]).map(|s| s.1), Some("jpg"));
        assert_eq!(sniff_scan(b"MZ\x90\x00"), None);
        assert_eq!(sniff_scan(b"<html>"), None);
    }

    #[test]
    fn chemins_stables() {
        let p = movement_dir(Some("ONB-00007"), "remise", "MVT-00001");
        assert_eq!(p.to_string_lossy().replace('\\', "/"), "dossiers/ONB-00007/remises/MVT-00001");
        let p = movement_dir(None, "restitution", "../../etc");
        assert!(!p.to_string_lossy().contains(".."));
    }
}
