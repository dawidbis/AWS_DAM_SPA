//! Tożsamość wywołującego na podstawie claimów JWT z autoryzatora API Gateway.
//!
//! Autoryzator HTTP API zweryfikował już podpis, issuer i audience tokenu.
//! Tu tylko odczytujemy claimy i sprawdzamy grupy. Każda Lambda API sama
//! decyduje, które grupy mogą wykonać daną operację (rozdział 4).

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// Grupa Cognito odpowiadająca roli A–D.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum UserGroup {
    Admin,
    Staff,
    Contributor,
    Viewer,
}

impl UserGroup {
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "admin" => Some(Self::Admin),
            "staff" => Some(Self::Staff),
            "contributor" => Some(Self::Contributor),
            "viewer" => Some(Self::Viewer),
            _ => None,
        }
    }
}

/// Zalogowany użytkownik wywołujący API.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Caller {
    /// `sub` z Cognito: stały identyfikator użytkownika.
    pub sub: String,
    pub email: Option<String>,
    pub groups: Vec<UserGroup>,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AuthError {
    #[error("brak claimu sub w tokenie")]
    MissingSubject,
    #[error("brak wymaganej grupy")]
    Forbidden,
}

impl Caller {
    /// Buduje wywołującego z claimów przekazanych przez autoryzator JWT.
    ///
    /// # Errors
    ///
    /// [`AuthError::MissingSubject`], gdy brakuje `sub` (fail closed).
    pub fn from_claims(claims: &HashMap<String, String>) -> Result<Self, AuthError> {
        let sub = claims
            .get("sub")
            .filter(|sub| !sub.is_empty())
            .ok_or(AuthError::MissingSubject)?
            .clone();
        let email = claims.get("email").cloned();
        let groups = claims
            .get("cognito:groups")
            .map(|raw| parse_groups(raw))
            .unwrap_or_default();
        Ok(Self { sub, email, groups })
    }

    #[must_use]
    pub fn has_any_group(&self, allowed: &[UserGroup]) -> bool {
        self.groups.iter().any(|group| allowed.contains(group))
    }

    /// # Errors
    ///
    /// [`AuthError::Forbidden`], gdy użytkownik nie należy do żadnej z grup.
    pub fn require_any_group(&self, allowed: &[UserGroup]) -> Result<(), AuthError> {
        if self.has_any_group(allowed) {
            Ok(())
        } else {
            Err(AuthError::Forbidden)
        }
    }
}

/// Parsuje claim `cognito:groups`.
///
/// Autoryzator HTTP API spłaszcza tablice JSON do napisu w postaci
/// `[admin staff]`; obsługujemy też zwykłą tablicę JSON i listę po przecinku.
/// Nieznane nazwy grup są pomijane, wynik jest posortowany i bez duplikatów.
#[must_use]
pub fn parse_groups(raw: &str) -> Vec<UserGroup> {
    let trimmed = raw.trim().trim_start_matches('[').trim_end_matches(']');
    let mut groups: Vec<UserGroup> = trimmed
        .split(|c: char| c.is_whitespace() || c == ',')
        .map(|item| item.trim_matches('"'))
        .filter_map(UserGroup::parse)
        .collect();
    groups.sort();
    groups.dedup();
    groups
}

#[cfg(test)]
mod tests {
    use super::*;

    fn claims(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect()
    }

    #[test]
    fn parses_api_gateway_flattened_array() {
        assert_eq!(
            parse_groups("[contributor admin]"),
            vec![UserGroup::Admin, UserGroup::Contributor]
        );
    }

    #[test]
    fn parses_json_array_and_comma_list() {
        assert_eq!(
            parse_groups(r#"["staff","viewer"]"#),
            vec![UserGroup::Staff, UserGroup::Viewer]
        );
        assert_eq!(parse_groups("staff, staff"), vec![UserGroup::Staff]);
    }

    #[test]
    fn ignores_unknown_groups() {
        assert_eq!(parse_groups("[root Admin admin]"), vec![UserGroup::Admin]);
        assert_eq!(parse_groups(""), Vec::<UserGroup>::new());
        assert_eq!(parse_groups("[]"), Vec::<UserGroup>::new());
    }

    #[test]
    fn builds_caller_from_claims() {
        let caller = Caller::from_claims(&claims(&[
            ("sub", "abc-123"),
            ("email", "foto@example.com"),
            ("cognito:groups", "[contributor]"),
        ]))
        .unwrap();

        assert_eq!(caller.sub, "abc-123");
        assert_eq!(caller.email.as_deref(), Some("foto@example.com"));
        assert_eq!(caller.groups, vec![UserGroup::Contributor]);
        assert!(
            caller
                .require_any_group(&[UserGroup::Admin, UserGroup::Contributor])
                .is_ok()
        );
        assert_eq!(
            caller.require_any_group(&[UserGroup::Admin]),
            Err(AuthError::Forbidden)
        );
    }

    #[test]
    fn caller_without_groups_has_no_access() {
        let caller = Caller::from_claims(&claims(&[("sub", "abc")])).unwrap();
        assert_eq!(caller.groups, Vec::<UserGroup>::new());
        assert!(!caller.has_any_group(&[UserGroup::Viewer]));
    }

    #[test]
    fn missing_subject_fails_closed() {
        assert_eq!(Caller::from_claims(&claims(&[])), Err(AuthError::MissingSubject));
        assert_eq!(
            Caller::from_claims(&claims(&[("sub", "")])),
            Err(AuthError::MissingSubject)
        );
    }
}
