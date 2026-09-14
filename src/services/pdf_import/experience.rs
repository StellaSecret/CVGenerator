use super::*;

/// Parse experience section lines into Experience entries.
/// Parse experience section lines into Experience entries. Also returns any
/// sidebar tool/skill list entries harvested along the way (see the
/// "is_skill_bleed_line" handling below) — these never become part of any
/// bullet, so they can't be recovered by a later bullet-scanning pass.
pub(super) fn parse_experiences(lines: &[String]) -> (Vec<Experience>, Vec<Skill>) {
    let lines = rejoin_fragmented_date_lines(lines);
    // Normalize any line whose date range `extract_date_range_from_end`
    // can't find (because it's not literally at the end of the line, or
    // uses a separator/shape it doesn't recognize — see
    // `find_date_range_span`'s doc comment) into the plain "<before> -
    // <start> - <end>" shape the rest of this function already knows how
    // to parse. Trailing text after the date range (e.g. a contract type
    // and city tacked on after the dates, "- CDI - La Rochelle") is
    // dropped here rather than preserved: recovering the job's existence,
    // title, company, dates, and bullets correctly is the priority, and
    // there's no reliable general way to route that trailing fragment into
    // the right Experience field from here.
    let lines: Vec<String> = lines
        .into_iter()
        .map(|line| {
            if extract_date_range_from_end(line.trim()).is_some() {
                return line;
            }
            // Guard against misfiring on ordinary prose bullets that
            // happen to mention a date range mid-sentence (e.g. version
            // upgrade notes) — real job/project header lines are
            // compact rows built mostly from proper nouns and short tags
            // (company name, contract type, city, country), not running
            // prose, so a raw word-count cutoff has to be generous enough
            // to admit a header with several trailing segments (e.g.
            // "Company - Month Year à Month Year - CDI - City - Country"
            // — 14 words) while still catching genuinely long sentences.
            if line.split_whitespace().count() > 20 {
                return line;
            }
            match find_date_range_span(line.trim()) {
                Some((span_start, _span_end, start, end)) => {
                    let before = line.trim()[..span_start]
                        .trim()
                        .trim_end_matches(['-', '–', '—'])
                        .trim();
                    // Require real text before the date — at least a
                    // couple of letters, not just a decorative icon glyph
                    // (e.g. this app's own icon-prefixed standalone date
                    // row). A line that's really just a date with no
                    // company/role text of its own is already handled
                    // correctly by `extract_standalone_date_range` further
                    // down (which recovers role+company from the
                    // *previous* two lines) — normalizing it here would
                    // instead wrongly hand this branch a bare icon
                    // character as "before_date" and hijack a case that
                    // already works.
                    if before.chars().filter(|c| c.is_alphabetic()).count() < 2 {
                        line
                    } else {
                        format!("{before} - {start} - {end}")
                    }
                }
                None => line,
            }
        })
        .collect();
    let lines = lines.as_slice();
    let mut experiences = Vec::new();
    let mut harvested_skills: Vec<Skill> = Vec::new();
    let mut current_exp: Option<Experience> = None;
    // The three pieces of the project currently being built. Previously
    // these were tracked in disconnected ways — a "candidate name" that got
    // silently overwritten by every plain line and only rarely actually
    // attached to the bullets it should have gone with, and Situation:/
    // Tasks:/Actions taken: intro text that was discarded outright. Now
    // they're flushed together, in one place, as a single coherent project.
    let mut current_project_name: Option<String> = None;
    // The current project's own date range, if it has one distinct from
    // the parent job's dates (e.g. "Project 1: ... \n February 2025 –
    // February 2026") — set when a standalone date line immediately
    // follows a "Project N:" header (see `just_after_project_header`
    // below) and carried through to `flush_project`.
    let mut current_project_start: String = String::new();
    let mut current_project_end: String = String::new();
    let mut current_context: Vec<String> = Vec::new();
    // Raw text of an in-progress "Techs: ..." line (which itself often wraps
    // across several PDF lines) — parsed into project.skill_ids (as staged
    // raw names, resolved to real Skill ids later) at flush time.
    let mut current_tools_text: String = String::new();
    let mut current_bullets: Vec<LocalizedText> = Vec::new();
    // Shadow lookback buffer of the last two plain (non-bullet, non-date)
    // lines seen, used to recover role+company when we hit a standalone
    // date-range line (see extract_standalone_date_range).
    let mut recent_plain: Vec<String> = Vec::new();
    // True only for the single line immediately following a "Project N:
    // ..." header — used to tell "this date range belongs to the project
    // whose header we JUST saw" from "a project is generically still open"
    // (current_project_name alone can't distinguish these: it stays Some
    // across everything up to the NEXT header, which is needed for the
    // eventual flush, but would otherwise make a much-later, genuinely new
    // job's date line look like it's still "inside" an old project).
    let mut just_after_project_header = false;
    // Set to `i + 2` when the date-range branch below consumes lines[i+1]
    // as a standalone role line (see the "company-first" layout comment
    // there) — skips it so it isn't also processed as stray plain text.
    let mut skip_until = 0usize;

    for i in 0..lines.len() {
        if i < skip_until {
            continue;
        }
        let trimmed = lines[i].trim();
        if trimmed.is_empty() {
            continue;
        }

        // Sidebar tool/skill lists sometimes bleed into the Experience
        // section (a multi-column PDF artifact — see the docs on
        // decode_operations/run_operations). A line that's itself already
        // in the "<tool> N+ yrs" shape is unambiguous; a short bullet
        // immediately followed by one (e.g. "• Cloud" right before "AWS 1+
        // yrs") is that list's category header. Skip both entirely —
        // touching neither bullets, context, nor the recent_plain
        // lookback — so this noise can't corrupt whatever's legitimately
        // pending (a real job's role+company waiting to be confirmed by
        // its date line, which may be lines away on the other side of a
        // whole block of this bleed). Harvest any "<tool> N+ yrs" segments
        // into real Skills here, since skipped lines never become bullets
        // for a later pass to find.
        let next_trimmed = lines.get(i + 1).map(|l| l.trim());
        // A line that starts with a bullet marker is narrative content, not a
        // sidebar "<tool> N+ yrs" fragment — even if it happens to contain a
        // years marker ("• Cloud AWS1+ yrs"). Only bare (non-bullet) lines can
        // be raw skill segments; the "bullet + next-line-is-skill" case is
        // handled separately below by is_skill_bleed_line's bullet branch.
        let this_line_segments = if trimmed.starts_with(['•', '·', '-', '–', '*', '▸', '▪'])
        {
            None
        } else {
            harvest_skill_segments(trimmed)
        };
        // A name that's split from its own "N+yrs" marker onto the very
        // next line (see is_bare_years_marker's doc comment for why that
        // happens even within a single visual sidebar row). Guarded on
        // this_line_segments being None so it only ever applies to a line
        // harvest_skill_segments couldn't already make sense of on its
        // own, and excludes anything that already reads as a bullet, a
        // block label, or (defensively) a marker itself, to keep this
        // narrowly scoped to the one real pattern it's for.
        let name_before_bare_marker = this_line_segments.is_none()
            && next_trimmed.map(is_bare_years_marker).unwrap_or(false)
            && !trimmed.starts_with(['•', '·', '-', '–', '*', '▸', '▪'])
            && trimmed.len() <= 40
            && !looks_like_block_label(trimmed)
            && !is_bare_years_marker(trimmed);
        let is_skill_bleed_line = this_line_segments.is_some()
            || (trimmed.starts_with(['•', '·', '-', '–', '*', '▸', '▪'])
                && trimmed
                    .trim_start_matches(['•', '·', '-', '–', '*', '▸', '▪'])
                    .trim()
                    .len()
                    <= 40
                && next_trimmed
                    .map(|n| harvest_skill_segments(n).is_some())
                    .unwrap_or(false))
            || name_before_bare_marker;
        if is_skill_bleed_line {
            for seg in this_line_segments.into_iter().flatten() {
                harvested_skills.push(Skill {
                    id: uuid::Uuid::new_v4().to_string(),
                    name: seg,
                    category: SkillCategory::default(),
                    level: SkillLevel::Intermediate,
                });
            }
            if name_before_bare_marker {
                harvested_skills.push(Skill {
                    id: uuid::Uuid::new_v4().to_string(),
                    name: format!(
                        "{} {}",
                        trimmed,
                        next_trimmed.expect("name_before_bare_marker implies next line exists")
                    ),
                    category: SkillCategory::default(),
                    level: SkillLevel::Intermediate,
                });
                // Also consume the marker line itself on the next
                // iteration — on its own it matches none of the other
                // branches (it's not a bullet, date range, or label), so
                // without this it would fall through to recent_plain as a
                // meaningless "2+yrs" fragment.
                skip_until = i + 2;
            }
            continue;
        }

        // Check for a new experience entry: has a date range at the END.
        // Guarded against `is_project_header` lines — a "Project N: Title
        // – Subtitle  Start – End" header (our own renderer now draws a
        // project's inline date range this way once it actually HAS
        // start/end dates, instead of the empty-date fallback that used to
        // wrap it to its own line) contains a dash inside the title itself
        // ("Title – Subtitle"), which `extract_date_range_from_end`'s
        // "another occurrence of the same separator marks off the start
        // too" fast path mistakes for the *start* of the date range —
        // splitting the line into a bogus new job whose role/company is
        // just the first half of the project title. Project headers are
        // never a new top-level job no matter what follows them, so let
        // them fall through untouched to the dedicated handling below,
        // which extracts a trailing date range the same safe way without
        // being fooled by an internal separator.
        if let Some((start, end)) =
            extract_date_range_from_end(trimmed).filter(|_| !is_project_header(trimmed))
        {
            // Parse role + company from the text BEFORE the date range,
            // computed here (before `flush_pending_lines` below) because
            // deciding layout (d) — see further down — needs to peek at,
            // and potentially claim, `recent_plain`'s last entry as a role
            // candidate before that flush drains it into the *closing*
            // job's context, where it would otherwise be permanently
            // misattributed to the wrong job.
            let mut before_date_peek = trimmed;
            if let Some(pos) = before_date_peek.rfind(&end) {
                before_date_peek = before_date_peek[..pos].trim();
            }
            before_date_peek = before_date_peek.trim_end_matches(['-', '–', '—']).trim();
            if let Some(pos) = before_date_peek.rfind(&start) {
                before_date_peek = before_date_peek[..pos].trim();
            }
            before_date_peek = before_date_peek
                .trim()
                .trim_end_matches(['-', '–', '—'])
                .trim();
            // Layout (d): "Role" on its own PRECEDING line, then "Company -
            // Start - End" — common in real-world exports where a job's
            // bullets have no extractable marker character, which would
            // otherwise make the first bullet indistinguishable from
            // layout (b)'s "role on the next line". Only claim
            // `recent_plain`'s last entry when `before_date_peek` doesn't
            // already look self-contained (it has no role/company
            // separator of its own) — otherwise this would second-guess a
            // line that already fully describes itself.
            //
            // `recent_plain`'s last entry is exactly as likely to be a
            // genuine role line (job N's title, right before job N's own
            // "Company - Dates" row) as it is to be the wrapped LAST line
            // of the PREVIOUS job's non-bulleted paragraph (e.g. this
            // app's own rendered output: "...concernant les" / "volets
            // sécurité et conformité") — both arrive here identically, as
            // the tail of a full `recent_plain` buffer. The two can't be
            // told apart by position/length, only by content:
            // `looks_like_bare_role_line`'s capitalization check is what
            // actually rejects the wrapped-tail case (a wrapped sentence
            // resumes lowercase; a title doesn't) — see its doc comment.
            let prev_plain_role = if before_date_peek.contains(" at ")
                || before_date_peek.contains(" chez ")
                || before_date_peek.contains(" · ")
                || before_date_peek.contains(" | ")
                || before_date_peek.contains(", ")
            {
                None
            } else {
                recent_plain
                    .last()
                    .filter(|l| looks_like_bare_role_line(l))
                    .cloned()
            };
            if prev_plain_role.is_some() {
                recent_plain.pop();
            }

            // Anything still pending is stray context belonging to the
            // experience we're about to close — commit it before flushing.
            flush_pending_lines(
                &mut recent_plain,
                &mut current_context,
                &mut current_tools_text,
            );
            just_after_project_header = false;
            if let Some(mut exp) = current_exp.take() {
                flush_project(
                    &mut exp,
                    &mut current_project_name,
                    &mut current_project_start,
                    &mut current_project_end,
                    &mut current_context,
                    &mut current_tools_text,
                    &mut current_bullets,
                );
                experiences.push(exp);
            }
            current_project_name = None;
            current_context.clear();
            current_tools_text.clear();
            current_bullets.clear();

            // Parse role + company from the text BEFORE the date range
            // The date range is at the END: "... - start_date - end_date"
            // Remove the last occurrence of end, then start, from the trimmed line
            let mut before_date = trimmed;
            // Strip end date from the right
            if let Some(pos) = before_date.rfind(&end) {
                before_date = before_date[..pos].trim();
            }
            // Strip trailing separator
            before_date = before_date.trim_end_matches(['-', '–', '—']).trim();
            // Strip start date from the right
            if let Some(pos) = before_date.rfind(&start) {
                before_date = before_date[..pos].trim();
            }
            before_date = before_date.trim();
            // Strip trailing separator
            before_date = before_date.trim_end_matches(['-', '–', '—']).trim();

            // Layout (c): this app's own renderer, on a job whose company
            // is the same as the one immediately before it, omits the
            // company (and role) text entirely — a pre-existing rendering
            // quirk, not something introduced by import — leaving a bare
            // "· Paris, France Jan 2024 – Nov 2024" row with nothing but a
            // dangling separator and a location before the dates. There's
            // no role/company data actually present on this line to
            // recover; treat the whole thing as location and leave
            // role/company empty rather than let the generic ", "-split
            // fallback below misread the location's own internal comma
            // (e.g. "Paris, France") as a role/company separator.
            if before_date
                .trim_start()
                .starts_with(['·', '-', '–', '—', '|'])
            {
                let location = before_date
                    .trim_start_matches(['·', '-', '–', '—', '|', ' '])
                    .trim()
                    .to_string();
                let exp = Experience {
                    id: uuid::Uuid::new_v4().to_string(),
                    location,
                    start_date: start,
                    end_date: end,
                    ..Default::default()
                };
                current_exp = Some(exp);
                recent_plain.clear();
                continue;
            }

            // Two different real-world layouts land here:
            //   (a) "Role at/chez/·/| Company - Start - End" — role and
            //       company are both on this line, role first.
            //   (b) "Company · Location   Start – End" on one line, with
            //       the role on its OWN following line (this app's own
            //       renderer: exp-header has company+location+dates, then
            //       a separate exp-role div right after). Nothing on
            //       *this* line distinguishes which layout it is — only
            //       the next line does, so peek at it.
            // " at "/" chez " are unambiguous role-first markers (a
            // company/location pair is never phrased "X at Y"), so those
            // always take the (a) path below.
            let next_line = lines.get(i + 1).map(|l| l.trim());
            let unambiguous_role_first =
                before_date.contains(" at ") || before_date.contains(" chez ");
            let mut consumed_role_line = false;
            let (role_text, company_text, location_text) = if let Some(prev_role) = prev_plain_role
            {
                // Layout (d), decided above (before the flush): the role
                // was on its own preceding plain line.
                (prev_role, before_date.to_string(), String::new())
            } else if !unambiguous_role_first
                && next_line.map(looks_like_bare_role_line).unwrap_or(false)
            {
                let (company, location) = split_company_and_location(before_date);
                consumed_role_line = true;
                (
                    next_line
                        .expect("bare-role branch implies a next line")
                        .to_string(),
                    company,
                    location,
                )
            } else if let Some(pos) = before_date.rfind(" at ") {
                (
                    before_date[..pos].trim().to_string(),
                    before_date[pos + 4..].trim().to_string(),
                    String::new(),
                )
            } else if let Some(pos) = before_date.rfind(" chez ") {
                (
                    before_date[..pos].trim().to_string(),
                    before_date[pos + 6..].trim().to_string(),
                    String::new(),
                )
            } else if let Some(pos) = before_date.rfind(" · ") {
                (
                    before_date[..pos].trim().to_string(),
                    before_date[pos + 3..].trim().to_string(),
                    String::new(),
                )
            } else if let Some(pos) = before_date.rfind(" | ") {
                (
                    before_date[..pos].trim().to_string(),
                    before_date[pos + 3..].trim().to_string(),
                    String::new(),
                )
            } else if let Some(pos) = before_date.rfind(", ") {
                (
                    before_date[..pos].trim().to_string(),
                    before_date[pos + 2..].trim().to_string(),
                    String::new(),
                )
            } else {
                (before_date.to_string(), String::new(), String::new())
            };
            if consumed_role_line {
                skip_until = i + 2;
            }

            let exp = Experience {
                id: uuid::Uuid::new_v4().to_string(),
                role: LocalizedText::same(role_text),
                company: company_text,
                location: location_text,
                start_date: start,
                end_date: end,
                ..Default::default()
            };
            current_exp = Some(exp);
            recent_plain.clear();
            continue;
        }

        // Check for a standalone date-range line (the "Role\nCompany\nDates
        // Location" three-line layout): recover role+company from the last
        // two plain lines we saw.
        if let Some((start, end, location)) = extract_standalone_date_range(trimmed) {
            // If the line immediately before this date range was itself a
            // "Project N: ..." header, this date almost certainly belongs
            // to that project (its own "Title\nDates" line pair), not a new
            // job — don't split the experience just because a project
            // happens to have its own date range. NOTE: we deliberately
            // check a narrow "was the very last line processed a project
            // header" flag here, NOT whether current_project_name is still
            // set — that stays set across everything up to the NEXT
            // "Project N:" header (needed so it can be flushed with the
            // right bullets), which would otherwise still be true dozens of
            // lines later at the start of a genuinely new job and wrongly
            // swallow it.
            let prev_line_was_project_header = just_after_project_header;
            just_after_project_header = false;
            if prev_line_was_project_header {
                // This date range belongs to the project whose header we
                // just saw, not a new job — store it on the in-progress
                // project (picked up by `flush_project`) instead of
                // discarding it. Previously this was silently dropped
                // entirely, since `ExperienceProject` had nowhere to put
                // it; that's exactly the kind of content loss idempotence
                // requires (re-importing our own rendered PDF re-detects
                // this same standalone date line, since our renderer now
                // draws it, and used to erase it every round trip).
                current_project_start = start;
                current_project_end = end;
                recent_plain.clear();
                continue;
            }

            // Otherwise treat it as a new job entry. Close out the previous
            // experience first.
            if let Some(mut exp) = current_exp.take() {
                flush_project(
                    &mut exp,
                    &mut current_project_name,
                    &mut current_project_start,
                    &mut current_project_end,
                    &mut current_context,
                    &mut current_tools_text,
                    &mut current_bullets,
                );
                experiences.push(exp);
            }
            current_project_name = None;
            current_context.clear();
            current_tools_text.clear();
            current_bullets.clear();

            let company_text = recent_plain.pop().unwrap_or_default();
            let role_text = recent_plain.pop().unwrap_or_default();
            recent_plain.clear();

            let exp = Experience {
                id: uuid::Uuid::new_v4().to_string(),
                role: LocalizedText::same(role_text),
                company: company_text,
                start_date: start,
                end_date: end,
                location: location.unwrap_or_default(),
                ..Default::default()
            };
            current_exp = Some(exp);
            continue;
        }

        // Check for bullet points
        let is_bullet = trimmed.starts_with("•")
            || trimmed.starts_with("·")
            || trimmed.starts_with("- ")
            || trimmed.starts_with("– ")
            || trimmed.starts_with("* ")
            || trimmed.starts_with("▸")
            || trimmed.starts_with("▪");
        if is_bullet {
            // A bullet appearing confirms whatever's still pending in
            // recent_plain was genuine context, not a new job's role/company
            // (that pattern always has a date line immediately after the
            // company, never a bullet) — commit it now.
            flush_pending_lines(
                &mut recent_plain,
                &mut current_context,
                &mut current_tools_text,
            );
            just_after_project_header = false;
            let bullet_text = trimmed
                .trim_start_matches(['•', '·', '-', '–', '*', '▸', '▪'])
                .trim()
                .to_string();
            if !bullet_text.is_empty() {
                current_bullets.push(LocalizedText::same(bullet_text));
            }
            continue;
        }

        // A wrapped continuation of the previous bullet: PDFs give us one
        // line per visually-wrapped row, not one line per bullet, so a long
        // bullet sentence that wraps to 2-3 lines shows up as a bullet line
        // followed by plain (non-bulleted) continuation lines. If the last
        // bullet doesn't end in terminal punctuation and this line isn't
        // itself a recognizable new block (a "Label:" header), treat it as
        // more of that same bullet rather than a new project/role name.
        //
        // `continues_pending_label` guards a narrower case: a
        // "Techs:"/"Situation:"/etc. label was already seen and either
        // (a) is still sitting somewhere in the 2-entry recent_plain
        // lookback buffer (it isn't committed to
        // current_tools_text/current_context immediately, only when a 3rd
        // plain line pushes it out — so check every entry currently in the
        // buffer, not just the last one, or a "Techs:" list with 3+ items
        // would still lose everything past its first item to this same bug
        // the moment the label scrolls past the most-recent slot), or (b)
        // has already been evicted and committed, with current_tools_text
        // now holding its content. Each tech name in "Techs: Kubernetes,
        // Docker, ..." is just a bare word once split onto its
        // own PDF line, so without this check every one of them would pass
        // `!looks_like_block_label(trimmed)` and get vacuumed into an
        // unrelated, still-open bullet instead of ever reaching the label
        // they actually belong to — silently emptying it (see
        // tools_row_html in renderer.rs, whose "Techs:" label round-trips
        // through exactly this path).
        let continues_pending_label = !current_tools_text.is_empty()
            || recent_plain.iter().any(|l| looks_like_block_label(l));
        if current_exp.is_some()
            && !current_bullets.is_empty()
            && current_bullets
                .last()
                .map(|b| !ends_with_terminal_punct(&b.en))
                .unwrap_or(false)
            && !looks_like_block_label(trimmed)
            && !continues_pending_label
        {
            if let Some(last) = current_bullets.last_mut() {
                last.en.push(' ');
                last.en.push_str(trimmed);
                last.fr = last.en.clone();
            }
            just_after_project_header = false;
            continue;
        }

        if current_exp.is_none() {
            // Before any experience — might be a role/company line for the
            // first job; recent_plain (below) is what actually supplies
            // those when the date line arrives, so there's nothing else to
            // do here but wait for it.
            recent_plain.push(trimmed.to_string());
            if recent_plain.len() > 2 {
                recent_plain.remove(0);
            }
            just_after_project_header = false;
            continue;
        }

        if is_project_header(trimmed) {
            // A new "Project N: ..." sub-entry starts here. Anything still
            // pending in recent_plain is now confirmed to be genuine
            // context (a project header, not a date line, followed those
            // 1-2 plain lines) — commit it, then flush the completed
            // project (name, context, tools, bullets) as one coherent unit
            // before starting fresh.
            flush_pending_lines(
                &mut recent_plain,
                &mut current_context,
                &mut current_tools_text,
            );
            if let Some(ref mut exp) = current_exp {
                flush_project(
                    exp,
                    &mut current_project_name,
                    &mut current_project_start,
                    &mut current_project_end,
                    &mut current_context,
                    &mut current_tools_text,
                    &mut current_bullets,
                );
            }
            // If the header line itself carries a trailing inline date
            // range (our own renderer draws "Project N: Title Start –
            // End" on one line once the project actually has dates), pull
            // it off now so it doesn't have to rely on a separate
            // standalone date line following — and so the name stored
            // doesn't include the dates as literal text.
            if let Some((name, start, end)) = extract_trailing_date_range_from_title(trimmed) {
                current_project_name = Some(name);
                current_project_start = start;
                current_project_end = end;
                just_after_project_header = false;
            } else {
                current_project_name = Some(trimmed.to_string());
                just_after_project_header = true;
            }
            continue;
        }

        // A "Situation:"/"Tasks:"/"Actions taken:"/"Techs:"/etc. label, or
        // just a plain descriptive sentence. Don't commit it to
        // context/tools yet — it might turn out to be the role or company
        // line of a NEW job, which we won't know until we see whether a
        // standalone date range follows. Stays pending in recent_plain;
        // aging out (a 3rd plain line pushes it out) or a bullet/project
        // header appearing both confirm it as genuine context and commit
        // it via flush_pending_lines.
        recent_plain.push(trimmed.to_string());
        if recent_plain.len() > 2 {
            let evicted = recent_plain.remove(0);
            commit_pending_line(&mut current_context, &mut current_tools_text, &evicted);
        }
        just_after_project_header = false;
    }

    // Flush remaining
    flush_pending_lines(
        &mut recent_plain,
        &mut current_context,
        &mut current_tools_text,
    );
    if let Some(mut exp) = current_exp.take() {
        flush_project(
            &mut exp,
            &mut current_project_name,
            &mut current_project_start,
            &mut current_project_end,
            &mut current_context,
            &mut current_tools_text,
            &mut current_bullets,
        );
        experiences.push(exp);
    }

    (experiences, harvested_skills)
}

/// Label prefixes that introduce a technology/tool list rather than
/// narrative description text (e.g. "Techs: Kubernetes, Docker, ...").
pub(super) const TOOLS_LABEL_PREFIXES: &[&str] = &["techs", "tech stack", "technologies"];

/// Commit one plain line to either the tools accumulator (if it's a
/// "Techs: ..." line or a continuation of one) or the context accumulator
/// (merging into the previous entry if it's a wrapped continuation of an
/// unfinished sentence), matching the logic used while a project is
/// actively being built. Used both for immediate commits and for lines
/// that age out of the recent_plain lookback buffer.
pub(super) fn commit_pending_line(context: &mut Vec<String>, tools_text: &mut String, line: &str) {
    let lower = line.to_lowercase();
    let starts_techs = TOOLS_LABEL_PREFIXES.iter().any(|p| lower.starts_with(p));
    // Deliberately NOT ends_with_terminal_punct() here: that treats a
    // trailing ':' as "this text is finished", which is exactly backwards
    // for tools_text specifically. Right after "Techs:" itself gets
    // committed, tools_text *is* "Techs:" — ending in ':' — and the very
    // next line (the first tool name) needs this check to still say
    // "yes, keep appending", or the whole list is lost after just the
    // label (see tools_row_html in renderer.rs and the wrapped-bullet-
    // continuation check above, both of which exist for this same
    // "Techs:"-list-fragmented-across-many-PDF-lines scenario).
    let tools_text_open = !tools_text.trim_end().ends_with(['.', '!', '?']);
    if starts_techs
        || (!tools_text.is_empty()
            && tools_text_open
            && !is_context_label(line)
            && !is_project_header(line))
    {
        if !tools_text.is_empty() {
            tools_text.push(' ');
        }
        tools_text.push_str(line);
        return;
    }
    let context_continues = context
        .last()
        .map(|l| !ends_with_terminal_punct(l))
        .unwrap_or(false)
        && !is_context_label(line)
        && !is_project_header(line);
    if context_continues {
        let last = context
            .last_mut()
            .expect("context_continues implies non-empty context");
        last.push(' ');
        last.push_str(line);
    } else {
        context.push(line.to_string());
    }
}

/// Commit every line still pending in the recent_plain lookback buffer, in
/// order, then empty it.
pub(super) fn flush_pending_lines(
    recent_plain: &mut Vec<String>,
    context: &mut Vec<String>,
    tools_text: &mut String,
) {
    for line in recent_plain.drain(..) {
        commit_pending_line(context, tools_text, &line);
    }
}

/// A short line that's ALL CAPS (e.g. "TOOLS", "SKILLS") reads as a stray
/// sidebar section heading that bled in, not genuine narrative context —
/// real context sentences are essentially never bare, all-uppercase, and
/// only one or two words long.
pub(super) fn looks_like_stray_heading(line: &str) -> bool {
    let words: Vec<&str> = line.split_whitespace().collect();
    if words.is_empty() || words.len() > 2 {
        return false;
    }
    let has_letters = line.chars().any(|c| c.is_alphabetic());
    has_letters
        && line
            .chars()
            .filter(|c| c.is_alphabetic())
            .all(|c| c.is_uppercase())
}

/// Combine the currently-tracked project name, context, tools, and bullets
/// into one ExperienceProject and push it onto `exp`, then clear all four
/// accumulators. A no-op if there's nothing to flush.
pub(super) fn flush_project(
    exp: &mut Experience,
    name: &mut Option<String>,
    start_date: &mut String,
    end_date: &mut String,
    context: &mut Vec<String>,
    tools_text: &mut String,
    bullets: &mut Vec<LocalizedText>,
) {
    if name.is_none()
        && start_date.is_empty()
        && end_date.is_empty()
        && context.is_empty()
        && tools_text.is_empty()
        && bullets.is_empty()
    {
        return;
    }

    // Bare block labels with nothing merged after them (e.g. "Actions
    // taken:" immediately followed by bullets, with no further sentence)
    // add no information on their own, and a stray all-caps fragment (e.g.
    // "TOOLS") reads as a sidebar heading that bled in rather than real
    // narrative text — drop both rather than leaving them dangling in the
    // description.
    //
    // The colon check is deliberately also gated on length: a genuine bare
    // label is short ("Situation:", "Tasks:", "Industrialization:"), but a
    // long, information-rich sentence can just as easily end in a colon
    // purely because that's where it happened to wrap onto the next
    // reconstructed line — e.g. "Situation: Critical internal tools
    // required improved observability, reduced cloud costs, and reduced
    // support workload. Tasks:" is one full sentence, not a bare label,
    // and dropping it on the colon check alone silently threw away the
    // entire project intro rather than just an empty label.
    const BARE_LABEL_MAX_LEN: usize = 40;
    // One list entry per already-merged logical line, not one joined
    // paragraph — `commit_pending_line` above already splits "Situation:
    // ..." and "Tasks: ..." into separate entries (a context-label line
    // always starts a fresh one), so this list is naturally shaped close
    // to "one bullet per narrative beat" already, without needing any
    // further re-splitting here.
    let context_items: Vec<LocalizedText> = context
        .drain(..)
        .filter(|c| {
            let trimmed = c.trim_end();
            !(looks_like_stray_heading(c)
                || (trimmed.ends_with(':') && trimmed.chars().count() <= BARE_LABEL_MAX_LEN))
        })
        .map(LocalizedText::same)
        .collect();

    let tools: Vec<String> = {
        let raw = std::mem::take(tools_text);
        let after_label = raw.find(':').map(|i| &raw[i + 1..]).unwrap_or(&raw);
        after_label
            .split(',')
            .map(|t| t.trim().trim_end_matches('.').trim().to_string())
            .filter(|t| !t.is_empty())
            .collect()
    };

    exp.projects.push(ExperienceProject {
        id: uuid::Uuid::new_v4().to_string(),
        name: LocalizedText::same(name.take().unwrap_or_default()),
        context: context_items,
        // Interim staging, NOT final IDs: at this point in parsing, the
        // canonical `cv.skills` list may not exist yet (section order in
        // the source PDF is whatever it is — "skills" doesn't necessarily
        // come before "experience"). These raw parsed tool NAMES are
        // temporarily placed in `skill_ids` and resolved into real
        // `Skill.id`s by `resolve_project_skill_ids`, called once from the
        // top-level import function after `cv.skills` is finalized. A
        // name with no matching skill is dropped there, not kept as
        // free text — see that function's doc comment.
        skill_ids: tools,
        bullets: std::mem::take(bullets),
        start_date: std::mem::take(start_date),
        end_date: std::mem::take(end_date),
    });
}

/// Parse projects section lines.
pub(super) fn parse_projects(lines: &[String]) -> Vec<Project> {
    let mut projects = Vec::new();
    let mut current: Option<Project> = None;
    let mut current_bullets = Vec::new();

    for line in lines {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let is_bullet = trimmed.starts_with("•")
            || trimmed.starts_with("·")
            || trimmed.starts_with("- ")
            || trimmed.starts_with("* ");
        if is_bullet {
            let text = trimmed
                .trim_start_matches(['•', '·', '-', '*'])
                .trim()
                .to_string();
            if !text.is_empty() {
                current_bullets.push(LocalizedText::same(text));
            }
            continue;
        }

        // Save previous project
        if let Some(mut proj) = current.take() {
            proj.bullets = current_bullets.clone();
            current_bullets.clear();
            projects.push(proj);
        }

        // New project — could be "Name: description" or just "Name"
        if let Some(pos) = trimmed.find(": ") {
            let name = trimmed[..pos].trim().to_string();
            let desc = trimmed[pos + 2..].trim().to_string();
            current = Some(Project {
                id: uuid::Uuid::new_v4().to_string(),
                name,
                description: LocalizedText::same(desc),
                ..Default::default()
            });
        } else {
            current = Some(Project {
                id: uuid::Uuid::new_v4().to_string(),
                name: trimmed.to_string(),
                ..Default::default()
            });
        }
    }

    if let Some(mut proj) = current.take() {
        proj.bullets = current_bullets;
        projects.push(proj);
    }
    projects
}

/// Multi-column PDFs can interleave a sidebar's section header (and its
/// short body — e.g. "RANDOM SKILLS", "EDUCATION", "LANGUAGES",
/// "CERTIFICATES", "OTHERS", "INTERESTS") into the middle of the main
/// column's Experience content, because the raw text stream reflects draw
/// order, not visual reading order. Our simple linear section-splitter has
/// no way to know that; it just ends "experience" right there and dumps
/// everything after — including entire subsequent job entries — into
/// whatever section happens to be "active", where it's silently lost.
///
/// This scans every section that comes after the first "experience" section
/// for the same "Role/Project title\nDates" trigger `parse_experiences`
/// itself looks for. When found, that line and everything after it in the
/// section (which almost always turns out to be more Experience content
/// that got stranded) is moved back onto the end of the experience section,
/// leaving only the section's genuine leading content in place.
/// Finds the start of a genuine job-boundary role+company pair sitting at
/// the very end of a reclaimed stray slice, if present — the same thing
/// `split_into_sections`'s own resumption recovery (see its comment) would
/// independently find and carve into its own `("experience", ...)` tuple.
/// That mechanism cuts its *old* section tuple off exactly where it found
/// the date line, which means the role+company pair it recovered — having
/// been the last two non-bleed lines *before* that date line — ends up as
/// the very last content in the old tuple, with the date line itself
/// already excised into the new one. So rather than searching for a date
/// line in here (there isn't one — it's already gone), this looks for that
/// same trailing pair directly: scan backward from the end, skipping
/// `looks_like_tool_bleed_line` lines, same as that other scan, and reject
/// a "Project ..." pair exactly as it does.
pub(super) fn find_duplicate_job_boundary(stray: &[String]) -> Option<usize> {
    let mut idx = stray.len();
    let mut recovered = Vec::new();
    while idx > 0 && recovered.len() < 2 {
        idx -= 1;
        if looks_like_tool_bleed_line(&stray[idx]) {
            continue;
        }
        recovered.push(idx);
    }
    if recovered.len() == 2 {
        let role_idx = recovered[1];
        let company_idx = recovered[0];
        let is_project_subheader = [role_idx, company_idx]
            .iter()
            .any(|&k| stray[k].trim_start().to_lowercase().starts_with("project"));
        if !is_project_subheader {
            return Some(role_idx);
        }
    }
    None
}

pub(super) fn reclaim_stray_experience_content(
    sections: Vec<(&str, Vec<String>)>,
) -> Vec<(&str, Vec<String>)> {
    let mut out: Vec<(&str, Vec<String>)> = Vec::new();
    let mut seen_experience = false;

    for (name, lines) in sections {
        if name == "experience" {
            seen_experience = true;
            out.push((name, lines));
            continue;
        }
        if !seen_experience {
            out.push((name, lines));
            continue;
        }
        // Scan "ignore" (genuinely unrecognized/miscellaneous content —
        // see the test below, which recovers a job that got swallowed by
        // a following "INTERESTS" blurb) and "skills" (a two-column
        // resume's sidebar "Technical Skills"/"Tools" heading commonly
        // interleaves mid-page into the main column's narrative — see
        // `split_into_sections`'s multi-column comment — which flips the
        // active section to "skills" right in the middle of an
        // Experience entry's own Project sub-entries; nothing in
        // `split_into_sections` itself can tell that apart from a
        // genuine Skills section, so it silently absorbs everything
        // after, including entire Project sub-entries with their own
        // "Situation/Tasks/Actions taken/Results achieved" bullets,
        // until the next real section header. `parse_skills`'s
        // block-join logic then mangles that absorbed prose into
        // garbled pseudo-skill entries — a data-corruption bug, not
        // just a placement one, so this is worth reclaiming even though
        // it costs a little more risk than the "ignore" case below).
        //
        // Well-known sections like Education have their own dedicated
        // parser and their own entirely legitimate "short line, then a
        // standalone date range" shape (a degree line followed by its
        // date range) — reusing this heuristic there mistook a real
        // Education entry's own date for a stray Experience job leaking
        // across the boundary, silently stripping it out of Education
        // and fabricating a bogus Experience entry out of what's left
        // (e.g. just a month name).
        if name != "ignore" && name != "skills" {
            out.push((name, lines));
            continue;
        }

        let mut split_at: Option<usize> = None;
        // Tracks whether `split_at` came from the "Project N:"/context-
        // label trigger (which sweeps everything to the end of this
        // section unbounded) vs. the pre-existing date-range trigger
        // (which is itself already a job-boundary detection, anchored
        // right at that boundary — nothing later in the section to
        // dedupe against). Only the former needs the duplicate-boundary
        // check below.
        let mut split_from_block_trigger = false;
        for (i, line) in lines.iter().enumerate() {
            // A "Project N:"/"Projet N:" sub-entry header is on its own an
            // unambiguous signal that this line — and everything after it
            // — is stranded Experience content: a genuine sidebar
            // skills/tools list never contains one. This catches the
            // "Technical Skills" sidebar-bleed case above even when the
            // stranded Project's own narrative runs many lines before its
            // date range, which the date-range trigger below can't see
            // that far back through on its own.
            if is_project_header(line) || is_context_label(line) {
                split_at = Some(i);
                split_from_block_trigger = true;
                break;
            }
            if i == 0 || extract_standalone_date_range(line).is_none() {
                continue;
            }
            let prev = lines[i - 1].trim();
            let prev_is_plausible = !prev.is_empty()
                && prev.len() <= 100
                && !prev.starts_with(['•', '·', '-', '–', '*']);
            if !prev_is_plausible {
                continue;
            }
            // Include one more line of preceding context (the "role" line)
            // when it also looks like plain header text — matching the
            // "Role\nCompany\nDates" shape parse_experiences expects.
            let idx = if i >= 2 {
                let prev2 = lines[i - 2].trim();
                if !prev2.is_empty()
                    && prev2.len() <= 100
                    && !prev2.starts_with(['•', '·', '-', '–', '*'])
                {
                    i - 2
                } else {
                    i - 1
                }
            } else {
                i - 1
            };
            split_at = Some(idx);
            break;
        }

        if let Some(idx) = split_at {
            // For the "skills" case specifically: a run of bullet lines
            // immediately preceding the trigger (e.g. one last stray
            // "– Writing of ADR framework documents..." bullet right
            // before a reclaimed "Results achieved:" label) is almost
            // always the tail of the very same leaked block, not
            // genuine skills content — this format's real skill lines
            // are bare ("GitLab-CI 3+ yrs"), never bullet-prefixed.
            // Walk backward over any such bullets so they're reclaimed
            // together with what follows them instead of being left
            // behind as an orphaned fragment.
            let idx = if name == "skills" || name == "ignore" {
                let mut idx = idx;
                loop {
                    if idx > 0
                        && lines[idx - 1]
                            .trim_start()
                            .starts_with(['•', '·', '-', '–', '*'])
                    {
                        idx -= 1;
                        continue;
                    }
                    // A bullet can wrap onto a second physical line with
                    // no bullet marker of its own (e.g. "– Writing of ADR
                    // framework documents (configuration repositories,
                    // PRA, upgrade" / "workflows)."). If the line right
                    // before our boundary doesn't look like a genuine
                    // skill tag (no "Name N+ yrs" shape) but the line
                    // before *that* one does start with a bullet, treat
                    // it as that bullet's wrapped tail too.
                    if idx > 1
                        && !looks_like_tool_bleed_line(&lines[idx - 1])
                        && lines[idx - 2]
                            .trim_start()
                            .starts_with(['•', '·', '-', '–', '*'])
                    {
                        idx -= 2;
                        continue;
                    }
                    break;
                }
                idx
            } else {
                idx
            };
            let mut lines = lines;
            let mut stray = lines.split_off(idx);
            // `split_into_sections`'s own resumption-after-interruption
            // recovery (see the comment there) independently walks
            // backward from the *next* genuine job-boundary date line —
            // skipping this same sidebar bleed — to recover that job's
            // role+company, and already emits its own separate
            // ("experience", [role, company, date, ...rest]) tuple
            // starting right there. Left unbounded, my scan above would
            // sweep straight through that same role+company pair too
            // (nothing about it looks like sidebar bleed) and reclaim a
            // second copy of it here, so `merge_duplicate_sections` would
            // concatenate both into one experience list with the job's
            // header — and everything after it — duplicated. Stop this
            // reclaimed slice right before that pair so each line is
            // recovered exactly once, by whichever mechanism finds it
            // first.
            if split_from_block_trigger {
                if let Some(dup_at) = find_duplicate_job_boundary(&stray) {
                    stray.truncate(dup_at);
                }
            }
            // Push the reclaimed suffix as its own "experience" section
            // right here, in document order, rather than deferring it
            // into a single accumulator appended after the whole loop
            // finishes. `merge_duplicate_sections` (run right after this
            // function) folds every "experience"-named section together
            // in the order they appear — appending everything to the end
            // instead would put content back in the CV, but in the wrong
            // place: e.g. a Project 2 stranded by a page's sidebar
            // heading would land after every later job's entries instead
            // of right after that job's Project 1, once again scrambling
            // chronological order even though the data itself is no
            // longer lost.
            out.push((name, lines));
            out.push(("experience", stray));
        } else {
            out.push((name, lines));
        }
    }

    out
}
