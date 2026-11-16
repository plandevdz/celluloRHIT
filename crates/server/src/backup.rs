//! Sauvegarde chiffrée depuis l'application (Administration → Sauvegardes).
//!
//! Le worker produit chaque jour, à l'heure choisie, quatre fichiers au
//! format de `deploy/backup.sh` (relus par `deploy/restore-isole.sh`) :
//! `cellulo-<D>.dump.gpg` (pg_dump, format custom), `documents-<D>.tar.gz.gpg`
//! (décharges et scans), `secrets-<D>.tar.gz.gpg` (clés Graph générées sur
//! le serveur, adresse publique et, si le volume est monté et lisible,
//! l'autorité de certification de Caddy) et `sha256-<D>.txt`.
//!
//! Chiffrement à clé publique (GnuPG) dès l'écriture : les flux passent de
//! pg_dump ou de l'archive tar directement à gpg, sans fichier en clair.
//! Le serveur ne détient que la clé publique ; l'ADMIN télécharge les
//! fichiers chiffrés depuis l'écran pour les garder hors de la machine.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use chrono::{Timelike, Utc};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::config::{BackupSettings, Config};
use crate::state::AppState;

/// Fichiers d'une sauvegarde : seuls ces noms peuvent être téléchargés.
pub fn is_backup_file(name: &str) -> bool {
    let ok_chars = name.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '.'));
    ok_chars
        && !name.contains("..")
        && (name.starts_with("cellulo-") || name.starts_with("documents-") || name.starts_with("secrets-") || name.starts_with("sha256-"))
        && (name.ends_with(".gpg") || name.ends_with(".txt"))
}

struct GpgHome(PathBuf);

impl Drop for GpgHome {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn gpg_home(parent: &Path) -> anyhow::Result<GpgHome> {
    let dir = parent.join(format!(".gnupg-{}", Uuid::new_v4().simple()));
    std::fs::create_dir_all(&dir)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700))?;
    }
    Ok(GpgHome(dir))
}

fn gpg(cfg: &Config, home: &Path) -> Command {
    let mut c = Command::new(&cfg.gpg_bin);
    // Chiffrement à clé publique : ni agent ni réseau (la clé privée n'est
    // jamais sur le serveur).
    c.arg("--batch").arg("--no-tty").arg("--no-autostart").arg("--homedir").arg(home);
    c
}

/// Vérifie une clé publique collée par l'ADMIN : une seule clé, capable de
/// chiffrer, jamais une clé privée. Rend (empreinte, identité).
pub fn inspect_public_key(cfg: &Config, armored: &str) -> anyhow::Result<(String, String)> {
    let armored = armored.trim();
    if armored.contains("PRIVATE KEY BLOCK") {
        anyhow::bail!("ceci est une clé PRIVÉE : collez uniquement la clé publique (elle reste hors du serveur)");
    }
    if !armored.starts_with("-----BEGIN PGP PUBLIC KEY BLOCK-----") {
        anyhow::bail!("clé publique PGP attendue (« -----BEGIN PGP PUBLIC KEY BLOCK----- »)");
    }
    std::fs::create_dir_all(&cfg.backup_dir)?;
    let home = gpg_home(&cfg.backup_dir)?;
    let key_file = home.0.join("cle.asc");
    std::fs::write(&key_file, armored)?;
    // Le code de sortie de l'import peut signaler un agent absent (sans
    // objet : aucune clé privée) : c'est la liste des clés qui fait foi.
    let out = gpg(cfg, &home.0).arg("--import").arg(&key_file).output().map_err(|e| anyhow::anyhow!("gpg introuvable ({}) : {e}", cfg.gpg_bin))?;
    let imported = gpg(cfg, &home.0).arg("--with-colons").arg("--list-keys").output()?;
    if !String::from_utf8_lossy(&imported.stdout).lines().any(|l| l.starts_with("pub:")) {
        anyhow::bail!("clé illisible par gpg : {}", String::from_utf8_lossy(&out.stderr).trim());
    }
    let secret = gpg(cfg, &home.0).arg("--with-colons").arg("--list-secret-keys").output()?;
    if String::from_utf8_lossy(&secret.stdout).lines().any(|l| l.starts_with("sec")) {
        anyhow::bail!("le texte contient une clé privée : collez uniquement la clé publique");
    }
    let list = gpg(cfg, &home.0).arg("--with-colons").arg("--list-keys").output()?;
    let text = String::from_utf8_lossy(&list.stdout).to_string();
    let pubs: Vec<&str> = text.lines().filter(|l| l.starts_with("pub:")).collect();
    if pubs.len() != 1 {
        anyhow::bail!("une seule clé publique attendue ({} trouvées)", pubs.len());
    }
    let caps = pubs[0].split(':').nth(11).unwrap_or_default();
    if !caps.contains('E') {
        anyhow::bail!("cette clé ne permet pas de chiffrer (capacité « E » absente)");
    }
    if pubs[0].split(':').nth(1) == Some("e") || pubs[0].split(':').nth(1) == Some("r") {
        anyhow::bail!("clé expirée ou révoquée");
    }
    let fpr = text
        .lines()
        .find(|l| l.starts_with("fpr:"))
        .and_then(|l| l.split(':').nth(9))
        .map(str::to_string)
        .ok_or_else(|| anyhow::anyhow!("empreinte introuvable"))?;
    let uid = text.lines().find(|l| l.starts_with("uid:")).and_then(|l| l.split(':').nth(9)).unwrap_or_default().to_string();
    Ok((fpr, uid))
}

/// Écrit un flux chiffré dans `out` : `feed` reçoit l'entrée de gpg.
fn encrypt_to(
    cfg: &Config,
    home: &Path,
    fingerprint: &str,
    out: &Path,
    feed: impl FnOnce(&mut std::process::ChildStdin) -> anyhow::Result<()>,
) -> anyhow::Result<()> {
    let partial = out.with_extension("partiel");
    let mut child = gpg(cfg, home)
        .args(["--yes", "--quiet", "--trust-model", "always", "--encrypt", "--recipient", fingerprint, "--output"])
        .arg(&partial)
        .stdin(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| anyhow::anyhow!("gpg introuvable ({}) : {e}", cfg.gpg_bin))?;
    let mut stdin = child.stdin.take().expect("entrée de gpg");
    let fed = feed(&mut stdin);
    drop(stdin);
    let output = child.wait_with_output()?;
    if let Err(e) = fed {
        let _ = std::fs::remove_file(&partial);
        return Err(e);
    }
    if !output.status.success() {
        let _ = std::fs::remove_file(&partial);
        anyhow::bail!("chiffrement en échec : {}", String::from_utf8_lossy(&output.stderr).trim());
    }
    std::fs::rename(&partial, out)?;
    Ok(())
}

/// Ajoute récursivement `src` sous le nom `name` dans l'archive.
fn append_tree<W: Write>(tar: &mut tar::Builder<W>, name: &Path, src: &Path, skip: &[PathBuf]) -> std::io::Result<()> {
    if skip.iter().any(|s| s == src) {
        return Ok(());
    }
    let meta = std::fs::metadata(src)?;
    if meta.is_dir() {
        tar.append_dir(name, src)?;
        let mut entries: Vec<_> = std::fs::read_dir(src)?.filter_map(Result::ok).collect();
        entries.sort_by_key(|e| e.file_name());
        for e in entries {
            let fname = e.file_name();
            if fname.to_string_lossy().ends_with(".partiel") {
                continue;
            }
            append_tree(tar, &name.join(&fname), &e.path(), skip)?;
        }
    } else if meta.is_file() {
        tar.append_path_with_name(src, name)?;
    }
    Ok(())
}

fn sha256_file(path: &Path) -> anyhow::Result<(String, u64)> {
    let mut f = std::fs::File::open(path)?;
    let mut h = Sha256::new();
    let n = std::io::copy(&mut f, &mut h)?;
    Ok((hex::encode(h.finalize()), n))
}

/// Exécute une sauvegarde complète (bloquant). Rend les fichiers produits et
/// les avertissements (CA de Caddy absente…).
pub fn run_blocking(cfg: &Config, settings: &BackupSettings, stamp: &str) -> anyhow::Result<(Vec<Value>, Vec<String>)> {
    let dir = &cfg.backup_dir;
    std::fs::create_dir_all(dir)?;
    let home = gpg_home(dir)?;
    let key_file = home.0.join("cle.asc");
    std::fs::write(&key_file, &settings.public_key)?;
    let imported = gpg(cfg, &home.0).arg("--import").arg(&key_file).output()?;
    let listed = gpg(cfg, &home.0).arg("--with-colons").arg("--list-keys").output()?;
    if !String::from_utf8_lossy(&listed.stdout).lines().any(|l| l.starts_with("fpr:") && l.contains(&settings.fingerprint)) {
        anyhow::bail!("clé publique illisible : {}", String::from_utf8_lossy(&imported.stderr).trim());
    }
    let fpr = settings.fingerprint.as_str();
    let mut warnings = Vec::new();
    let mut produced: Vec<PathBuf> = Vec::new();

    // 1. Base : pg_dump → gpg.
    let dump = dir.join(format!("cellulo-{stamp}.dump.gpg"));
    encrypt_to(cfg, &home.0, fpr, &dump, |stdin| {
        let mut pg = Command::new(&cfg.pg_dump_bin);
        pg.arg("--format=custom").arg(format!("--dbname={}", cfg.database_url)).stdout(Stdio::piped()).stderr(Stdio::piped());
        if let Some(p) = &cfg.database_password {
            pg.env("PGPASSWORD", p);
        }
        let mut child = pg.spawn().map_err(|e| anyhow::anyhow!("pg_dump introuvable ({}) : {e}", cfg.pg_dump_bin))?;
        let mut out = child.stdout.take().expect("sortie de pg_dump");
        std::io::copy(&mut out, stdin)?;
        let status = child.wait_with_output()?;
        if !status.status.success() {
            anyhow::bail!("pg_dump en échec : {}", String::from_utf8_lossy(&status.stderr).trim());
        }
        Ok(())
    })?;
    produced.push(dump);

    // 2. Documents (sans les clés ni les sauvegardes elles-mêmes).
    let docs = dir.join(format!("documents-{stamp}.tar.gz.gpg"));
    let data_dir = cfg.data_dir.clone();
    let skip = vec![data_dir.join("secrets"), cfg.backup_dir.clone()];
    encrypt_to(cfg, &home.0, fpr, &docs, |stdin| {
        let gz = flate2::write::GzEncoder::new(stdin, flate2::Compression::default());
        let mut tar = tar::Builder::new(gz);
        tar.follow_symlinks(false);
        if data_dir.is_dir() {
            let mut entries: Vec<_> = std::fs::read_dir(&data_dir)?.filter_map(Result::ok).collect();
            entries.sort_by_key(|e| e.file_name());
            for e in entries {
                append_tree(&mut tar, Path::new(&e.file_name()), &e.path(), &skip)?;
            }
        }
        tar.into_inner()?.finish()?;
        Ok(())
    })?;
    produced.push(docs);

    // 3. Secrets : clés Graph, adresse publique, CA de Caddy si lisible.
    let secrets = dir.join(format!("secrets-{stamp}.tar.gz.gpg"));
    let host = cfg.public_url.trim_start_matches("https://").trim_start_matches("http://").split('/').next().unwrap_or_default().to_string();
    let env_text = format!(
        "# Paramètres non secrets de l'instance sauvegardée (pas de mot de passe).\nSERVER_ADDRESS={host}\nPUBLIC_URL={}\n",
        cfg.public_url
    );
    let caddy_pki = cfg.caddy_data_dir.as_ref().map(|d| d.join("caddy").join("pki"));
    let ca_readable = caddy_pki
        .as_ref()
        .map(|p| std::fs::read(p.join("authorities").join("local").join("root.key")).is_ok())
        .unwrap_or(false);
    if !ca_readable {
        warnings.push(
            "Autorité de certification HTTPS (Caddy) non incluse : sauvegardez-la une fois avec deploy/backup.sh. \
             Sans elle, une restauration impose de réinstaller le certificat racine sur les PC."
                .to_string(),
        );
    }
    let graph_dir = cfg.data_dir.join("secrets");
    encrypt_to(cfg, &home.0, fpr, &secrets, |stdin| {
        let gz = flate2::write::GzEncoder::new(stdin, flate2::Compression::default());
        let mut tar = tar::Builder::new(gz);
        let mut header = tar::Header::new_gnu();
        header.set_size(env_text.len() as u64);
        header.set_mode(0o600);
        header.set_mtime(Utc::now().timestamp() as u64);
        header.set_cksum();
        tar.append_data(&mut header, "cfg/.env", env_text.as_bytes())?;
        if graph_dir.is_dir() {
            append_tree(&mut tar, Path::new("cfg/secrets"), &graph_dir, &[])?;
        }
        if let (true, Some(pki)) = (ca_readable, caddy_pki.as_ref()) {
            append_tree(&mut tar, Path::new("caddy/caddy/pki"), pki, &[])?;
        }
        tar.into_inner()?.finish()?;
        Ok(())
    })?;
    produced.push(secrets);

    // 4. Empreintes.
    let mut files = Vec::new();
    let mut sums = String::new();
    for p in &produced {
        let (sha, size) = sha256_file(p)?;
        let name = p.file_name().unwrap_or_default().to_string_lossy().to_string();
        sums.push_str(&format!("{sha}  {name}\n"));
        files.push(json!({ "name": name, "size": size, "sha256": sha }));
    }
    let sums_path = dir.join(format!("sha256-{stamp}.txt"));
    std::fs::write(&sums_path, &sums)?;
    let (sha, size) = sha256_file(&sums_path)?;
    files.push(json!({ "name": format!("sha256-{stamp}.txt"), "size": size, "sha256": sha }));
    Ok((files, warnings))
}

/// Cycle du worker : sauvegarde demandée, ou quotidienne à l'heure choisie.
pub async fn tick(state: &AppState) -> anyhow::Result<()> {
    let cfg = state.cfg();
    // Une sauvegarde interrompue par un redémarrage n'est jamais « réussie ».
    sqlx::query(
        "UPDATE backup_runs SET status = 'echec', error = 'interrompue (redémarrage du worker)', finished_at = now()
          WHERE status = 'en_cours' AND started_at < now() - interval '3 hours'",
    )
    .execute(&state.pool)
    .await?;
    let Some(settings) = cfg.backup.clone() else {
        sqlx::query("UPDATE backup_runs SET status = 'echec', error = 'sauvegarde non configurée', finished_at = now() WHERE status = 'demande'")
            .execute(&state.pool)
            .await?;
        return Ok(());
    };
    let local = crate::documents::to_algiers(Utc::now());
    if local.hour() >= settings.hour {
        // Sauvegarde du jour (heure locale) déjà faite ou en cours ?
        let done: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM backup_runs WHERE trigger = 'auto' AND status <> 'echec'
                              AND (requested_at AT TIME ZONE 'Africa/Algiers')::date = (now() AT TIME ZONE 'Africa/Algiers')::date)",
        )
        .fetch_one(&state.pool)
        .await?;
        let failures_today: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM backup_runs WHERE trigger = 'auto' AND status = 'echec'
                AND (requested_at AT TIME ZONE 'Africa/Algiers')::date = (now() AT TIME ZONE 'Africa/Algiers')::date",
        )
        .fetch_one(&state.pool)
        .await?;
        if !done && failures_today < 3 {
            let _ = sqlx::query("INSERT INTO backup_runs (trigger) VALUES ('auto') ON CONFLICT DO NOTHING").execute(&state.pool).await;
        }
    }
    let picked: Option<Uuid> = sqlx::query_scalar(
        "UPDATE backup_runs SET status = 'en_cours', started_at = now(), fingerprint = $1
          WHERE id = (SELECT id FROM backup_runs WHERE status = 'demande' ORDER BY requested_at LIMIT 1 FOR UPDATE SKIP LOCKED)
          RETURNING id",
    )
    .bind(&settings.fingerprint)
    .fetch_optional(&state.pool)
    .await?;
    let Some(run_id) = picked else { return Ok(()) };
    let stamp = local.format("%Y%m%d-%H%M%S").to_string();
    sqlx::query("UPDATE backup_runs SET stamp = $2 WHERE id = $1").bind(run_id).bind(&stamp).execute(&state.pool).await?;
    let pool = state.pool.clone();
    let job_cfg = cfg.clone();
    // Tâche séparée : une longue sauvegarde ne retarde pas les avis.
    tokio::spawn(async move {
        let stamp2 = stamp.clone();
        let result = tokio::task::spawn_blocking(move || run_blocking(&job_cfg, &settings, &stamp2)).await;
        let outcome = match result {
            Ok(Ok((files, warnings))) => {
                sqlx::query("UPDATE backup_runs SET status = 'reussie', files = $2, warnings = $3, finished_at = now() WHERE id = $1")
                    .bind(run_id)
                    .bind(Value::Array(files))
                    .bind(json!(warnings))
                    .execute(&pool)
                    .await
            }
            Ok(Err(e)) => fail(&pool, run_id, &e.to_string()).await,
            Err(e) => fail(&pool, run_id, &format!("tâche interrompue : {e}")).await,
        };
        if let Err(e) = outcome {
            tracing::error!(error = ?e, "enregistrement du résultat de sauvegarde en échec");
        }
        if let Err(e) = purge(&pool, &cfg).await {
            tracing::error!(error = ?e, "rotation des sauvegardes en échec");
        }
    });
    Ok(())
}

async fn fail(pool: &sqlx::PgPool, id: Uuid, error: &str) -> Result<sqlx::postgres::PgQueryResult, sqlx::Error> {
    tracing::error!(%error, "sauvegarde en échec");
    sqlx::query("UPDATE backup_runs SET status = 'echec', error = $2, finished_at = now() WHERE id = $1")
        .bind(id)
        .bind(error.chars().take(2000).collect::<String>())
        .execute(pool)
        .await
}

/// Ne garde sur le serveur que les `keep` dernières sauvegardes réussies.
async fn purge(pool: &sqlx::PgPool, cfg: &Config) -> anyhow::Result<()> {
    let Some(settings) = cfg.backup.as_ref() else { return Ok(()) };
    let old: Vec<(Uuid, Value)> = sqlx::query_as(
        "SELECT id, files FROM backup_runs WHERE status = 'reussie' AND purged_at IS NULL
          ORDER BY finished_at DESC OFFSET $1",
    )
    .bind(settings.keep as i64)
    .fetch_all(pool)
    .await?;
    for (id, files) in old {
        for f in files.as_array().cloned().unwrap_or_default() {
            if let Some(name) = f["name"].as_str().filter(|n| is_backup_file(n)) {
                let _ = std::fs::remove_file(cfg.backup_dir.join(name));
            }
        }
        sqlx::query("UPDATE backup_runs SET purged_at = now() WHERE id = $1").bind(id).execute(pool).await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn noms_telechargeables() {
        assert!(super::is_backup_file("cellulo-20261116-020000.dump.gpg"));
        assert!(super::is_backup_file("sha256-20261116-020000.txt"));
        assert!(!super::is_backup_file("../.env"));
        assert!(!super::is_backup_file("cle.asc"));
        assert!(!super::is_backup_file("documents-x/../../a.gpg"));
    }
}
