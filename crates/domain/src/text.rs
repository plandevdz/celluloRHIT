//! Normalisations de texte : matricule et clé de ressemblance des noms.

/// Matricule normalisé pour l'unicité : sans espaces, en majuscules.
/// Les zéros initiaux sont conservés (c'est du texte). Vide → `None`.
pub fn normalize_matricule(raw: &str) -> Option<String> {
    let n: String = raw.chars().filter(|c| !c.is_whitespace()).collect::<String>().to_uppercase();
    if n.is_empty() {
        None
    } else {
        Some(n)
    }
}

/// Nettoie une saisie libre : espaces de bord retirés, espaces internes
/// réduits. Vide → `None`.
pub fn clean(raw: &str) -> Option<String> {
    let s = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    if s.is_empty() {
        None
    } else {
        Some(s)
    }
}

/// Clé de ressemblance « prénom nom » : minuscules, sans accents latins,
/// sans ponctuation. Sert uniquement à proposer des homonymes ; ce n'est
/// jamais une clé d'identité.
pub fn name_key(first_name: &str, last_name: &str) -> String {
    let mut words: Vec<String> = format!("{first_name} {last_name}")
        .split(|c: char| c.is_whitespace() || c == '-' || c == '\'' || c == '’')
        .filter(|w| !w.is_empty())
        .map(|w| w.chars().map(fold_char).collect::<String>().to_lowercase())
        .collect();
    // L'ordre prénom/nom est souvent inversé dans les saisies.
    words.sort();
    words.join(" ")
}

fn fold_char(c: char) -> char {
    match c {
        'à' | 'â' | 'ä' | 'á' | 'ã' | 'À' | 'Â' | 'Ä' | 'Á' => 'a',
        'é' | 'è' | 'ê' | 'ë' | 'É' | 'È' | 'Ê' | 'Ë' => 'e',
        'î' | 'ï' | 'í' | 'Î' | 'Ï' => 'i',
        'ô' | 'ö' | 'ó' | 'Ô' | 'Ö' => 'o',
        'ù' | 'û' | 'ü' | 'ú' | 'Ù' | 'Û' | 'Ü' => 'u',
        'ç' | 'Ç' => 'c',
        'ÿ' => 'y',
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matricule_garde_les_zeros() {
        assert_eq!(normalize_matricule(" 00 42a "), Some("0042A".into()));
        assert_eq!(normalize_matricule("   "), None);
    }

    #[test]
    fn homonymes_detectes_malgre_accents_et_ordre() {
        assert_eq!(name_key("Amine", "Benali"), name_key("BENALI", "amine"));
        assert_eq!(name_key("Hélène", "Aït-Ahmed"), name_key("Helene", "Ait Ahmed"));
        assert_ne!(name_key("Amine", "Benali"), name_key("Amina", "Benali"));
    }

    #[test]
    fn noms_arabes_preserves() {
        assert_eq!(clean("  محمد   أمين "), Some("محمد أمين".into()));
        assert_eq!(name_key("محمد", "أمين"), name_key("أمين", "محمد"));
    }
}
