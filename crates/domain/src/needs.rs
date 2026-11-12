use crate::{text_enum, Role, RuleError};

text_enum! {
    /// Besoins qu'un RH peut exprimer pour un salarié.
    NeedCategory {
        Pc => "pc",
        Telephone => "telephone",
        Sim => "sim",
        EmailPro => "email_pro",
        Xena => "xena",
        Intelix => "intelix",
        CarteVisite => "carte_visite",
        Vehicule => "vehicule",
        Formation => "formation",
    }
}

text_enum! {
    /// Réponse RH. Une absence de réponse vaut « à préciser », jamais « non ».
    NeedAnswer {
        Oui => "oui",
        Non => "non",
        APreciser => "a_preciser",
    }
}

text_enum! {
    /// Préférence de forme de PC, distincte du besoin de PC lui-même.
    PcPreference {
        Fixe => "fixe",
        Portable => "portable",
        SansPreference => "sans_preference",
    }
}

impl NeedCategory {
    /// Équipe responsable de la réalisation du besoin.
    pub fn responsible_role(self) -> Role {
        match self {
            NeedCategory::Pc
            | NeedCategory::Telephone
            | NeedCategory::Sim
            | NeedCategory::EmailPro
            | NeedCategory::Xena
            | NeedCategory::Intelix => Role::It,
            NeedCategory::CarteVisite => Role::Graphiste,
            NeedCategory::Vehicule => Role::Rh,
            NeedCategory::Formation => Role::Formation,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            NeedCategory::Pc => "PC",
            NeedCategory::Telephone => "Téléphone",
            NeedCategory::Sim => "SIM / ligne",
            NeedCategory::EmailPro => "Adresse email professionnelle",
            NeedCategory::Xena => "Accès Xena",
            NeedCategory::Intelix => "Accès Intelix",
            NeedCategory::CarteVisite => "Cartes de visite",
            NeedCategory::Vehicule => "Véhicule",
            NeedCategory::Formation => "Formations",
        }
    }

    /// Libellé de la tâche générée quand le besoin est confirmé.
    pub fn task_title(self) -> &'static str {
        match self {
            NeedCategory::Pc => "Préparer et remettre le PC",
            NeedCategory::Telephone => "Fournir le téléphone",
            NeedCategory::Sim => "Fournir la SIM / ligne",
            NeedCategory::EmailPro => "Créer l'adresse email professionnelle",
            NeedCategory::Xena => "Ouvrir l'accès Xena",
            NeedCategory::Intelix => "Ouvrir l'accès Intelix",
            NeedCategory::CarteVisite => "Réaliser les cartes de visite",
            NeedCategory::Vehicule => "Affecter un véhicule",
            NeedCategory::Formation => "Planifier les formations",
        }
    }

    /// Clé de génération unique par dossier : rejouer la commande ne crée pas
    /// de seconde tâche.
    pub fn task_key(self) -> String {
        format!("need:{}", self.as_str())
    }
}

/// Spécification d'une tâche à créer, indépendante du stockage.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskSpec {
    pub key: String,
    pub role: Role,
    pub title: String,
}

/// Besoin tel que saisi par le RH.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NeedInput {
    pub category: NeedCategory,
    pub answer: NeedAnswer,
    pub quantity: Option<i32>,
    pub preference: Option<PcPreference>,
    pub detail: Option<String>,
}

impl NeedInput {
    pub fn validate(&self) -> Result<(), RuleError> {
        if self.preference.is_some() && self.category != NeedCategory::Pc {
            return Err(RuleError::invalid(
                "preference",
                "la préférence fixe/portable ne s'applique qu'au PC",
            ));
        }
        if let Some(q) = self.quantity {
            if !(1..=999).contains(&q) {
                return Err(RuleError::invalid("quantity", "quantité entre 1 et 999"));
            }
        }
        Ok(())
    }
}

/// Tâche à générer pour un besoin. Seul un « oui » crée du travail ;
/// « non » et « à préciser » n'inventent ni compte ni matériel.
/// Les formations produisent une session par formation nommée, pas cette
/// tâche générique.
pub fn task_for_need(need: &NeedInput) -> Option<TaskSpec> {
    if need.answer != NeedAnswer::Oui || need.category == NeedCategory::Formation {
        return None;
    }
    Some(TaskSpec {
        key: need.category.task_key(),
        role: need.category.responsible_role(),
        title: need.category.task_title().to_string(),
    })
}

/// Tâche d'une session de formation nommée.
pub fn task_for_training(catalog_key: &str, training_name: &str) -> TaskSpec {
    TaskSpec {
        key: format!("formation:{catalog_key}"),
        role: Role::Formation,
        title: format!("Formation : {training_name}"),
    }
}

/// Équipes à prévenir à l'annonce d'une arrivée : IT et Formation toujours,
/// en parallèle, plus les équipes concernées par les besoins confirmés.
pub fn roles_to_notify_on_arrival(needs: &[NeedInput]) -> Vec<Role> {
    let mut roles = vec![Role::It, Role::Formation];
    for need in needs {
        if need.answer == NeedAnswer::Oui {
            let role = need.category.responsible_role();
            if !roles.contains(&role) {
                roles.push(role);
            }
        }
    }
    roles
}

#[cfg(test)]
mod tests {
    use super::*;

    fn need(category: NeedCategory, answer: NeedAnswer) -> NeedInput {
        NeedInput { category, answer, quantity: None, preference: None, detail: None }
    }

    #[test]
    fn seul_oui_genere_une_tache() {
        assert!(task_for_need(&need(NeedCategory::Pc, NeedAnswer::Non)).is_none());
        assert!(task_for_need(&need(NeedCategory::Pc, NeedAnswer::APreciser)).is_none());
        let t = task_for_need(&need(NeedCategory::Pc, NeedAnswer::Oui)).unwrap();
        assert_eq!(t.role, Role::It);
        assert_eq!(t.key, "need:pc");
    }

    #[test]
    fn routage_des_besoins() {
        assert_eq!(NeedCategory::CarteVisite.responsible_role(), Role::Graphiste);
        assert_eq!(NeedCategory::Vehicule.responsible_role(), Role::Rh);
        assert_eq!(NeedCategory::EmailPro.responsible_role(), Role::It);
    }

    #[test]
    fn formation_sans_tache_generique() {
        assert!(task_for_need(&need(NeedCategory::Formation, NeedAnswer::Oui)).is_none());
        let t = task_for_training("abc", "Sécurité");
        assert_eq!(t.role, Role::Formation);
        assert_eq!(t.key, "formation:abc");
    }

    #[test]
    fn annonce_previent_it_et_formation_meme_sans_besoin() {
        assert_eq!(roles_to_notify_on_arrival(&[]), vec![Role::It, Role::Formation]);
        let roles = roles_to_notify_on_arrival(&[need(NeedCategory::CarteVisite, NeedAnswer::Oui)]);
        assert_eq!(roles, vec![Role::It, Role::Formation, Role::Graphiste]);
    }

    #[test]
    fn preference_reservee_au_pc() {
        let mut n = need(NeedCategory::Telephone, NeedAnswer::Oui);
        n.preference = Some(PcPreference::Fixe);
        assert!(n.validate().is_err());
        n.category = NeedCategory::Pc;
        assert!(n.validate().is_ok());
    }
}
