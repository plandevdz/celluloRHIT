//! Amorçage : premier administrateur et jeu de démonstration fictif.

use sqlx::PgPool;
use uuid::Uuid;

use crate::auth;
use crate::config::Config;

pub async fn create_admin(pool: &PgPool, config: &Config, email: &str, name: &str) -> anyhow::Result<String> {
    let email = email.trim().to_lowercase();
    anyhow::ensure!(email.contains('@'), "adresse email invalide");
    let name = domain::text::clean(name).ok_or_else(|| anyhow::anyhow!("nom obligatoire"))?;
    let (token, hash) = auth::new_token();
    let mut tx = pool.begin().await?;
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO users (email, display_name, activation_token_hash, activation_expires_at)
         VALUES ($1, $2, $3, now() + interval '72 hours')
         ON CONFLICT ((lower(email))) DO UPDATE
            SET activation_token_hash = EXCLUDED.activation_token_hash,
                activation_expires_at = EXCLUDED.activation_expires_at, active = true
         RETURNING id",
    )
    .bind(&email)
    .bind(&name)
    .bind(&hash)
    .fetch_one(&mut *tx)
    .await?;
    sqlx::query("INSERT INTO user_roles (user_id, role) VALUES ($1, 'ADMIN') ON CONFLICT DO NOTHING")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    sqlx::query(
        "INSERT INTO audit_events (actor_id, action, entity, entity_id, correlation_id, data)
         VALUES (NULL, 'creation_admin_console', 'user', $1, gen_random_uuid(), jsonb_build_object('email', $2::text))",
    )
    .bind(id)
    .bind(&email)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(format!("{}/activation?token={token}", config.public_url.trim_end_matches('/')))
}

/// Données FICTIVES pour la démonstration et la recette (05-Recette.md).
/// Refuse de s'exécuter si des salariés existent déjà.
pub async fn seed_demo(pool: &PgPool) -> anyhow::Result<()> {
    let password = std::env::var("DEMO_PASSWORD")
        .map_err(|_| anyhow::anyhow!("définissez DEMO_PASSWORD (mot de passe des comptes de démonstration)"))?;
    auth::check_password_policy(&password).map_err(|e| anyhow::anyhow!("DEMO_PASSWORD : {e}"))?;
    let existing: i64 = sqlx::query_scalar("SELECT count(*) FROM employees").fetch_one(pool).await?;
    anyhow::ensure!(existing == 0, "la base contient déjà des salariés : démonstration refusée");

    let hash = auth::hash_password(password).await?;
    let mut tx = pool.begin().await?;

    let oran: Uuid = sqlx::query_scalar(
        "INSERT INTO sites (code, name) VALUES ('ORAN', 'Oran — Usine') ON CONFLICT (code) DO UPDATE SET name = EXCLUDED.name RETURNING id",
    )
    .fetch_one(&mut *tx)
    .await?;
    let alger: Uuid = sqlx::query_scalar(
        "INSERT INTO sites (code, name) VALUES ('ALGER', 'Alger — Siège') ON CONFLICT (code) DO UPDATE SET name = EXCLUDED.name RETURNING id",
    )
    .fetch_one(&mut *tx)
    .await?;
    for (site, name) in [(oran, "Atelier"), (oran, "Bureau IT"), (alger, "Open space"), (alger, "Salle serveur")] {
        sqlx::query("INSERT INTO locations (site_id, name) VALUES ($1, $2) ON CONFLICT DO NOTHING")
            .bind(site)
            .bind(name)
            .execute(&mut *tx)
            .await?;
    }

    let users: &[(&str, &str, &[&str])] = &[
        ("admin.demo@exemple.test", "Admin Démo", &["ADMIN"]),
        ("rh.demo@exemple.test", "Samira RH (démo)", &["RH"]),
        ("it.demo@exemple.test", "Karim IT (démo)", &["IT"]),
        ("it2.demo@exemple.test", "Nadia IT (démo)", &["IT"]),
        ("it.formation.demo@exemple.test", "Yacine IT + Formation (démo)", &["IT", "FORMATION"]),
        ("formation.demo@exemple.test", "Lina Formation (démo)", &["FORMATION"]),
        ("graphiste.demo@exemple.test", "Omar Graphiste (démo)", &["GRAPHISTE"]),
    ];
    for (email, name, roles) in users {
        let id: Uuid = sqlx::query_scalar(
            "INSERT INTO users (email, display_name, password_hash) VALUES ($1, $2, $3)
             ON CONFLICT ((lower(email))) DO UPDATE SET password_hash = EXCLUDED.password_hash RETURNING id",
        )
        .bind(email)
        .bind(name)
        .bind(&hash)
        .fetch_one(&mut *tx)
        .await?;
        for role in *roles {
            sqlx::query("INSERT INTO user_roles (user_id, role) VALUES ($1, $2) ON CONFLICT DO NOTHING")
                .bind(id)
                .bind(role)
                .execute(&mut *tx)
                .await?;
        }
    }

    let assets: &[(&str, &str, &str, &str, Uuid)] = &[
        ("PC-042", "pc_portable", "Portable Dell Latitude 5440", "DL5440-7H2K9Q3", oran),
        ("PC-043", "pc_fixe", "PC fixe HP ProDesk 400", "HPPD400-55XZ1", oran),
        ("PC-044", "pc_portable", "Portable Lenovo ThinkPad E14", "LNV-E14-8812", alger),
        ("TEL-010", "telephone", "Samsung Galaxy A35", "R58W30ABCDE", oran),
    ];
    for (inv, kind, label, serial, site) in assets {
        sqlx::query(
            "INSERT INTO assets (inventory_no, qr_token, kind, label, serial_no, site_id)
             VALUES ($1, 'CP-DEMO' || replace($1, '-', ''), $2, $3, $4, $5) ON CONFLICT DO NOTHING",
        )
        .bind(inv)
        .bind(kind)
        .bind(label)
        .bind(serial)
        .bind(site)
        .execute(&mut *tx)
        .await?;
    }
    for (label, qty) in [("Chargeur PC portable", 10), ("Souris USB", 20), ("Sacoche PC", 5)] {
        sqlx::query("INSERT INTO accessories (label, site_id, quantity_on_hand) VALUES ($1, $2, $3) ON CONFLICT DO NOTHING")
            .bind(label)
            .bind(oran)
            .bind(qty)
            .execute(&mut *tx)
            .await?;
    }
    let line: Uuid = sqlx::query_scalar(
        "INSERT INTO mobile_lines (number, operator) VALUES ('0550 00 00 01', 'Mobilis (fictif)')
         ON CONFLICT (number) DO UPDATE SET operator = EXCLUDED.operator RETURNING id",
    )
    .fetch_one(&mut *tx)
    .await?;
    sqlx::query("INSERT INTO sim_cards (iccid, line_id) VALUES ('8921300000000000001', $1) ON CONFLICT DO NOTHING")
        .bind(line)
        .execute(&mut *tx)
        .await?;
    for name in ["Sécurité au poste de travail", "Prise en main de Xena", "Qualité et hygiène"] {
        sqlx::query("INSERT INTO training_catalog (name) VALUES ($1) ON CONFLICT DO NOTHING")
            .bind(name)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;

    println!("Jeu de démonstration FICTIF chargé. Comptes (mot de passe = DEMO_PASSWORD) :");
    for (email, _, roles) in users {
        println!("  {email:<36} {}", roles.join(" + "));
    }
    Ok(())
}
