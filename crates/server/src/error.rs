use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::{json, Value};

/// Erreur d'API. Le corps est toujours `{ error, message, field?, detail? }`.
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("authentification requise")]
    Unauthorized,
    #[error("email ou mot de passe incorrect")]
    BadCredentials,
    #[error("{0}")]
    Forbidden(String),
    #[error("{0}")]
    NotFound(String),
    #[error("{message}")]
    Conflict { message: String, detail: Option<Value> },
    #[error("{0}")]
    VersionMismatch(String),
    #[error("{message}")]
    Validation { field: String, message: String },
    #[error("{0}")]
    TooManyRequests(String),
    #[error(transparent)]
    Internal(#[from] anyhow::Error),
}

pub type AppResult<T> = Result<T, AppError>;

impl AppError {
    pub fn forbidden() -> Self {
        AppError::Forbidden("action non autorisée pour vos rôles".into())
    }
    pub fn not_found(what: &str) -> Self {
        AppError::NotFound(format!("{what} introuvable"))
    }
    pub fn conflict(message: impl Into<String>) -> Self {
        AppError::Conflict { message: message.into(), detail: None }
    }
    pub fn invalid(field: impl Into<String>, message: impl Into<String>) -> Self {
        AppError::Validation { field: field.into(), message: message.into() }
    }
    pub fn stale() -> Self {
        AppError::VersionMismatch(
            "les données ont été modifiées entre-temps ; rechargez puis vérifiez avant de réessayer"
                .into(),
        )
    }
}

impl From<domain::RuleError> for AppError {
    fn from(e: domain::RuleError) -> Self {
        match e {
            domain::RuleError::Invalid { field, message } => AppError::invalid(field, message),
            domain::RuleError::Conflict(m) => AppError::conflict(m),
        }
    }
}

impl From<sqlx::Error> for AppError {
    fn from(e: sqlx::Error) -> Self {
        if let sqlx::Error::Database(db) = &e {
            // 23505 unique_violation, 23P01 exclusion_violation
            match db.code().as_deref() {
                Some("23505") => {
                    return AppError::Conflict {
                        message: "cet élément existe déjà ou est déjà utilisé".into(),
                        detail: db.constraint().map(|c| json!({ "constraint": c })),
                    }
                }
                Some("23P01") => {
                    return AppError::conflict("chevauchement de périodes avec une situation existante")
                }
                Some("23514") => {
                    return AppError::Conflict {
                        message: "règle d'intégrité non respectée".into(),
                        detail: db.constraint().map(|c| json!({ "constraint": c })),
                    }
                }
                _ => {}
            }
        }
        AppError::Internal(e.into())
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, code) = match &self {
            AppError::Unauthorized => (StatusCode::UNAUTHORIZED, "non_authentifie"),
            AppError::BadCredentials => (StatusCode::UNAUTHORIZED, "identifiants_incorrects"),
            AppError::Forbidden(_) => (StatusCode::FORBIDDEN, "interdit"),
            AppError::NotFound(_) => (StatusCode::NOT_FOUND, "introuvable"),
            AppError::Conflict { .. } => (StatusCode::CONFLICT, "conflit"),
            AppError::VersionMismatch(_) => (StatusCode::PRECONDITION_FAILED, "version_perimee"),
            AppError::Validation { .. } => (StatusCode::UNPROCESSABLE_ENTITY, "invalide"),
            AppError::TooManyRequests(_) => (StatusCode::TOO_MANY_REQUESTS, "trop_de_tentatives"),
            AppError::Internal(_) => (StatusCode::INTERNAL_SERVER_ERROR, "erreur_interne"),
        };
        let mut body = json!({ "error": code, "message": self.to_string() });
        match &self {
            AppError::Validation { field, .. } => body["field"] = json!(field),
            AppError::Conflict { detail: Some(d), .. } => body["detail"] = d.clone(),
            AppError::Internal(e) => {
                // Pas de détail technique côté navigateur ; trace côté serveur.
                tracing::error!(error = ?e, "erreur interne");
                body["message"] = json!("erreur interne, réessayez ou contactez l'administrateur");
            }
            _ => {}
        }
        (status, Json(body)).into_response()
    }
}
