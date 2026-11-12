//! Règles métier pures de Celluloplast | Parcours salarié.
//!
//! Ce crate ne dépend ni de la base ni du réseau : il porte les invariants
//! (rôles, types de dossier, routage des besoins, transitions de tâches,
//! normalisations) et se teste sans infrastructure.

pub mod cases;
pub mod movements;
pub mod needs;
pub mod roles;
pub mod tasks;
pub mod text;

pub use cases::{CaseKind, CaseStatus};
pub use needs::{NeedAnswer, NeedCategory, PcPreference};
pub use roles::Role;
pub use tasks::{TaskAction, TaskResolution, TaskStatus};

/// Erreur de règle métier, traduite en 409/422 par l'API.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RuleError {
    #[error("{field} : {message}")]
    Invalid { field: &'static str, message: String },
    #[error("{0}")]
    Conflict(String),
}

impl RuleError {
    pub fn invalid(field: &'static str, message: impl Into<String>) -> Self {
        RuleError::Invalid { field, message: message.into() }
    }
}

/// Déclare une énumération stockée en base sous forme de texte.
macro_rules! text_enum {
    ($(#[$meta:meta])* $name:ident { $($variant:ident => $text:literal),+ $(,)? }) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
        pub enum $name {
            $(#[serde(rename = $text)] $variant),+
        }

        impl $name {
            pub const ALL: &'static [$name] = &[$($name::$variant),+];

            pub fn as_str(self) -> &'static str {
                match self { $($name::$variant => $text),+ }
            }

            pub fn parse(value: &str) -> Option<Self> {
                match value { $($text => Some($name::$variant),)+ _ => None }
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(self.as_str())
            }
        }
    };
}
pub(crate) use text_enum;
