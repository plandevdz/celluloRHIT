use crate::text_enum;

text_enum! {
    /// Type de dossier : arrivée, mutation, départ, demande ponctuelle.
    CaseKind {
        Onb => "ONB",
        Mut => "MUT",
        Off => "OFF",
        Dem => "DEM",
    }
}

text_enum! {
    CaseStatus {
        Ouvert => "ouvert",
        Clos => "clos",
        Annule => "annule",
    }
}

impl CaseKind {
    /// Nom de la séquence PostgreSQL qui numérote ce type de dossier.
    pub fn sequence_name(self) -> &'static str {
        match self {
            CaseKind::Onb => "case_ref_onb",
            CaseKind::Mut => "case_ref_mut",
            CaseKind::Off => "case_ref_off",
            CaseKind::Dem => "case_ref_dem",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            CaseKind::Onb => "Arrivée",
            CaseKind::Mut => "Mutation",
            CaseKind::Off => "Départ",
            CaseKind::Dem => "Demande ponctuelle",
        }
    }
}

/// Référence publique lisible : `ONB-00007`. Le numéro vient toujours d'une
/// séquence serveur, jamais d'un MAX+1.
pub fn format_reference(kind: CaseKind, number: i64) -> String {
    format!("{}-{:05}", kind.as_str(), number)
}

/// Référence d'un mouvement de parc (remise ou restitution).
pub fn format_movement_reference(number: i64) -> String {
    format!("MVT-{:05}", number)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reference_lisible() {
        assert_eq!(format_reference(CaseKind::Onb, 7), "ONB-00007");
        assert_eq!(format_reference(CaseKind::Dem, 123456), "DEM-123456");
        assert_eq!(format_movement_reference(42), "MVT-00042");
    }
}
