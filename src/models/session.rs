use crate::services::score::ScoreMode;
use serde::{Deserialize, Serialize};

/// Lifecycle of a job application tracked in the saved-sessions list.
/// Serialized as snake_case (`applied`, `interviewing`, `offer`,
/// `rejected`) so stored values are stable and human-readable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApplicationStatus {
    #[default]
    Applied,
    Interviewing,
    Offer,
    Rejected,
}

impl ApplicationStatus {
    pub const ALL: [ApplicationStatus; 4] = [
        Self::Applied,
        Self::Interviewing,
        Self::Offer,
        Self::Rejected,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Applied => "applied",
            Self::Interviewing => "interviewing",
            Self::Offer => "offer",
            Self::Rejected => "rejected",
        }
    }

    pub fn from_key(s: &str) -> Self {
        match s {
            "interviewing" => Self::Interviewing,
            "offer" => Self::Offer,
            "rejected" => Self::Rejected,
            _ => Self::Applied,
        }
    }

    /// i18n key rendering this status's display label.
    pub fn i18n_key(self) -> &'static str {
        match self {
            Self::Applied => "tl_status_applied",
            Self::Interviewing => "tl_status_interviewing",
            Self::Offer => "tl_status_offer",
            Self::Rejected => "tl_status_rejected",
        }
    }
}

/// A snapshot of everything needed to resume a tailoring session: the job
/// description text, the score mode used to generate against it, and the
/// person's manual project-selection overrides (see
/// `matcher::apply_manual_project_selection`).
///
/// Used two ways, both persisted the same way, but into different storage
/// slots (see `storage.rs`):
///
/// - **Current session** (one slot, auto-saved continuously): survives an
///   accidental reload/navigation-away, with no explicit save action
///   needed. This is what makes losing a JD you were part-way through
///   pasting, or a checklist you'd just spent time adjusting, no longer a
///   real risk.
/// - **Saved sessions** (a named list, only touched by explicit
///   Save/Delete): for applying to multiple jobs — each keeps its own JD
///   text and manual selections, so switching between "L'Oréal — Ansible
///   Expert" and "Acme — Platform Engineer" doesn't require re-doing the
///   manual curation each time.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct TailoringSession {
    pub id: String,
    /// Empty for the auto-saved "current session" slot — it isn't meant
    /// to be picked from a list, so it never needs a name. Only ever
    /// non-empty for an entry in the saved-sessions list, set via
    /// "Save as…".
    pub name: String,
    pub job_title: String,
    pub jd_text: String,
    #[serde(default)]
    pub score_mode: ScoreMode,
    /// `ExperienceProject` ids the person has manually checked — the same
    /// ids `apply_manual_project_selection` consumes. A `Vec`, not a
    /// `HashSet`: serialization is simpler and the order has no meaning
    /// either way, so there's nothing a `Vec` costs here that a `HashSet`
    /// would have bought.
    #[serde(default)]
    pub checked_project_ids: Vec<String>,
    /// `Skill` ids the person has manually checked — the same ids
    /// `apply_manual_skill_selection` consumes. Same storage choice as
    /// `checked_project_ids` (a `Vec`: order is meaningless and JSON
    /// round-trips trivially).
    #[serde(default)]
    pub checked_skill_ids: Vec<String>,
    /// Named summary chosen for this session — `None`/missing means the
    /// base "Default" summary. When `Some(name)` it selects the matching
    /// `PersonalInfo::summaries` variant at Apply time (a `{{skills}}`
    /// placeholder inside it expands to this session's JD-pertinent
    /// skills).
    #[serde(default)]
    pub summary_choice: Option<String>,
    /// `Skill` ids the person has manually chosen to fill a `{{skills}}`
    /// placeholder in the selected summary — the auto-calculated top-5 by
    /// relevance when empty. Kept as a `Vec` (order is meaningless, same
    /// reasoning as `checked_skill_ids`).
    #[serde(default)]
    pub summary_skill_ids: Vec<String>,
    #[serde(default)]
    pub updated_at_ms: i64,
    /// Match score (0.0–1.0, the same fraction `TailoredCV.match_score`
    /// uses) captured when the session was saved, so the saved-sessions
    /// list doubles as a lightweight application tracker.
    #[serde(default)]
    pub match_score: f32,
    /// ISO date (YYYY-MM-DD) the person applied, captured at save time.
    #[serde(default)]
    pub date_applied: String,
    /// Lifecycle of this application. Defaults to `Applied` so sessions
    /// and backups written before this field existed still deserialize.
    #[serde(default)]
    pub status: ApplicationStatus,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deserializes_sessions_written_before_tracking_fields_existed() {
        let old = r#"{"id":"s1","name":"Acme","job_title":"Engineer","jd_text":"jd","checked_project_ids":["p1"],"updated_at_ms":0}"#;
        let s: TailoringSession = serde_json::from_str(old).expect("old JSON must still load");
        assert_eq!(s.status, ApplicationStatus::Applied);
        assert_eq!(s.date_applied, "");
        assert_eq!(s.match_score, 0.0);
        assert!(
            s.checked_skill_ids.is_empty(),
            "old JSON has no skill selection; it must deserialize to empty"
        );
        assert_eq!(
            s.summary_choice, None,
            "old JSON has no summary choice; it must default to the base summary"
        );
        assert!(
            s.summary_skill_ids.is_empty(),
            "old JSON has no summary-skill override; it must default to automatic"
        );
    }

    #[test]
    fn status_serde_roundtrip_and_labels_agree() {
        for st in ApplicationStatus::ALL {
            let json = serde_json::to_string(&st).unwrap();
            assert_eq!(json, format!("\"{}\"", st.as_str()));
            assert_eq!(ApplicationStatus::from_key(st.as_str()), st);
            assert!(!st.i18n_key().is_empty());
        }
    }
}
