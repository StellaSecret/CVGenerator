use super::*;
use lopdf::{Document, Object};

/// One logical line of text together with the (x, y) position (in PDF page
/// space, origin bottom-left) where it starts.
///
/// `x`/`y` are correctly computed (properly composed through the full
/// graphics state — `q`/`Q`/`cm`/Form XObject placement — see
/// `run_operations`). They're used for one narrow purpose in
/// `extract_text_from_page` — gluing together immediately-adjacent lines
/// that sit on the same visual row (see the comment there) — rather than
/// for general column-aware reordering, which was tried and reverted; see
/// that function for why.
#[derive(Debug, Clone)]
pub(super) struct PositionedLine {
    pub(super) x: f64,
    pub(super) y: f64,
    pub(super) text: String,
}

/// A minimal parsed ToUnicode CMap: maps a fixed-width source code to decoded
/// text. lopdf's bundled ToUnicode CMap parser assumes source codes are
/// always 2 bytes wide, which fails for simple (Type1/TrueType) fonts using
/// the very common 1-byte-code convention — exactly what real-world PDFs
/// (including design-tool exports) tend to use. That failure meant every
/// font's encoding silently fell back to raw-byte decoding, corrupting
/// ligatures ("fi" → a stray control character), dashes, and accented
/// letters throughout the extracted text. This is our own small, permissive
/// parser that handles both 1-byte and 2-byte source codes.
#[derive(Debug, Default, Clone)]
pub(super) struct ToUnicodeMap {
    pub(super) code_bytes: usize,
    pub(super) map: std::collections::HashMap<u32, String>,
}

impl ToUnicodeMap {
    pub(super) fn decode(&self, bytes: &[u8]) -> Option<String> {
        if self.code_bytes == 0 || bytes.is_empty() {
            return None;
        }
        let mut out = String::new();
        for chunk in bytes.chunks(self.code_bytes) {
            if chunk.len() < self.code_bytes {
                break; // incomplete trailing chunk
            }
            let code = bytes_to_u32(chunk);
            if let Some(s) = self.map.get(&code) {
                out.push_str(s);
            } else if chunk.len() == 1 {
                // Unmapped single byte: best-effort Latin-1 fallback —
                // but only for bytes that plausibly encode a real Latin-1
                // character. Custom font subsets sometimes assign a
                // low/control-range byte (e.g. 0x00) to a glyph ID for a
                // ligature or kerned pair that has no ToUnicode entry at
                // all (seen in practice: "Wilfried" → "Wil<NUL>ied", the
                // "fr" glyph silently replaced by a literal control
                // character). Inserting that control byte verbatim
                // doesn't recover the missing character — Latin-1 byte
                // 0x00 was never really "NUL" here, it's just an
                // unresolved glyph ID — and a stray NUL later corrupts
                // downstream heuristics that assume plain text (e.g.
                // `guess_name`'s alphabetic check rejects the whole
                // line). Dropping it is strictly better: the rest of the
                // word survives intact instead of the whole line being
                // discarded.
                let c = chunk[0] as char;
                if !c.is_control() {
                    out.push(c);
                }
            }
        }
        if out.is_empty() {
            None
        } else {
            Some(out)
        }
    }
}

pub(super) fn bytes_to_u32(b: &[u8]) -> u32 {
    b.iter().fold(0u32, |acc, &x| (acc << 8) | x as u32)
}

pub(super) fn utf16be_bytes_to_string(b: &[u8]) -> String {
    let u16s: Vec<u16> = b
        .chunks(2)
        .filter_map(|c| {
            if c.len() == 2 {
                Some(u16::from_be_bytes([c[0], c[1]]))
            } else {
                None
            }
        })
        .collect();
    String::from_utf16_lossy(&u16s)
}

pub(super) fn parse_hex_token(tok: &str) -> Option<Vec<u8>> {
    let t = tok.trim();
    let t = t.strip_prefix('<')?;
    let t = t.strip_suffix('>')?;
    if t.is_empty() || t.len() % 2 != 0 {
        return None;
    }
    (0..t.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&t[i..i + 2], 16).ok())
        .collect()
}

/// Parse a ToUnicode CMap stream's decoded text content into a lookup table.
/// Handles `beginbfchar`/`endbfchar` (explicit source->dest pairs) and
/// `beginbfrange`/`endbfrange` (either a single incrementing destination, or
/// an explicit `[ ... ]` array of destinations) — the two constructs the PDF
/// spec defines for ToUnicode CMaps.
pub(super) fn parse_tounicode_cmap(text: &str) -> Option<ToUnicodeMap> {
    let tokens: Vec<&str> = text.split_whitespace().collect();
    let mut map = std::collections::HashMap::new();
    let mut code_bytes: usize = 0;
    let mut i = 0;

    while i < tokens.len() {
        match tokens[i] {
            "beginbfchar" => {
                i += 1;
                while i < tokens.len() && tokens[i] != "endbfchar" {
                    if i + 1 >= tokens.len() {
                        break;
                    }
                    if let (Some(src), Some(dst)) =
                        (parse_hex_token(tokens[i]), parse_hex_token(tokens[i + 1]))
                    {
                        if code_bytes == 0 {
                            code_bytes = src.len().max(1);
                        }
                        map.insert(bytes_to_u32(&src), utf16be_bytes_to_string(&dst));
                    }
                    i += 2;
                }
            }
            "beginbfrange" => {
                i += 1;
                while i < tokens.len() && tokens[i] != "endbfrange" {
                    if i + 2 >= tokens.len() {
                        break;
                    }
                    let (Some(start_b), Some(end_b)) =
                        (parse_hex_token(tokens[i]), parse_hex_token(tokens[i + 1]))
                    else {
                        i += 1;
                        continue;
                    };
                    if code_bytes == 0 {
                        code_bytes = start_b.len().max(1);
                    }
                    let start = bytes_to_u32(&start_b);
                    let end = bytes_to_u32(&end_b);

                    if tokens[i + 2].starts_with('[') {
                        // Array destination form: [ <d1> <d2> ... ]
                        let mut j = i + 2;
                        let mut first = tokens[j];
                        // '[' may be its own token or glued to the first hex token
                        if first == "[" {
                            j += 1;
                            first = if j < tokens.len() { tokens[j] } else { "" };
                        } else {
                            first = first.trim_start_matches('[');
                        }
                        let mut offset: u32 = 0;
                        let mut cur = first;
                        loop {
                            if cur.is_empty() || j >= tokens.len() {
                                break;
                            }
                            let closing = cur.ends_with(']');
                            let hex_part = cur.trim_end_matches(']');
                            if let Some(dst) = parse_hex_token(hex_part) {
                                map.insert(start + offset, utf16be_bytes_to_string(&dst));
                                offset += 1;
                            }
                            j += 1;
                            if closing {
                                break;
                            }
                            if j >= tokens.len() {
                                break;
                            }
                            cur = tokens[j];
                        }
                        i = j + 1;
                        continue;
                    } else if let Some(dst_b) = parse_hex_token(tokens[i + 2]) {
                        // Single destination, incrementing per source code.
                        let n = end.saturating_sub(start);
                        if dst_b.len() == 2 {
                            let dst_base = bytes_to_u32(&dst_b);
                            for k in 0..=n {
                                if let Some(c) = char::from_u32(dst_base + k) {
                                    map.insert(start + k, c.to_string());
                                }
                            }
                        } else {
                            // Rare: multi-code-unit destination; apply verbatim to the
                            // first code, best-effort for the rest.
                            let s = utf16be_bytes_to_string(&dst_b);
                            for k in 0..=n {
                                map.insert(start + k, s.clone());
                            }
                        }
                        i += 3;
                        continue;
                    } else {
                        i += 1;
                        continue;
                    }
                }
            }
            _ => {}
        }
        i += 1;
    }

    if map.is_empty() {
        None
    } else {
        Some(ToUnicodeMap {
            code_bytes: code_bytes.max(1),
            map,
        })
    }
}

/// A 2D affine transform matching the PDF matrix convention `[a b c d e f]`:
/// `x' = a*x + c*y + e`, `y' = b*x + d*y + f`. Used to correctly compose the
/// text matrix with the current transformation matrix (CTM) so that text
/// position can be computed in true page (device) space, even when the text
/// lives inside one or more nested Form XObjects each with their own
/// placement transform (`cm`) — extremely common in design-tool-exported
/// PDFs, which often implement small repeated UI elements (e.g. skill-tag
/// "pill" badges) as one shared Form XObject invoked many times, once per
/// badge, each with a different placement matrix.
#[derive(Clone, Copy, Debug)]
pub(super) struct Matrix {
    pub(super) a: f64,
    pub(super) b: f64,
    pub(super) c: f64,
    pub(super) d: f64,
    pub(super) e: f64,
    pub(super) f: f64,
}

impl Matrix {
    pub(super) fn identity() -> Self {
        Matrix {
            a: 1.0,
            b: 0.0,
            c: 0.0,
            d: 1.0,
            e: 0.0,
            f: 0.0,
        }
    }

    pub(super) fn from_six(v: [f64; 6]) -> Self {
        Matrix {
            a: v[0],
            b: v[1],
            c: v[2],
            d: v[3],
            e: v[4],
            f: v[5],
        }
    }

    /// Compose so that a point is transformed by `self` first, then by
    /// `other` — i.e. `self` is the "inner" (more local) transform and
    /// `other` is the "outer" one. This matches how PDF's `cm` operator
    /// prepends a new matrix ahead of the existing CTM, and how a text
    /// matrix (Tm) is applied within the current CTM.
    pub(super) fn compose(&self, other: &Matrix) -> Matrix {
        Matrix {
            a: self.a * other.a + self.b * other.c,
            b: self.a * other.b + self.b * other.d,
            c: self.c * other.a + self.d * other.c,
            d: self.c * other.b + self.d * other.d,
            e: self.e * other.a + self.f * other.c + other.e,
            f: self.e * other.b + self.f * other.d + other.f,
        }
    }

    /// Where this matrix maps the local origin (0, 0) to.
    pub(super) fn origin(&self) -> (f64, f64) {
        (self.e, self.f)
    }
}

/// Resolve a PDF value that may be either an inline Dictionary or a
/// Reference to one.
pub(super) fn resolve_to_dict<'a>(
    doc: &'a Document,
    obj: &'a Object,
) -> Option<&'a lopdf::Dictionary> {
    match obj {
        Object::Dictionary(d) => Some(d),
        Object::Reference(id) => doc.get_dictionary(*id).ok(),
        _ => None,
    }
}

/// Build a map from font resource name (e.g. `F1`) to that font's parsed
/// ToUnicode CMap, for every font referenced in the given Resources
/// dictionary. Fonts without a ToUnicode entry (or whose CMap fails to
/// parse) are simply omitted, so callers fall back to naive byte decoding
/// for them.
pub(super) fn build_font_cmaps_from_resources(
    doc: &Document,
    resources: &lopdf::Dictionary,
) -> std::collections::BTreeMap<Vec<u8>, ToUnicodeMap> {
    let mut out = std::collections::BTreeMap::new();
    let Ok(font_obj) = resources.get(b"Font") else {
        return out;
    };
    let Some(font_dict) = resolve_to_dict(doc, font_obj) else {
        return out;
    };
    for (name, value) in font_dict.iter() {
        let font = match value {
            Object::Reference(id) => doc.get_dictionary(*id).ok(),
            Object::Dictionary(d) => Some(d),
            _ => None,
        };
        let Some(font) = font else { continue };
        let Ok(Object::Reference(id)) = font.get(b"ToUnicode") else {
            continue;
        };
        let Ok(Object::Stream(stream)) = doc.get_object(*id) else {
            continue;
        };
        let mut stream = stream.clone();
        let _ = stream.decompress();
        let Ok(content) = stream.get_plain_content() else {
            continue;
        };
        let text = String::from_utf8_lossy(&content);
        if let Some(cmap) = parse_tounicode_cmap(&text) {
            out.insert(name.clone(), cmap);
        }
    }
    out
}

/// Build a map from font resource name (e.g. `F1`) to that font's parsed
/// ToUnicode CMap, for every font used on the given page that has one.
/// Fonts without a ToUnicode entry (or whose CMap fails to parse) are simply
/// omitted, so callers fall back to naive byte decoding for them.
pub(super) fn build_font_cmaps(
    doc: &Document,
    page_id: lopdf::ObjectId,
) -> std::collections::BTreeMap<Vec<u8>, ToUnicodeMap> {
    let mut out = std::collections::BTreeMap::new();
    let Ok(fonts) = doc.get_page_fonts(page_id) else {
        return out;
    };
    for (name, font) in fonts {
        let Ok(Object::Reference(id)) = font.get(b"ToUnicode") else {
            continue;
        };
        let Ok(Object::Stream(stream)) = doc.get_object(*id) else {
            continue;
        };
        let mut stream = stream.clone();
        let _ = stream.decompress();
        let Ok(content) = stream.get_plain_content() else {
            continue;
        };
        let text = String::from_utf8_lossy(&content);
        if let Some(cmap) = parse_tounicode_cmap(&text) {
            out.insert(name, cmap);
        }
    }
    out
}

/// Extract raw text from PDF bytes.
/// Recursively follows Form XObjects (text nested in sub-streams).
pub fn extract_text(bytes: &[u8]) -> Result<String, String> {
    if bytes.len() < 8 || !bytes.starts_with(b"%PDF") {
        return Err("Not a valid PDF file.".to_string());
    }
    if is_encrypted(bytes) {
        return Err(
            "This PDF is encrypted/password-protected. Please unlock it first.".to_string(),
        );
    }

    let doc = match Document::load_mem(bytes) {
        Ok(doc) => doc,
        Err(e) => return Err(format!("Failed to parse PDF: {e}")),
    };
    let pages = doc.get_pages();
    if pages.is_empty() {
        return Err("PDF has no pages.".to_string());
    }

    let mut all_text = Vec::new();
    for page_id in pages.values() {
        let text = extract_text_from_page(&doc, *page_id);
        if !text.trim().is_empty() {
            all_text.push(text);
        }
    }
    if all_text.is_empty() {
        return Err("No text could be extracted. The PDF may contain only images (scanned) or use an unsupported encoding. Try re-exporting from your PDF editor as text-based.".to_string());
    }
    // Our own renderer inserts U+200C (zero-width non-joiner) around
    // letter pairs like "fi"/"fl" purely to stop the print-to-PDF path
    // from fusing them into a ligature glyph that doesn't survive
    // re-extraction (see renderer::break_ligatures). It carries no
    // content of its own, so strip it here rather than let it leak into
    // the parsed model — otherwise re-importing our own PDF would bake an
    // invisible character into every affected word, and re-exporting a
    // second time would (harmlessly, but pointlessly) look for a place to
    // insert another one.
    // Third-party PDFs (not produced by this app) commonly map an
    // "fi"/"fl"/"ff"/"ffi"/"ffl" ligature glyph straight to the single
    // precomposed Unicode presentation-form character (U+FB00-FB04)
    // rather than back to the plain letters. Left as-is, that one odd
    // character breaks exact-text matching throughout this file — e.g. a
    // section header literally spelled "Certiﬁcations" in the source PDF
    // never equals the plain-ASCII "certifications" this parser looks
    // for, so the whole section silently fails to be recognized. Expand
    // these back to plain letters for the same reason renderer.rs's
    // `expand_ligature_chars` does on the way out: the ligature-or-not
    // choice is a font rendering detail, not a difference in content.
    let expand_ligature_chars = |s: &str| -> String {
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
    };
    Ok(expand_ligature_chars(
        &all_text.join("\n").replace('\u{200C}', ""),
    ))
}

/// Extract text from a page by reading its Contents and recursing into Form XObjects.
pub(super) fn extract_text_from_page(doc: &Document, page_id: lopdf::ObjectId) -> String {
    let mut lines: Vec<PositionedLine> = Vec::new();
    let page_obj = match doc.get_object(page_id) {
        Ok(o) => o.clone(),
        Err(_) => return String::new(),
    };
    let page_dict = match &page_obj {
        Object::Dictionary(d) => d,
        _ => return String::new(),
    };

    // Build the font ToUnicode-CMap map for this page (see build_font_cmaps).
    let encodings: std::collections::BTreeMap<Vec<u8>, ToUnicodeMap> =
        build_font_cmaps(doc, page_id);

    let resources_obj = page_dict.get(b"Resources").ok().cloned();
    let resources_dict = resources_obj.as_ref().and_then(|r| resolve_to_dict(doc, r));

    let content_ids: Vec<lopdf::ObjectId> = match page_dict.get(b"Contents") {
        Ok(Object::Reference(id)) => vec![*id],
        Ok(Object::Array(arr)) => arr
            .iter()
            .filter_map(|o| {
                if let Object::Reference(id) = o {
                    Some(*id)
                } else {
                    None
                }
            })
            .collect(),
        _ => vec![],
    };

    // Per the PDF spec, multiple content streams for one page are logically
    // one continuous stream (so graphics state like the q/Q stack and CTM
    // carries across them) — concatenate their operations before
    // interpreting. If a given stream fails to parse at all, fall back to
    // the raw byte scanner for just that stream rather than losing it
    // entirely (this mirrors the previous per-stream fallback behavior).
    let mut all_ops: Vec<lopdf::content::Operation> = Vec::new();
    for cid in &content_ids {
        if let Ok(Object::Stream(stream)) = doc.get_object(*cid) {
            let mut s = stream.clone();
            let _ = s.decompress();
            match s.decode_content() {
                Ok(content) => all_ops.extend(content.operations),
                Err(_) => {
                    if let Ok(data) = s.get_plain_content() {
                        let text = decode_content_raw(&data);
                        if !text.is_empty() {
                            lines.push(PositionedLine {
                                x: 0.0,
                                y: 0.0,
                                text,
                            });
                        }
                    }
                }
            }
        }
    }

    if let Some(resources_dict) = resources_dict {
        let mut visited: Vec<lopdf::ObjectId> = Vec::new();
        run_operations(
            doc,
            &all_ops,
            resources_dict,
            &encodings,
            Matrix::identity(),
            &mut visited,
            &mut lines,
        );
    }

    // NOTE on column reordering: see the row-aware pass further down,
    // right before the final join — it's deliberately placed after the
    // same-row gluing below (rather than here) because it operates on the
    //
    // One narrow, purely-local exception: our own renderer lays out a
    // job's company/location and its date range in the same visual row via
    // flexbox (`justify-content: space-between`), and Chromium's print
    // engine renders each flex child as its own separate BT/ET text
    // object rather than one continuous run — even though visually they
    // share one line, e.g. "DTNUM/SDAN/BFO ·" and "Paris, France
    // December 2024 – February 2026" come out as two consecutive
    // PositionedLines at (nearly) the same y. parse_experiences expects
    // that whole row on one line, so left as-is this silently drops every
    // experience entry when re-importing our own PDF output. Unlike the
    // column-gutter heuristic above, this doesn't reorder anything or
    // guess at layout — it only glues lines together when they are
    // *immediately adjacent in stream order* and sit at virtually
    // identical y, which in practice only happens for genuinely
    // same-row, flex-split text.
    //
    // Two deliberately-narrow rules, applied within each same-y run
    // (never across a Y change, and never reordering anything):
    //
    //   1. A fragment that is itself "(...)" (starts with "(", ends with
    //      ")") always glues onto the fragment right before it. This app
    //      renders e.g. a language's proficiency as its own trailing
    //      `<span class="lang-level">({level})</span>` right after the
    //      name span, so several flex-*wrapped* "name (level)" tags can
    //      legitimately share one visual row — "English", "(Conversational)",
    //      "French", "(Conversational)", "Vietnamese", "(Conversational)"
    //      — and this reunites each parenthetical with its own label
    //      without guessing where one wrapped tag ends and the next
    //      begins from position alone.
    //   2. AFTER that pass, if a run has been reduced to exactly two
    //      fragments, merge those two as well. This is what catches the
    //      job-header case above. Deliberately restricted to exactly two:
    //      a two-part flex row (label ... value) is unambiguous, but nothing
    //      stops a genuine list of independent same-row tags (e.g. a row of
    //      3+ plain skill badges with no parenthetical) from also being
    //      exactly-N — merging those would corrupt them. Two, and two only,
    //      is the safe case.
    const SAME_ROW_Y_EPSILON: f64 = 0.75;
    let mut merged: Vec<PositionedLine> = Vec::with_capacity(lines.len());
    let mut i = 0;
    while i < lines.len() {
        let same_row_run_end = {
            let mut j = i + 1;
            while j < lines.len()
                // Compare each fragment to the one right before it (chained),
                // not to the run's first fragment. A row can be made of
                // several small fragments (e.g. company text, then a
                // differently-styled nested <span> for location, then the
                // dates span) whose baselines drift by a fraction of a
                // point from one style change to the next; chaining the
                // comparison tolerates that gradual drift along the row,
                // where comparing everything back to the first fragment
                // would reject the last fragment(s) over a drift that
                // never exceeds the epsilon between any *adjacent* pair.
                && (lines[j].y - lines[j - 1].y).abs() < SAME_ROW_Y_EPSILON
                && lines[j].x > lines[j - 1].x
            {
                j += 1;
            }
            j
        };

        // Rule 1a: fold any "(...)" fragment into the one before it.
        // Rule 1b: fold a fragment that's nothing but a trailing separator
        // ("·", "-", "|", etc., with no other content) into the *next*
        // fragment. This app's own renderer emits a job's company and
        // location as "Company" then (when there's a location) a second,
        // differently-styled fragment starting with " · Location" — i.e.
        // the separator sits at the END of the company fragment, not the
        // start of the location one, so this direction of folding is what
        // reunites them; unlike the parenthetical rule, checking the
        // *next* fragment must happen before pushing the current one.
        let mut run: Vec<PositionedLine> = Vec::with_capacity(same_row_run_end - i);
        let mut k = i;
        while k < same_row_run_end {
            let mut line = lines[k].clone();
            let is_parenthetical = {
                let t = line.text.trim();
                t.starts_with('(') && t.ends_with(')')
            };
            if is_parenthetical {
                if let Some(prev) = run.last_mut() {
                    if !prev.text.ends_with(' ') {
                        prev.text.push(' ');
                    }
                    prev.text.push_str(&line.text);
                    k += 1;
                    continue;
                }
            }
            // Rule 1b: fold a fragment that ENDS WITH a dangling "·" (this
            // app's own separator between a job's company and location,
            // e.g. this app's own renderer emits company text as
            // "DTNUM/SDAN/BFO ·" followed by a *differently-styled*
            // "Paris, France" fragment for the location — the style
            // change is what splits them, so the separator lands on the
            // end of the first fragment, not the start of the second)
            // forward into the fragment right after it. "·" specifically
            // (not a general dash) because it's this app's distinctive
            // choice of separator and unlikely to appear at the end of
            // unrelated content, keeping this rule narrow.
            let ends_with_dangling_middot = line.text.trim_end().ends_with('·');
            if ends_with_dangling_middot && k + 1 < same_row_run_end {
                let next = &lines[k + 1];
                if !line.text.ends_with(' ') && !next.text.starts_with(' ') {
                    line.text.push(' ');
                }
                line.text.push_str(&next.text);
                run.push(line);
                k += 2;
                continue;
            }
            run.push(line);
            k += 1;
        }

        // Rule 1c: iteratively fold a fragment that is ENTIRELY a bare
        // connector token — "-", "–", "—", "·", "|", or the French range
        // word "à" — into its two neighbors, joining prev+connector+next
        // into one fragment and repeating. This is what reunites a job
        // header that a producer split into many small pieces across one
        // row, e.g. "EMUNDUS" "-" "Depuis" "février 2024" (a hyphen and an
        // open-ended-since date, each their own text run) into one
        // "EMUNDUS - Depuis février 2024" line, or an even longer chain
        // like "OpenXtrem" "-" "Juin 2016" "à" "avril 2021" "-" "CDI" "-"
        // "La Rochelle" "-" "France" into one continuous line. Restricted
        // to tokens that are never legitimately their own standalone
        // fragment for any other reason — pure punctuation, plus "à"
        // specifically because it's this exact "Start à End" range shape's
        // separator and nothing else. Deliberately NOT "to"/"au"/other
        // ordinary short words, which are common enough elsewhere in prose
        // that folding on them blindly would risk mis-joining unrelated
        // same-row content (e.g. a genuine list of short standalone tags).
        fn is_bare_row_connector(text: &str) -> bool {
            matches!(text.trim(), "-" | "–" | "—" | "·" | "|" | "à")
        }
        let mut fold_idx = 0;
        while fold_idx < run.len() {
            if is_bare_row_connector(&run[fold_idx].text)
                && fold_idx > 0
                && fold_idx + 1 < run.len()
            {
                let connector = run[fold_idx].text.trim().to_string();
                let next_text = run[fold_idx + 1].text.clone();
                let prev = &mut run[fold_idx - 1];
                if !prev.text.ends_with(' ') {
                    prev.text.push(' ');
                }
                prev.text.push_str(&connector);
                if !next_text.starts_with(' ') {
                    prev.text.push(' ');
                }
                prev.text.push_str(&next_text);
                run.remove(fold_idx + 1);
                run.remove(fold_idx);
                // Step back to the (now-combined) previous element in case
                // it's adjacent to another bare connector after this
                // merge, so a whole chain collapses in one pass.
                fold_idx = fold_idx.saturating_sub(1);
            } else {
                fold_idx += 1;
            }
        }

        // Rule 2: a run reduced to exactly two fragments is a two-part
        // flex row (label ... value) — merge fully.
        //
        // (This used to also require a minimum rightward x-gap between the
        // two fragments, to rule out a block-level heading immediately
        // followed by the next block's paragraph text landing within
        // SAME_ROW_Y_EPSILON of each other. That guard was reverted: real
        // same-row pairs turned out to routinely have a SMALL x-gap too —
        // an icon glyph right next to its label, a name wrapped across two
        // adjacent runs, a date value split mid-run — and the x-gap
        // requirement broke all of those (observed directly: it silently
        // dropped an entire job entry by splitting "EMUNDUS - Depuis
        // février 2024" into two lines, so the combined line's date range
        // was never recognized). The original heading/paragraph mis-merge
        // this was meant to fix is handled at the `parse_experiences`
        // level instead — see `looks_like_bare_role_line`'s capitalization
        // check — which discriminates on the actual content instead of
        // position, and doesn't have this failure mode.)
        if run.len() == 2 {
            let mut joined = run[0].text.clone();
            if !joined.ends_with(' ') && !run[1].text.starts_with(' ') {
                joined.push(' ');
            }
            joined.push_str(&run[1].text);
            merged.push(PositionedLine {
                x: run[0].x,
                y: run[0].y,
                text: joined,
            });
        } else {
            merged.extend(run);
        }
        i = same_row_run_end;
    }

    // --- Row-aware column detection & reordering -----------------------
    //
    // Some PDF producers (design-tool exports especially) don't paint a
    // multi-column page in visual reading order — they paint by layer/pass
    // instead, e.g. every job's title+company+dates across the WHOLE page
    // first, then every job's bullet paragraphs in a second pass, then the
    // sidebar. `merged` above is still in that raw paint order. An earlier
    // attempt to fix this used a single global "widest gap" split over
    // *every* line on the page (see git history) and caused real
    // regressions — a stray right-aligned header snippet, or a
    // contact-info block that sits visually to the right of the name/title
    // near the top of the page, got misread as "the right column" and
    // dragged out of place, corrupting a part of the page (the header)
    // that was already in correct order.
    //
    // This version is deliberately narrower in scope to avoid that:
    //
    //   1. It only ever reorders the BODY of the page — everything from
    //      the first recognized section heading (`detect_section`, e.g.
    //      "Expériences"/"Compétences") onward. Everything before that
    //      (name, title, badges, contact info) is left byte-for-byte as
    //      extracted, in its original stream order. That header region is
    //      not a multi-pass layout in practice (verified against a real
    //      two-column resume — the contact-info block, despite sitting far
    //      to the right, was already painted top-to-bottom in the stream),
    //      so reordering it only risks the corruption seen before for no
    //      benefit — e.g. dragging the header's contact info out from
    //      before the first section (where personal-info extraction looks
    //      for it) to after everything else.
    //   2. It groups fragments into visual ROWS first (by y-proximity,
    //      same rule used above for same-row gluing), and clusters WHOLE
    //      ROWS by their leftmost x — never individual fragments — so a
    //      single stray fragment can't be misread as its own column.
    //   3. It only commits to a split when both sides have a healthy
    //      number of rows (not just one or two) AND the gap between them
    //      is wide enough to be a real column gutter rather than routine
    //      bullet/heading indentation within one column (this resume's own
    //      bullets sit only ~7pt right of their section headings — nowhere
    //      near a real gutter, which measured ~260pt+ here). If neither
    //      holds, the body is left exactly as extracted — no reordering —
    //      which is always at least as safe as the previous behavior.
    //   4. When it DOES commit, each detected column is re-sorted purely
    //      by y (top to bottom) — which is what actually fixes the
    //      multi-pass painting, since a row's true vertical position on
    //      the page doesn't depend on which paint pass wrote it.
    const MIN_ROWS_PER_COLUMN: usize = 4;
    const MIN_COLUMN_GAP: f64 = 100.0;

    let body_start = merged
        .iter()
        .position(|l| detect_section(&l.text).is_some());

    let final_lines: Vec<PositionedLine> = match body_start {
        None => merged,
        Some(start) => {
            let (header, body) = merged.split_at(start);
            let mut out = header.to_vec();
            let rows = group_into_rows(body, SAME_ROW_Y_EPSILON);
            match find_column_split(&rows, MIN_ROWS_PER_COLUMN, MIN_COLUMN_GAP) {
                None => {
                    out.extend(body.iter().cloned());
                }
                Some(split_x) => {
                    let mut left_rows: Vec<&Vec<PositionedLine>> = Vec::new();
                    let mut right_rows: Vec<&Vec<PositionedLine>> = Vec::new();
                    for row in &rows {
                        if row_repr_x(row) < split_x {
                            left_rows.push(row);
                        } else {
                            right_rows.push(row);
                        }
                    }

                    // Guard against a *partial-height* sidebar — e.g. a
                    // "Values" / "Core Competencies" self-rating box that
                    // only occupies the top portion of the page next to
                    // the start of a much longer left column (Summary,
                    // then Experience, continuing for the rest of the
                    // page and beyond). Column detection above only
                    // checks that both sides have enough ROWS and a wide
                    // enough x-GAP — neither of which catches this case,
                    // since a tall sidebar box can easily have plenty of
                    // rows. But flat "all of the left column, then all of
                    // the right column" concatenation is only correct
                    // when both columns run the page's full height; for a
                    // partial-height sidebar it instead teleports that
                    // sidebar's content away from where it visually sits
                    // (near the top) to after everything below it in the
                    // *other* column — which, on a page busy enough, can
                    // land it mid-sentence inside unrelated content and
                    // (worse) have it accidentally match a section-header
                    // keyword, truncating that section entirely.
                    //
                    // Require the shorter column's y-span to cover a good
                    // majority of the taller column's — i.e. both columns
                    // genuinely run (close to) the full height of this
                    // page's body — before trusting the split.
                    const MIN_SPAN_RATIO: f64 = 0.6;
                    let y_span = |rows: &[&Vec<PositionedLine>]| -> f64 {
                        let ys: Vec<f64> = rows.iter().map(|r| row_y(r)).collect();
                        let max = ys.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
                        let min = ys.iter().cloned().fold(f64::INFINITY, f64::min);
                        if max.is_finite() && min.is_finite() {
                            max - min
                        } else {
                            0.0
                        }
                    };
                    let left_span = y_span(&left_rows);
                    let right_span = y_span(&right_rows);
                    let taller = left_span.max(right_span);
                    let shorter = left_span.min(right_span);
                    let spans_full_height = taller <= 0.0 || shorter / taller >= MIN_SPAN_RATIO;

                    if !spans_full_height {
                        out.extend(body.iter().cloned());
                    } else {
                        left_rows.sort_by(|a, b| {
                            row_y(b)
                                .partial_cmp(&row_y(a))
                                .unwrap_or(std::cmp::Ordering::Equal)
                        });
                        right_rows.sort_by(|a, b| {
                            row_y(b)
                                .partial_cmp(&row_y(a))
                                .unwrap_or(std::cmp::Ordering::Equal)
                        });
                        for row in left_rows {
                            out.extend(row.iter().cloned());
                        }
                        for row in right_rows {
                            out.extend(row.iter().cloned());
                        }
                    }
                }
            }
            out
        }
    };

    final_lines
        .into_iter()
        .map(|l| l.text)
        .collect::<Vec<_>>()
        .join("\n")
}

/// Group a stream-ordered list of positioned fragments into visual rows —
/// runs of fragments that sit at (nearly) the same y, chained the same way
/// as the same-row gluing above. Keeps each row's fragments separate
/// (doesn't merge their text) — this is used purely to compute a row's
/// representative x/y for column detection; the fragments still need to
/// come out as separate lines afterward, same as when no reordering
/// happens at all.
pub(super) fn group_into_rows(
    lines: &[PositionedLine],
    y_epsilon: f64,
) -> Vec<Vec<PositionedLine>> {
    let mut rows: Vec<Vec<PositionedLine>> = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let mut j = i + 1;
        while j < lines.len() && (lines[j].y - lines[j - 1].y).abs() < y_epsilon {
            j += 1;
        }
        rows.push(lines[i..j].to_vec());
        i = j;
    }
    rows
}

/// A row's representative x for column clustering — its leftmost fragment.
pub(super) fn row_repr_x(row: &[PositionedLine]) -> f64 {
    row.iter().fold(f64::INFINITY, |acc, l| acc.min(l.x))
}

/// A row's y (all its fragments sit within `y_epsilon` of each other by
/// construction, so the first is representative enough).
pub(super) fn row_y(row: &[PositionedLine]) -> f64 {
    row.first().map(|l| l.y).unwrap_or(0.0)
}

/// Find the best x to split `rows` into a left and right column, if any
/// split is well-supported enough to trust. Looks at every gap between
/// consecutive rows sorted by their representative x, and picks the
/// largest gap that leaves at least `min_rows` rows on each side — i.e.
/// prefers a big, well-populated gutter over a technically-larger gap that
/// only isolates a couple of stray rows. Returns `None` if no candidate
/// gap is both wide enough (`min_gap`) and well-populated on both sides,
/// which happens for genuinely single-column pages (no real gap at all)
/// as well as pages where only a couple of rows drift to one side (e.g. a
/// wrapped bullet, or a right-aligned page number) — deliberately erring
/// toward "don't reorder" in ambiguous cases, since that's always at least
/// as safe as the previous, unconditional natural-order behavior.
pub(super) fn find_column_split(
    rows: &[Vec<PositionedLine>],
    min_rows: usize,
    min_gap: f64,
) -> Option<f64> {
    if rows.len() < min_rows * 2 {
        return None;
    }
    let mut xs: Vec<f64> = rows.iter().map(|r| row_repr_x(r)).collect();
    xs.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    let mut best: Option<(f64, f64)> = None; // (gap, split_x)
    for i in 0..xs.len().saturating_sub(1) {
        let left_count = i + 1;
        let right_count = xs.len() - left_count;
        if left_count < min_rows || right_count < min_rows {
            continue;
        }
        let gap = xs[i + 1] - xs[i];
        if gap < min_gap {
            continue;
        }
        if best.map(|(best_gap, _)| gap > best_gap).unwrap_or(true) {
            best = Some((gap, (xs[i] + xs[i + 1]) / 2.0));
        }
    }
    best.map(|(_, split_x)| split_x)
}

/// Convert a numeric PDF Object (Integer or Real) to f64.
pub(super) fn object_to_f64(obj: &Object) -> Option<f64> {
    match obj {
        Object::Integer(i) => Some(*i as f64),
        Object::Real(f) => Some(*f as f64),
        _ => None,
    }
}

/// Threshold (in thousandths of text space units) beyond which a TJ array's
/// numeric kerning adjustment is treated as an actual word gap rather than
/// ordinary letter-spacing. Many PDF producers omit real space characters and
/// rely entirely on this adjustment to separate words.
///
/// This must stay comfortably above the per-character kerning adjustment
/// Chromium emits for CSS `letter-spacing`: at roughly 1 "unit" per
/// 0.001em, a modest `letter-spacing: 0.12em` (as used for this app's own
/// `.section-title`, e.g. "EXPERIENCE") comes out as -120 (occasionally
/// -121, depending on per-glyph font-metric rounding) between *every*
/// letter pair — nowhere near a real word gap, but close enough to a
/// too-low threshold to trip it on some pairs and not others, corrupting
/// section headings into e.g. "EXPE RIENC E" and breaking downstream
/// section detection entirely. Real word gaps (see the test below) run
/// noticeably higher (250+), so keeping this threshold well clear of the
/// letter-spacing range avoids false positives without missing genuine
/// word-gap-only PDFs.
pub(super) const TJ_WORD_GAP_THRESHOLD: f64 = 180.0;

/// True if a synthetic space is safe to insert immediately before `next`
/// (the just-decoded text of the run that follows a same-line Td/TD/Tm
/// jump or wide TJ kerning number — see `pending_space` in
/// `run_operations`). Correctly-typeset text never has a space directly
/// before closing punctuation — "Orsay, France", never "Orsay , France";
/// "(SISR)", never "(SISR )" — so if the next run starts with one of
/// these, the same-line jump that preceded it almost certainly wasn't a
/// real word gap at all, just a font/kerning-driven run split (observed in
/// this app's own Chromium print-to-PDF output: a new text-showing run
/// occasionally starts right at a punctuation glyph with no actual space
/// in the source text). Before this guard, that false positive compounded
/// every time our own rendered PDF was re-imported and re-rendered ("Orsay,
/// France" → "Orsay , France" → "Orsay  , France" → ...), an idempotence
/// bug. Deliberately narrow — only suppresses a space that would otherwise
/// be visibly wrong, never one before genuine word content.
pub(super) fn should_precede_with_space(next: &str) -> bool {
    !matches!(
        next.trim_start().chars().next(),
        Some(',' | '.' | ';' | ':' | '!' | '?' | ')' | ']' | '}' | '’' | '”' | '»' | '%')
    )
}

/// Decode a single PDF string operand's raw bytes to text, preferring the
/// active font's real encoding and falling back to naive byte-as-char
/// mapping if no encoding is known or decoding fails.
/// How many glyphs a Tj/TJ string operand represents, i.e. how many
/// character *codes* it contains — not how many Unicode characters its
/// decoded text has. These differ for ligature glyphs: a font's
/// ToUnicode CMap conventionally maps a single "ﬀ"/"ﬁ"/"ﬂ" ligature
/// glyph (one code, one character position in the PDF) to a 2-character
/// string like "ff" for copy/paste purposes. Using the decoded string's
/// `chars().count()` there wrongly looks like "a multi-character run",
/// which trips the word-gap heuristics in `run_operations` (both the
/// Td/TD same-line case and the TJ kerning-number case) into inserting a
/// bogus space around what is visually one contiguous word, e.g.
/// "offboarding" becomes "off boarding" — see
/// `run_operations_ligature_glyph_does_not_insert_spurious_space` below.
pub(super) fn glyph_count(
    bytes: &[u8],
    current_font: Option<&[u8]>,
    encodings: &std::collections::BTreeMap<Vec<u8>, ToUnicodeMap>,
) -> usize {
    let code_bytes = current_font
        .and_then(|f| encodings.get(f))
        .map(|c| c.code_bytes)
        .filter(|&n| n > 0)
        .unwrap_or(1);
    (bytes.len() / code_bytes).max(1)
}

pub(super) fn decode_bytes(
    bytes: &[u8],
    current_font: Option<&[u8]>,
    encodings: &std::collections::BTreeMap<Vec<u8>, ToUnicodeMap>,
) -> Option<String> {
    if let Some(font_name) = current_font {
        if let Some(cmap) = encodings.get(font_name) {
            if let Some(s) = cmap.decode(bytes) {
                return Some(s);
            }
        }
    }
    decode_bytes_fallback(bytes)
}

/// Decode a content stream's operations into positioned lines.
///
/// Text-showing operators (`Tj`/`TJ`/`'`/`"`) only ever give us the glyphs —
/// they say nothing about whether the next run of glyphs belongs on the same
/// line, a new line, or is just a separate word. That information lives in
/// the positioning operators (`Td`, `TD`, `T*`, `Tm`), so we track the text
/// cursor across those to decide whether to insert a space or start a new
/// line before the next shown text. Without this, PDFs that position each
/// word/field as its own run (common with design tools like Canva/Figma)
/// produce one long glued-together blob instead of readable lines.
///
/// Crucially, this also tracks the full graphics state — `q`/`Q` (save/
/// restore) and `cm` (concatenate to the current transformation matrix) —
/// and recurses into Form XObjects on `Do`, composing each one's placement
/// matrix into the running CTM. Without this, a glyph's "position" is just
/// whatever raw numbers the current text matrix happens to contain, which
/// for text living inside a Form XObject (very often used by design tools
/// to implement small repeated UI elements — e.g. one shared "skill tag"
/// pill badge invoked once per tag, each with its own placement transform)
/// is a position in that XObject's own local coordinate space, not the
/// page. Composing through the CTM at every level gives the true device
/// (page) position — the only sound way to reconstruct real reading order
/// for a multi-column layout afterward.
///
/// Maximum vertical (in text-space units) a same-line Td/TD/Tm jump may
/// move before it's treated as a new line instead. lopdf stores PDF "Real"
/// numbers as `f32`, and `object_to_f64` widens them to `f64` — so a PDF
/// value of `0.1` becomes `0.1f32 as f64 = 0.10000000149011612`, slightly
/// MORE than the f64 literal `0.1`. Comparing against the literal therefore
/// spuriously crossed the threshold for any parsed `0.1` (and made the
/// `>`--`>=` boundary unreachable: no PDF operand can produce an f64 value
/// of exactly `0.1`). Pin the threshold to the f32-widened value instead,
/// which is what the operands actually compare against.
const SAME_LINE_Y_EPSILON: f64 = 0.1f32 as f64;

/// `resources` / `encodings` are the (initially page-level) resources
/// dictionary and font ToUnicode-CMap map active for `ops`; both may be
/// swapped out for a Form XObject's own if it declares them (see the `Do`
/// case). `visited` is a stack (not a permanent set) of currently-open
/// XObject ids, purely to guard against a form recursively invoking itself;
/// the SAME shared XObject legitimately gets invoked many times at
/// different placements (like the repeated pill badges above), so it must
/// remain re-enterable once the earlier invocation has finished.
pub(super) fn run_operations(
    doc: &Document,
    ops: &[lopdf::content::Operation],
    resources: &lopdf::Dictionary,
    encodings: &std::collections::BTreeMap<Vec<u8>, ToUnicodeMap>,
    base_ctm: Matrix,
    visited: &mut Vec<lopdf::ObjectId>,
    lines: &mut Vec<PositionedLine>,
) {
    let mut ctm_stack: Vec<Matrix> = vec![base_ctm];
    let mut text_matrix = Matrix::identity();
    let mut current_text = String::new();
    let mut current_line_pos: Option<(f64, f64)> = None;
    let mut have_text = false;
    let mut current_font: Option<&[u8]> = None;
    // Number of characters decoded by the most recent Tj/TJ text-showing
    // operator. Used to decide whether a following same-line Td/TD/Tm
    // deserves a synthetic space (see the comment at those match arms).
    let mut last_run_chars: usize = 0;
    // Set by a same-line Td/TD/Tm jump (or a wide negative TJ kerning
    // number) that *might* stand in for a real space. Deliberately not
    // pushed into `current_text` immediately — the decision of whether it
    // actually was a word gap is deferred until the next run's decoded
    // text is known (see `should_precede_with_space`), since a run split
    // landing right on closing punctuation ("," ")" etc.) is never a real
    // space no matter how the position moved.
    let mut pending_space = false;

    for op in ops {
        match op.operator.as_str() {
            "q" => {
                let top = *ctm_stack.last().unwrap_or(&base_ctm);
                ctm_stack.push(top);
            }
            "Q" => {
                if ctm_stack.len() > 1 {
                    ctm_stack.pop();
                }
            }
            "cm" => {
                let vals: Vec<f64> = op.operands.iter().filter_map(object_to_f64).collect();
                if vals.len() == 6 {
                    let m =
                        Matrix::from_six([vals[0], vals[1], vals[2], vals[3], vals[4], vals[5]]);
                    if let Some(top) = ctm_stack.last_mut() {
                        *top = m.compose(top);
                    }
                }
            }
            "BT" => {
                // Flush any pending line using its already-captured device
                // position before resetting; NOTE: per spec BT resets the
                // text matrix to identity, but any pending line's position
                // was already computed correctly (composed with the CTM
                // active *when that line started*), so this reset doesn't
                // retroactively corrupt it.
                if have_text {
                    flush_line(lines, &mut current_text, &mut current_line_pos);
                    have_text = false;
                }
                text_matrix = Matrix::identity();
                last_run_chars = 0;
                pending_space = false;
            }
            "Tf" => {
                current_font = op.operands.first().and_then(|o| o.as_name().ok());
            }
            "Td" | "TD" => {
                let tx = op.operands.first().and_then(object_to_f64).unwrap_or(0.0);
                let ty = op.operands.get(1).and_then(object_to_f64).unwrap_or(0.0);
                let translate = Matrix {
                    a: 1.0,
                    b: 0.0,
                    c: 0.0,
                    d: 1.0,
                    e: tx,
                    f: ty,
                };
                text_matrix = translate.compose(&text_matrix);
                if have_text {
                    if ty.abs() > SAME_LINE_Y_EPSILON {
                        flush_line(lines, &mut current_text, &mut current_line_pos);
                        have_text = false;
                        pending_space = false;
                    } else if last_run_chars > 1 {
                        // The previous Tj/TJ rendered a whole multi-character
                        // run (typical of design-tool exports that position
                        // each *word* as its own run with no embedded space
                        // glyph) — this same-line jump likely is a word gap.
                        // Deferred (see `pending_space`'s doc comment): only
                        // actually inserted once we see whether the next run
                        // starts with something a space can legitimately
                        // precede.
                        pending_space = true;
                    }
                    // else: the previous run was a single glyph (typical of
                    // Chromium's print-to-PDF, which emits one Tj+Td per
                    // character, including a real space glyph for actual
                    // spaces). Inserting a synthetic space here as well
                    // would add a spurious extra space after every single
                    // character, gluing/breaking words apart (e.g. "V I N
                    // C E N T"). Real spaces already come through as their
                    // own decoded glyph, so nothing extra is needed.
                }
            }
            "T*" => {
                if have_text {
                    flush_line(lines, &mut current_text, &mut current_line_pos);
                    have_text = false;
                    pending_space = false;
                }
            }
            "Tm" => {
                let mut vals = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];
                for (i, slot) in vals.iter_mut().enumerate() {
                    if let Some(v) = op.operands.get(i).and_then(object_to_f64) {
                        *slot = v;
                    }
                }
                let new_tm = Matrix::from_six(vals);
                let dy = new_tm.f - text_matrix.f;
                text_matrix = new_tm;
                if have_text {
                    if dy.abs() > SAME_LINE_Y_EPSILON {
                        flush_line(lines, &mut current_text, &mut current_line_pos);
                        have_text = false;
                        pending_space = false;
                    } else if last_run_chars > 1 {
                        // See the matching comment in the Td/TD arm above.
                        pending_space = true;
                    }
                }
            }
            "Tj" => {
                if let Some(Object::String(bytes, _)) = op.operands.first() {
                    if let Some(s) = decode_bytes(bytes, current_font, encodings) {
                        if current_line_pos.is_none() {
                            let device = text_matrix.compose(ctm_stack.last().unwrap_or(&base_ctm));
                            current_line_pos = Some(device.origin());
                        }
                        if pending_space && should_precede_with_space(&s) {
                            current_text.push(' ');
                        }
                        pending_space = false;
                        last_run_chars = glyph_count(bytes, current_font, encodings);
                        current_text.push_str(&s);
                        have_text = true;
                    }
                }
            }
            "TJ" => {
                if let Some(Object::Array(arr)) = op.operands.first() {
                    for item in arr {
                        if let Object::String(bytes, _) = item {
                            if let Some(s) = decode_bytes(bytes, current_font, encodings) {
                                if current_line_pos.is_none() {
                                    let device =
                                        text_matrix.compose(ctm_stack.last().unwrap_or(&base_ctm));
                                    current_line_pos = Some(device.origin());
                                }
                                if pending_space && should_precede_with_space(&s) {
                                    current_text.push(' ');
                                }
                                pending_space = false;
                                last_run_chars = glyph_count(bytes, current_font, encodings);
                                current_text.push_str(&s);
                                have_text = true;
                            }
                        } else if let Some(n) = object_to_f64(item) {
                            // Large negative adjustment inside a TJ array
                            // usually stands in for a real space character.
                            // Deferred the same way as the Td/TD/Tm case
                            // above (see `pending_space`), since the very
                            // next item in this same array can be a string
                            // starting with closing punctuation.
                            if n < -TJ_WORD_GAP_THRESHOLD && have_text {
                                pending_space = true;
                            }
                        }
                    }
                }
            }
            "'" | "\"" => {
                if have_text {
                    flush_line(lines, &mut current_text, &mut current_line_pos);
                    have_text = false;
                }
                pending_space = false;
                if let Some(s) = op.operands.last().and_then(|o| match o {
                    Object::String(bytes, _) => decode_bytes(bytes, current_font, encodings),
                    _ => None,
                }) {
                    let device = text_matrix.compose(ctm_stack.last().unwrap_or(&base_ctm));
                    current_line_pos = Some(device.origin());
                    current_text.push_str(&s);
                    flush_line(lines, &mut current_text, &mut current_line_pos);
                    have_text = false;
                }
            }
            "Do" => {
                run_xobject(
                    doc,
                    op,
                    resources,
                    encodings,
                    ctm_stack.last().unwrap_or(&base_ctm),
                    visited,
                    lines,
                );
            }
            _ => {}
        }
    }
    if have_text {
        flush_line(lines, &mut current_text, &mut current_line_pos);
    }
}

/// Handle a single `Do` (XObject invocation) operator: resolve the named
/// XObject, and if it's a Form (not an Image, which has no text), recurse
/// into its content stream with the placement matrix (its own `/Matrix`
/// composed with the current CTM) and its own `/Resources` if it declares
/// any (falling back to the parent's otherwise).
pub(super) fn run_xobject(
    doc: &Document,
    op: &lopdf::content::Operation,
    resources: &lopdf::Dictionary,
    encodings: &std::collections::BTreeMap<Vec<u8>, ToUnicodeMap>,
    current_ctm: &Matrix,
    visited: &mut Vec<lopdf::ObjectId>,
    lines: &mut Vec<PositionedLine>,
) {
    let Some(Object::Name(name)) = op.operands.first() else {
        return;
    };
    let Ok(xobj_entry) = resources.get(b"XObject") else {
        return;
    };
    let Some(xobj_dict) = resolve_to_dict(doc, xobj_entry) else {
        return;
    };
    let Ok(Object::Reference(xobj_id)) = xobj_dict.get(name) else {
        return;
    };
    let xobj_id = *xobj_id;
    if visited.contains(&xobj_id) {
        return; // cycle guard: a form (in)directly invoking itself
    }
    let Ok(Object::Stream(form_stream)) = doc.get_object(xobj_id) else {
        return;
    };
    let is_form = form_stream
        .dict
        .get(b"Subtype")
        .ok()
        .map(|s| matches!(s, Object::Name(n) if n == b"Form"))
        .unwrap_or(false);
    if !is_form {
        return; // Image XObject or unrecognized: no text to extract.
    }

    let form_matrix = form_stream
        .dict
        .get(b"Matrix")
        .ok()
        .and_then(|m| {
            if let Object::Array(a) = m {
                Some(a)
            } else {
                None
            }
        })
        .and_then(|a| {
            let v: Vec<f64> = a.iter().filter_map(object_to_f64).collect();
            if v.len() == 6 {
                Some(Matrix::from_six([v[0], v[1], v[2], v[3], v[4], v[5]]))
            } else {
                None
            }
        })
        .unwrap_or_else(Matrix::identity);
    let new_base_ctm = form_matrix.compose(current_ctm);
    let form_resources_entry = form_stream.dict.get(b"Resources").ok().cloned();

    let mut fs = form_stream.clone();
    let _ = fs.decompress();
    let Ok(sub_content) = fs.decode_content() else {
        return;
    };

    visited.push(xobj_id);
    match form_resources_entry
        .as_ref()
        .and_then(|r| resolve_to_dict(doc, r))
    {
        Some(fr_dict) => {
            let form_encodings = build_font_cmaps_from_resources(doc, fr_dict);
            run_operations(
                doc,
                &sub_content.operations,
                fr_dict,
                &form_encodings,
                new_base_ctm,
                visited,
                lines,
            );
        }
        None => {
            run_operations(
                doc,
                &sub_content.operations,
                resources,
                encodings,
                new_base_ctm,
                visited,
                lines,
            );
        }
    }
    visited.pop();
}

/// Finish the line currently being accumulated (if it has any content) and
/// push it, tagged with the position of its first glyph.
pub(super) fn flush_line(
    lines: &mut Vec<PositionedLine>,
    current_text: &mut String,
    line_pos: &mut Option<(f64, f64)>,
) {
    let trimmed = current_text.trim();
    if !trimmed.is_empty() {
        let (x, y) = line_pos.unwrap_or((0.0, 0.0));
        lines.push(PositionedLine {
            x,
            y,
            text: trimmed.to_string(),
        });
    }
    current_text.clear();
    *line_pos = None;
}

/// Naive fallback text decoding: treat each raw byte as a Unicode codepoint.
/// This is only correct for plain ASCII/Latin-1-ish simple encodings; it is
/// used when we have no better font encoding to decode with.
pub(super) fn decode_bytes_fallback(bytes: &[u8]) -> Option<String> {
    if bytes.len() >= 2 && bytes[0] == 0xFE && bytes[1] == 0xFF {
        let u16s: Vec<u16> = bytes[2..]
            .chunks(2)
            .filter_map(|c| {
                if c.len() == 2 {
                    Some(u16::from_be_bytes([c[0], c[1]]))
                } else {
                    None
                }
            })
            .collect();
        Some(String::from_utf16_lossy(&u16s))
    } else if bytes.len() >= 2 && bytes[0] == 0xFF && bytes[1] == 0xFE {
        let u16s: Vec<u16> = bytes[2..]
            .chunks(2)
            .filter_map(|c| {
                if c.len() == 2 {
                    Some(u16::from_le_bytes([c[0], c[1]]))
                } else {
                    None
                }
            })
            .collect();
        Some(String::from_utf16_lossy(&u16s))
    } else {
        let s: String = bytes.iter().map(|&b| b as char).collect();
        if s.trim().is_empty() {
            None
        } else {
            Some(s)
        }
    }
}

/// Quick heuristic check for encryption markers in PDF bytes.
pub(super) fn is_encrypted(bytes: &[u8]) -> bool {
    let text = String::from_utf8_lossy(bytes);
    text.contains("/Encrypt")
        && (text.contains("/StdID") || text.contains("/O ") || text.contains("/U "))
}

/// Raw content stream parser: extracts string literals from BT/ET blocks.
pub(super) fn decode_content_raw(content: &[u8]) -> String {
    let mut result = String::new();
    let mut i = 0;
    let len = content.len();
    while i < len {
        // Look for BT (Begin Text)
        if i + 1 < len && content[i] == b'B' && content[i + 1] == b'T' {
            i += 2;
            // Parse until ET
            while i < len {
                if i + 1 < len && content[i] == b'E' && content[i + 1] == b'T' {
                    i += 2;
                    break;
                }
                // Extract (...) string literal
                if content[i] == b'(' {
                    i += 1;
                    let mut depth = 1u32;
                    let start = i;
                    while i < len && depth > 0 {
                        match content[i] {
                            b'(' => depth += 1,
                            b')' => {
                                depth -= 1;
                                if depth == 0 {
                                    break;
                                }
                            }
                            b'\\' => {
                                i += 1;
                            } // skip escape
                            _ => {}
                        }
                        i += 1;
                    }
                    let raw = &content[start..i];
                    if depth == 0 {
                        i += 1;
                    } // skip closing )
                      // Decode the raw bytes to string, skip non-printable
                    let decoded = String::from_utf8_lossy(raw);
                    let cleaned: String = decoded
                        .chars()
                        .filter(|c| !c.is_control() || *c == '\n')
                        .collect();
                    if !cleaned.trim().is_empty() {
                        result.push_str(cleaned.trim());
                        result.push(' ');
                    }
                }
                // Extract <...> hex string
                else if content[i] == b'<' {
                    i += 1;
                    while i < len && content[i] != b'>' {
                        i += 1;
                    }
                    if i < len {
                        i += 1;
                    }
                }
                // Anything else (including '[' / ']' array delimiters, which
                // need no special handling): just advance.
                else {
                    i += 1;
                }
            }
        } else {
            i += 1;
        }
    }
    // Clean up: collapse whitespace, remove lone punctuation
    result.split_whitespace().collect::<Vec<&str>>().join(" ")
}
