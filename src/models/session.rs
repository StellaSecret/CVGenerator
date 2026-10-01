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
    /// Top-level `Project` ids the person has manually checked — the same
    /// ids `apply_manual_top_project_selection` consumes.
    ///
    /// Deliberately a SEPARATE field from `checked_project_ids` rather than
    /// a shared one: the two id spaces are distinct types living in
    /// distinct parts of the CV (`Experience::projects` vs
    /// `LifetimeCV::projects`, i.e. the standalone "Projects" section a CV
    /// generator renders between the experience and education blocks), and
    /// their ids are generated independently. Merging them into one set
    /// would make a toggle on one section silently edit the other.
    ///
    /// Same semantics as `checked_project_ids`: an empty vec means "no
    /// personal project survives Apply", not "no manual override" — the
    /// checklist is always seeded from the algorithm's selection at
    /// "Générer" time, so the set is only empty because either the
    /// algorithm picked nothing or the person cleared it.
    #[serde(default)]
    pub checked_top_project_ids: Vec<String>,
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
    /// The algorithm's OWN selection from the last generate, for each of
    /// the three manual-selection lists (`checked_project_ids`,
    /// `checked_top_project_ids`, `checked_skill_ids` respectively).
    ///
    /// These are what make a saved session's manual edits survive being
    /// loaded back and regenerated. Regeneration merges the new algorithm
    /// picks into whatever is checked, EXCEPT ids the person had
    /// previously removed — and "previously removed" can only be computed
    /// as `algo − checked`. Without the algo set persisted, a loaded
    /// session has to seed `last_algo_*` from the checked set itself,
    /// which makes that difference always empty: every item the fresh run
    /// picks silently comes back, so hand-removing a project and reloading
    /// the session a moment later undid the removal.
    ///
    /// Grouped in one struct rather than three loose fields because they
    /// are always written and read together, and mixing them up silently
    /// corrupts the merge.
    #[serde(default)]
    pub algo_selections: AlgoSelections,
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

/// The algorithm's own picks, per manual-selection list, as of the last
/// generate. See `TailoringSession::algo_selections` for why this is
/// persisted separately from the checked sets.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct AlgoSelections {
    /// `ExperienceProject` ids the scorer picked.
    #[serde(default)]
    pub project_ids: Vec<String>,
    /// Top-level `Project` ids the scorer picked.
    #[serde(default)]
    pub top_project_ids: Vec<String>,
    /// `Skill` ids the scorer kept.
    #[serde(default)]
    pub skill_ids: Vec<String>,
}

impl TailoringSession {
    /// Fold a freshly-built snapshot of the tailor form into this saved
    /// session, in place.
    ///
    /// `updated` is a whole new `TailoringSession` built from the live form
    /// state (same `id` as `self`); everything describing the *tailoring* is
    /// taken from it wholesale, so this stays correct as fields are added to
    /// the struct rather than needing a hand-maintained list of assignments.
    ///
    /// Two things are deliberately NOT taken from `updated`:
    ///
    /// - `status`, because it tracks the application through its lifecycle
    ///   and is edited from the saved-sessions list's own dropdown. Taking
    ///   the snapshot's default here would silently reset an application
    ///   someone had marked "Offer" every time they tweaked the CV.
    /// - `date_applied`, because that is when they applied, not when they
    ///   last edited. Re-stamping it on every edit would turn the list into
    ///   a record of when the file was touched.
    ///
    /// `id` and `name` come from `updated`, so renaming a saved session works
    /// by way of this too.
    pub fn apply_update(&mut self, updated: TailoringSession) {
        let status = self.status;
        let date_applied = std::mem::take(&mut self.date_applied);
        *self = updated;
        self.status = status;
        self.date_applied = date_applied;
    }
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
            s.checked_top_project_ids.is_empty(),
            "old JSON has no personal-project selection; it must deserialize to empty"
        );
        assert_eq!(
            s.algo_selections,
            AlgoSelections::default(),
            "old JSON predates persisted algo selections; must default to all-empty"
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

    /// A save/load cycle must carry the algorithm's own selection, not
    /// just the person's checked boxes. If it doesn't, a reloaded session
    /// can no longer tell "unticked on purpose" from "never picked", and
    /// the next Generate silently re-adds everything that was removed —
    /// see `AlgoSelections` and `matcher::merge_selection`.
    #[test]
    fn roundtrip_preserves_algo_selections_apart_from_checked() {
        let saved = TailoringSession {
            id: "s1".to_string(),
            name: "Acme".to_string(),
            job_title: "Engineer".to_string(),
            jd_text: "rust".to_string(),
            score_mode: ScoreMode::Keyword,
            // p1, p3 kept; p2 was picked by the scorer but unticked.
            checked_project_ids: vec!["p1".into(), "p3".into()],
            checked_top_project_ids: vec!["t1".into()],
            checked_skill_ids: vec!["k1".into()],
            summary_choice: Some("short".to_string()),
            summary_skill_ids: vec!["k2".into()],
            algo_selections: AlgoSelections {
                project_ids: vec!["p1".into(), "p2".into(), "p3".into()],
                top_project_ids: vec!["t1".into(), "t2".into()],
                skill_ids: vec!["k1".into(), "k3".into()],
            },
            updated_at_ms: 42,
            match_score: 0.5,
            date_applied: "2026-01-01".to_string(),
            status: ApplicationStatus::Interviewing,
        };

        let json = serde_json::to_string(&saved).unwrap();
        let back: TailoringSession = serde_json::from_str(&json).unwrap();
        assert_eq!(back, saved);

        // The removal is still visible after the round trip, which is the
        // whole point: algo − checked = {p2}.
        let checked: std::collections::HashSet<&String> = back.checked_project_ids.iter().collect();
        let removed: std::collections::HashSet<&String> = back
            .algo_selections
            .project_ids
            .iter()
            .filter(|id| !checked.contains(id))
            .collect();
        assert_eq!(
            removed,
            std::collections::HashSet::from([&"p2".to_string()]),
            "the hand-removed project must still be identifiable as removed after reload"
        );
    }

    /// A helper for building the "snapshot of the form" that
    /// `apply_update` consumes: same id, everything else fresh.
    fn snapshot(id: &str) -> TailoringSession {
        TailoringSession {
            id: id.to_string(),
            name: "Acme".to_string(),
            job_title: "Engineer".to_string(),
            jd_text: "rust".to_string(),
            score_mode: ScoreMode::Keyword,
            checked_project_ids: vec!["p1".into()],
            checked_top_project_ids: vec![],
            checked_skill_ids: vec!["k1".into()],
            summary_choice: None,
            summary_skill_ids: vec![],
            algo_selections: AlgoSelections::default(),
            updated_at_ms: 0,
            match_score: 0.0,
            date_applied: String::new(),
            status: Default::default(),
        }
    }

    /// Editing a saved session must rewrite the tailoring without touching
    /// the application tracking the person maintains by hand.
    #[test]
    fn apply_update_keeps_status_and_applied_date() {
        let mut saved = snapshot("s1");
        saved.status = ApplicationStatus::Offer;
        saved.date_applied = "2026-01-01".to_string();
        saved.match_score = 0.2;

        let mut edited = snapshot("s1");
        edited.name = "Acme — Staff Engineer".to_string();
        edited.jd_text = "rust and wasm".to_string();
        edited.checked_project_ids = vec!["p1".into(), "p2".into()];
        edited.match_score = 0.9;
        edited.updated_at_ms = 1_700_000_000_000;
        // The snapshot is built from the form, which has no opinion about
        // the application — left at the default, as it is in the view.
        assert_eq!(edited.status, ApplicationStatus::Applied);

        saved.apply_update(edited);

        // Tailoring came from the snapshot.
        assert_eq!(saved.name, "Acme — Staff Engineer");
        assert_eq!(saved.jd_text, "rust and wasm");
        assert_eq!(
            saved.checked_project_ids,
            vec!["p1".to_string(), "p2".to_string()]
        );
        assert_eq!(saved.match_score, 0.9);
        assert_eq!(saved.updated_at_ms, 1_700_000_000_000);

        // …and the tracking did not.
        assert_eq!(
            saved.status,
            ApplicationStatus::Offer,
            "editing the CV must not reset an application marked as an offer"
        );
        assert_eq!(
            saved.date_applied, "2026-01-01",
            "the applied date records when they applied, not when they last edited"
        );
        assert_eq!(saved.id, "s1", "an update must keep the entry's identity");
    }
}
