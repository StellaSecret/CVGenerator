//! Derives each skill's "years of experience" from the actual date ranges
//! of the experiences/projects that reference it, rather than a
//! manually-typed number. A manually-entered figure drifts out of date the
//! moment time passes and nobody remembers to bump it (the same failure
//! mode `SkillLevel` already has, informally — it's set once and never
//! revisited); a derived figure is always consistent with whatever the
//! CV's own dates already say, with no extra upkeep.
//!
//! All logic here is pure and takes "now" as an explicit parameter rather
//! than reading the real clock, so it's fully unit-testable without any
//! platform/WASM dependency — only the two real call sites (`renderer.rs`
//! for the printed CV, and the Skills editor's "Experience summary" panel
//! in `cv_editor.rs`) need to supply the actual current date via
//! `current_year_month` (which is likewise platform-neutral for tests and
//! clock-backed on wasm).

use crate::models::{Experience, Skill, SkillCategory};

/// A (year, month) pair, month 1-12. Only calendar-month granularity is
/// needed since CV dates are always "Month Year", never exact days.
pub type YearMonth = (i32, u32);

/// Parses a free-text CV date like "Jan 2021", "Janvier 2021", "Mars
/// 2026", "Present", or "Actuel" into a `YearMonth`. Returns `now` for any
/// recognized "ongoing" token (case-insensitive), and `None` for anything
/// it can't confidently parse.
///
/// Deliberately conservative on failure: silently guessing wrong here
/// would corrupt a duration total, whereas skipping an unparseable range
/// just slightly undercounts — the safer direction to be wrong in for a
/// number presented as factual.
pub fn parse_month_year(s: &str, now: YearMonth) -> Option<YearMonth> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    let lower = s.to_lowercase();
    if matches!(
        lower.as_str(),
        "present" | "actuel" | "actuelle" | "aujourd'hui" | "current" | "now"
    ) {
        return Some(now);
    }

    // Find a plausible 4-digit year and a recognized month name anywhere
    // in the string — order-independent, tolerant of punctuation/extra
    // words (handles "Jan 2021", "January 2021", en/em dashes, etc.).
    let mut year: Option<i32> = None;
    let mut month: Option<u32> = None;
    for word in lower.split(|c: char| !c.is_alphanumeric()) {
        if word.len() == 4 {
            if let Ok(y) = word.parse::<i32>() {
                if (1950..=2100).contains(&y) {
                    year = Some(y);
                    continue;
                }
            }
        }
        if let Some(m) = month_from_name(word) {
            month = Some(m);
        }
    }
    let year = year?;
    Some((year, month.unwrap_or(1)))
}

fn month_from_name(word: &str) -> Option<u32> {
    // English + French, full names and common abbreviations. Accents
    // already stripped isn't assumed here — both accented and
    // unaccented forms are listed since callers may pass either.
    Some(match word {
        "jan" | "january" | "janv" | "janvier" => 1,
        "feb" | "february" | "fev" | "fevr" | "fevrier" | "févr" | "février" => 2,
        "mar" | "march" | "mars" => 3,
        "apr" | "april" | "avr" | "avril" => 4,
        "may" | "mai" => 5,
        "jun" | "june" | "juin" => 6,
        "jul" | "july" | "juil" | "juillet" => 7,
        "aug" | "august" | "aout" | "août" => 8,
        "sep" | "sept" | "september" | "septembre" => 9,
        "oct" | "october" | "octobre" => 10,
        "nov" | "november" | "novembre" => 11,
        "dec" | "december" | "decembre" | "déc" | "décembre" => 12,
        _ => return None,
    })
}

/// Absolute month index (NOT a calendar month) so subtracting two of
/// these is a plain integer difference: `year * 12 + month`.
fn month_index(ym: YearMonth) -> i64 {
    ym.0 as i64 * 12 + ym.1 as i64
}

/// Inclusive month count from `start` to `end` (both ends count), clamped
/// to at least 1 so a same-month start/end still counts as 1 month
/// rather than 0.
fn months_between(start: YearMonth, end: YearMonth) -> i64 {
    (month_index(end) - month_index(start) + 1).max(1)
}

/// Merges overlapping/adjacent `[start, end]` month-intervals (inclusive)
/// so using the same skill across two overlapping (or back-to-back)
/// experiences doesn't double-count the overlap.
fn merge_intervals(mut intervals: Vec<(YearMonth, YearMonth)>) -> Vec<(YearMonth, YearMonth)> {
    intervals.sort_by_key(|(s, _)| month_index(*s));
    let mut merged: Vec<(YearMonth, YearMonth)> = Vec::new();
    for (start, end) in intervals {
        if let Some(last) = merged.last_mut() {
            // "<= last.1 + 1", not just "<= last.1": also merges
            // back-to-back ranges (e.g. one role ending Dec 2021, the
            // next starting Jan 2022) into one continuous span, not just
            // ones that literally overlap.
            if month_index(start) <= month_index(last.1) + 1 {
                if month_index(end) > month_index(last.1) {
                    last.1 = end;
                }
                continue;
            }
        }
        merged.push((start, end));
    }
    merged
}

/// Total months of experience with `skill_id`, derived from every
/// project that references it (via `skill_ids`), with overlapping time
/// ranges deduplicated rather than summed twice.
///
/// Uses each project's own dates, falling back independently per-field to
/// the parent experience's dates when the project doesn't have its own —
/// a project often only has one of start/end set, or neither.
pub fn total_months_for_skill(skill_id: &str, experiences: &[Experience], now: YearMonth) -> i64 {
    merged_months(skill_match_intervals(experiences, now, |id| id == skill_id))
}

/// Total months during which the user did any work in `category` — the
/// union of every interval on which some project used *any* skill of that
/// category. Overlapping/back-to-back ranges are deduplicated once across
/// the whole category, so working with Linux + Kubernetes + Docker on the
/// same 3-year project counts as 3 years, not 9.
///
/// This is deliberately NOT a sum of each skill's individual months:
/// `total_months_for_skill` already dedupes per-skill, but summing those
/// correct-per-skill numbers across a category reintroduces exactly the
/// shared-project double-counting that per-skill merging avoids. A project
/// spanning several tools still spans only its own time range once.
///
/// A category whose skills are all untagged (nothing references them)
/// returns 0; callers present that as "not measurable", not "under a
/// year" — `format_years(0)` returns an empty string.
pub fn total_months_for_category(
    category: SkillCategory,
    skills: &[Skill],
    experiences: &[Experience],
    now: YearMonth,
) -> i64 {
    let ids: std::collections::HashSet<&str> = skills
        .iter()
        .filter(|s| s.category == category)
        .map(|s| s.id.as_str())
        .collect();
    merged_months(skill_match_intervals(experiences, now, |id| {
        ids.contains(id)
    }))
}

/// Effective `(start, end)` interval of every project in `experiences`
/// that references (via `skill_ids`) an id for which `keep` returns true.
/// Shared by the per-skill and per-category totals, which differ only in
/// which `skill_ids` match.
fn skill_match_intervals(
    experiences: &[Experience],
    now: YearMonth,
    keep: impl Fn(&str) -> bool,
) -> Vec<(YearMonth, YearMonth)> {
    let mut intervals: Vec<(YearMonth, YearMonth)> = Vec::new();

    for exp in experiences {
        for proj in &exp.projects {
            if !proj.skill_ids.iter().any(|id| keep(id)) {
                continue;
            }
            let start_str = if !proj.start_date.is_empty() {
                &proj.start_date
            } else {
                &exp.start_date
            };
            let end_str = if !proj.end_date.is_empty() {
                &proj.end_date
            } else {
                &exp.end_date
            };
            if let (Some(s), Some(e)) = (
                parse_month_year(start_str, now),
                parse_month_year(end_str, now),
            ) {
                if month_index(s) <= month_index(e) {
                    intervals.push((s, e));
                }
            }
        }
    }
    intervals
}

/// Months spanned by `intervals` with overlapping/adjacent ranges merged
/// once (see `merge_intervals`).
fn merged_months(intervals: Vec<(YearMonth, YearMonth)>) -> i64 {
    merge_intervals(intervals)
        .iter()
        .map(|(s, e)| months_between(*s, *e))
        .sum()
}

/// Formats a month count as a short display string. Anything under 12
/// months reads as "< 1 yr" rather than "0 yrs" (which would look like no
/// experience at all) or rounding up to "1 yr" (which would overstate a
/// couple months of exposure). 12+ months round to the nearest whole
/// year — 11.6 years reads as "12 yrs" to a human, not "11 yrs".
pub fn format_years(months: i64) -> String {
    format_years_with(months, "< 1 yr", "1 yr", "yrs")
}

/// French counterpart of `format_years` — same bucketing rules ("< 1 an",
/// "1 an", "N ans"), just localized wording.
pub fn format_years_fr(months: i64) -> String {
    format_years_with(months, "< 1 an", "1 an", "ans")
}

fn format_years_with(months: i64, under_one: &str, exactly_one: &str, plural_unit: &str) -> String {
    if months <= 0 {
        return String::new();
    }
    if months < 12 {
        return under_one.to_string();
    }
    let years = ((months as f64) / 12.0).round() as i64;
    if years <= 1 {
        exactly_one.to_string()
    } else {
        format!("{years} {plural_unit}")
    }
}

/// Current (year, month) — `month` is 1-12, calendar convention (NOT the
/// 0-indexed convention JS `Date.getMonth()` uses; converted below). Used
/// only to resolve "Present"/"Actuel" when deriving a skill's years of
/// experience — all the actual date-math logic above is pure; this is the
/// one real call to an actual clock, isolated here the same way
/// `drive.rs`'s `now_ms()` isolates its own clock access.
pub fn current_year_month() -> YearMonth {
    #[cfg(target_arch = "wasm32")]
    {
        let d = js_sys::Date::new_0();
        (d.get_full_year() as i32, d.get_month() as u32 + 1)
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        // Native builds only exist for `cargo test`/`clippy` in this
        // project — nothing here ever renders a real CV/UI outside wasm, so
        // exact accuracy doesn't matter, only that it compiles and is in
        // the right ballpark (tests inject their own fixed `now` and
        // never call this). A rough days-since-epoch/365.25 estimate is
        // enough for that.
        use std::time::{SystemTime, UNIX_EPOCH};
        let days = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs() / 86400)
            .unwrap_or(0) as f64;
        let year = 1970 + (days / 365.25) as i32;
        (year, 6) // mid-year placeholder month
    }
}

/// Derived months of experience for every skill in `skills` — `(skill_id,
/// months)` pairs, one per skill in the given order. Skills never tagged
/// against a project (nothing in any project's `skill_ids` references
/// them) come back as `0`, which callers must present as "not measurable"
/// rather than "under a year" — `format_years(0)` deliberately returns an
/// empty string.
pub fn months_by_skill(
    skills: &[Skill],
    experiences: &[Experience],
    now: YearMonth,
) -> Vec<(String, i64)> {
    skills
        .iter()
        .map(|s| {
            (
                s.id.clone(),
                total_months_for_skill(&s.id, experiences, now),
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{Experience, ExperienceProject, LocalizedText, Skill, SkillCategory};

    const NOW: YearMonth = (2026, 6);

    #[test]
    fn parses_common_formats() {
        assert_eq!(parse_month_year("Jan 2021", NOW), Some((2021, 1)));
        assert_eq!(parse_month_year("January 2021", NOW), Some((2021, 1)));
        assert_eq!(parse_month_year("Janvier 2021", NOW), Some((2021, 1)));
        assert_eq!(parse_month_year("Février 2025", NOW), Some((2025, 2)));
        assert_eq!(parse_month_year("Mars 2026", NOW), Some((2026, 3)));
        assert_eq!(parse_month_year("Décembre 2024", NOW), Some((2024, 12)));
    }

    #[test]
    fn parses_every_month_arm() {
        // Full names and common abbreviations for every month except the
        // four already covered by parses_common_formats (Jan/Feb/Mar/Dec) —
        // each arm of month_from_name must be reachable and return its own
        // month number.
        assert_eq!(parse_month_year("Apr 2021", NOW), Some((2021, 4)));
        assert_eq!(parse_month_year("April 2021", NOW), Some((2021, 4)));
        assert_eq!(parse_month_year("Avril 2021", NOW), Some((2021, 4)));
        assert_eq!(parse_month_year("May 2021", NOW), Some((2021, 5)));
        assert_eq!(parse_month_year("Mai 2021", NOW), Some((2021, 5)));
        assert_eq!(parse_month_year("Jun 2021", NOW), Some((2021, 6)));
        assert_eq!(parse_month_year("June 2021", NOW), Some((2021, 6)));
        assert_eq!(parse_month_year("Juin 2021", NOW), Some((2021, 6)));
        assert_eq!(parse_month_year("Jul 2021", NOW), Some((2021, 7)));
        assert_eq!(parse_month_year("July 2021", NOW), Some((2021, 7)));
        assert_eq!(parse_month_year("Juillet 2021", NOW), Some((2021, 7)));
        assert_eq!(parse_month_year("Aug 2021", NOW), Some((2021, 8)));
        assert_eq!(parse_month_year("August 2021", NOW), Some((2021, 8)));
        assert_eq!(parse_month_year("Août 2021", NOW), Some((2021, 8)));
        assert_eq!(parse_month_year("Sep 2021", NOW), Some((2021, 9)));
        assert_eq!(parse_month_year("September 2021", NOW), Some((2021, 9)));
        assert_eq!(parse_month_year("Septembre 2021", NOW), Some((2021, 9)));
        assert_eq!(parse_month_year("Oct 2021", NOW), Some((2021, 10)));
        assert_eq!(parse_month_year("October 2021", NOW), Some((2021, 10)));
        assert_eq!(parse_month_year("Octobre 2021", NOW), Some((2021, 10)));
        assert_eq!(parse_month_year("Nov 2021", NOW), Some((2021, 11)));
        assert_eq!(parse_month_year("November 2021", NOW), Some((2021, 11)));
        assert_eq!(parse_month_year("Novembre 2021", NOW), Some((2021, 11)));
    }

    #[test]
    fn month_from_name_jan_variants() {
        // Deleting the "jan" | "january" | "janv" | "janvier" arm is
        // invisible through parse_month_year because the default month is
        // also 1 (January). Test month_from_name directly to catch that.
        assert_eq!(month_from_name("jan"), Some(1));
        assert_eq!(month_from_name("january"), Some(1));
        assert_eq!(month_from_name("janv"), Some(1));
        assert_eq!(month_from_name("janvier"), Some(1));
    }

    #[test]
    fn parses_ongoing_tokens_as_now() {
        assert_eq!(parse_month_year("Present", NOW), Some(NOW));
        assert_eq!(parse_month_year("Actuel", NOW), Some(NOW));
        assert_eq!(parse_month_year("actuelle", NOW), Some(NOW));
        assert_eq!(parse_month_year("CURRENT", NOW), Some(NOW));
    }

    #[test]
    fn unparseable_or_empty_returns_none() {
        assert_eq!(parse_month_year("", NOW), None);
        assert_eq!(parse_month_year("sometime maybe", NOW), None);
        assert_eq!(parse_month_year("2021", NOW), Some((2021, 1)));
    }

    #[test]
    fn months_between_is_inclusive_and_never_zero() {
        assert_eq!(months_between((2021, 1), (2021, 1)), 1);
        assert_eq!(months_between((2021, 1), (2021, 12)), 12);
        assert_eq!(months_between((2021, 1), (2022, 1)), 13);
    }

    #[test]
    fn merge_intervals_combines_overlapping_ranges() {
        let merged = merge_intervals(vec![((2021, 1), (2021, 6)), ((2021, 4), (2021, 10))]);
        assert_eq!(merged, vec![((2021, 1), (2021, 10))]);
    }

    #[test]
    fn merge_intervals_combines_back_to_back_ranges() {
        // Dec 2021 immediately followed by Jan 2022 — no gap, should merge.
        let merged = merge_intervals(vec![((2022, 1), (2022, 6)), ((2021, 1), (2021, 12))]);
        assert_eq!(merged, vec![((2021, 1), (2022, 6))]);
    }

    #[test]
    fn merge_intervals_keeps_genuinely_separate_ranges_apart() {
        // A real gap (Feb 2021 to Dec 2021) between the two — must NOT merge.
        let merged = merge_intervals(vec![((2020, 1), (2021, 2)), ((2021, 12), (2022, 6))]);
        assert_eq!(
            merged,
            vec![((2020, 1), (2021, 2)), ((2021, 12), (2022, 6))]
        );
    }

    fn exp_with_project_skill(
        exp_start: &str,
        exp_end: &str,
        proj_start: &str,
        proj_end: &str,
        skill_id: &str,
    ) -> Experience {
        Experience {
            start_date: exp_start.to_string(),
            end_date: exp_end.to_string(),
            projects: vec![ExperienceProject {
                name: LocalizedText::same("Some Project"),
                start_date: proj_start.to_string(),
                end_date: proj_end.to_string(),
                skill_ids: vec![skill_id.to_string()],
                ..Default::default()
            }],
            ..Default::default()
        }
    }

    fn exp_with_project_skills(
        exp_start: &str,
        exp_end: &str,
        proj_start: &str,
        proj_end: &str,
        skill_ids: &[&str],
    ) -> Experience {
        Experience {
            start_date: exp_start.to_string(),
            end_date: exp_end.to_string(),
            projects: vec![ExperienceProject {
                name: LocalizedText::same("Some Project"),
                start_date: proj_start.to_string(),
                end_date: proj_end.to_string(),
                skill_ids: skill_ids.iter().map(|s| s.to_string()).collect(),
                ..Default::default()
            }],
            ..Default::default()
        }
    }

    fn skill(id: &str, category: &SkillCategory) -> Skill {
        Skill {
            id: id.to_string(),
            name: id.to_string(),
            category: category.clone(),
            ..Default::default()
        }
    }

    #[test]
    fn total_months_uses_project_dates_when_present() {
        let exps = vec![exp_with_project_skill(
            "Jan 2018", "Dec 2023", "Jan 2021", "Dec 2021", "s-rust",
        )];
        // Should use the PROJECT's Jan 2021 - Dec 2021 (12 months), not
        // the parent experience's much longer Jan 2018 - Dec 2023 span.
        assert_eq!(total_months_for_skill("s-rust", &exps, NOW), 12);
    }

    #[test]
    fn total_months_falls_back_to_experience_dates_when_project_dates_missing() {
        let exps = vec![exp_with_project_skill(
            "Jan 2020", "Dec 2020", "", "", "s-rust",
        )];
        assert_eq!(total_months_for_skill("s-rust", &exps, NOW), 12);
    }

    #[test]
    fn total_months_deduplicates_overlapping_experiences() {
        let exps = vec![
            exp_with_project_skill("Jan 2020", "Dec 2021", "Jan 2020", "Dec 2021", "s-k8s"),
            // Fully overlapping second "job" (e.g. a duplicate entry, or
            // a freelance gig alongside a full-time role) — must not
            // double the total.
            exp_with_project_skill("Jun 2020", "Jun 2021", "Jun 2020", "Jun 2021", "s-k8s"),
        ];
        assert_eq!(total_months_for_skill("s-k8s", &exps, NOW), 24);
    }

    #[test]
    fn total_months_ignores_experiences_not_tagged_with_the_skill() {
        let exps = vec![exp_with_project_skill(
            "Jan 2020", "Dec 2020", "Jan 2020", "Dec 2020", "s-other",
        )];
        assert_eq!(total_months_for_skill("s-rust", &exps, NOW), 0);
    }

    #[test]
    fn total_months_ignores_experiences_with_no_tagged_projects() {
        let exp = Experience {
            start_date: "Jan 2019".to_string(),
            end_date: "Dec 2019".to_string(),
            projects: vec![], // no projects at all — nothing to tag against
            ..Default::default()
        };
        assert_eq!(total_months_for_skill("s-leadership", &[exp], NOW), 0);
    }

    #[test]
    fn total_months_for_category_unions_skills_shared_across_one_project() {
        // The regression that motivated the category total: one 3-year
        // project running Linux + Kubernetes + OpenShift + Docker
        // simultaneously. Each skill measures 36 months; naively summing
        // them would claim 144 months (12 years) of "Platforms &
        // Infrastructure" work. The union must stay 36 — the project
        // spanned only three years, regardless of how many tools ran on it.
        let cat = SkillCategory::PlatformsInfrastructure;
        let skills = [
            skill("s-linux", &cat),
            skill("s-k8s", &cat),
            skill("s-ocp", &cat),
            skill("s-dkr", &cat),
        ];
        let exp = exp_with_project_skills(
            "Jan 2020",
            "Dec 2022",
            "Jan 2020",
            "Dec 2022",
            &["s-linux", "s-k8s", "s-ocp", "s-dkr"],
        );
        assert_eq!(
            total_months_for_skill("s-linux", std::slice::from_ref(&exp), NOW),
            36
        );
        assert_eq!(
            total_months_for_skill("s-k8s", std::slice::from_ref(&exp), NOW),
            36
        );
        assert_eq!(
            total_months_for_category(cat, &skills, &[exp], NOW),
            36,
            "union, not the 144-month per-skill sum"
        );
    }

    #[test]
    fn total_months_for_category_sums_disjoint_spans() {
        // No overlap between the two tools' time ranges — the union really
        // is their sum (1 year in 2018 + 3 years in 2020-2022).
        let cat = SkillCategory::Programming;
        let skills = [skill("s-rs", &cat), skill("s-go", &cat)];
        let exps = vec![
            exp_with_project_skill("Jan 2018", "Dec 2018", "Jan 2018", "Dec 2018", "s-rs"),
            exp_with_project_skill("Jan 2020", "Dec 2022", "Jan 2020", "Dec 2022", "s-go"),
        ];
        assert_eq!(total_months_for_category(cat, &skills, &exps, NOW), 12 + 36);
    }

    #[test]
    fn total_months_for_category_dedupes_overlap_across_experiences() {
        // Two different category skills, used at two DIFFERENT employers
        // whose time ranges overlap by a year — the overlap must not be
        // counted twice (union is Jan 2020–Dec 2022, not 48 months).
        let cat = SkillCategory::Database;
        let skills = [skill("s-pg", &cat), skill("s-mys", &cat)];
        let exps = vec![
            exp_with_project_skill("Jan 2020", "Dec 2022", "Jan 2020", "Dec 2022", "s-pg"),
            exp_with_project_skill("Jan 2021", "Dec 2022", "Jan 2021", "Dec 2022", "s-mys"),
        ];
        assert_eq!(total_months_for_category(cat, &skills, &exps, NOW), 36);
    }

    #[test]
    fn total_months_for_category_ignores_other_categories() {
        // A project tagged only with a Programming skill must not inflate
        // the Platforms total, even though both live in the same
        // experience's timeframe.
        let progr = SkillCategory::Programming;
        let infra = SkillCategory::PlatformsInfrastructure;
        let skills = [skill("s-rust", &progr), skill("s-dkr", &infra)];
        let exp = exp_with_project_skill("Jan 2020", "Dec 2022", "Jan 2020", "Dec 2022", "s-rust");
        assert_eq!(
            total_months_for_category(infra, &skills, std::slice::from_ref(&exp), NOW),
            0
        );
        assert_eq!(total_months_for_category(progr, &skills, &[exp], NOW), 36);
    }

    #[test]
    fn total_months_for_category_untagged_skills_return_zero() {
        let cat = SkillCategory::Monitoring;
        let skills = [skill("s-graf", &cat), skill("s-prom", &cat)];
        let exp = Experience {
            start_date: "Jan 2019".to_string(),
            end_date: "Dec 2019".to_string(),
            projects: vec![],
            ..Default::default()
        };
        assert_eq!(total_months_for_category(cat, &skills, &[exp], NOW), 0);
    }

    #[test]
    fn format_years_buckets_correctly() {
        assert_eq!(format_years(0), "");
        assert_eq!(format_years(6), "< 1 yr");
        assert_eq!(format_years(12), "1 yr");
        assert_eq!(format_years(18), "2 yrs"); // rounds up from 1.5
        assert_eq!(format_years(24), "2 yrs");
        assert_eq!(format_years(139), "12 yrs"); // 11.58 rounds to 12
    }

    #[test]
    fn format_years_fr_buckets_correctly() {
        assert_eq!(format_years_fr(0), "");
        assert_eq!(format_years_fr(6), "< 1 an");
        assert_eq!(format_years_fr(12), "1 an");
        assert_eq!(format_years_fr(18), "2 ans");
        assert_eq!(format_years_fr(24), "2 ans");
        assert_eq!(format_years_fr(139), "12 ans");
    }

    #[test]
    fn current_year_month_native_returns_plausible_year() {
        let (year, month) = current_year_month();
        assert!((1970..=2100).contains(&year), "implausible year {year}");
        assert!((1..=12).contains(&month), "implausible month {month}");
    }

    // The loose 1970..=2100 bound above wasn't tight enough to catch a
    // `d.as_secs() / 86400` mutated to `% 86400`: that swap replaces "days
    // since epoch" with "seconds into the current day" (0..86399), which
    // — divided by 365.25 — still lands inside 1970..=2100 for roughly the
    // first half of any given UTC day, so it only failed intermittently
    // depending on when CI happened to run. Mirroring the exact same
    // division here (not the `%`) to compute the expected year pins this
    // deterministically regardless of time of day, without depending on a
    // date/time crate this project doesn't otherwise use.
    #[test]
    fn current_year_month_native_divides_total_seconds_by_a_day_not_modulo() {
        use std::time::{SystemTime, UNIX_EPOCH};
        let days = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs() / 86400)
            .unwrap_or(0) as f64;
        let expected_year = 1970 + (days / 365.25) as i32;
        let (year, month) = current_year_month();
        assert_eq!(
            year, expected_year,
            "current_year_month must divide total elapsed seconds by 86400 \
             (whole days elapsed), not take the remainder (seconds into the \
             current day)"
        );
        assert_eq!(
            month, 6,
            "native fallback always uses the mid-year placeholder month"
        );
    }

    #[test]
    fn months_by_skill_maps_every_skill_to_derived_months() {
        let skills = vec![
            Skill {
                id: "s-rust".to_string(),
                name: "Rust".to_string(),
                ..Default::default()
            },
            Skill {
                id: "s-k8s".to_string(),
                name: "Kubernetes".to_string(),
                ..Default::default()
            },
        ];
        let exps = vec![
            exp_with_project_skill("Jan 2020", "Dec 2021", "Jan 2020", "Dec 2020", "s-rust"),
            // Never tagged against any project — must measure as 0.
            exp_with_project_skill("Jan 2020", "Dec 2021", "Jan 2020", "Dec 2021", "s-other"),
        ];
        let measured = months_by_skill(&skills, &exps, NOW);
        let by_id: std::collections::HashMap<&str, i64> =
            measured.iter().map(|(id, m)| (id.as_str(), *m)).collect();
        assert_eq!(by_id["s-rust"], 12);
        assert_eq!(by_id["s-k8s"], 0);
    }
}

// ── WASM-only tests ──────────────────────────────────────────────────────────
//
// `current_year_month` isn't itself `#[cfg(target_arch = "wasm32")]`-gated
// (only its internal branch is), but its wasm32 branch — the one real code
// path this project's `js_sys::Date` clock access actually runs through in
// production — is still invisible to native `cargo test --lib`. Directly
// mirrors `js_sys::Date::new_0()` here to derive an independently-computed
// expected value, the same way the native
// `current_year_month_native_divides_total_seconds_by_a_day_not_modulo`
// test above mirrors `SystemTime::now()`.
#[cfg(all(test, target_arch = "wasm32"))]
// cargo-mutants only auto-skips functions carrying an attribute
// whose last path segment is literally `test` (`#[test]`,
// `#[tokio::test]`, ...) or an enclosing `#[cfg(test)]` it detects
// directly on that item — `#[wasm_bindgen_test]`'s path doesn't
// match that check, and the `cfg(test)` on this module wasn't
// enough either in practice, so without this every helper and
// test function below got "mutated" to `()` and reported as a
// missed mutant (trivially: a test that asserts nothing passes).
#[cfg_attr(test, mutants::skip)]
mod wasm_tests {
    use super::*;
    use wasm_bindgen_test::*;

    wasm_bindgen_test_configure!(run_in_browser);

    #[wasm_bindgen_test]
    fn current_year_month_wasm_matches_js_date_now() {
        let expected = js_sys::Date::new_0();
        let (year, month) = current_year_month();
        assert_eq!(year, expected.get_full_year() as i32);
        // JS `Date.getMonth()` is 0-indexed; `current_year_month` converts
        // to the 1-12 calendar convention this codebase uses everywhere
        // else (see e.g. `YearMonth`'s own doc comment) — pins that `+ 1`
        // conversion specifically, distinct from the native side's
        // `/`-vs-`%` day-count mutant.
        assert_eq!(month, expected.get_month() as u32 + 1);
    }
}
