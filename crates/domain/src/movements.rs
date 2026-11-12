//! Remises et restitutions de biens.

use crate::{text_enum, RuleError};

text_enum! {
    MovementKind {
        Remise => "remise",
        Restitution => "restitution",
    }
}

text_enum! {
    /// Nature d'une ligne de mouvement : chaque type a sa clé étrangère,
    /// pas de référence libre invérifiable.
    ItemType {
        Asset => "asset",
        Accessoire => "accessoire",
        Sim => "sim",
    }
}

text_enum! {
    AssetStatus {
        Disponible => "disponible",
        Reserve => "reserve",
        Affecte => "affecte",
        Controle => "controle",
        Reparation => "reparation",
        Reforme => "reforme",
    }
}

text_enum! {
    AssetKind {
        PcPortable => "pc_portable",
        PcFixe => "pc_fixe",
        Telephone => "telephone",
        Ecran => "ecran",
        Imprimante => "imprimante",
        Autre => "autre",
    }
}

impl AssetKind {
    pub fn label(self) -> &'static str {
        match self {
            AssetKind::PcPortable => "PC portable",
            AssetKind::PcFixe => "PC fixe",
            AssetKind::Telephone => "Téléphone",
            AssetKind::Ecran => "Écran",
            AssetKind::Imprimante => "Imprimante",
            AssetKind::Autre => "Autre",
        }
    }
}

/// Un actif ne peut être remis que s'il est disponible ou réservé.
pub fn can_hand_over(status: AssetStatus) -> Result<(), RuleError> {
    match status {
        AssetStatus::Disponible | AssetStatus::Reserve => Ok(()),
        other => Err(RuleError::Conflict(format!(
            "actif non remettable : statut « {} »",
            other.as_str()
        ))),
    }
}

/// Un retour partiel ne peut dépasser le solde encore détenu.
pub fn check_return_quantity(requested: i32, open_balance: i32) -> Result<(), RuleError> {
    if requested < 1 {
        return Err(RuleError::invalid("quantity", "quantité rendue au moins égale à 1"));
    }
    if requested > open_balance {
        return Err(RuleError::invalid(
            "quantity",
            format!("quantité rendue ({requested}) supérieure au solde détenu ({open_balance})"),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retour_partiel_borne() {
        assert!(check_return_quantity(1, 2).is_ok());
        assert!(check_return_quantity(2, 2).is_ok());
        assert!(check_return_quantity(3, 2).is_err());
        assert!(check_return_quantity(0, 2).is_err());
    }

    #[test]
    fn actif_affecte_non_remettable() {
        assert!(can_hand_over(AssetStatus::Disponible).is_ok());
        assert!(can_hand_over(AssetStatus::Affecte).is_err());
        assert!(can_hand_over(AssetStatus::Reforme).is_err());
    }
}
