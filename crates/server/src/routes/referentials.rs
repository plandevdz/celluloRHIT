//! Référentiels : wilayas et profils de poste. Un profil suggère des
//! besoins à l'annonce ; le modifier ne change aucun dossier existant.

use axum::extract::{Path, State};
use axum::routing::{get, patch, put};
use axum::{Json, Router};
use domain::{NeedCategory, PcPreference, Role};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::PgConnection;
use uuid::Uuid;

use crate::audit::{self, Audit};
use crate::auth::CurrentUser;
use crate::error::{AppError, AppResult};
use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/wilayas", get(list_wilayas).post(create_wilaya))
        .route("/wilayas/{id}", patch(update_wilaya))
        .route("/job-profiles", get(list_profiles).post(create_profile))
        .route("/job-profiles/{id}", get(profile_detail))
        .route("/job-profiles/{id}", put(update_profile))
}

async fn list_wilayas(State(state): State<AppState>, _user: CurrentUser) -> AppResult<Json<Value>> {
    let v: Value = sqlx::query_scalar(
        "SELECT coalesce(jsonb_agg(jsonb_build_object('id', id, 'code', code, 'name', name, 'region', region, 'active', active)
                 ORDER BY code), '[]') FROM wilayas",
    )
    .fetch_one(&state.pool)
    .await?;
    Ok(Json(v))
}

#[derive(Deserialize)]
struct WilayaInput {
    code: i32,
    name: String,
    region: Option<String>,
    active: Option<bool>,
}

async fn create_wilaya(State(state): State<AppState>, user: CurrentUser, Json(i): Json<WilayaInput>) -> AppResult<Json<Value>> {
    user.require(&[Role::Admin])?;
    let name = domain::text::clean(&i.name).ok_or_else(|| AppError::invalid("name", "nom obligatoire"))?;
    let mut tx = state.pool.begin().await?;
    let id: Uuid = sqlx::query_scalar("INSERT INTO wilayas (code, name, region) VALUES ($1, $2, $3) RETURNING id")
        .bind(i.code)
        .bind(&name)
        .bind(i.region.as_deref().and_then(domain::text::clean))
        .fetch_one(&mut *tx)
        .await?;
    audit::record(&mut tx, &user, Audit { action: "wilaya_creee", entity: "wilaya", entity_id: Some(id), data: Some(json!({ "code": i.code, "name": name })), ..Default::default() }).await?;
    tx.commit().await?;
    Ok(Json(json!({ "id": id })))
}

async fn update_wilaya(State(state): State<AppState>, user: CurrentUser, Path(id): Path<Uuid>, Json(i): Json<WilayaInput>) -> AppResult<Json<Value>> {
    user.require(&[Role::Admin])?;
    let name = domain::text::clean(&i.name).ok_or_else(|| AppError::invalid("name", "nom obligatoire"))?;
    let mut tx = state.pool.begin().await?;
    let n = sqlx::query("UPDATE wilayas SET code = $2, name = $3, region = $4, active = coalesce($5, active) WHERE id = $1")
        .bind(id)
        .bind(i.code)
        .bind(&name)
        .bind(i.region.as_deref().and_then(domain::text::clean))
        .bind(i.active)
        .execute(&mut *tx)
        .await?
        .rows_affected();
    if n == 0 {
        return Err(AppError::not_found("wilaya"));
    }
    audit::record(&mut tx, &user, Audit { action: "wilaya_modifiee", entity: "wilaya", entity_id: Some(id), ..Default::default() }).await?;
    tx.commit().await?;
    Ok(Json(json!({ "ok": true })))
}

// ------------------------------------------------------------------ profils de poste

const PROFILE_JSON: &str = "jsonb_build_object(
    'id', p.id, 'name', p.name, 'department', p.department, 'description', p.description,
    'typical_titles', p.typical_titles, 'access_notes', p.access_notes, 'field_based', p.field_based,
    'active', p.active, 'version', p.version, 'updated_at', p.updated_at,
    'needs', (SELECT coalesce(jsonb_agg(jsonb_build_object('category', n.category, 'quantity', n.quantity,
                     'preference', n.preference, 'detail', n.detail)
                     ORDER BY array_position(ARRAY['pc','ecran','tablette','imprimante','accessoire','telephone','sim','email_pro','xena','intelix','carte_visite','vehicule'], n.category)), '[]')
                FROM profile_needs n WHERE n.profile_id = p.id),
    'trainings', (SELECT coalesce(jsonb_agg(jsonb_build_object('catalog_id', c.id, 'name', c.name) ORDER BY c.name), '[]')
                    FROM profile_trainings t JOIN training_catalog c ON c.id = t.catalog_id WHERE t.profile_id = p.id))";

async fn list_profiles(State(state): State<AppState>, user: CurrentUser) -> AppResult<Json<Value>> {
    user.require(&[Role::Rh, Role::Admin, Role::It])?;
    let v: Value = sqlx::query_scalar(&format!("SELECT coalesce(jsonb_agg({PROFILE_JSON} ORDER BY p.name), '[]') FROM job_profiles p"))
        .fetch_one(&state.pool)
        .await?;
    Ok(Json(v))
}

async fn profile_detail(State(state): State<AppState>, user: CurrentUser, Path(id): Path<Uuid>) -> AppResult<Json<Value>> {
    user.require(&[Role::Rh, Role::Admin, Role::It])?;
    let v: Option<Value> = sqlx::query_scalar(&format!("SELECT {PROFILE_JSON} FROM job_profiles p WHERE p.id = $1"))
        .bind(id)
        .fetch_optional(&state.pool)
        .await?;
    Ok(Json(v.ok_or_else(|| AppError::not_found("profil"))?))
}

#[derive(Deserialize)]
struct ProfileNeedInput {
    category: NeedCategory,
    quantity: Option<i32>,
    preference: Option<PcPreference>,
    detail: Option<String>,
}

#[derive(Deserialize)]
struct ProfileInput {
    version: Option<i32>,
    name: String,
    department: Option<String>,
    description: Option<String>,
    typical_titles: Option<String>,
    access_notes: Option<String>,
    #[serde(default)]
    field_based: bool,
    active: Option<bool>,
    #[serde(default)]
    needs: Vec<ProfileNeedInput>,
    #[serde(default)]
    trainings: Vec<Uuid>,
}

async fn write_profile_parts(conn: &mut PgConnection, id: Uuid, i: &ProfileInput) -> AppResult<()> {
    sqlx::query("DELETE FROM profile_needs WHERE profile_id = $1").bind(id).execute(&mut *conn).await?;
    sqlx::query("DELETE FROM profile_trainings WHERE profile_id = $1").bind(id).execute(&mut *conn).await?;
    for n in &i.needs {
        if n.category == NeedCategory::Formation {
            continue;
        }
        if n.preference.is_some() && n.category != NeedCategory::Pc {
            return Err(AppError::invalid("needs", "la préférence fixe/portable ne s'applique qu'au PC"));
        }
        sqlx::query(
            "INSERT INTO profile_needs (profile_id, category, quantity, preference, detail) VALUES ($1, $2, $3, $4, $5)
             ON CONFLICT (profile_id, category) DO NOTHING",
        )
        .bind(id)
        .bind(n.category.as_str())
        .bind(n.quantity)
        .bind(n.preference.map(|p| p.as_str()))
        .bind(n.detail.as_deref().and_then(domain::text::clean))
        .execute(&mut *conn)
        .await?;
    }
    for t in &i.trainings {
        sqlx::query("INSERT INTO profile_trainings (profile_id, catalog_id) VALUES ($1, $2) ON CONFLICT DO NOTHING")
            .bind(id)
            .bind(t)
            .execute(&mut *conn)
            .await?;
    }
    Ok(())
}

async fn create_profile(State(state): State<AppState>, user: CurrentUser, Json(i): Json<ProfileInput>) -> AppResult<Json<Value>> {
    user.require(domain::roles::HR_MANAGERS)?;
    let name = domain::text::clean(&i.name).ok_or_else(|| AppError::invalid("name", "intitulé obligatoire"))?;
    let mut tx = state.pool.begin().await?;
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO job_profiles (name, department, description, typical_titles, access_notes, field_based)
         VALUES ($1, $2, $3, $4, $5, $6) RETURNING id",
    )
    .bind(&name)
    .bind(i.department.as_deref().and_then(domain::text::clean))
    .bind(i.description.as_deref().and_then(domain::text::clean))
    .bind(i.typical_titles.as_deref().and_then(domain::text::clean))
    .bind(i.access_notes.as_deref().and_then(domain::text::clean))
    .bind(i.field_based)
    .fetch_one(&mut *tx)
    .await?;
    write_profile_parts(&mut tx, id, &i).await?;
    audit::record(&mut tx, &user, Audit { action: "profil_cree", entity: "job_profile", entity_id: Some(id), data: Some(json!({ "name": name })), ..Default::default() }).await?;
    tx.commit().await?;
    Ok(Json(json!({ "id": id })))
}

async fn update_profile(State(state): State<AppState>, user: CurrentUser, Path(id): Path<Uuid>, Json(i): Json<ProfileInput>) -> AppResult<Json<Value>> {
    user.require(domain::roles::HR_MANAGERS)?;
    let name = domain::text::clean(&i.name).ok_or_else(|| AppError::invalid("name", "intitulé obligatoire"))?;
    let mut tx = state.pool.begin().await?;
    let version: Option<i32> = sqlx::query_scalar("SELECT version FROM job_profiles WHERE id = $1 FOR UPDATE")
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?;
    let version = version.ok_or_else(|| AppError::not_found("profil"))?;
    if i.version != Some(version) {
        return Err(AppError::stale());
    }
    sqlx::query(
        "UPDATE job_profiles SET name = $2, department = $3, description = $4, typical_titles = $5, access_notes = $6,
                field_based = $7, active = coalesce($8, active), version = version + 1, updated_at = now()
          WHERE id = $1",
    )
    .bind(id)
    .bind(&name)
    .bind(i.department.as_deref().and_then(domain::text::clean))
    .bind(i.description.as_deref().and_then(domain::text::clean))
    .bind(i.typical_titles.as_deref().and_then(domain::text::clean))
    .bind(i.access_notes.as_deref().and_then(domain::text::clean))
    .bind(i.field_based)
    .bind(i.active)
    .execute(&mut *tx)
    .await?;
    write_profile_parts(&mut tx, id, &i).await?;
    audit::record(&mut tx, &user, Audit { action: "profil_modifie", entity: "job_profile", entity_id: Some(id), ..Default::default() }).await?;
    tx.commit().await?;
    Ok(Json(json!({ "version": version + 1 })))
}
