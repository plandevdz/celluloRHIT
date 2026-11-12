use crate::{text_enum, RuleError};

text_enum! {
    TaskStatus {
        AFaire => "a_faire",
        EnCours => "en_cours",
        Bloque => "bloque",
        Termine => "termine",
        Annule => "annule",
    }
}

impl TaskStatus {
    pub fn label(self) -> &'static str {
        match self {
            TaskStatus::AFaire => "À prendre",
            TaskStatus::EnCours => "En cours",
            TaskStatus::Bloque => "En attente",
            TaskStatus::Termine => "Terminée",
            TaskStatus::Annule => "Annulée",
        }
    }

    pub fn is_open(self) -> bool {
        !matches!(self, TaskStatus::Termine | TaskStatus::Annule)
    }
}

text_enum! {
    /// Comment une tâche a été soldée. Une exception n'est jamais comptée
    /// comme du matériel remis.
    TaskResolution {
        Remis => "remis",
        DejaDetenu => "deja_detenu",
        Exception => "exception",
        Realise => "realise",
    }
}

impl TaskResolution {
    pub fn label(self) -> &'static str {
        match self {
            TaskResolution::Remis => "Remis",
            TaskResolution::DejaDetenu => "Déjà détenu",
            TaskResolution::Exception => "Exception motivée",
            TaskResolution::Realise => "Réalisé",
        }
    }
}

/// Actions possibles sur une tâche.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskAction {
    Claim,
    Release,
    Block,
    Unblock,
    Complete,
}

/// Transition autorisée ? `is_owner` : l'acteur est le propriétaire actuel.
/// La prise (claim) est en plus garantie atomique par la base.
pub fn transition(
    from: TaskStatus,
    action: TaskAction,
    is_owner: bool,
) -> Result<TaskStatus, RuleError> {
    use TaskAction::*;
    use TaskStatus::*;
    let next = match (from, action) {
        (AFaire, Claim) => EnCours,
        (EnCours, Release) | (Bloque, Release) if is_owner => AFaire,
        (EnCours, Block) if is_owner => Bloque,
        (Bloque, Unblock) if is_owner => EnCours,
        (EnCours, Complete) if is_owner => Termine,
        (EnCours | Bloque, _) if !is_owner => {
            return Err(RuleError::Conflict(
                "cette tâche est prise en charge par une autre personne".into(),
            ))
        }
        (s, a) => {
            return Err(RuleError::Conflict(format!(
                "action {a:?} impossible sur une tâche « {} »",
                s.label()
            )))
        }
    };
    Ok(next)
}

#[cfg(test)]
mod tests {
    use super::*;
    use TaskAction::*;
    use TaskStatus::*;

    #[test]
    fn cycle_nominal() {
        assert_eq!(transition(AFaire, Claim, false), Ok(EnCours));
        assert_eq!(transition(EnCours, Block, true), Ok(Bloque));
        assert_eq!(transition(Bloque, Unblock, true), Ok(EnCours));
        assert_eq!(transition(EnCours, Complete, true), Ok(Termine));
    }

    #[test]
    fn pas_de_prise_concurrente() {
        assert!(transition(EnCours, Claim, false).is_err());
        assert!(transition(EnCours, Complete, false).is_err());
    }

    #[test]
    fn terminee_est_figee() {
        assert!(transition(Termine, Release, true).is_err());
        assert!(transition(Annule, Claim, false).is_err());
    }
}
