//! Inventaire QR : classement d'un pointage. Un scan n'est jamais une
//! affectation ; « non vu » ne veut pas dire perdu.

use crate::text_enum;

text_enum! {
    ScanResult {
        /// Attendu dans ce périmètre et vu au bon endroit (ou sans local attendu).
        Vu => "vu",
        /// Attendu, mais vu dans un autre local que celui attendu.
        Ailleurs => "ailleurs",
        /// Actif connu, hors du périmètre figé de la campagne.
        HorsPerimetre => "hors_perimetre",
        /// Code inconnu : n'ouvre rien, reste à examiner.
        Inconnu => "inconnu",
        /// Déjà pointé dans ce local : conservé, non compté.
        Doublon => "doublon",
    }
}

impl ScanResult {
    pub fn message(self) -> &'static str {
        match self {
            ScanResult::Vu => "Vu au bon endroit",
            ScanResult::Ailleurs => "Vu ailleurs que l'emplacement attendu : écart à examiner",
            ScanResult::HorsPerimetre => "Actif hors du périmètre de la campagne : écart à examiner",
            ScanResult::Inconnu => "Code inconnu : aucun actif ne porte ce code",
            ScanResult::Doublon => "Déjà pointé dans ce local",
        }
    }
}

/// Ce que l'on sait de l'actif scanné dans la campagne.
pub enum Known<T: PartialEq> {
    /// Aucun actif ne porte ce code.
    Unknown,
    /// Actif connu, hors du périmètre figé.
    OutOfScope,
    /// Attendu, avec son local attendu s'il en a un.
    Expected(Option<T>),
}

/// Classe un pointage. `already_here` : l'actif a déjà un pointage utile
/// dans ce local pour cette campagne.
pub fn classify<T: PartialEq>(known: Known<T>, scanned_location: &T, already_here: bool) -> ScanResult {
    match known {
        Known::Unknown => ScanResult::Inconnu,
        _ if already_here => ScanResult::Doublon,
        Known::OutOfScope => ScanResult::HorsPerimetre,
        Known::Expected(None) => ScanResult::Vu,
        Known::Expected(Some(loc)) if &loc == scanned_location => ScanResult::Vu,
        Known::Expected(Some(_)) => ScanResult::Ailleurs,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classement_des_pointages() {
        let a = 1;
        assert_eq!(classify(Known::Expected(Some(1)), &a, false), ScanResult::Vu);
        assert_eq!(classify(Known::Expected(Some(1)), &a, true), ScanResult::Doublon);
        assert_eq!(classify(Known::Expected(Some(2)), &a, false), ScanResult::Ailleurs);
        assert_eq!(classify(Known::<i32>::Expected(None), &a, false), ScanResult::Vu);
        assert_eq!(classify(Known::<i32>::OutOfScope, &a, false), ScanResult::HorsPerimetre);
        assert_eq!(classify(Known::<i32>::Unknown, &a, true), ScanResult::Inconnu);
    }
}
