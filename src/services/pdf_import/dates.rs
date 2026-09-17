// Pure std date parsing; no dependency on other parser nodes.

/// Find a date range at the END of an experience line.
/// Returns (start_date, end_date) and the separator used.
/// E.g. "Software Engineer at Acme - Jan 2021 - Present" → ("Jan 2021", "Present")
/// Also handles a start date with no separator of its own before it, only
/// whitespace — e.g. this app's own "Company · Location  December 2024 –
/// February 2026" row layout (company/location and the date range aren't
/// dash-separated at all; only the two dates are).
pub(super) fn extract_date_range_from_end(line: &str) -> Option<(String, String)> {
    let lower = line.to_lowercase();
    let present_words = ["present", "current", "actuel", "prèsent"];

    // Find the LAST date separator (" – ", " - ", " — ")
    for sep in &[" – ", " - ", " — "] {
        if let Some(last_pos) = lower.rfind(sep) {
            let end_part = line[last_pos + sep.len()..].trim();
            let end_lower = end_part.to_lowercase();
            let is_present = present_words.iter().any(|pw| end_lower.contains(pw));
            let end_has_year = end_part.chars().any(|c| c.is_ascii_digit());
            if !is_present && !end_has_year {
                continue;
            }
            let end = if is_present {
                "Present".to_string()
            } else {
                end_part.to_string()
            };

            let left_of_end = &line[..last_pos];

            // Preferred path: another occurrence of the SAME separator
            // marks off the start date too, e.g.
            // "Acme - Jan 2021 - Present".
            if let Some(prev_pos) = lower[..last_pos].rfind(sep) {
                let start_part = line[prev_pos + sep.len()..last_pos].trim();
                let before_start = line[..prev_pos].trim();
                if !start_part.is_empty() && !before_start.is_empty() {
                    return Some((start_part.to_string(), end));
                }
            }

            // Fallback: no second separator (company/location and the date
            // range are just whitespace-separated, not dash-separated).
            // Find where the start date itself begins by scanning
            // left_of_end's trailing whitespace-separated tokens for a
            // month name or a bare year — "... France December 2024" ->
            // start date is "December 2024", not dash-delimited at all.
            //
            // Guard: only attempt this when `end` itself is short/clean
            // (<=3 words). Without a second separator to anchor the end of
            // the date range, `end_part` is "everything after the last
            // separator" — for a line like "\u{11} December 2024 –
            // February 2026 · Paris, France" (icon-prefixed date range
            // immediately followed by a location, no separator between
            // them — the three-line CV layout's date row) that would
            // wrongly swallow the trailing location into `end`. A clean
            // date is at most a couple of words ("February 2026",
            // "Present", "No Expiration Date"); anything longer signals
            // trailing junk, so bail and let extract_standalone_date_range
            // (which already handles that layout correctly) take it
            // instead.
            if end.split_whitespace().count() > 3 {
                continue;
            }
            let words: Vec<&str> = left_of_end.split_whitespace().collect();
            // Guard: `before_start` must look like a real company/location
            // string, not a lone icon glyph — this app's own three-line
            // layout puts a bare icon character in front of a project's
            // own date range (e.g. "\u{11} January 2025 – June 2025",
            // handled separately by extract_standalone_date_range with its
            // own just_after_project_header guard). Without this check
            // that icon character alone would satisfy the emptiness check
            // below and get misread as a brand new experience header,
            // duplicating the current one.
            let real_company_text = |s: &str| s.chars().filter(|c| c.is_alphabetic()).count() >= 2;
            // Abbreviated-month start date, whitespace-separated from a
            // real company/location on the same line, e.g. the layout-(c)
            // row "· Paris, France Jan 2024 – Nov 2024". `looks_like_date_token`
            // only recognizes full month names and bare years, so without
            // this an abbreviated start month would fall through and the
            // bare-year branch below would swallow only "2024", leaving
            // "Jan" glued onto the location. This check runs *before* the
            // strict month-name / bare-year `else if` chain below and is
            // gated on there being real company text before the month —
            // a bare 2-word date row like "Dec 2024 – Feb 2026" (the
            // three-line layout, whose role/company live on *previous*
            // lines) has no company text on this line, so it skips this
            // branch and still falls through to the bare-year branch,
            // preserving the pre-existing recovery path.
            if words.len() >= 3
                && looks_like_date_token_loose(words[words.len() - 2])
                && words[words.len() - 1].chars().all(|c| c.is_ascii_digit())
                && words[words.len() - 1].len() == 4
            {
                let start_part = format!("{} {}", words[words.len() - 2], words[words.len() - 1]);
                let before_start = words[..words.len() - 2].join(" ");
                if real_company_text(&before_start) {
                    return Some((start_part, end));
                }
            }
            if words.len() >= 2 && looks_like_date_token(words[words.len() - 2]) {
                let start_part = format!("{} {}", words[words.len() - 2], words[words.len() - 1]);
                let before_start = words[..words.len() - 2].join(" ");
                if real_company_text(&before_start) {
                    return Some((start_part, end));
                }
            } else if let Some(&last_word) = words.last() {
                if last_word.chars().all(|c| c.is_ascii_digit()) && last_word.len() == 4 {
                    let before_start = words[..words.len() - 1].join(" ");
                    if real_company_text(&before_start) {
                        return Some((last_word.to_string(), end));
                    }
                }
            }
        }
    }
    None
}

/// Detect an open-ended "Company - Depuis <date>" (or English "... - Since
/// Splits `line` into whitespace-separated tokens, keeping each token's byte
/// span in `line` — used by `find_date_range_span` to locate a date range
/// that can appear *anywhere* in the line (not just at the very end) and
/// still cleanly slice out the text before and after it.
pub(super) fn tokenize_with_spans(line: &str) -> Vec<(usize, usize, &str)> {
    let mut out = Vec::new();
    let mut start: Option<usize> = None;
    for (i, c) in line.char_indices() {
        if c.is_whitespace() {
            if let Some(s) = start.take() {
                out.push((s, i, &line[s..i]));
            }
        } else if start.is_none() {
            start = Some(i);
        }
    }
    if let Some(s) = start {
        out.push((s, line.len(), &line[s..]));
    }
    out
}

/// Finds a date range *anywhere* in a line — not just as its trailing
/// segment like `extract_date_range_from_end` requires — by scanning
/// whitespace-separated tokens for an actual month/year (or bare-year)
/// pattern, rather than splitting on the last occurrence of a separator
/// character. This matters for two common real-world layouts that the
/// separator-splitting approach can't handle:
///
///   - A still-ongoing role stated as "Depuis <date>" / "Since <date>" —
///     one date, not a start-end pair — rather than "<date> - Present".
///   - A date range with more text *after* it on the same line, e.g. the
///     common French CV convention "Company - Avril 2021 à janvier 2024 -
///     CDI - La Rochelle" (contract type and city tacked on after the
///     dates). `extract_date_range_from_end`'s "last separator" search
///     finds "- La Rochelle" first and gives up there, never reaching the
///     actual date range earlier in the line. It also only recognizes
///     dash-family separators, not "à"/"au" ("to"), which French CVs use
///     between a range's two dates.
///
/// Returns `(span_start, span_end, start_date, end_date)` — the byte span
/// covering the whole matched date expression (including a leading
/// "depuis"/"since", so callers can cleanly drop it from the surrounding
/// text) plus the extracted start/end date strings. Trailing text after
/// `span_end` (e.g. "- CDI - La Rochelle") is deliberately left for the
/// caller to deal with rather than parsed here — this function's only job
/// is finding *where* the date range is.
pub(super) fn find_date_range_span(line: &str) -> Option<(usize, usize, String, String)> {
    let tokens = tokenize_with_spans(line);
    let is_month = |t: &str| {
        let l = t.trim_end_matches(['.', ',']).to_lowercase();
        MONTH_NAMES.contains(&l.as_str())
    };
    let is_year = |t: &str| t.len() == 4 && t.chars().all(|c| c.is_ascii_digit());
    let is_present_word = |t: &str| {
        let l = t.to_lowercase();
        [
            "present", "current", "actuel", "présent", "prèsent", "aujourd",
        ]
        .iter()
        .any(|p| l.contains(p))
    };
    let is_since_word =
        |t: &str| t.eq_ignore_ascii_case("depuis") || t.eq_ignore_ascii_case("since");
    let is_sep_word = |t: &str| {
        matches!(t, "-" | "–" | "—")
            || t.eq_ignore_ascii_case("à")
            || t.eq_ignore_ascii_case("au")
            || t.eq_ignore_ascii_case("to")
    };

    for t in 0..tokens.len() {
        // "Depuis <Month> <Year>" / "Since <Month> <Year>" — ongoing.
        if is_since_word(tokens[t].2) {
            if t + 2 < tokens.len() && is_month(tokens[t + 1].2) && is_year(tokens[t + 2].2) {
                return Some((
                    tokens[t].0,
                    tokens[t + 2].1,
                    format!("{} {}", tokens[t + 1].2, tokens[t + 2].2),
                    "Present".to_string(),
                ));
            }
            // "Depuis <Year>" — ongoing, bare year.
            if t + 1 < tokens.len() && is_year(tokens[t + 1].2) {
                return Some((
                    tokens[t].0,
                    tokens[t + 1].1,
                    tokens[t + 1].2.to_string(),
                    "Present".to_string(),
                ));
            }
        }

        // "<Month> <Year> <sep> ..." — a full range starting with a
        // month-and-year date.
        if t + 1 < tokens.len() && is_month(tokens[t].2) && is_year(tokens[t + 1].2) {
            let date1 = format!("{} {}", tokens[t].2, tokens[t + 1].2);
            let sep_idx = t + 2;
            if sep_idx < tokens.len() && is_sep_word(tokens[sep_idx].2) {
                if sep_idx + 2 < tokens.len()
                    && is_month(tokens[sep_idx + 1].2)
                    && is_year(tokens[sep_idx + 2].2)
                {
                    return Some((
                        tokens[t].0,
                        tokens[sep_idx + 2].1,
                        date1,
                        format!("{} {}", tokens[sep_idx + 1].2, tokens[sep_idx + 2].2),
                    ));
                }
                if sep_idx + 1 < tokens.len() && is_year(tokens[sep_idx + 1].2) {
                    return Some((
                        tokens[t].0,
                        tokens[sep_idx + 1].1,
                        date1,
                        tokens[sep_idx + 1].2.to_string(),
                    ));
                }
                if sep_idx + 1 < tokens.len() && is_present_word(tokens[sep_idx + 1].2) {
                    return Some((
                        tokens[t].0,
                        tokens[sep_idx + 1].1,
                        date1,
                        "Present".to_string(),
                    ));
                }
            }
        }

        // "<Year> <sep> (<Year>|Present)" — a bare-year range, no months.
        if is_year(tokens[t].2) {
            let sep_idx = t + 1;
            if sep_idx < tokens.len() && is_sep_word(tokens[sep_idx].2) {
                if sep_idx + 1 < tokens.len() && is_year(tokens[sep_idx + 1].2) {
                    return Some((
                        tokens[t].0,
                        tokens[sep_idx + 1].1,
                        tokens[t].2.to_string(),
                        tokens[sep_idx + 1].2.to_string(),
                    ));
                }
                if sep_idx + 1 < tokens.len() && is_present_word(tokens[sep_idx + 1].2) {
                    return Some((
                        tokens[t].0,
                        tokens[sep_idx + 1].1,
                        tokens[t].2.to_string(),
                        "Present".to_string(),
                    ));
                }
            }
        }
    }
    None
}

/// Extract a trailing date range from a "Project N: Title – Subtitle  Start
/// – End" header line, returning (name_without_dates, start, end). Unlike
/// `extract_date_range_from_end`, this deliberately does NOT try "another
/// occurrence of the same separator marks off the start too" — a project
/// title very often contains its own " – " ("Title – Subtitle"), which that
/// fast path would mistake for the boundary between name and start date.
/// Instead this only ever scans the trailing whitespace-separated tokens
/// for a month/year (or bare year) pattern immediately before the end
/// date — the same safe fallback `extract_date_range_from_end` itself
/// falls back to when there's no second separator — so an internal dash in
/// the title is never treated as anything but title text.
pub(super) fn extract_trailing_date_range_from_title(
    line: &str,
) -> Option<(String, String, String)> {
    let lower = line.to_lowercase();
    let present_words = ["present", "current", "actuel", "prèsent"];
    for sep in &[" – ", " - ", " — "] {
        if let Some(last_pos) = lower.rfind(sep) {
            let end_part = line[last_pos + sep.len()..].trim();
            let end_lower = end_part.to_lowercase();
            let is_present = present_words.iter().any(|pw| end_lower.contains(pw));
            let end_has_year = end_part.chars().any(|c| c.is_ascii_digit());
            if !is_present && !end_has_year {
                continue;
            }
            let end = if is_present {
                "Present".to_string()
            } else {
                end_part.to_string()
            };
            if end.split_whitespace().count() > 3 {
                continue;
            }
            let left_of_end = &line[..last_pos];
            let words: Vec<&str> = left_of_end.split_whitespace().collect();
            if words.len() >= 2 && looks_like_date_token(words[words.len() - 2]) {
                let start_part = format!("{} {}", words[words.len() - 2], words[words.len() - 1]);
                let name = words[..words.len() - 2].join(" ");
                if !name.trim().is_empty() {
                    return Some((name.trim().to_string(), start_part, end));
                }
            } else if let Some(&last_word) = words.last() {
                if last_word.chars().all(|c| c.is_ascii_digit()) && last_word.len() == 4 {
                    let name = words[..words.len() - 1].join(" ");
                    if !name.trim().is_empty() {
                        return Some((name.trim().to_string(), last_word.to_string(), end));
                    }
                }
            }
        }
    }
    None
}

/// Try to detect a date range like "Jan 2021 - Present" or "2020 - 2024" or "2020 - Présent".
pub(super) fn extract_date_range(line: &str) -> Option<(String, String)> {
    let present_words = ["present", "current", "aujourd", "prèsent", "actuel"];

    let lower = line.to_lowercase();

    // Try each separator, prefer the LAST occurrence (for "Role · Company – 2020 – 2024")
    for sep in &[" – ", " - ", " — ", " to ", " à ", " au "] {
        if let Some(pos) = lower.rfind(sep) {
            let left = line[..pos].trim();
            let right = line[pos + sep.len()..].trim();
            let right_lower = right.to_lowercase();

            // Verify left side doesn't look too short (avoid matching "A - B" within names)
            if left.len() < 3 {
                continue;
            }

            let is_present = present_words.iter().any(|pw| right_lower.contains(pw));

            let start = left.to_string();
            let end = if is_present {
                "Present".to_string()
            } else {
                right.to_string()
            };
            return Some((start, end));
        }
    }

    None
}

/// Month names (English + French) used to sanity-check that a token really
/// looks like the start of a date, not just any word.
pub(super) const MONTH_NAMES: &[&str] = &[
    "january",
    "february",
    "march",
    "april",
    "may",
    "june",
    "july",
    "august",
    "september",
    "october",
    "november",
    "december",
    "janvier",
    "février",
    "fevrier",
    "mars",
    "avril",
    "mai",
    "juin",
    "juillet",
    "août",
    "aout",
    "septembre",
    "octobre",
    "novembre",
    "décembre",
    "decembre",
];

pub(super) fn looks_like_date_token(s: &str) -> bool {
    let trimmed = s.trim();
    // Strip a single leading decorative icon glyph (a calendar icon
    // sometimes rendered as a stray non-alphanumeric character glued
    // directly onto the month name with no space, e.g. "\u{11}February")
    // before checking for a month name — otherwise this token silently
    // fails to look like a date, and callers that use this check to
    // decide "does the date range actually start here" (extract_date_range_from_end's
    // fallback path in particular) fall back to treating just the bare
    // year as the start and misread the icon+month as leftover
    // role/company text, spawning a bogus new job/experience entry out of
    // what is really just this project's own icon-prefixed date range.
    let unwrapped = match trimmed.chars().next() {
        Some(c) if !c.is_alphanumeric() => trimmed[c.len_utf8()..].trim_start(),
        _ => trimmed,
    };
    let lower = unwrapped.to_lowercase();
    if lower.is_empty() {
        return false;
    }
    if MONTH_NAMES.iter().any(|m| lower.starts_with(m)) {
        return true;
    }
    if lower.chars().take(4).all(|c| c.is_ascii_digit()) {
        return true;
    }
    let present_words = ["present", "current", "actuel", "aujourd", "no expiration"];
    present_words.iter().any(|p| lower.contains(p))
}

/// Detect a line that is ENTIRELY a date range — optionally prefixed by an
/// icon glyph (common: a calendar icon rendered as a stray character) and/or
/// followed by a location — with no role/company text on the same line.
/// This is the common "Role\nCompany\nDates Location" three-line CV layout,
/// as opposed to the single-line "Role - Start - End" layout that
/// `extract_date_range_from_end` handles. Returns (start, end, location).
pub(super) fn extract_standalone_date_range(
    line: &str,
) -> Option<(String, String, Option<String>)> {
    let stripped = line.trim_start_matches(|c: char| !c.is_ascii_alphanumeric());
    if stripped.is_empty() {
        return None;
    }
    let (start, end) = extract_date_range(stripped)?;
    let start = start.trim();
    if !looks_like_date_token(start) {
        return None;
    }

    let end_lower = end.to_lowercase();
    let present_words = ["present", "current", "actuel", "aujourd"];
    if present_words.iter().any(|p| end_lower.contains(p)) {
        return Some((start.to_string(), "Present".to_string(), None));
    }

    // `end` may have trailing location text after the actual end date, e.g.
    // "February 2026 ½ Paris, France". Find the first 4-digit year run and
    // split everything after it off as location.
    let chars: Vec<char> = end.chars().collect();
    let mut year_end_char_idx = None;
    let mut i = 0;
    while i + 4 <= chars.len() {
        if chars[i..i + 4].iter().all(|c| c.is_ascii_digit()) {
            year_end_char_idx = Some(i + 4);
            break;
        }
        i = i.saturating_add(1);
    }
    let year_end_idx = year_end_char_idx?;
    let byte_idx = end
        .char_indices()
        .nth(year_end_idx)
        .map(|(b, _)| b)
        .unwrap_or(end.len());
    let end_date = end[..byte_idx].trim().to_string();
    let rest = end[byte_idx..]
        .trim()
        .trim_start_matches(|c: char| !c.is_ascii_alphanumeric())
        .trim();
    let location = if rest.is_empty() {
        None
    } else {
        Some(rest.to_string())
    };
    Some((start.to_string(), end_date, location))
}

/// Common month abbreviations (English + French), used only by the looser
/// Education date check below — kept separate from `looks_like_date_token`
/// so relaxing it can't cause false positives in Experience job-boundary
/// detection or the stray-content reclaim pass (e.g. mistaking a
/// Certification's "Aug 2018" for the start of a new job).
pub(super) const MONTH_ABBREVIATIONS: &[&str] = &[
    "jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "sept", "oct", "nov", "dec",
    "janv", "févr", "fevr", "mars", "avr", "juil", "juin", "aout", "août", "déc",
];

pub(super) fn looks_like_date_token_loose(s: &str) -> bool {
    if looks_like_date_token(s) {
        return true;
    }
    let trimmed = s.trim();
    // Same leading-icon-glyph tolerance as `looks_like_date_token` (see its
    // comment) — the abbreviated-month fallback needs it too, since the
    // Education section's own trailing-date detector (`extract_trailing_date_range_loose`)
    // hits the exact same "\u{11}Sept 2014 – ..." pattern.
    let unwrapped = match trimmed.chars().next() {
        Some(c) if !c.is_alphanumeric() => trimmed[c.len_utf8()..].trim_start(),
        _ => trimmed,
    };
    let lower = unwrapped.to_lowercase();
    MONTH_ABBREVIATIONS.iter().any(|m| lower.starts_with(m))
}

/// Detect a standalone date-range line, allowing abbreviated month names —
/// used for Education entries, which commonly abbreviate ("Sept 2014 – Oct
/// 2017"). Kept separate from `extract_standalone_date_range` (used for
/// Experience) to avoid loosening validation in places where a false match
/// would misfire a job boundary.
pub(super) fn extract_standalone_date_range_loose(line: &str) -> Option<(String, String)> {
    let stripped = line.trim_start_matches(|c: char| !c.is_ascii_alphanumeric());
    if stripped.is_empty() {
        return None;
    }
    let (start, end) = extract_date_range(stripped)?;
    let start = start.trim();
    if !looks_like_date_token_loose(start) {
        return None;
    }
    Some((start.to_string(), end.trim().to_string()))
}

/// Parse education section lines into Education entries.
///
/// Common CV layout: a degree-title line, then one or more field-of-study
/// lines (which may wrap across 2+ physical lines), then one or more
/// institution/location lines (which may also wrap, e.g. "University X,
/// City," followed by "Country" on the next line), then a standalone date
/// range completing that entry. Everything before the date is plain text
/// with no bullet markers, so we buffer plain lines until a date range is
/// found, then split the buffer into degree / field / institution — see
/// `build_education_from_buffer`.
/// Same idea as `extract_date_range_from_end`'s whitespace-only fallback
/// (see the comment there), but using the loose, abbreviated-month-aware
/// token check, and also returning the leading text found before the
/// date. Used by `parse_education` for this app's own "University of X,
/// Location  Sept 2014 – Oct 2017" row — institution+location and the
/// date range share one line via the same Chromium same-row flex-split
/// behavior noted on `extract_date_range_from_end`, just with the
/// abbreviated month names education dates commonly use.
pub(super) fn extract_trailing_date_range_loose(line: &str) -> Option<(String, String, String)> {
    let lower = line.to_lowercase();
    let present_words = ["present", "current", "actuel", "prèsent"];
    for sep in &[" – ", " - ", " — "] {
        if let Some(last_pos) = lower.rfind(sep) {
            let end_part = line[last_pos + sep.len()..].trim();
            let end_lower = end_part.to_lowercase();
            let is_present = present_words.iter().any(|pw| end_lower.contains(pw));
            let end_has_year = end_part.chars().any(|c| c.is_ascii_digit());
            if !is_present && !end_has_year {
                continue;
            }
            let end = if is_present {
                "Present".to_string()
            } else {
                end_part.to_string()
            };
            if end.split_whitespace().count() > 3 {
                continue;
            }
            let left_of_end = &line[..last_pos];
            let words: Vec<&str> = left_of_end.split_whitespace().collect();
            let real_text = |s: &str| s.chars().filter(|c| c.is_alphabetic()).count() >= 2;
            if words.len() >= 2 && looks_like_date_token_loose(words[words.len() - 2]) {
                let start_part = format!("{} {}", words[words.len() - 2], words[words.len() - 1]);
                let before = words[..words.len() - 2].join(" ");
                if real_text(&before) {
                    return Some((before, start_part, end));
                }
            } else if let Some(&last_word) = words.last() {
                if last_word.chars().all(|c| c.is_ascii_digit()) && last_word.len() == 4 {
                    let before = words[..words.len() - 1].join(" ");
                    if real_text(&before) {
                        return Some((before, last_word.to_string(), end));
                    }
                }
            }
        }
    }
    None
}

/// This app's own renderer prefixes every date range with a small
/// calendar-icon glyph. Chromium's print engine sometimes fragments that
/// combined "icon + date range" text far more aggressively than the
/// same-row splitting handled in `extract_text_from_page` — not just
/// company/location landing on a different text object, but the icon,
/// the month name, and the "year – year" portion each ending up as their
/// own separate line, e.g.:
///   "\u{11}"
///   "Sept"
///   "2014 – Oct 2017"
/// instead of one "\u{11} Sept 2014 – Oct 2017" line — which breaks every
/// date-range parser downstream (none of them expect the month name to be
/// off on its own line, disconnected from its year). This rejoins that
/// specific pattern: lone icon-glyph lines (a single non-alphanumeric
/// character — carries no information either way) are dropped, and a
/// lone month name is reunited with an immediately following "YYYY – …"
/// line.
pub(super) fn rejoin_fragmented_date_lines(lines: &[String]) -> Vec<String> {
    let is_lone_icon_glyph = |s: &str| {
        let t = s.trim();
        let mut chars = t.chars();
        match (chars.next(), chars.next()) {
            (Some(c), None) => !c.is_alphanumeric(),
            _ => false,
        }
    };
    let is_lone_month = |s: &str| {
        let t = s.trim().to_lowercase();
        !t.is_empty()
            && t.chars().all(|c| c.is_alphabetic())
            && (MONTH_NAMES.contains(&t.as_str()) || MONTH_ABBREVIATIONS.contains(&t.as_str()))
    };
    // A month name, optionally still carrying its own leading icon glyph
    // (e.g. "\u{11} February", not yet split off by `is_lone_icon_glyph`
    // because it never got its own separate line — see the reversed-order
    // case below).
    let month_after_optional_icon = |s: &str| -> Option<String> {
        let t = s.trim();
        let mut chars = t.chars();
        let first = chars.next()?;
        let rest = if !first.is_alphanumeric() {
            t[first.len_utf8()..].trim_start()
        } else {
            t
        };
        is_lone_month(rest).then(|| rest.to_string())
    };
    let starts_with_year = |s: &str| {
        let t = s.trim();
        t.len() >= 4 && t.as_bytes()[..4].iter().all(|b| b.is_ascii_digit())
    };
    // A date range missing its start month — just "YYYY – …" — where the
    // month that belongs at the very front ended up stranded on the
    // *following* line instead of the preceding one (the mirror image of
    // the "lone month, then year range" case above; which order Chromium
    // fragments a given date row into isn't consistent).
    let starts_with_bare_year_then_dash = |s: &str| {
        let t = s.trim();
        let mut parts = t.splitn(2, char::is_whitespace);
        match parts.next() {
            Some(first) if first.len() == 4 && first.chars().all(|c| c.is_ascii_digit()) => {
                let rest = parts.next().unwrap_or("").trim_start();
                rest.starts_with(['–', '-', '—'])
            }
            _ => false,
        }
    };

    let mut out: Vec<String> = Vec::with_capacity(lines.len());
    // Consume the slice through an iterator rather than a `while i < len`
    // counter: each "skip the joined partner line" step is `iter.next()`
    // (structural, can't regress), so cargo-mutants has no arithmetic
    // index whose `+=`→`*=` (e.g. `i *= 2` on `i == 0`) turns the loop
    // non-terminating and hangs the whole mutation shard. The peek/next
    // pairing below mirrors the old `lines.get(i + 1)` / `i += 2` exactly.
    let mut iter = lines.iter().peekable();
    while let Some(line) = iter.next() {
        if is_lone_icon_glyph(line) {
            let next = iter.peek().copied();
            if next.is_some_and(|n| is_lone_month(n) || starts_with_year(n)) {
                // Confirmed decorative calendar icon directly preceding a
                // date fragment — safe to drop.
                continue;
            }
            // Otherwise this is just some lone symbol character that
            // happened to land on its own line (e.g. an approx sign "∼"
            // separated from the number it belongs to, "∼1M rows…") —
            // not confirmed to be a decorative icon, so preserve it by
            // attaching it to whatever follows instead of silently
            // dropping data. No inserted space: this mirrors how such a
            // glyph directly touches the text it decorates/prefixes both
            // visually and in the source content.
            if let Some(next) = next {
                out.push(format!("{}{}", line.trim(), next.trim()));
                iter.next();
                continue;
            }
            continue;
        }
        if is_lone_month(line) {
            if let Some(next) = iter.peek().copied() {
                if starts_with_year(next) {
                    out.push(format!("{} {}", line.trim(), next.trim()));
                    iter.next();
                    continue;
                }
            }
        }
        if starts_with_bare_year_then_dash(line) {
            if let Some(next) = iter.peek().copied() {
                if let Some(month) = month_after_optional_icon(next) {
                    out.push(format!("{} {}", month, line.trim()));
                    iter.next();
                    continue;
                }
            }
        }
        out.push(line.clone());
    }
    out
}
