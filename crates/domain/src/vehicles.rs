//! Parc véhicules RH : statuts, kilométrage et échéances.

use crate::{text_enum, RuleError};

text_enum! {
    VehicleKind {
        Propriete => "propriete",
        Location => "location",
    }
}

text_enum! {
    VehicleStatus {
        Disponible => "disponible",
        Affecte => "affecte",
        /// Au garage, en panne, sans papiers valides…
        Immobilise => "immobilise",
        /// Location rendue au loueur : sort définitivement des disponibilités.
        RenduLoueur => "rendu_loueur",
        Reforme => "reforme",
    }
}

impl VehicleStatus {
    pub fn label(self) -> &'static str {
        match self {
            VehicleStatus::Disponible => "Disponible",
            VehicleStatus::Affecte => "Affecté",
            VehicleStatus::Immobilise => "Immobilisé",
            VehicleStatus::RenduLoueur => "Rendu au loueur",
            VehicleStatus::Reforme => "Réformé",
        }
    }
}

/// Seul un véhicule disponible peut être remis.
pub fn can_hand_over(status: VehicleStatus) -> Result<(), RuleError> {
    match status {
        VehicleStatus::Disponible => Ok(()),
        other => Err(RuleError::Conflict(format!("véhicule non affectable : {}", other.label().to_lowercase()))),
    }
}

/// Changements de statut manuels : l'affectation passe par une remise, le
/// retour au loueur par sa commande dédiée.
pub fn manual_status_allowed(from: VehicleStatus, to: VehicleStatus) -> Result<(), RuleError> {
    use VehicleStatus::*;
    match (from, to) {
        (a, b) if a == b => Ok(()),
        (Affecte, _) | (_, Affecte) => Err(RuleError::invalid("status", "l'affectation passe par une remise ou une restitution")),
        (RenduLoueur, _) | (_, RenduLoueur) => {
            Err(RuleError::invalid("status", "le retour au loueur passe par sa commande dédiée et n'est pas réversible"))
        }
        (Disponible | Immobilise | Reforme, Disponible | Immobilise | Reforme) => Ok(()),
    }
}

/// Contrôle du compteur : un relevé inférieur au dernier connu est une
/// anomalie, acceptée seulement avec une justification.
/// Renvoie `true` si le relevé est une correction justifiée.
pub fn check_odometer(last_known: Option<i32>, reading: Option<i32>, justification: Option<&str>) -> Result<bool, RuleError> {
    match (last_known, reading) {
        (Some(last), Some(new)) if new < last => {
            if justification.map(|j| !j.trim().is_empty()).unwrap_or(false) {
                Ok(true)
            } else {
                Err(RuleError::invalid(
                    "odometer_km",
                    format!("kilométrage {new} inférieur au dernier relevé ({last}) : corrigez ou justifiez l'écart"),
                ))
            }
        }
        _ => Ok(false),
    }
}

/// Échéance kilométrique proche ou dépassée ?
pub fn km_due(odometer: Option<i32>, next_due_km: Option<i32>, warn_km: i32) -> bool {
    match (odometer, next_due_km) {
        (Some(o), Some(n)) => n - o <= warn_km,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rendu_au_loueur_non_affectable() {
        assert!(can_hand_over(VehicleStatus::Disponible).is_ok());
        assert!(can_hand_over(VehicleStatus::RenduLoueur).is_err());
        assert!(can_hand_over(VehicleStatus::Affecte).is_err());
        assert!(manual_status_allowed(VehicleStatus::RenduLoueur, VehicleStatus::Disponible).is_err());
        assert!(manual_status_allowed(VehicleStatus::Disponible, VehicleStatus::Immobilise).is_ok());
        assert!(manual_status_allowed(VehicleStatus::Disponible, VehicleStatus::Affecte).is_err());
    }

    #[test]
    fn kilometrage_incoherent_justifie() {
        assert_eq!(check_odometer(Some(10_000), Some(10_500), None), Ok(false));
        assert!(check_odometer(Some(10_000), Some(9_000), None).is_err());
        assert!(check_odometer(Some(10_000), Some(9_000), Some("  ")).is_err());
        assert_eq!(check_odometer(Some(10_000), Some(9_000), Some("compteur remplacé")), Ok(true));
        assert_eq!(check_odometer(None, Some(5), None), Ok(false));
    }

    #[test]
    fn vidange_au_kilometrage() {
        assert!(km_due(Some(14_200), Some(15_000), 1_000));
        assert!(!km_due(Some(13_000), Some(15_000), 1_000));
        assert!(km_due(Some(15_100), Some(15_000), 1_000));
        assert!(!km_due(None, Some(15_000), 1_000));
    }
}
