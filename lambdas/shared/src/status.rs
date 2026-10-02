//! Cykl życia assetu (rozdział 5 dokumentu projektu).

use serde::{Deserialize, Serialize};

/// Status assetu przechowywany w tabeli `assets`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AssetStatus {
    Uploading,
    Quarantined,
    Scanning,
    Rejected,
    Infected,
    ScanFailed,
    CleanDraft,
    Published,
    Archived,
}

/// Próba niedozwolonego przejścia statusu.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("niedozwolone przejście statusu: {from:?} -> {to:?}")]
pub struct TransitionError {
    pub from: AssetStatus,
    pub to: AssetStatus,
}

impl AssetStatus {
    pub const ALL: [Self; 9] = [
        Self::Uploading,
        Self::Quarantined,
        Self::Scanning,
        Self::Rejected,
        Self::Infected,
        Self::ScanFailed,
        Self::CleanDraft,
        Self::Published,
        Self::Archived,
    ];

    /// Nazwa statusu tak, jak jest zapisywana w DynamoDB.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Uploading => "UPLOADING",
            Self::Quarantined => "QUARANTINED",
            Self::Scanning => "SCANNING",
            Self::Rejected => "REJECTED",
            Self::Infected => "INFECTED",
            Self::ScanFailed => "SCAN_FAILED",
            Self::CleanDraft => "CLEAN_DRAFT",
            Self::Published => "PUBLISHED",
            Self::Archived => "ARCHIVED",
        }
    }

    /// Statusy, z których wolno przejść do `self`.
    ///
    /// Używane do budowy `ConditionExpression` przy warunkowym zapisie
    /// w DynamoDB, dzięki czemu przejścia są idempotentne i nie da się np.
    /// opublikować zainfekowanego pliku.
    #[must_use]
    pub const fn allowed_predecessors(self) -> &'static [Self] {
        match self {
            Self::Uploading => &[],
            Self::Quarantined => &[Self::Uploading],
            Self::Scanning => &[Self::Quarantined, Self::ScanFailed],
            // Odrzucenie już przy uploadzie: rozmiar niezgodny z deklaracją.
            Self::Rejected => &[Self::Uploading, Self::Scanning],
            Self::Infected | Self::ScanFailed | Self::CleanDraft => &[Self::Scanning],
            Self::Published => &[Self::CleanDraft, Self::Archived],
            Self::Archived => &[Self::Published],
        }
    }

    #[must_use]
    pub fn can_transition_to(self, next: Self) -> bool {
        next.allowed_predecessors().contains(&self)
    }

    /// Sprawdza przejście `self -> next`.
    ///
    /// # Errors
    ///
    /// Zwraca [`TransitionError`], gdy przejście nie jest dozwolone.
    pub fn transition_to(self, next: Self) -> Result<Self, TransitionError> {
        if self.can_transition_to(next) {
            Ok(next)
        } else {
            Err(TransitionError { from: self, to: next })
        }
    }
}

impl std::fmt::Display for AssetStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::AssetStatus::{self, *};

    #[test]
    fn serializes_like_dynamodb_value() {
        for status in AssetStatus::ALL {
            let json = serde_json::to_string(&status).unwrap();
            assert_eq!(json, format!("\"{}\"", status.as_str()));
        }
    }

    #[test]
    fn happy_path_is_allowed() {
        let path = [
            Uploading,
            Quarantined,
            Scanning,
            CleanDraft,
            Published,
            Archived,
            Published,
        ];
        for pair in path.windows(2) {
            assert!(pair[0].can_transition_to(pair[1]), "{} -> {}", pair[0], pair[1]);
        }
    }

    #[test]
    fn infected_can_never_be_published() {
        assert!(Infected.transition_to(Published).is_err());
        assert!(Infected.transition_to(CleanDraft).is_err());
        assert!(Infected.transition_to(Scanning).is_err());
    }

    #[test]
    fn scan_failed_can_only_be_retried() {
        let allowed: Vec<_> = AssetStatus::ALL
            .into_iter()
            .filter(|next| ScanFailed.can_transition_to(*next))
            .collect();
        assert_eq!(allowed, vec![Scanning]);
    }

    #[test]
    fn upload_can_be_rejected_before_scanning() {
        assert!(Uploading.can_transition_to(Rejected));
        assert!(!Uploading.can_transition_to(CleanDraft));
        assert!(!Uploading.can_transition_to(Scanning));
    }

    #[test]
    fn terminal_statuses_have_no_successors() {
        for terminal in [Rejected, Infected] {
            assert!(
                AssetStatus::ALL
                    .into_iter()
                    .all(|next| !terminal.can_transition_to(next))
            );
        }
    }
}
