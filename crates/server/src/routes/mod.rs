pub mod admin;
pub mod assets;
pub mod auth;
pub mod cases;
pub mod departures;
pub mod employees;
pub mod imports;
pub mod inventory;
pub mod movements;
pub mod parametres;
pub mod referentials;
pub mod reports;
pub mod reprise;
pub mod software;
pub mod tasks;
pub mod trainings;
pub mod transfers;
pub mod vehicles;

use axum::extract::{Request, State};
use axum::http::{Method, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use serde_json::json;

use crate::config::Module;
use crate::state::AppState;

/// En-tête obligatoire sur les commandes : un formulaire d'un autre site ne
/// peut pas l'ajouter sans autorisation CORS (jamais accordée). Complète le
/// cookie `SameSite=Strict`.
pub const CLIENT_HEADER: &str = "x-cellulo-client";

async fn require_client_header(req: Request, next: Next) -> Response {
    let safe = matches!(*req.method(), Method::GET | Method::HEAD | Method::OPTIONS);
    if !safe && !req.headers().contains_key(CLIENT_HEADER) {
        return (
            StatusCode::FORBIDDEN,
            Json(json!({ "error": "interdit", "message": "requête refusée : client non reconnu" })),
        )
            .into_response();
    }
    next.run(req).await
}

async fn health(State(state): State<AppState>) -> Response {
    let db = sqlx::query_scalar::<_, i32>("SELECT 1").fetch_one(&state.pool).await.is_ok();
    let status = if db { StatusCode::OK } else { StatusCode::SERVICE_UNAVAILABLE };
    (
        status,
        Json(json!({
            "status": if db { "ok" } else { "base indisponible" },
            "version": crate::VERSION,
            "mail_mode": state.cfg().mail_mode.as_str(),
            "archive_mode": state.cfg().archive_mode.as_str(),
            "modules_desactives": state.cfg().disabled_modules.iter().map(|m| m.as_str()).collect::<Vec<_>>(),
        })),
    )
        .into_response()
}

/// Module facultatif auquel appartient une route de l'API (chemin relatif à
/// `/api/v1`). `None` : cœur de l'application, toujours ouvert. La lecture
/// `/reports/overview` reste ouverte : l'accueil RH y lit les signatures en
/// attente ; seul l'export des états est rattaché au module « États ».
pub fn module_of(path: &str) -> Option<Module> {
    let p = path.strip_prefix("/api/v1").unwrap_or(path);
    let seg: Vec<&str> = p.trim_start_matches('/').split('/').collect();
    let employee_sub = |name: &str| seg.len() == 3 && seg[0] == "employees" && seg[2] == name;
    match seg.first().copied().unwrap_or_default() {
        "vehicles" => Some(Module::Vehicules),
        "software" | "license-allocations" => Some(Module::Logiciels),
        "inventories" => Some(Module::Inventaire),
        "assets" if seg.get(1) == Some(&"labels.pdf") => Some(Module::Inventaire),
        "transfers" => Some(Module::Mutation),
        "departures" | "exit-controls" => Some(Module::Depart),
        "reports" if seg.get(1) == Some(&"export.xlsx") => Some(Module::Etats),
        _ if employee_sub("vehicles") => Some(Module::Vehicules),
        _ if employee_sub("licenses") => Some(Module::Logiciels),
        // Correction de poste datée : outil du module Mutation.
        _ if employee_sub("positions") => Some(Module::Mutation),
        _ => None,
    }
}

/// Refuse les routes d'un module désactivé, quel que soit le rôle (liens
/// directs, anciens favoris, appels manuels).
async fn module_guard(State(state): State<AppState>, req: Request, next: Next) -> Response {
    if let Some(m) = module_of(req.uri().path()) {
        if !state.cfg().module_on(m) {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({
                    "error": "module_desactive",
                    "message": format!("« {} » n'est pas ouvert dans cette version (pilote).", m.label()),
                })),
            )
                .into_response();
        }
    }
    next.run(req).await
}

async fn not_found() -> Response {
    (StatusCode::NOT_FOUND, Json(json!({ "error": "introuvable", "message": "route inconnue" }))).into_response()
}

pub fn api_router(state: &AppState) -> Router<AppState> {
    Router::new()
        .route("/health", get(health))
        .merge(auth::routes())
        .merge(admin::routes())
        .merge(parametres::routes())
        .merge(employees::routes())
        .merge(cases::routes())
        .merge(tasks::routes())
        .merge(trainings::routes())
        .merge(assets::routes())
        .merge(movements::routes(state.cfg().max_upload_bytes))
        .merge(imports::routes(state.cfg().max_upload_bytes))
        .merge(reprise::routes(state.cfg().max_upload_bytes))
        .merge(vehicles::routes())
        .merge(referentials::routes())
        .merge(software::routes())
        .merge(inventory::routes())
        .merge(transfers::routes())
        .merge(departures::routes())
        .merge(reports::routes())
        .fallback(not_found)
        .layer(middleware::from_fn_with_state(state.clone(), module_guard))
        .layer(middleware::from_fn(require_client_header))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn routes_rattachees_aux_modules() {
        assert_eq!(module_of("/vehicles"), Some(Module::Vehicules));
        assert_eq!(module_of("/api/v1/vehicles/42/handover"), Some(Module::Vehicules));
        assert_eq!(module_of("/employees/1/vehicles"), Some(Module::Vehicules));
        assert_eq!(module_of("/employees/1/licenses"), Some(Module::Logiciels));
        assert_eq!(module_of("/license-allocations/1/release"), Some(Module::Logiciels));
        assert_eq!(module_of("/assets/labels.pdf"), Some(Module::Inventaire));
        assert_eq!(module_of("/inventories/1/scan"), Some(Module::Inventaire));
        assert_eq!(module_of("/transfers"), Some(Module::Mutation));
        assert_eq!(module_of("/employees/1/positions"), Some(Module::Mutation));
        assert_eq!(module_of("/exit-controls/1/confirm"), Some(Module::Depart));
        assert_eq!(module_of("/reports/export.xlsx"), Some(Module::Etats));
        // Le cœur reste ouvert : scan d'un bien à la remise, accueil, fiche.
        assert_eq!(module_of("/assets/lookup/PC-1"), None);
        assert_eq!(module_of("/assets"), None);
        assert_eq!(module_of("/reports/overview"), None);
        assert_eq!(module_of("/employees/1"), None);
        assert_eq!(module_of("/employees/1/holdings"), None);
        assert_eq!(module_of("/handovers"), None);
        assert_eq!(module_of("/returns"), None);
    }

    #[test]
    fn liste_des_modules() {
        assert_eq!(Module::parse_list("pilote").unwrap().len(), 6);
        assert_eq!(Module::parse_list("").unwrap().len(), 0);
        assert_eq!(Module::parse_list("Vehicules, depart").unwrap(), vec![Module::Depart, Module::Vehicules]);
        assert!(Module::parse_list("vehicule").is_err(), "faute de frappe : refus au démarrage");
    }
}
