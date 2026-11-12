use crate::text_enum;

text_enum! {
    /// Les cinq rôles applicatifs, cumulables pour un même utilisateur.
    Role {
        Admin => "ADMIN",
        It => "IT",
        Rh => "RH",
        Graphiste => "GRAPHISTE",
        Formation => "FORMATION",
    }
}

impl Role {
    pub fn label(self) -> &'static str {
        match self {
            Role::Admin => "Administration",
            Role::It => "IT",
            Role::Rh => "Ressources humaines",
            Role::Graphiste => "Graphiste",
            Role::Formation => "Formation",
        }
    }
}

/// Vrai si l'utilisateur possède au moins un des rôles demandés.
pub fn has_any(user_roles: &[Role], wanted: &[Role]) -> bool {
    user_roles.iter().any(|r| wanted.contains(r))
}

/// Rôles autorisés à gérer le personnel et à annoncer des parcours.
pub const HR_MANAGERS: &[Role] = &[Role::Rh, Role::Admin];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exactement_cinq_roles() {
        assert_eq!(Role::ALL.len(), 5);
        for r in Role::ALL {
            assert_eq!(Role::parse(r.as_str()), Some(*r));
        }
        assert_eq!(Role::parse("MANAGER"), None);
    }

    #[test]
    fn cumul_de_roles() {
        let user = [Role::It, Role::Formation];
        assert!(has_any(&user, &[Role::Formation]));
        assert!(!has_any(&user, HR_MANAGERS));
    }
}
