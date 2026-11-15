//! Reprise des listes SharePoint : correspondance des valeurs de choix de
//! l'ancienne application vers les codes de la base. Une valeur inconnue
//! n'est jamais devinée : elle renvoie `None` et la ligne est à revoir.

use crate::text::fold_char;

/// Forme comparable d'un libellé : minuscules, sans accents latins, espaces
/// réduits, apostrophes et tirets neutralisés.
pub fn fold(raw: &str) -> String {
    raw.chars()
        .map(fold_char)
        .map(|c| if c == '\u{2019}' || c == '-' || c == '_' { ' ' } else { c })
        .collect::<String>()
        .to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn pick(raw: &str, table: &[(&str, &'static str)]) -> Option<&'static str> {
    let f = fold(raw);
    table.iter().find(|(k, _)| fold(k) == f).map(|(_, v)| *v)
}

/// Catégorie de MaterielIT → type d'actif.
pub fn asset_kind(raw: &str) -> Option<&'static str> {
    pick(
        raw,
        &[
            ("PC portable", "pc_portable"),
            ("Portable", "pc_portable"),
            ("PC fixe", "pc_fixe"),
            ("Smartphone", "telephone"),
            ("Téléphone", "telephone"),
            ("Tablette", "tablette"),
            ("Écran", "ecran"),
            ("Imprimante", "imprimante"),
            ("Scanner", "scanner"),
            ("Serveur", "serveur"),
            ("Onduleur", "onduleur"),
            ("Réseau", "reseau"),
            ("Pointeuse", "pointeuse"),
            ("Vidéoprojecteur", "videoprojecteur"),
            ("Destructeur", "destructeur"),
            ("Vidéosurveillance", "videosurveillance"),
            ("Accessoire", "autre"),
            ("Autre", "autre"),
        ],
    )
}

/// Statut de MaterielIT → statut d'actif. « Attribué » n'est pas repris
/// tel quel : la garde se reconstitue depuis le détenteur (AttribueA ou
/// Affectations), jamais par un simple statut.
pub fn asset_status(raw: &str) -> Option<&'static str> {
    pick(
        raw,
        &[
            ("En stock", "disponible"),
            ("Disponible", "disponible"),
            ("Attribué", "disponible"),
            ("En service", "en_service"),
            ("En réparation", "reparation"),
            ("À contrôler", "controle"),
            ("Réformé", "reforme"),
        ],
    )
}

pub fn asset_condition(raw: &str) -> Option<&'static str> {
    pick(raw, &[("Neuf", "neuf"), ("Bon", "bon"), ("Usé", "use"), ("Use", "use"), ("HS", "hs"), ("Hors service", "hs")])
}

pub fn line_type(raw: &str) -> Option<&'static str> {
    pick(raw, &[("Voix", "voix"), ("Data", "data"), ("Voix+Data", "voix_data"), ("Voix + Data", "voix_data")])
}

/// Statut de LignesMobiles. « Attribuée » se reconstitue par la garde de la SIM.
pub fn line_status(raw: &str) -> Option<&'static str> {
    pick(
        raw,
        &[("Disponible", "active"), ("Attribuée", "active"), ("Active", "active"), ("Suspendue", "suspendue"), ("Résiliée", "resiliee")],
    )
}

/// Statut de Personnel. « En départ » reste actif : le départ se gère par
/// un dossier OFF, pas par un statut importé.
pub fn employee_status(raw: &str) -> Option<&'static str> {
    pick(raw, &[("À venir", "a_venir"), ("Actif", "actif"), ("En départ", "actif"), ("Sorti", "sorti")])
}

pub fn contract_type(raw: &str) -> Option<&'static str> {
    pick(
        raw,
        &[
            ("CDI", "cdi"),
            ("CDD", "cdd"),
            ("Stage", "stage"),
            ("Intérim", "interim"),
            ("Apprentissage", "apprentissage"),
            ("Consultant", "consultant"),
        ],
    )
}

pub fn training_category(raw: &str) -> Option<&'static str> {
    pick(raw, &[("Produit", "produit"), ("Solution digitale", "solution_digitale"), ("QHSE", "qhse"), ("Autre", "autre")])
}

pub fn training_status(raw: &str) -> Option<&'static str> {
    pick(
        raw,
        &[("Planifiée", "planifiee"), ("Réalisée", "realisee"), ("Reportée", "reportee"), ("Dispensée", "dispensee")],
    )
}

pub fn training_evaluation(raw: &str) -> Option<&'static str> {
    pick(raw, &[("Acquis", "acquis"), ("À revoir", "a_revoir")])
}

pub fn vehicle_kind(raw: &str) -> Option<&'static str> {
    pick(raw, &[("Propriété", "propriete"), ("Location", "location"), ("Loué", "location")])
}

/// Statut de Vehicules. « Attribué » se reconstitue par la garde.
pub fn vehicle_status(raw: &str) -> Option<&'static str> {
    pick(
        raw,
        &[
            ("Disponible", "disponible"),
            ("Attribué", "disponible"),
            ("En entretien", "immobilise"),
            ("Rendu au loueur", "rendu_loueur"),
            ("Réformé", "reforme"),
        ],
    )
}

/// Équipe de UtilisateursApp → rôle applicatif. Manager et Moyens
/// généraux ne sont pas des rôles de l'application.
pub fn team_role(raw: &str) -> Option<&'static str> {
    pick(raw, &[("RH", "RH"), ("IT", "IT"), ("Graphiste", "GRAPHISTE"), ("Formateur", "FORMATION"), ("Formation", "FORMATION")])
}

/// Case à cocher ou choix Oui/Non. Vide → `None` (à préciser).
pub fn boolean(raw: &str) -> Option<bool> {
    match fold(raw).as_str() {
        "oui" | "yes" | "true" | "vrai" | "1" | "x" => Some(true),
        "non" | "no" | "false" | "faux" | "0" => Some(false),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn libelles_sharepoint() {
        assert_eq!(asset_kind("PC portable"), Some("pc_portable"));
        assert_eq!(asset_kind("  smartphone "), Some("telephone"));
        assert_eq!(asset_kind("Vidéosurveillance"), Some("videosurveillance"));
        assert_eq!(asset_kind("Videosurveillance"), Some("videosurveillance"));
        assert_eq!(asset_kind("Grille-pain"), None);
        assert_eq!(asset_status("À contrôler"), Some("controle"));
        assert_eq!(asset_status("Attribué"), Some("disponible"));
        assert_eq!(line_type("Voix+Data"), Some("voix_data"));
        assert_eq!(employee_status("En départ"), Some("actif"));
        assert_eq!(contract_type("Intérim"), Some("interim"));
        assert_eq!(team_role("Formateur"), Some("FORMATION"));
        assert_eq!(team_role("Manager"), None);
        assert_eq!(team_role("Moyens généraux"), None);
    }

    #[test]
    fn booleens_vide_a_preciser() {
        assert_eq!(boolean("Oui"), Some(true));
        assert_eq!(boolean("FAUX"), Some(false));
        assert_eq!(boolean(""), None);
        assert_eq!(boolean("peut-être"), None);
    }
}
