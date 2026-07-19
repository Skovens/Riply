use serde::{Deserialize, Serialize};
use std::fmt;

/// Represents the status of a file in the pipeline.
///
/// This enum defines the state machine for media files as they progress
/// through the scanning, analysis, review, and moving phases.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PipelineStatus {
    /// File has been scanned but not yet analyzed.
    Scanned,
    /// File has been successfully matched to a title.
    Matched,
    /// File needs manual review (confidence below threshold).
    NeedsReview,
    /// File analysis failed (no match found).
    Failed,
    /// File has been confirmed and is ready to be moved.
    Confirmed,
    /// File has been moved to its final destination.
    Moved,
}

impl fmt::Display for PipelineStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PipelineStatus::Scanned => write!(f, "Scanned"),
            PipelineStatus::Matched => write!(f, "Matched"),
            PipelineStatus::NeedsReview => write!(f, "Needs Review"),
            PipelineStatus::Failed => write!(f, "Failed"),
            PipelineStatus::Confirmed => write!(f, "Confirmed"),
            PipelineStatus::Moved => write!(f, "Moved"),
        }
    }
}

impl PipelineStatus {
    /// Check if this status represents a terminal state.
    pub fn is_terminal(&self) -> bool {
        matches!(self, PipelineStatus::Moved)
    }

    /// Check if this status requires user intervention.
    pub fn requires_review(&self) -> bool {
        matches!(self, PipelineStatus::NeedsReview)
    }

    /// Check if this status indicates a failure.
    pub fn is_failed(&self) -> bool {
        matches!(self, PipelineStatus::Failed)
    }

    /// Check if this status indicates success.
    pub fn is_successful(&self) -> bool {
        matches!(self, PipelineStatus::Matched | PipelineStatus::Confirmed | PipelineStatus::Moved)
    }
}

/// Represents a transition between pipeline states.
///
/// This enum defines which state transitions are valid.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PipelineTransition {
    /// Transition from Scanned to Matched.
    ScanToMatch,
    /// Transition from Scanned to NeedsReview.
    ScanToReview,
    /// Transition from Scanned to Failed.
    ScanToFail,
    /// Transition from Matched to Confirmed.
    MatchToConfirm,
    /// Transition from NeedsReview to Confirmed.
    ReviewToConfirm,
    /// Transition from NeedsReview to Failed.
    ReviewToFail,
    /// Transition from Failed to NeedsReview (reset).
    FailToReview,
    /// Transition from Confirmed to Moved.
    ConfirmToMove,
    /// Transition from Matched to NeedsReview (reset).
    MatchToReview,
}

impl PipelineTransition {
    /// Get the source state for this transition.
    pub fn from_state(&self) -> PipelineStatus {
        match self {
            PipelineTransition::ScanToMatch => PipelineStatus::Scanned,
            PipelineTransition::ScanToReview => PipelineStatus::Scanned,
            PipelineTransition::ScanToFail => PipelineStatus::Scanned,
            PipelineTransition::MatchToConfirm => PipelineStatus::Matched,
            PipelineTransition::ReviewToConfirm => PipelineStatus::NeedsReview,
            PipelineTransition::ReviewToFail => PipelineStatus::NeedsReview,
            PipelineTransition::FailToReview => PipelineStatus::Failed,
            PipelineTransition::ConfirmToMove => PipelineStatus::Confirmed,
            PipelineTransition::MatchToReview => PipelineStatus::Matched,
        }
    }

    /// Get the target state for this transition.
    pub fn to_state(&self) -> PipelineStatus {
        match self {
            PipelineTransition::ScanToMatch => PipelineStatus::Matched,
            PipelineTransition::ScanToReview => PipelineStatus::NeedsReview,
            PipelineTransition::ScanToFail => PipelineStatus::Failed,
            PipelineTransition::MatchToConfirm => PipelineStatus::Confirmed,
            PipelineTransition::ReviewToConfirm => PipelineStatus::Confirmed,
            PipelineTransition::ReviewToFail => PipelineStatus::Failed,
            PipelineTransition::FailToReview => PipelineStatus::NeedsReview,
            PipelineTransition::ConfirmToMove => PipelineStatus::Moved,
            PipelineTransition::MatchToReview => PipelineStatus::NeedsReview,
        }
    }

    /// Validate if this transition is valid from the given source state.
    pub fn is_valid_from(&self, source: &PipelineStatus) -> bool {
        &self.from_state() == source
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pipeline_status_display() {
        assert_eq!(format!("{}", PipelineStatus::Scanned), "Scanned");
        assert_eq!(format!("{}", PipelineStatus::Matched), "Matched");
        assert_eq!(format!("{}", PipelineStatus::NeedsReview), "Needs Review");
        assert_eq!(format!("{}", PipelineStatus::Failed), "Failed");
        assert_eq!(format!("{}", PipelineStatus::Confirmed), "Confirmed");
        assert_eq!(format!("{}", PipelineStatus::Moved), "Moved");
    }

    #[test]
    fn test_is_terminal() {
        assert!(!PipelineStatus::Scanned.is_terminal());
        assert!(!PipelineStatus::Matched.is_terminal());
        assert!(!PipelineStatus::NeedsReview.is_terminal());
        assert!(!PipelineStatus::Failed.is_terminal());
        assert!(!PipelineStatus::Confirmed.is_terminal());
        assert!(PipelineStatus::Moved.is_terminal());
    }

    #[test]
    fn test_requires_review() {
        assert!(!PipelineStatus::Scanned.requires_review());
        assert!(!PipelineStatus::Matched.requires_review());
        assert!(PipelineStatus::NeedsReview.requires_review());
        assert!(!PipelineStatus::Failed.requires_review());
        assert!(!PipelineStatus::Confirmed.requires_review());
        assert!(!PipelineStatus::Moved.requires_review());
    }

    #[test]
    fn test_is_failed() {
        assert!(!PipelineStatus::Scanned.is_failed());
        assert!(!PipelineStatus::Matched.is_failed());
        assert!(!PipelineStatus::NeedsReview.is_failed());
        assert!(PipelineStatus::Failed.is_failed());
        assert!(!PipelineStatus::Confirmed.is_failed());
        assert!(!PipelineStatus::Moved.is_failed());
    }

    #[test]
    fn test_is_successful() {
        assert!(!PipelineStatus::Scanned.is_successful());
        assert!(PipelineStatus::Matched.is_successful());
        assert!(!PipelineStatus::NeedsReview.is_successful());
        assert!(!PipelineStatus::Failed.is_successful());
        assert!(PipelineStatus::Confirmed.is_successful());
        assert!(PipelineStatus::Moved.is_successful());
    }

    #[test]
    fn test_transition_validity() {
        // Valid transition
        assert!(PipelineTransition::ScanToMatch.is_valid_from(&PipelineStatus::Scanned));
        assert!(!PipelineTransition::ScanToMatch.is_valid_from(&PipelineStatus::Matched));

        // Invalid transition
        assert!(!PipelineTransition::ConfirmToMove.is_valid_from(&PipelineStatus::Scanned));
        assert!(PipelineTransition::ConfirmToMove.is_valid_from(&PipelineStatus::Confirmed));
    }

    #[test]
    fn test_transition_states() {
        let transition = PipelineTransition::ScanToMatch;
        assert_eq!(transition.from_state(), PipelineStatus::Scanned);
        assert_eq!(transition.to_state(), PipelineStatus::Matched);

        let transition = PipelineTransition::ConfirmToMove;
        assert_eq!(transition.from_state(), PipelineStatus::Confirmed);
        assert_eq!(transition.to_state(), PipelineStatus::Moved);
    }
}