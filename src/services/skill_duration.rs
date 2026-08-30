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
//! platform/WASM dependency — only the one real call site in `renderer.rs`
//! needs to supply the actual current date.

use crate::models::Experience;

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
/// experience/project that references it (via `skill_ids`), with
/// overlapping time ranges deduplicated rather than summed twice.
///
/// Checks both levels: an `Experience`'s own `skill_ids` (using that
/// experience's full date range), and each of its `projects`' `skill_ids`
/// (using that project's own dates, falling back independently per-field
/// to the parent experience's dates when the project doesn't have its
/// own — a project often only has one of start/end set, or neither).
/// Checking both can't double-count: identical or overlapping intervals
/// from both simply merge into one span.
pub fn total_months_for_skill(skill_id: &str, experiences: &[Experience], now: YearMonth) -> i64 {
    let mut intervals: Vec<(YearMonth, YearMonth)> = Vec::new();

    for exp in experiences {
        if exp.skill_ids.iter().any(|id| id == skill_id) {
            if let (Some(s), Some(e)) = (
                parse_month_year(&exp.start_date, now),
                parse_month_year(&exp.end_date, now),
            ) {
                if month_index(s) <= month_index(e) {
                    intervals.push((s, e));
                }
            }
        }

        for proj in &exp.projects {
            if !proj.skill_ids.iter().any(|id| id == skill_id) {
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

    let merged = merge_intervals(intervals);
    merged.iter().map(|(s, e)| months_between(*s, *e)).sum()
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{Experience, ExperienceProject, LocalizedText};

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
    fn total_months_counts_experience_level_skill_ids_too() {
        let exp = Experience {
            start_date: "Jan 2019".to_string(),
            end_date: "Dec 2019".to_string(),
            skill_ids: vec!["s-leadership".to_string()],
            projects: vec![], // no projects at all — only the exp-level tag
            ..Default::default()
        };
        assert_eq!(total_months_for_skill("s-leadership", &[exp], NOW), 12);
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
}
