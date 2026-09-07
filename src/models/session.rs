use crate::services::score::ScoreMode;
use serde::{Deserialize, Serialize};

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
    #[serde(default)]
    pub updated_at_ms: i64,
}
