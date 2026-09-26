//! Canonical storage + display formatting for CV dates (experience and
//! project start/end dates).
//!
//! `Experience`/`ExperienceProject` still store `start_date`/`end_date` as a
//! plain `String` (see `models::cv`) — this module doesn't change that
//! storage shape, so it stays compatible with existing saved CVs, the
//! LinkedIn/PDF importers, and the "years of experience" calculation in
//! `skill_duration.rs`, all of which already work with free-text date
//! strings.
//!
//! What changes is *what the editor writes* into that string. Before this
//! module, the date fields were free text: typing "Février 2025" into the
//! editor stored exactly that, and the renderer printed it verbatim — so
//! the same French text also showed up in the English version of the CV.
//! Now the editor's month/year picker (see `views::cv_editor::DatePickerField`)
//! only ever writes one of three canonical, language-neutral shapes:
//!   - `"<English month name> <year>"`, e.g. `"February 2025"`
//!   - the literal sentinel `"Present"`
//!   - `""` (empty — no date set)
//!
//! `display_date` renders that canonical string in whichever language the
//! CV is currently being viewed/printed in. It also recognizes month names
//! in French (and common EN/FR abbreviations), so it transparently
//! translates dates from CVs saved before this change too, as long as they
//! already happen to be in a clean "<month> <year>" shape. Anything else
//! (freeform text such as "Depuis mars 2020", or OCR noise picked up by the
//! PDF importer) is returned unchanged — there's no reliable way to
//! translate arbitrary free text, so this only ever touches text it's
//! confident it understands.

use crate::i18n_core::{tr, Lang};

/// Full month names, 1-indexed (index 0 unused so `table[month]` just works).
const EN_MONTHS: [&str; 13] = [
    "",
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];
const FR_MONTHS: [&str; 13] = [
    "",
    "Janvier",
    "Février",
    "Mars",
    "Avril",
    "Mai",
    "Juin",
    "Juillet",
    "Août",
    "Septembre",
    "Octobre",
    "Novembre",
    "Décembre",
];

/// The canonical "ongoing" sentinel written to storage by the editor's
/// "Present" toggle. `is_present` recognizes a few other tokens too (for
/// CVs saved before this sentinel existed), but this is the only one the
/// editor ever writes going forward.
pub const PRESENT: &str = "Present";

/// Full month name (1-12) in the requested language. Empty string for an
/// out-of-range month (0, or >12) so callers building a dropdown's blank
/// "no selection" option can pass 0 straight through without a branch.
pub fn month_name(month: u32, lang: Lang) -> &'static str {
    let table = match lang {
        Lang::En => &EN_MONTHS,
        Lang::Fr => &FR_MONTHS,
    };
    table.get(month as usize).copied().unwrap_or("")
}

/// Recognizes a month name/abbreviation in either English or French,
/// case-insensitively, and returns its 1-12 number. Deliberately kept
/// independent from `skill_duration`'s own (private) month table rather
/// than sharing it — that module's parsing is mutation-tested and scoped
/// specifically to duration math, and duplicating this small table here is
/// lower-risk than reaching into it.
fn month_number(word: &str) -> Option<u32> {
    Some(match word.to_lowercase().as_str() {
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

/// True if `raw` is a recognized "ongoing" sentinel — the canonical
/// `"Present"` this app writes, plus a few older/localized tokens ("Actuel"
/// etc.) so CVs saved before this module existed still show up correctly.
pub fn is_present(raw: &str) -> bool {
    matches!(
        raw.trim().to_lowercase().as_str(),
        "present" | "actuel" | "actuelle" | "current" | "now"
    )
}

/// Builds the canonical stored string for a given month/year, e.g.
/// `canonical_date(2025, 2) == "February 2025"`. Always in English,
/// regardless of the editor's current display language — see the module
/// doc comment for why.
pub fn canonical_date(year: i32, month: u32) -> String {
    format!("{} {}", month_name(month, Lang::En), year)
}

/// Renders a stored date string (see module doc comment for the shapes it
/// recognizes) in the requested display language. Empty input stays empty;
/// the "Present" sentinel (and older equivalents) is translated via the
/// existing `ed_present` UI string; a recognized "<month> <year>" (in
/// either order, either language) is re-rendered in `lang`; anything else
/// passes through unchanged.
pub fn display_date(raw: &str, lang: Lang) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    if is_present(trimmed) {
        return tr("ed_present", lang).to_string();
    }
    match parse_date(trimmed) {
        Some((year, month)) => format!("{} {}", month_name(month, lang), year),
        None => trimmed.to_string(),
    }
}

/// Parses a stored date string into `(year, month)`, accepting either
/// "<month> <year>" or "<year> <month>" order, in English or French, full
/// name or common abbreviation. Returns `None` for anything else —
/// including "", "Present", or free text with more than two words — so
/// callers (the display formatter, and the editor's picker when it
/// pre-fills from an existing value) can tell "understood" apart from
/// "pass through unchanged".
pub fn parse_date(raw: &str) -> Option<(i32, u32)> {
    let trimmed = raw.trim();
    let mut words = trimmed.split_whitespace();
    let (first, second, rest) = (words.next(), words.next(), words.next());
    if rest.is_some() {
        return None;
    }
    let (a, b) = (first?, second?);
    if let (Some(month), Ok(year)) = (month_number(a), b.parse::<i32>()) {
        return Some((year, month));
    }
    if let (Ok(year), Some(month)) = (a.parse::<i32>(), month_number(b)) {
        return Some((year, month));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_date_is_always_english() {
        assert_eq!(canonical_date(2025, 2), "February 2025");
    }

    #[test]
    fn display_date_translates_canonical_english_to_french() {
        assert_eq!(display_date("February 2025", Lang::Fr), "Février 2025");
        assert_eq!(display_date("February 2025", Lang::En), "February 2025");
    }

    #[test]
    fn display_date_translates_legacy_french_to_english() {
        // A date typed by hand before this module existed, in French,
        // should now also translate correctly when viewed in English.
        assert_eq!(display_date("Février 2025", Lang::En), "February 2025");
    }

    #[test]
    fn display_date_translates_abbreviations() {
        assert_eq!(display_date("Jan 2021", Lang::Fr), "Janvier 2021");
        assert_eq!(display_date("janv 2021", Lang::Fr), "Janvier 2021");
    }

    #[test]
    fn display_date_handles_year_first_order() {
        assert_eq!(display_date("2021 January", Lang::Fr), "Janvier 2021");
    }

    #[test]
    fn display_date_present_sentinel() {
        assert_eq!(
            display_date("Present", Lang::Fr),
            tr("ed_present", Lang::Fr)
        );
        assert_eq!(display_date("Actuel", Lang::En), tr("ed_present", Lang::En));
    }

    #[test]
    fn display_date_empty_stays_empty() {
        assert_eq!(display_date("", Lang::Fr), "");
        assert_eq!(display_date("   ", Lang::En), "");
    }

    #[test]
    fn display_date_passes_through_unrecognized_free_text() {
        // Freeform imported text with more than two words is left alone —
        // there's no reliable way to translate it.
        assert_eq!(
            display_date("Depuis mars 2020", Lang::En),
            "Depuis mars 2020"
        );
    }

    #[test]
    fn parse_date_roundtrips_canonical_form() {
        assert_eq!(parse_date("February 2025"), Some((2025, 2)));
        assert_eq!(parse_date("Present"), None);
        assert_eq!(parse_date(""), None);
    }
}
