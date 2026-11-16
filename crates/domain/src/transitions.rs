//! Mutation (M05) et départ (M06) : règles pures.

use crate::{text_enum, NeedCategory, RuleError};

text_enum! {
    /// Action proposée pour un élément lors d'une mutation.
    DeltaAction {
        Conserver => "conserver",
        Ajouter => "ajouter",
        Reprendre => "reprendre",
        Verifier => "verifier",
    }
}

impl DeltaAction {
    pub fn label(self) -> &'static str {
        match self {
            DeltaAction::Conserver => "Conserver",
            DeltaAction::Ajouter => "Ajouter",
            DeltaAction::Reprendre => "Reprendre",
            DeltaAction::Verifier => "Vérifier",
        }
    }
}

/// Un élément détenu au moment de la comparaison.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Held {
    /// Catégorie de besoin correspondante, si elle existe (PC, Xena…).
    pub category: Option<NeedCategory>,
    pub is_vehicle: bool,
}

/// Cible connue : besoins du profil cible (catégories). `None` : aucune
/// cible connue pour cet élément.
pub type Target<'a> = Option<&'a [NeedCategory]>;

/// Proposition pour un élément détenu. Sans cible connue : « à vérifier ».
/// Un véhicule qui n'est plus prévu est à reprendre (tâche RH) ; un accès
/// ou un équipement non prévu est à vérifier avant toute réduction.
pub fn propose_for_held(held: &Held, target: Target<'_>) -> DeltaAction {
    let Some(needs) = target else { return DeltaAction::Verifier };
    match held.category {
        Some(c) if needs.contains(&c) => DeltaAction::Conserver,
        _ if held.is_vehicle => DeltaAction::Reprendre,
        _ => DeltaAction::Verifier,
    }
}

/// Besoins du profil cible qui ne sont pas déjà couverts : à ajouter.
pub fn needs_to_add(target: &[NeedCategory], held: &[Held]) -> Vec<NeedCategory> {
    target
        .iter()
        .copied()
        .filter(|c| *c != NeedCategory::Formation && !held.iter().any(|h| h.category == Some(*c)))
        .collect()
}

/// La date d'effet doit suivre le début de la situation en cours.
pub fn check_effective_date(current_from: chrono_like::Date, effective: chrono_like::Date) -> Result<(), RuleError> {
    if effective <= current_from {
        return Err(RuleError::invalid("effective_date", "la date d'effet doit suivre le début du poste actuel"));
    }
    Ok(())
}

/// Dates sans dépendance : (année, mois, jour), comparables.
pub mod chrono_like {
    pub type Date = (i32, u32, u32);
}

text_enum! {
    /// État affiché d'un accès lors d'un départ : jamais « coupé » sans
    /// confirmation de l'IT.
    AccessState {
        APlanifier => "a_planifier",
        Exigible => "exigible",
        AVerifier => "a_verifier",
        Coupe => "coupe",
    }
}

/// `now`, `cutoff` et `grace` en secondes ; `confirmed` : coupure
/// confirmée par l'IT.
pub fn access_state(now: i64, cutoff: i64, grace_secs: i64, confirmed: bool) -> AccessState {
    if confirmed {
        AccessState::Coupe
    } else if now < cutoff {
        AccessState::APlanifier
    } else if now < cutoff + grace_secs {
        AccessState::Exigible
    } else {
        AccessState::AVerifier
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use NeedCategory::*;

    #[test]
    fn r09_xena_conserve_vehicule_repris() {
        let target = [Xena, Pc];
        let xena = Held { category: Some(Xena), is_vehicle: false };
        let vehicle = Held { category: Some(Vehicule), is_vehicle: true };
        let phone = Held { category: Some(Telephone), is_vehicle: false };
        assert_eq!(propose_for_held(&xena, Some(&target)), DeltaAction::Conserver);
        assert_eq!(propose_for_held(&vehicle, Some(&target)), DeltaAction::Reprendre);
        assert_eq!(propose_for_held(&phone, Some(&target)), DeltaAction::Verifier);
        assert_eq!(needs_to_add(&target, &[xena, vehicle, phone]), vec![Pc]);
    }

    #[test]
    fn sans_cible_a_verifier() {
        let held = Held { category: Some(EmailPro), is_vehicle: false };
        assert_eq!(propose_for_held(&held, None), DeltaAction::Verifier);
    }

    #[test]
    fn date_d_effet() {
        assert!(check_effective_date((2026, 1, 1), (2026, 1, 1)).is_err());
        assert!(check_effective_date((2026, 1, 1), (2026, 2, 1)).is_ok());
    }

    #[test]
    fn jamais_coupe_sans_confirmation() {
        assert_eq!(access_state(0, 100, 3600, false), AccessState::APlanifier);
        assert_eq!(access_state(200, 100, 3600, false), AccessState::Exigible);
        assert_eq!(access_state(10_000, 100, 3600, false), AccessState::AVerifier);
        assert_eq!(access_state(10_000, 100, 3600, true), AccessState::Coupe);
    }
}
