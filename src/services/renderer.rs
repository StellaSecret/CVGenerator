use crate::i18n_core::{self, Lang};
use crate::models::{LifetimeCV, SkillCategory, TailoredCV};

// ── Helpers ───────────────────────────────────────────────────────────────────

fn esc(s: &str) -> String {
    break_ligatures(
        &s.replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
            .replace('"', "&quot;"),
    )
}

/// Expands precomposed Unicode ligature *characters* (U+FB00–FB04: ﬀ, ﬁ,
/// ﬂ, ﬃ, ﬄ) into their plain-letter spelling.
///
/// These don't come from anything we type — they show up in *imported*
/// text: a source PDF's font commonly maps its "ff"/"fi"/etc. ligature
/// glyph's ToUnicode entry straight to the single precomposed codepoint
/// (that's exactly what our own pdf_import.rs's ToUnicode CMap parser
/// decodes, faithfully, from real-world PDFs — including our own output,
/// see below). If that raw codepoint reaches the browser unchanged, our
/// main body font typically has no glyph for it directly (it only forms
/// ligatures via the "liga" GSUB feature applied to a *sequence* of plain
/// letters, not via this standalone presentation-form character), so the
/// browser silently font-substitutes just that one character from a
/// fallback font. Printing to PDF then emits that single word as three
/// separate font runs (main/fallback/main), each its own BT/ET text
/// object — which breaks the same-row line reconstruction in
/// pdf_import.rs (the runs no longer glue back into one bullet without a
/// spurious space, e.g. "offboarding" round-trips as "o ff boarding").
/// Expanding back to plain letters keeps the whole word on one font/one
/// text run when this render gets printed, so it round-trips cleanly.
/// `break_ligatures()` (called from `esc()`) is just this function —
/// see its own doc comment for why it doesn't do anything beyond this.
fn expand_ligature_chars(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\u{FB00}' => out.push_str("ff"),
            '\u{FB01}' => out.push_str("fi"),
            '\u{FB02}' => out.push_str("fl"),
            '\u{FB03}' => out.push_str("ffi"),
            '\u{FB04}' => out.push_str("ffl"),
            _ => out.push(c),
        }
    }
    out
}

/// Normalizes ligature *characters* back to plain letters before HTML
/// escaping (see `expand_ligature_chars`'s doc comment for why this needs
/// to happen at all). This is deliberately just that — it no longer also
/// tries to *prevent* Chromium's print-to-PDF path from re-fusing "f" +
/// "i"/"l"/"f" back into a ligature glyph while printing.
///
/// We used to insert a zero-width non-joiner (U+200C) between such letter
/// pairs for that purpose. It did stop the visible fusion, but at a cost
/// we didn't anticipate: forcing "f" and "i" to shape as separate glyph
/// clusters changes the advance width/kerning between them relative to a
/// fused ligature glyph, and that shifted spacing was large enough to
/// cross pdf_import.rs's same-row word-gap heuristic on re-import — so a
/// *plain, never-corrupted* word like "defined" would round-trip as
/// "def ined", with a real inserted space. That's a strictly worse outcome
/// than the ligature-fusion problem it was meant to prevent, and it hit
/// every word with an "fi"/"fl"/"ff" pair, not just previously-imported
/// ones.
///
/// The chosen fix is to allow the fusion and instead make it harmless:
/// `expand_ligature_chars` already normalizes any precomposed ligature
/// character back to plain letters on *every* render pass, including ones
/// fed by re-imported text. So if this render's output gets printed,
/// re-imported, and printed again, each cycle just re-normalizes and
/// (possibly) re-fuses — the underlying text stays correct, it's only the
/// glyph-level ligature-or-not presentation that varies, and that was
/// never something either this renderer or pdf_import.rs promised to
/// preserve.
fn break_ligatures(s: &str) -> String {
    expand_ligature_chars(s)
}

// Renders a bullet-list `<li>`. The `•` marker is a real, literal text
// character rather than the browser's default `list-style: disc` marker
// (which the CSS above suppresses via `list-style: none`). This matters
// beyond styling: when Chromium prints a page to PDF, native `<li>` markers
// are drawn as a small vector shape rather than an extractable glyph, so a
// PDF re-imported from our own "Download PDF" output would have no textual
// trace of where each bullet started — every bullet in a project/role
// silently merges into one run-on paragraph on re-import (see
// pdf_import::parse_experiences, which detects bullets via a leading "•").
// Emitting the bullet as ordinary text keeps round-tripping our own PDFs
// working.
fn bullet_li(text: &str) -> String {
    format!("<li>• {}</li>", render_inline(text))
}

/// Escapes `text` for HTML and applies inline `**bold**` markup, without the
/// paragraph/`<br>` splitting that `render_rich_text` does. Use this for any
/// single-line, user-authored field (bullets, one-line context/description
/// strings) where a highlighted keyword or phrase should render as
/// `<strong>`, but the field isn't free-form multi-paragraph prose.
fn render_inline(text: &str) -> String {
    apply_bold(&esc(text))
}

fn tag(href: &str, label: &str) -> String {
    format!(r#"<span class="tag">{}</span>"#, esc(label)).replace(
        "tag",
        if href.is_empty() {
            "tag"
        } else {
            "tag tag-link"
        },
    )
}

/// Renders a set of skill references (`skill_ids`, resolved via
/// `all_skills`) grouped by `SkillCategory`, the same
/// "Programming: A, B · Cloud & Infrastructure: C, D" format used for an
/// `Experience`'s own skills line — shared so a `ExperienceProject`'s tools
/// (also `skill_ids` now, not free text) render identically instead of as a
/// flat, uncategorized "Techs: A, B, C" chip row.
fn categorized_skill_line(
    wrapper_class: &str,
    skill_ids: &[String],
    all_skills: &[crate::models::Skill],
) -> String {
    let matched: Vec<&crate::models::Skill> = skill_ids
        .iter()
        .filter_map(|id| all_skills.iter().find(|s| &s.id == id))
        .collect();
    if matched.is_empty() {
        return String::new();
    }
    let categories = SkillCategory::all();
    let mut blocks: Vec<String> = Vec::new();
    for cat in &categories {
        let cat_skills: Vec<&&crate::models::Skill> =
            matched.iter().filter(|s| &s.category == cat).collect();
        if cat_skills.is_empty() {
            continue;
        }
        let names: Vec<String> = cat_skills.iter().map(|s| esc(&s.name)).collect();
        blocks.push(format!(
            r#"<span class="exp-skill-category">{}:</span> <span class="exp-skill-list">{}</span>"#,
            cat.label(),
            names.join(", "),
        ));
    }
    if blocks.is_empty() {
        String::new()
    } else {
        format!(
            r#"<div class="{}">{}</div>"#,
            wrapper_class,
            blocks.join(" &middot; ")
        )
    }
}

// ── Shared CSS ────────────────────────────────────────────────────────────────

const CV_CSS: &str = r#"
:root { color-scheme: light; }
.cv-doc, .cv-doc * { margin: 0; padding: 0; box-sizing: border-box; }
.cv-doc {
  font-family: 'Segoe UI', Helvetica, Arial, sans-serif;
  /* Best-effort attempt to stop Chromium's print-to-PDF path from fusing
     "fi"/"fl"/etc. into a single ligature glyph. In practice this alone
     hasn't been reliable (verified by round-tripping our own output —
     the ligature still formed regardless). We no longer try to actively
     prevent the fusion elsewhere either (see break_ligatures() in esc()
     — it only *normalizes* an already-fused ligature character back to
     plain letters, it doesn't try to stop a fresh fusion from happening
     during this print pass): a previous attempt to force letters apart
     with a zero-width non-joiner stopped the fusion but perturbed glyph
     spacing enough to trip pdf_import.rs's word-gap heuristic on
     re-import, corrupting plain text that was never even a ligature.
     Whether this word ends up as one fused ligature glyph or separate
     letters in the printed PDF is harmless either way — esc() normalizes
     it back to plain letters on every render, so it can't cascade into
     corruption on a later re-import regardless of which one Chromium
     picks. These CSS rules are kept only as a low-cost, no-downside
     nicety for any text that reaches the page without going through
     esc(). */
  font-variant-ligatures: none;
  -webkit-font-variant-ligatures: none;
  font-feature-settings: "liga" 0, "clig" 0, "dlig" 0;
  color: #1a1a2e;
  max-width: 860px;
  margin: 0 auto;
  padding: 48px 40px;
  font-size: 14px;
  line-height: 1.5;
  background: #fff;
}
.cv-doc .toolbar {
  display: flex;
  gap: 10px;
  margin-bottom: 24px;
  padding-bottom: 16px;
  border-bottom: 1px solid #e8eaf0;
}
.cv-doc .btn {
  padding: 8px 18px;
  border-radius: 6px;
  border: none;
  cursor: pointer;
  font-size: 13px;
  font-weight: 600;
}
.cv-doc .btn-primary { background: #2563eb; color: #fff; }
.cv-doc .btn-secondary { background: #f1f5f9; color: #334155; border: 1px solid #e2e8f0; }

/* ── Header ── */
.cv-doc .header { margin-bottom: 28px; }
.cv-doc .name { font-size: 2rem; font-weight: 700; color: #0f172a; letter-spacing: -0.5px; }
.cv-doc .job-title { font-size: 1.05rem; color: #475569; margin-top: 4px; }
.cv-doc .contact {
  display: flex;
  flex-wrap: wrap;
  gap: 6px 20px;
  margin-top: 10px;
  font-size: 0.8rem;
  color: #64748b;
}
.cv-doc .contact a { color: #2563eb; text-decoration: none; }
.cv-doc .contact-item { display: inline-flex; align-items: center; gap: 4px; white-space: nowrap; }
.cv-doc .summary {
  margin-top: 14px;
  color: #334155;
  font-size: 0.9rem;
  line-height: 1.7;
  max-width: 700px;
}

/* ── Section ── */
.cv-doc .section { margin-bottom: 26px; }
.cv-doc .section-title {
  font-size: 0.68rem;
  font-weight: 700;
  text-transform: uppercase;
  letter-spacing: 0.12em;
  color: #2563eb;
  border-bottom: 1.5px solid #e2e8f0;
  padding-bottom: 5px;
  margin-bottom: 14px;
}

/* ── Experience ── */
.cv-doc .exp-item { margin-bottom: 18px; }
.cv-doc .exp-header { display: flex; justify-content: space-between; align-items: baseline; flex-wrap: wrap; gap: 4px; }
.cv-doc .exp-company { font-weight: 700; color: #0f172a; }
.cv-doc .exp-role { font-style: italic; color: #475569; font-size: 0.9rem; margin-top: 1px; }
.cv-doc .exp-dates { font-size: 0.8rem; color: #94a3b8; white-space: nowrap; }
.cv-doc .exp-location { font-size: 0.8rem; color: #94a3b8; }
.cv-doc .exp-bullets { margin: 8px 0 0 18px; list-style: none; padding: 0; }
.cv-doc .exp-bullets li { color: #334155; margin-bottom: 3px; font-size: 0.88rem; padding-left: 1em; text-indent: -1em; }
.cv-doc .exp-tools { margin-top: 7px; display: flex; flex-wrap: wrap; gap: 5px; }
.cv-doc .exp-project-skills { margin-top: 6px; font-size: 0.78rem; color: #64748b; line-height: 1.45; }
.cv-doc .exp-skill-category { font-weight: 600; color: #1e293b; }
.cv-doc .exp-skill-list { color: #475569; }
.cv-doc .exp-project { margin-top: 8px; padding-left: 12px; border-left: 2px solid #e2e8f0; }
.cv-doc .exp-project-header { display: flex; justify-content: space-between; align-items: baseline; flex-wrap: wrap; gap: 4px; }
.cv-doc .exp-project-name { font-weight: 600; color: #1e293b; font-size: 0.88rem; margin-bottom: 2px; }
.cv-doc .exp-project-dates { font-size: 0.78rem; color: #94a3b8; white-space: nowrap; }
.cv-doc .exp-project-context { margin: 0 0 4px 18px; list-style: none; padding: 0; }
.cv-doc .exp-project-context li { font-style: italic; color: #64748b; font-size: 0.82rem; margin-bottom: 2px; padding-left: 1em; text-indent: -1em; }
.cv-doc .exp-project-label { font-size: 0.78rem; font-weight: 700; color: #94a3b8; text-transform: uppercase; letter-spacing: 0.04em; display: block; margin: 6px 0 2px 0; }

/* ── Skills ── */
.cv-doc .skills-block { margin-bottom: 8px; }
.cv-doc .skills-category { font-weight: 600; font-size: 0.82rem; color: #475569; display: inline; }
.cv-doc .skills-list { display: inline; font-size: 0.88rem; color: #334155; }

/* ── Projects ── */
.cv-doc .proj-item { margin-bottom: 16px; }
.cv-doc .proj-header { display: flex; justify-content: space-between; align-items: baseline; }
.cv-doc .proj-name { font-weight: 700; color: #0f172a; }
.cv-doc .proj-url { font-size: 0.8rem; color: #2563eb; text-decoration: none; }
.cv-doc .proj-desc { font-size: 0.88rem; color: #475569; margin-top: 3px; }
.cv-doc .proj-bullets { margin: 6px 0 0 18px; list-style: none; padding: 0; }
.cv-doc .proj-bullets li { color: #334155; margin-bottom: 3px; font-size: 0.88rem; padding-left: 1em; text-indent: -1em; }
.cv-doc .proj-tools { margin-top: 6px; display: flex; flex-wrap: wrap; gap: 5px; }

/* ── Education ── */
.cv-doc .edu-item { margin-bottom: 12px; }
.cv-doc .edu-header { display: flex; justify-content: space-between; align-items: baseline; flex-wrap: wrap; }
.cv-doc .edu-inst { font-weight: 700; color: #0f172a; }
.cv-doc .edu-degree { color: #475569; font-size: 0.88rem; margin-top: 2px; }
.cv-doc .edu-dates { font-size: 0.8rem; color: #94a3b8; }
.cv-doc .edu-achievements { margin: 6px 0 0 18px; list-style: none; padding: 0; }
.cv-doc .edu-achievements li { color: #334155; font-size: 0.88rem; margin-bottom: 2px; padding-left: 1em; text-indent: -1em; }

/* ── Languages & Certs ── */
.cv-doc .lang-list { display: flex; flex-wrap: wrap; gap: 12px; }
.cv-doc .lang-item { font-size: 0.88rem; }
.cv-doc .lang-name { font-weight: 600; color: #0f172a; }
.cv-doc .lang-level { color: #64748b; font-size: 0.8rem; }

/* ── Tags ── */
.cv-doc .tag {
  background: #eff6ff;
  color: #2563eb;
  padding: 2px 8px;
  border-radius: 10px;
  font-size: 0.78rem;
  font-weight: 500;
  white-space: nowrap;
}

/* ── Gap analysis (tailored only) ── */
.cv-doc .gap-banner {
  background: #fafafa;
  border: 1px solid #e2e8f0;
  border-radius: 8px;
  padding: 14px 18px;
  margin-bottom: 24px;
  font-size: 0.85rem;
}
.cv-doc .gap-score { font-weight: 700; font-size: 1.1rem; color: #2563eb; }
.cv-doc .gap-section { margin-top: 8px; }
.cv-doc .gap-label { font-weight: 600; color: #475569; margin-bottom: 4px; }
.cv-doc .kw-matched { color: #16a34a; background: #f0fdf4; }
.cv-doc .kw-missing { color: #dc2626; background: #fef2f2; }

@media print {
  .cv-doc .toolbar { display: none; }
  /* The gap-analysis banner (match score + matched/missing keyword tags) is
     an on-screen editing aid for the candidate, not part of the CV itself —
     it must never appear on the document a recruiter actually receives.
     Screen still shows it (see .gap-banner above); print/PDF output hides
     it entirely rather than just visually de-emphasizing it, since even a
     faint "Match score: 57%" printed above your name would look wrong on
     a document going out to an employer. */
  .cv-doc .gap-banner { display: none; }
  .cv-doc { padding: 20px; }
  @page { margin: 1.5cm; }
  /* Browsers strip background colors during print by default to save ink;
     without this, section-title borders/tag backgrounds etc. would print
     as plain black-and-white instead of matching the on-screen preview. */
  .cv-doc, .cv-doc * {
    -webkit-print-color-adjust: exact;
    print-color-adjust: exact;
    color-adjust: exact;
  }
}

/* ── Keep items intact across page breaks (print & PDF) ──
   NOTE: .section-head (title + first item, see wrap_section) and
   .exp-item are deliberately NOT in this list, even though .exp-item
   used to be. The same failure mode applies to both: an Experience
   entry can carry several nested sub-projects and easily grow taller
   than a full page. If a block that large is marked break-inside:avoid,
   it can't fit on the remaining space of the current page *or* on a
   fresh one — so the browser pushes the whole thing to the next page
   and lets it overflow there, leaving the entire remainder of the
   current page blank (confirmed against real Chromium print-to-PDF
   output, not just this project's own CSS reasoning).
   Instead, only the two *small, bounded* trouble spots get explicit
   protection below: the section title shouldn't be stranded above an
   empty rest-of-page, and an exp-item's company/role header shouldn't
   be stranded above its own first project. Both use the same
   break-after (on the heading) + break-before (on what follows)
   pairing rather than break-inside:avoid on a wrapper, so a break can
   still land further inside a large block without relocating the whole
   thing. */
.cv-doc .header,
.cv-doc .proj-item,
.cv-doc .edu-item,
.cv-doc .skills-block,
.cv-doc .gap-banner,
.cv-doc .gap-section {
  break-inside: avoid;
  page-break-inside: avoid;
}
.cv-doc .section-title,
.cv-doc .exp-role {
  break-after: avoid;
  page-break-after: avoid;
}
.cv-doc .section-title + *,
.cv-doc .exp-project:first-of-type {
  break-before: avoid;
  page-break-before: avoid;
}
"#;

// ── Shared HTML scaffolding ───────────────────────────────────────────────────

fn html_wrap(title: &str, body: &str) -> String {
    format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8"/>
  <meta name="viewport" content="width=device-width,initial-scale=1"/>
  <meta name="color-scheme" content="light"/>
  <title>{title}</title>
  <style>{css}</style>
</head>
<body>
<div class="cv-doc">
{body}
</div>
</body>
</html>"#,
        title = esc(title),
        css = CV_CSS,
        body = body,
    )
}

// ── Section builders ──────────────────────────────────────────────────────────

// ── Rich text renderer ───────────────────────────────────────────────────────
// Converts plain text (as pasted from LinkedIn) into HTML:
//   - Blank lines → paragraph breaks
//   - Single newlines → <br>
//   - **text** → <strong>text</strong>
//   - HTML special chars are escaped first
fn render_rich_text(text: &str) -> String {
    // First escape HTML
    let escaped = esc(text);
    // Convert **bold** markers
    let bolded = apply_bold(&escaped);
    // Split into paragraphs on blank lines, then preserve single newlines
    bolded
        .split("\n\n")
        .map(|para| {
            let lines = para
                .split('\n')
                .map(|l| l.trim())
                .collect::<Vec<_>>()
                .join("<br>");
            format!("<p>{lines}</p>")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn apply_bold(text: &str) -> String {
    let mut result = String::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '*' && chars.peek() == Some(&'*') {
            chars.next(); // consume second *
                          // Collect until next **
            let mut inner = String::new();
            let mut closed = false;
            loop {
                match chars.next() {
                    Some('*') if chars.peek() == Some(&'*') => {
                        chars.next();
                        closed = true;
                        break;
                    }
                    Some(ch) => inner.push(ch),
                    None => break,
                }
            }
            if closed {
                result.push_str(&format!("<strong>{inner}</strong>"));
            } else {
                // Unterminated marker: leave the original text (including
                // the "**") exactly as written rather than also wrapping
                // it in <strong> — the two used to run unconditionally
                // one after the other, duplicating `inner` in both its
                // literal and bolded form (e.g. "done **soon" rendered as
                // "done **soon<strong>soon</strong>").
                result.push_str("**");
                result.push_str(&inner);
            }
        } else {
            result.push(c);
        }
    }
    result
}

// Wraps a section title together with its first item/block in a single
// "section-head" container so the pagination logic (native browser
// print-to-PDF's break-inside/break-after CSS, see the @media print block in
// CV_CSS above) treats them as one atomic unit. This is what stops a heading
// from being stranded alone at the bottom of a page while its content starts
// on the next one.
fn wrap_section(title: &str, mut items: Vec<String>) -> String {
    if items.is_empty() {
        return String::new();
    }
    let first = items.remove(0);
    let rest: String = items.join("");
    format!(
        r#"<div class="section"><div class="section-head"><div class="section-title">{title}</div>{first}</div>{rest}</div>"#,
        title = title,
        first = first,
        rest = rest,
    )
}

fn render_header(p: &crate::models::PersonalInfo, lang: Lang) -> String {
    let mut contacts = vec![];
    if !p.email.is_empty() {
        contacts.push(format!(
            r#"<span class="contact-item">✉️ <a href="mailto:{}">{}</a></span>"#,
            esc(&p.email),
            esc(&p.email)
        ));
    }
    if !p.phone.is_empty() {
        contacts.push(format!(
            r#"<span class="contact-item">📞 {}</span>"#,
            esc(&p.phone)
        ));
    }
    if !p.location.is_empty() {
        contacts.push(format!(
            r#"<span class="contact-item">📍 {}</span>"#,
            esc(&p.location)
        ));
    }
    if !p.linkedin.is_empty() {
        // Render the URL itself as the link text (not a generic "LinkedIn"
        // label). A PDF's visible text is the only thing our own importer
        // can recover on re-import (see pdf_import::extract_urls, which
        // scans visible text for "linkedin.com" / "github.com" substrings —
        // it does not read PDF link annotations). A static label would
        // silently lose this field every time our own PDF is re-imported.
        contacts.push(format!(
            r#"<span class="contact-item">🔗 <a href="{}">{}</a></span>"#,
            esc(&p.linkedin),
            esc(&p.linkedin)
        ));
    }
    if !p.github.is_empty() {
        // See comment above on the LinkedIn link: keep the URL as the
        // visible text so round-tripping through our own PDF export/import
        // preserves the field.
        contacts.push(format!(
            r#"<span class="contact-item">💻 <a href="{}">{}</a></span>"#,
            esc(&p.github),
            esc(&p.github)
        ));
    }
    if !p.website.is_empty() {
        contacts.push(format!(
            r#"<span class="contact-item">🌐 <a href="{}">{}</a></span>"#,
            esc(&p.website),
            esc(&p.website)
        ));
    }

    let contact_html = if contacts.is_empty() {
        String::new()
    } else {
        format!(r#"<div class="contact">{}</div>"#, contacts.join(""))
    };

    let summary_text = p.summary.get(lang);
    let summary_html = if summary_text.is_empty() {
        String::new()
    } else {
        format!(
            r#"<div class="summary">{}</div>"#,
            render_rich_text(summary_text)
        )
    };

    format!(
        r#"<div class="header">
  <div class="name">{name}</div>
  <div class="job-title">{title}</div>
  {contact}
  {summary}
</div>"#,
        name = esc(&p.name),
        title = esc(p.title.get(lang)),
        contact = contact_html,
        summary = summary_html,
    )
}

fn render_experience(
    experiences: &[crate::models::Experience],
    all_skills: &[crate::models::Skill],
    lang: Lang,
) -> String {
    if experiences.is_empty() {
        return String::new();
    }
    let mut items: Vec<String> = Vec::new();
    for exp in experiences {
        let location_html = if exp.location.is_empty() {
            String::new()
        } else {
            format!(
                r#" · <span class="exp-location">{}</span>"#,
                esc(&exp.location)
            )
        };
        let mut projects_html = String::new();
        for proj in &exp.projects {
            let bullets: String = proj
                .bullets
                .iter()
                .map(|b| b.get(lang))
                .filter(|b| !b.is_empty())
                .map(bullet_li)
                .collect();
            let tools_div =
                categorized_skill_line("exp-project-skills", &proj.skill_ids, all_skills);
            let project_dates_html = if proj.start_date.is_empty() && proj.end_date.is_empty() {
                String::new()
            } else {
                format!(
                    r#"<span class="exp-project-dates">{} – {}</span>"#,
                    esc(&proj.start_date),
                    esc(&proj.end_date)
                )
            };
            let name_html = if proj.name.get(lang).is_empty() && project_dates_html.is_empty() {
                String::new()
            } else {
                format!(
                    r#"<div class="exp-project-header"><span class="exp-project-name">{}</span>{}</div>"#,
                    esc(proj.name.get(lang)),
                    project_dates_html
                )
            };
            let context_items: String = proj
                .context
                .iter()
                .map(|c| c.get(lang))
                .filter(|c| !c.is_empty())
                .map(bullet_li)
                .collect();
            let context_html = if context_items.is_empty() {
                String::new()
            } else {
                format!(
                    r#"<span class="exp-project-label">{}</span><ul class="exp-project-context">{}</ul>"#,
                    if lang == Lang::Fr {
                        "Contexte :"
                    } else {
                        "Context :"
                    },
                    context_items
                )
            };
            let bullets_block = if bullets.is_empty() {
                String::new()
            } else {
                format!(
                    r#"<span class="exp-project-label">Actions &amp; Impact:</span><ul class="exp-bullets">{}</ul>"#,
                    bullets
                )
            };
            projects_html.push_str(&format!(
                r#"<div class="exp-project">{name}{context}{bullets}{tools}</div>"#,
                name = name_html,
                context = context_html,
                bullets = bullets_block,
                tools = tools_div,
            ));
        }
        items.push(format!(
            r#"<div class="exp-item">
  <div class="exp-header">
    <span class="exp-company">{company}{location}</span>
    <span class="exp-dates">{start} – {end}</span>
  </div>
  <div class="exp-role">{role}</div>
  {projects}
</div>"#,
            company = esc(&exp.company),
            location = location_html,
            start = esc(&exp.start_date),
            end = esc(&exp.end_date),
            role = esc(exp.role.get(lang)),
            projects = projects_html,
        ));
    }
    wrap_section(i18n_core::tr("rs_experience", lang), items)
}

fn render_skills(skills: &[crate::models::Skill], lang: Lang) -> String {
    if skills.is_empty() {
        return String::new();
    }

    // Group by category
    let categories = SkillCategory::all();
    let mut blocks: Vec<String> = Vec::new();
    for cat in &categories {
        let cat_skills: Vec<&crate::models::Skill> =
            skills.iter().filter(|s| &s.category == cat).collect();
        if cat_skills.is_empty() {
            continue;
        }
        let names: Vec<String> = cat_skills.iter().map(|s| esc(&s.name)).collect();
        blocks.push(format!(
            r#"<div class="skills-block">
  <span class="skills-category">{cat}: </span>
  <span class="skills-list">{list}</span>
</div>"#,
            cat = cat.label(),
            list = names.join(", "),
        ));
    }
    wrap_section(i18n_core::tr("rs_skills", lang), blocks)
}

fn render_projects(projects: &[crate::models::Project], lang: Lang) -> String {
    if projects.is_empty() {
        return String::new();
    }
    let mut items: Vec<String> = Vec::new();
    for proj in projects {
        let bullets: String = proj
            .bullets
            .iter()
            .map(|b| b.get(lang))
            .filter(|b| !b.is_empty())
            .map(bullet_li)
            .collect();
        let bullets_html = if bullets.is_empty() {
            String::new()
        } else {
            format!(r#"<ul class="proj-bullets">{}</ul>"#, bullets)
        };
        // NOTE: intentionally NOT using the categorized "Techs:"-style
        // rendering used for an ExperienceProject's tools here. This is the
        // standalone top-level "Projects" section (crate::models::Project),
        // parsed on import by parse_projects() — a different, simpler
        // parser than the one used for a Project nested inside an
        // Experience. parse_projects() has no concept of a "Techs:" tools
        // label at all, and worse, treats *any* "Label: text" line as the
        // start of a brand-new project — so emitting that label here would
        // make re-import spawn a bogus extra project and silently drop
        // whatever project these tools actually belonged to. Left as the
        // original plain chip row (round-trips no better than before, but
        // doesn't regress either) until parse_projects() gets equivalent
        // handling.
        let tools_html: String = proj
            .tools
            .iter()
            .filter(|t| !t.is_empty())
            .map(|t| tag("", t))
            .collect::<Vec<_>>()
            .join(" ");
        let tools_div = if tools_html.is_empty() {
            String::new()
        } else {
            format!(r#"<div class="proj-tools">{}</div>"#, tools_html)
        };
        let url_html = if proj.url.is_empty() {
            String::new()
        } else {
            format!(
                r#"<a href="{}" class="proj-url">{}</a>"#,
                esc(&proj.url),
                esc(&proj.url)
            )
        };
        items.push(format!(
            r#"<div class="proj-item">
  <div class="proj-header">
    <span class="proj-name">{name}</span>
    {url}
  </div>
  <div class="proj-desc">{desc}</div>
  {bullets}
  {tools}
</div>"#,
            name = esc(&proj.name),
            url = url_html,
            desc = render_inline(proj.description.get(lang)),
            bullets = bullets_html,
            tools = tools_div,
        ));
    }
    wrap_section(i18n_core::tr("rs_projects", lang), items)
}

fn render_education(education: &[crate::models::Education], lang: Lang) -> String {
    if education.is_empty() {
        return String::new();
    }
    let mut items: Vec<String> = Vec::new();
    for edu in education {
        let achievements: String = edu
            .achievements
            .iter()
            .map(|a| a.get(lang))
            .filter(|a| !a.is_empty())
            .map(bullet_li)
            .collect();
        let ach_html = if achievements.is_empty() {
            String::new()
        } else {
            format!(r#"<ul class="edu-achievements">{}</ul>"#, achievements)
        };
        // Only show the dates span when there's actually a start and/or
        // end year, and only join degree/field with " · " when a field is
        // actually present — unconditionally rendering "{start} – {end}"
        // and "{degree} · {field}" (the same failure mode already fixed in
        // render_certifications) left a dangling "–" or "·" with nothing
        // on one side whenever a source resume doesn't give dates, or a
        // degree has no separately-tracked field of study (both common:
        // this app's own pdf_import doesn't always split a field out from
        // the institution line, and plenty of resumes list education with
        // no dates at all). That dangling punctuation isn't just visually
        // wrong — re-importing our own PDF then had to make sense of a
        // trailing "–"/"·" with nothing following it, which confused the
        // entry-boundary detection and merged entries that should have
        // stayed separate, compounding by one more dangling separator on
        // every subsequent round trip.
        let start = esc(&edu.start_year);
        let end = esc(&edu.end_year);
        let dates_html = match (start.is_empty(), end.is_empty()) {
            (true, true) => String::new(),
            (false, true) => format!(r#"<span class="edu-dates">{start}</span>"#),
            (true, false) => format!(r#"<span class="edu-dates">{end}</span>"#),
            (false, false) => format!(r#"<span class="edu-dates">{start} – {end}</span>"#),
        };
        let degree = esc(edu.degree.get(lang));
        let field = esc(edu.field.get(lang));
        let degree_line = if field.is_empty() {
            degree
        } else {
            format!("{degree} · {field}")
        };
        items.push(format!(
            r#"<div class="edu-item">
  <div class="edu-header">
    <span class="edu-inst">{inst}</span>
    {dates_html}
  </div>
  <div class="edu-degree">{degree_line}</div>
  {achievements}
</div>"#,
            inst = esc(&edu.institution),
            achievements = ach_html,
        ));
    }
    wrap_section(i18n_core::tr("rs_education", lang), items)
}

fn render_languages(languages: &[crate::models::Language], lang: Lang) -> String {
    if languages.is_empty() {
        return String::new();
    }
    let items: String = languages.iter().map(|l| {
        format!(
            r#"<div class="lang-item"><span class="lang-name">{name}</span> <span class="lang-level">({level})</span></div>"#,
            name  = esc(&l.name),
            level = if lang == Lang::Fr { l.level.label_fr() } else { l.level.label() },
        )
    }).collect();
    wrap_section(
        i18n_core::tr("rs_languages", lang),
        vec![format!(r#"<div class="lang-list">{}</div>"#, items)],
    )
}

fn render_certifications(certs: &[crate::models::Certification], lang: Lang) -> String {
    if certs.is_empty() {
        return String::new();
    }
    let items: String =
        certs
            .iter()
            .map(|c| {
                let name = if c.url.is_empty() {
                    esc(&c.name)
                } else {
                    format!(
                        r#"<a href="{}" class="proj-url">{}</a>"#,
                        esc(&c.url),
                        esc(&c.name)
                    )
                };
                // Only show the "· issuer, date" suffix when there's actually an
                // issuer and/or date to show — unconditionally appending it
                // (previously: always "· {issuer}, {date}") left a dangling
                // "· ," on any certification with both fields empty. That's
                // especially easy to hit after a round trip through this app's
                // own PDF: on import, issuer/date aren't always split out from
                // the name (see pdf_import::build_certification_from_buffer), so
                // they're legitimately blank, and re-exporting used to bolt on
                // an empty "· ," suffix every time — compounding by one more
                // "· ," on every subsequent import/export cycle.
                let issuer = esc(&c.issuer);
                let date = esc(&c.date);
                let suffix = match (issuer.is_empty(), date.is_empty()) {
                    (true, true) => String::new(),
                    (false, true) => format!(" <span class=\"lang-level\">· {issuer}</span>"),
                    (true, false) => format!(" <span class=\"lang-level\">· {date}</span>"),
                    (false, false) => {
                        format!(" <span class=\"lang-level\">· {issuer}, {date}</span>")
                    }
                };
                format!(
                    r#"<div class="lang-item"><span class="lang-name">{name}</span>{suffix}</div>"#,
                )
            })
            .collect();
    wrap_section(
        i18n_core::tr("rs_certifications", lang),
        vec![format!(r#"<div class="lang-list">{}</div>"#, items)],
    )
}

// ── Public renderers ──────────────────────────────────────────────────────────

/// Render the complete lifetime CV — every item, nothing filtered.
pub fn render_lifetime_cv(cv: &LifetimeCV, lang: Lang) -> String {
    let body = format!(
        "{header}{skills}{exp}{projects}{edu}{lang}{certs}",
        header = render_header(&cv.personal, lang),
        skills = render_skills(&cv.skills, lang),
        exp = render_experience(&cv.experiences, &cv.skills, lang),
        projects = render_projects(&cv.projects, lang),
        edu = render_education(&cv.education, lang),
        lang = render_languages(&cv.languages, lang),
        certs = render_certifications(&cv.certifications, lang),
    );
    let title = if cv.personal.name.is_empty() {
        "My CV".to_string()
    } else {
        format!("{} — CV", cv.personal.name)
    };
    html_wrap(&title, &body)
}

/// Render a tailored CV with a gap-analysis banner at the top.
// keep pub(crate) score_color visible to tests
pub(crate) fn score_color_for(score: f32) -> &'static str {
    if score >= 0.6 {
        "#16a34a"
    } else if score >= 0.3 {
        "#d97706"
    } else {
        "#dc2626"
    }
}

/// Render a tailored CV with a gap-analysis banner at the top.
pub fn render_tailored_cv(cv: &TailoredCV, job_title: &str, lang: Lang) -> String {
    let score_pct = (cv.match_score * 100.0).round() as u32;
    let score_color = score_color_for(cv.match_score);

    let matched_tags: String = cv
        .matched_keywords
        .iter()
        .map(|k| format!(r#"<span class="tag kw-matched">{}</span>"#, esc(k)))
        .collect::<Vec<_>>()
        .join(" ");

    let missing_tags: String = cv
        .missing_keywords
        .iter()
        .take(15)
        .map(|k| format!(r#"<span class="tag kw-missing">{}</span>"#, esc(k)))
        .collect::<Vec<_>>()
        .join(" ");

    let gap_banner = format!(
        r#"<div class="gap-banner">
  <div>Match score: <span class="gap-score" style="color:{color}">{score}%</span>
  {for_role}</div>
  {matched}
  {missing}
</div>"#,
        color = score_color,
        score = score_pct,
        for_role = if job_title.is_empty() {
            String::new()
        } else {
            format!(" · <strong>{}</strong>", esc(job_title))
        },
        matched = if matched_tags.is_empty() {
            String::new()
        } else {
            format!(
                r#"<div class="gap-section"><div class="gap-label">✓ Matched keywords</div>{}</div>"#,
                matched_tags
            )
        },
        missing = if missing_tags.is_empty() {
            String::new()
        } else {
            format!(
                r#"<div class="gap-section"><div class="gap-label">✗ Gap keywords — consider adding these to your CV</div>{}</div>"#,
                missing_tags
            )
        },
    );

    let body = format!(
        "{banner}{header}{skills}{exp}{projects}{edu}{lang}{certs}",
        banner = gap_banner,
        header = render_header(&cv.personal, lang),
        skills = render_skills(&cv.skills, lang),
        exp = render_experience(&cv.experiences, &cv.skills, lang),
        projects = render_projects(&cv.projects, lang),
        edu = render_education(&cv.education, lang),
        lang = render_languages(&cv.languages, lang),
        certs = render_certifications(&cv.certifications, lang),
    );

    let title = if cv.personal.name.is_empty() {
        "Tailored CV".to_string()
    } else {
        format!("{} — Tailored CV", cv.personal.name)
    };
    html_wrap(&title, &body)
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::*;

    // ── Ligature handling ────────────────────────────────────────────────────
    // Regression coverage for the idempotence bug where a precomposed
    // ligature character (e.g. "ﬀ" U+FB00, decoded verbatim from a source
    // PDF's ToUnicode CMap by pdf_import.rs) reached the browser untouched,
    // forcing a mid-word font fallback that fragmented the word across
    // separate PDF text objects — which on re-import glued back together
    // with a spurious space ("offboarding" → "o ff boarding").

    #[test]
    fn expand_ligature_chars_restores_plain_letters() {
        assert_eq!(expand_ligature_chars("o\u{FB00}boarding"), "offboarding");
        assert_eq!(expand_ligature_chars("\u{FB01}le"), "file");
        assert_eq!(expand_ligature_chars("\u{FB02}oor"), "floor");
        assert_eq!(expand_ligature_chars("o\u{FB03}ce"), "office");
        assert_eq!(expand_ligature_chars("ru\u{FB04}e"), "ruffle");
        assert_eq!(expand_ligature_chars("plain text"), "plain text");
    }

    #[test]
    fn break_ligatures_expands_precomposed_ligature_char_with_no_extra_chars() {
        // A precomposed "ﬀ" must end up as plain "f","f" — not pass through
        // as the single ligature codepoint, and critically: no ZWNJ, no
        // space, nothing inserted between the two letters. (We used to
        // insert a ZWNJ here; that's the thing that caused a *new*
        // round-trip regression — see this function's doc comment — so
        // this test pins down that the output is exactly the plain
        // letters and nothing more.)
        let out = break_ligatures("o\u{FB00}boarding");
        assert_eq!(out, "offboarding");
        assert!(!out.contains(' '));
        assert!(!out.contains('\u{200C}'));
        assert!(!out.contains('\u{FB00}'));
    }

    #[test]
    fn esc_normalizes_precomposed_ligature_in_bullet_text() {
        // End-to-end through esc(): the exact scenario from the reported
        // round-trip bug ("Automated offboarding pipelines...").
        let html = esc("Automated o\u{FB00}boarding pipelines");
        assert_eq!(html, "Automated offboarding pipelines");
        assert!(html.contains("offboarding"));
        assert!(!html.contains('\u{200C}'));
    }

    #[test]
    fn esc_does_not_insert_zwnj_for_plain_never_imported_text() {
        // Regression test for the specific new bug: plain text that was
        // never a ligature at all (typed directly, e.g. from a freshly
        // imported source resume) must render completely unmodified aside
        // from HTML-escaping — no ZWNJ, no altered spacing, so it can't
        // trip pdf_import.rs's word-gap heuristic on a later re-import.
        for word in [
            "defined",
            "configuration",
            "fixes",
            "workflows",
            "offboarding",
        ] {
            let html = esc(word);
            assert_eq!(html, word, "esc() must not alter plain text {word:?}");
            assert!(!html.contains('\u{200C}'));
        }
    }

    // ── Categorized skill/tools line ─────────────────────────────────────────
    // `categorized_skill_line` is shared by both an Experience's own skills
    // line and an ExperienceProject's tools line — both are `skill_ids` now
    // (see ExperienceProject's doc comment in models/cv.rs), so both render
    // in the same "Category: a, b · Category2: c, d" format instead of a
    // flat, uncategorized chip row.
    //
    // NOTE: this intentionally does NOT preserve the old flat-chip
    // rendering's re-import round-trip property (comma-joined chips under a
    // recognizable "Techs:" label that pdf_import.rs's flush_project could
    // parse back via a simple split(',')). That parser has not been updated
    // to understand this categorized, multi-segment format — re-importing a
    // PDF exported after this change will not recover a project's tool tags
    // automatically; they'd need retagging via the editor's skill picker.
    // This was a deliberate trade accepted for now in favor of visual
    // consistency with the experience-level line, not an oversight.

    #[test]
    fn categorized_skill_line_empty_ids_renders_nothing() {
        let out = categorized_skill_line("exp-project-skills", &[], &[]);
        assert_eq!(out, "");
    }

    #[test]
    fn categorized_skill_line_groups_by_category_in_category_order() {
        let skills = vec![
            crate::models::Skill {
                id: "s1".to_string(),
                name: "Ansible".to_string(),
                category: SkillCategory::AutomationDevOps,
                ..Default::default()
            },
            crate::models::Skill {
                id: "s2".to_string(),
                name: "Kubernetes".to_string(),
                category: SkillCategory::CloudInfrastructure,
                ..Default::default()
            },
            crate::models::Skill {
                id: "s3".to_string(),
                name: "AWX".to_string(),
                category: SkillCategory::AutomationDevOps,
                ..Default::default()
            },
        ];
        let ids = vec!["s1".to_string(), "s2".to_string(), "s3".to_string()];
        let out = categorized_skill_line("exp-project-skills", &ids, &skills);
        // Same category's skills (Ansible, AWX) grouped into one segment,
        // not two separate "DevOps: Ansible" / "DevOps: AWX" segments.
        assert!(out.contains("Ansible, AWX") || out.contains("AWX, Ansible"));
        assert!(out.contains("Kubernetes"));
        assert!(out.contains(r#"<div class="exp-project-skills">"#));
        assert!(out.contains("exp-skill-category"));
        assert!(out.contains("exp-skill-list"));
        // Categories joined with a middle dot, matching the experience-level line.
        assert!(out.contains(" &middot; "));
    }

    #[test]
    fn categorized_skill_line_unmatched_ids_are_dropped() {
        // An id with no corresponding Skill (e.g. a stale reference) should
        // be silently skipped, not panic or render an empty/garbled entry.
        let skills = vec![crate::models::Skill {
            id: "s1".to_string(),
            name: "Terraform".to_string(),
            category: SkillCategory::AutomationDevOps,
            ..Default::default()
        }];
        let ids = vec!["s1".to_string(), "does-not-exist".to_string()];
        let out = categorized_skill_line("exp-project-skills", &ids, &skills);
        assert!(out.contains("Terraform"));
    }

    fn minimal_cv() -> LifetimeCV {
        LifetimeCV {
            personal: PersonalInfo {
                name: "Jane Smith".to_string(),
                email: "jane@example.com".to_string(),
                title: LocalizedText::same("Rust Engineer"),
                location: "Paris, France".to_string(),
                ..Default::default()
            },
            ..Default::default()
        }
    }

    fn full_cv() -> LifetimeCV {
        let mut cv = minimal_cv();
        cv.experiences.push(Experience {
            id: "e1".to_string(),
            company: "Acme Corp".to_string(),
            role: LocalizedText::same("Software Engineer"),
            start_date: "Jan 2021".to_string(),
            end_date: "Present".to_string(),
            projects: vec![ExperienceProject {
                name: LocalizedText::same("API Platform"),
                context: vec![LocalizedText::same("Legacy monolith needed decomposition")],
                bullets: vec![LocalizedText::same("Built distributed systems")],
                skill_ids: vec!["s1".to_string(), "s2".to_string()],
                ..Default::default()
            }],
            ..Default::default()
        });
        cv.skills.push(Skill {
            id: "s1".to_string(),
            name: "Rust".to_string(),
            category: SkillCategory::Programming,
            level: SkillLevel::Expert,
        });
        cv.skills.push(Skill {
            id: "s2".to_string(),
            name: "PostgreSQL".to_string(),
            category: SkillCategory::Database,
            level: SkillLevel::Advanced,
        });
        cv.education.push(Education {
            id: "edu1".to_string(),
            institution: "MIT".to_string(),
            degree: LocalizedText::same("MSc"),
            field: LocalizedText::same("Computer Science"),
            start_year: "2017".to_string(),
            end_year: "2019".to_string(),
            achievements: vec![],
        });
        cv.projects.push(Project {
            id: "p1".to_string(),
            name: "CV Generator".to_string(),
            description: LocalizedText::same("A cool Rust project"),
            url: "https://github.com/me/cv-gen".to_string(),
            tools: vec!["Rust".to_string()],
            bullets: vec![LocalizedText::same("Implemented keyword matching")],
        });
        cv
    }

    // ── HTML structure ────────────────────────────────────────────────────────

    #[test]
    fn empty_cv_does_not_panic() {
        let html = render_lifetime_cv(&LifetimeCV::default(), Lang::En);
        assert!(html.contains("<!DOCTYPE html>"));
    }

    #[test]
    fn output_is_complete_html_document() {
        let html = render_lifetime_cv(&minimal_cv(), Lang::En);
        assert!(html.contains("<!DOCTYPE html>"));
        assert!(html.contains("<html"));
        assert!(html.contains("</html>"));
        assert!(html.contains("<style>"));
    }

    #[test]
    fn page_title_includes_name() {
        let html = render_lifetime_cv(&minimal_cv(), Lang::En);
        assert!(html.contains("<title>Jane Smith"));
    }

    // ── Personal ──────────────────────────────────────────────────────────────

    #[test]
    fn personal_name_in_output() {
        assert!(render_lifetime_cv(&minimal_cv(), Lang::En).contains("Jane Smith"));
    }

    #[test]
    fn personal_email_is_mailto_link() {
        let html = render_lifetime_cv(&minimal_cv(), Lang::En);
        assert!(html.contains("mailto:jane@example.com"));
    }

    #[test]
    fn personal_location_in_output() {
        assert!(render_lifetime_cv(&minimal_cv(), Lang::En).contains("Paris, France"));
    }

    // ── Experience ────────────────────────────────────────────────────────────

    #[test]
    fn experience_company_and_role_in_output() {
        let html = render_lifetime_cv(&full_cv(), Lang::En);
        assert!(html.contains("Acme Corp"));
        assert!(html.contains("Software Engineer"));
    }

    #[test]
    fn experience_bullets_in_output() {
        assert!(render_lifetime_cv(&full_cv(), Lang::En).contains("Built distributed systems"));
    }

    #[test]
    fn experience_context_in_output() {
        let html = render_lifetime_cv(&full_cv(), Lang::En);
        assert!(html.contains("Legacy monolith"));
        assert!(html.contains(r#"class="exp-project-context""#));
    }

    #[test]
    fn experience_tools_rendered_as_tags() {
        let html = render_lifetime_cv(&full_cv(), Lang::En);
        assert!(html.contains("PostgreSQL"));
        assert!(html.contains(r#"class="tag""#));
    }

    #[test]
    fn no_experience_section_when_empty() {
        let html = render_lifetime_cv(&minimal_cv(), Lang::En);
        assert!(!html.contains(">Experience<"));
    }

    // ── Skills ────────────────────────────────────────────────────────────────

    #[test]
    fn skills_show_category_labels() {
        let html = render_lifetime_cv(&full_cv(), Lang::En);
        assert!(html.contains("Programming"));
        assert!(html.contains("Database"));
    }

    // ── Education ────────────────────────────────────────────────────────────

    #[test]
    fn education_fields_in_output() {
        let html = render_lifetime_cv(&full_cv(), Lang::En);
        assert!(html.contains("MIT"));
        assert!(html.contains("MSc"));
        assert!(html.contains("Computer Science"));
    }

    // ── Projects ─────────────────────────────────────────────────────────────

    #[test]
    fn project_name_url_description_in_output() {
        let html = render_lifetime_cv(&full_cv(), Lang::En);
        assert!(html.contains("CV Generator"));
        assert!(html.contains("https://github.com/me/cv-gen"));
        assert!(html.contains("A cool Rust project"));
        assert!(html.contains("Implemented keyword matching"));
    }

    // ── HTML escaping ─────────────────────────────────────────────────────────

    #[test]
    fn xss_in_name_is_escaped() {
        let mut cv = minimal_cv();
        cv.personal.name = "<script>alert('xss')</script>".to_string();
        let html = render_lifetime_cv(&cv, Lang::En);
        assert!(
            !html.contains("<script>alert"),
            "Raw <script> must not appear"
        );
        assert!(html.contains("&lt;script&gt;"));
    }

    #[test]
    fn ampersand_in_company_is_escaped() {
        let mut cv = minimal_cv();
        cv.experiences.push(Experience {
            id: "e1".to_string(),
            company: "Smith & Jones Ltd".to_string(),
            ..Default::default()
        });
        let html = render_lifetime_cv(&cv, Lang::En);
        assert!(html.contains("Smith &amp; Jones Ltd"));
        assert!(!html.contains("Smith & Jones"));
    }

    // ── Tailored CV ───────────────────────────────────────────────────────────

    #[test]
    fn tailored_shows_score_as_percentage() {
        let cv = TailoredCV {
            match_score: 0.75,
            ..Default::default()
        };
        assert!(render_tailored_cv(&cv, "", Lang::En).contains("75%"));
    }

    #[test]
    fn gap_banner_is_hidden_in_print_output() {
        // The gap-analysis banner (match score + matched/missing keywords)
        // is an on-screen editing aid, not part of the CV a recruiter should
        // receive. It must still render in the screen HTML (checked above),
        // but the embedded CSS must hide it under @media print so it never
        // shows up in the downloaded/printed PDF.
        let cv = TailoredCV {
            match_score: 0.75,
            ..Default::default()
        };
        let html = render_tailored_cv(&cv, "", Lang::En);
        assert!(
            html.contains(".gap-banner") && html.contains("display: none"),
            "expected a print-media rule hiding .gap-banner"
        );

        let print_block_start = html
            .find("@media print")
            .expect("no @media print block found");
        let print_block_end = html[print_block_start..]
            .find("\n}\n")
            .map(|i| print_block_start + i)
            .unwrap_or(html.len());
        let print_block = &html[print_block_start..print_block_end];
        assert!(
            print_block.contains(".gap-banner") && print_block.contains("display: none"),
            "the display:none rule for .gap-banner must be inside the @media print block, not just present somewhere in the stylesheet"
        );
    }

    #[test]
    fn tailored_shows_job_title() {
        let cv = TailoredCV {
            match_score: 0.5,
            ..Default::default()
        };
        assert!(render_tailored_cv(&cv, "Senior Rust Engineer", Lang::En)
            .contains("Senior Rust Engineer"));
    }

    #[test]
    fn tailored_matched_keywords_use_green_class() {
        let cv = TailoredCV {
            matched_keywords: vec!["rust".to_string()],
            ..Default::default()
        };
        let html = render_tailored_cv(&cv, "", Lang::En);
        assert!(html.contains("rust"));
        assert!(html.contains("kw-matched"));
    }

    #[test]
    fn tailored_missing_keywords_use_red_class() {
        let cv = TailoredCV {
            missing_keywords: vec!["golang".to_string()],
            ..Default::default()
        };
        let html = render_tailored_cv(&cv, "", Lang::En);
        assert!(html.contains("golang"));
        assert!(html.contains("kw-missing"));
    }

    #[test]
    fn tailored_missing_keywords_capped_at_fifteen() {
        let cv = TailoredCV {
            missing_keywords: (0..30).map(|i| format!("keyword{i}")).collect(),
            ..Default::default()
        };
        let html = render_tailored_cv(&cv, "", Lang::En);
        assert!(html.contains("keyword14"), "14th keyword should appear");
        assert!(!html.contains("keyword15"), "15th+ should be truncated");
    }

    // ── render_inline / bullet_li bold highlighting ────────────────────────────

    #[test]
    fn render_inline_applies_bold_markers() {
        assert_eq!(
            render_inline("Led a **critical** migration"),
            "Led a <strong>critical</strong> migration"
        );
    }

    #[test]
    fn render_inline_escapes_html_before_bolding() {
        // esc() must run first so `**<script>**` can't inject a real tag —
        // apply_bold only ever wraps already-escaped text in <strong>.
        assert_eq!(
            render_inline("**<script>**"),
            "<strong>&lt;script&gt;</strong>"
        );
    }

    #[test]
    fn render_inline_unterminated_bold_marker_is_left_literal() {
        // A trailing "**" with no closing pair shouldn't eat the rest of
        // the text or panic; it's just emitted as-is.
        assert_eq!(render_inline("done **soon"), "done **soon");
    }

    #[test]
    fn bullet_li_highlights_bold_text_in_bullets() {
        assert_eq!(
            bullet_li("Cut **P0 incidents** by 40%"),
            "<li>• Cut <strong>P0 incidents</strong> by 40%</li>"
        );
    }

    #[test]
    fn render_experience_bolds_project_context_and_bullets() {
        use crate::models::{Experience, ExperienceProject, LifetimeCV, LocalizedText};
        let mut cv = LifetimeCV::default();
        cv.experiences.push(Experience {
            id: "e1".into(),
            company: "Acme".into(),
            role: LocalizedText::same("Engineer"),
            location: String::new(),
            start_date: "2020".into(),
            end_date: "Present".into(),
            projects: vec![ExperienceProject {
                name: LocalizedText::same("Platform"),
                context: vec![LocalizedText::same("Owned the **core** service")],
                bullets: vec![LocalizedText::same("Shipped **key** feature")],
                skill_ids: vec![],
                start_date: String::new(),
                end_date: String::new(),
            }],
            skill_ids: vec![],
        });
        let html = render_lifetime_cv(&cv, Lang::En);
        assert!(html.contains("Owned the <strong>core</strong> service"));
        assert!(html.contains("Shipped <strong>key</strong> feature"));
    }

    // ── score_color_for ───────────────────────────────────────────────────────

    #[test]
    fn score_color_green_high() {
        assert_eq!(score_color_for(1.00), "#16a34a");
    }
    #[test]
    fn score_color_green_exact() {
        assert_eq!(score_color_for(0.60), "#16a34a");
    }
    #[test]
    fn score_color_amber_mid() {
        assert_eq!(score_color_for(0.50), "#d97706");
    }
    #[test]
    fn score_color_amber_exact() {
        assert_eq!(score_color_for(0.30), "#d97706");
    }
    #[test]
    fn score_color_red_low() {
        assert_eq!(score_color_for(0.00), "#dc2626");
    }
    #[test]
    fn score_color_red_just_under_amber() {
        assert_eq!(score_color_for(0.29), "#dc2626");
    }
}
