//! Clés d'idempotence : même clé + même contenu = même résultat ;
//! même clé + autre contenu = refus. Enregistrées dans la transaction métier.

use axum::http::{HeaderMap, StatusCode};
use serde_json::Value;
use sha2::{Digest, Sha256};
use sqlx::PgConnection;
use uuid::Uuid;

use crate::error::{AppError, AppResult};

pub const HEADER: &str = "idempotency-key";

pub enum Begin {
    Fresh,
    Replay(StatusCode, Value),
}

pub fn key_from(headers: &HeaderMap) -> AppResult<Option<String>> {
    match headers.get(HEADER) {
        None => Ok(None),
        Some(v) => {
            let k = v.to_str().map_err(|_| AppError::invalid("Idempotency-Key", "clé illisible"))?.trim();
            if k.is_empty() || k.len() > 100 {
                return Err(AppError::invalid("Idempotency-Key", "clé vide ou trop longue"));
            }
            Ok(Some(k.to_string()))
        }
    }
}

pub fn require_key(headers: &HeaderMap) -> AppResult<String> {
    key_from(headers)?.ok_or_else(|| AppError::invalid("Idempotency-Key", "en-tête obligatoire pour cette commande"))
}

pub fn hash_request(endpoint: &str, body: &Value) -> String {
    let mut h = Sha256::new();
    h.update(endpoint.as_bytes());
    h.update(b"\n");
    h.update(serde_json::to_vec(body).unwrap_or_default());
    hex::encode(h.finalize())
}

pub async fn begin(
    conn: &mut PgConnection,
    user_id: Uuid,
    key: &str,
    endpoint: &str,
    request_hash: &str,
) -> AppResult<Begin> {
    // Une insertion concurrente avec la même clé attend la fin de la
    // première transaction puis tombe dans la branche « déjà vue ».
    let inserted = sqlx::query(
        "INSERT INTO command_keys (user_id, key, endpoint, request_hash) VALUES ($1, $2, $3, $4)
         ON CONFLICT (user_id, key) DO NOTHING",
    )
    .bind(user_id)
    .bind(key)
    .bind(endpoint)
    .bind(request_hash)
    .execute(&mut *conn)
    .await?
    .rows_affected();
    if inserted == 1 {
        return Ok(Begin::Fresh);
    }
    let (stored_endpoint, stored_hash, status, body): (String, String, Option<i32>, Option<Value>) =
        sqlx::query_as(
            "SELECT endpoint, request_hash, response_status, response_body FROM command_keys
              WHERE user_id = $1 AND key = $2",
        )
        .bind(user_id)
        .bind(key)
        .fetch_one(&mut *conn)
        .await?;
    if stored_endpoint != endpoint || stored_hash != request_hash {
        return Err(AppError::invalid(
            "Idempotency-Key",
            "cette clé a déjà servi pour une autre commande",
        ));
    }
    match (status, body) {
        (Some(s), Some(b)) => Ok(Begin::Replay(
            StatusCode::from_u16(s as u16).unwrap_or(StatusCode::OK),
            b,
        )),
        _ => Err(AppError::conflict("commande identique déjà en cours de traitement")),
    }
}

pub async fn finish(
    conn: &mut PgConnection,
    user_id: Uuid,
    key: &str,
    status: StatusCode,
    body: &Value,
) -> AppResult<()> {
    sqlx::query(
        "UPDATE command_keys SET response_status = $3, response_body = $4 WHERE user_id = $1 AND key = $2",
    )
    .bind(user_id)
    .bind(key)
    .bind(status.as_u16() as i32)
    .bind(body)
    .execute(conn)
    .await?;
    Ok(())
}
