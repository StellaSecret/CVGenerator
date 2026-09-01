use crate::models::*;
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
struct PositionedLine {
    x: f64,
    y: f64,
    text: String,
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
struct ToUnicodeMap {
    code_bytes: usize,
    map: std::collections::HashMap<u32, String>,
}

impl ToUnicodeMap {
    fn decode(&self, bytes: &[u8]) -> Option<String> {
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

fn bytes_to_u32(b: &[u8]) -> u32 {
    b.iter().fold(0u32, |acc, &x| (acc << 8) | x as u32)
}

fn utf16be_bytes_to_string(b: &[u8]) -> String {
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

fn parse_hex_token(tok: &str) -> Option<Vec<u8>> {
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
fn parse_tounicode_cmap(text: &str) -> Option<ToUnicodeMap> {
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
struct Matrix {
    a: f64,
    b: f64,
    c: f64,
    d: f64,
    e: f64,
    f: f64,
}

impl Matrix {
    fn identity() -> Self {
        Matrix {
            a: 1.0,
            b: 0.0,
            c: 0.0,
            d: 1.0,
            e: 0.0,
            f: 0.0,
        }
    }

    fn from_six(v: [f64; 6]) -> Self {
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
    fn compose(&self, other: &Matrix) -> Matrix {
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
    fn origin(&self) -> (f64, f64) {
        (self.e, self.f)
    }
}

/// Resolve a PDF value that may be either an inline Dictionary or a
/// Reference to one.
fn resolve_to_dict<'a>(doc: &'a Document, obj: &'a Object) -> Option<&'a lopdf::Dictionary> {
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
fn build_font_cmaps_from_resources(
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
        stream.decompress();
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
fn build_font_cmaps(
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
        stream.decompress();
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
fn extract_text_from_page(doc: &Document, page_id: lopdf::ObjectId) -> String {
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
            s.decompress();
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
fn group_into_rows(lines: &[PositionedLine], y_epsilon: f64) -> Vec<Vec<PositionedLine>> {
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
fn row_repr_x(row: &[PositionedLine]) -> f64 {
    row.iter().fold(f64::INFINITY, |acc, l| acc.min(l.x))
}

/// A row's y (all its fragments sit within `y_epsilon` of each other by
/// construction, so the first is representative enough).
fn row_y(row: &[PositionedLine]) -> f64 {
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
fn find_column_split(rows: &[Vec<PositionedLine>], min_rows: usize, min_gap: f64) -> Option<f64> {
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
fn object_to_f64(obj: &Object) -> Option<f64> {
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
const TJ_WORD_GAP_THRESHOLD: f64 = 180.0;

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
fn should_precede_with_space(next: &str) -> bool {
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
fn glyph_count(
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

fn decode_bytes(
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
/// `resources` / `encodings` are the (initially page-level) resources
/// dictionary and font ToUnicode-CMap map active for `ops`; both may be
/// swapped out for a Form XObject's own if it declares them (see the `Do`
/// case). `visited` is a stack (not a permanent set) of currently-open
/// XObject ids, purely to guard against a form recursively invoking itself;
/// the SAME shared XObject legitimately gets invoked many times at
/// different placements (like the repeated pill badges above), so it must
/// remain re-enterable once the earlier invocation has finished.
fn run_operations(
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
                    if ty.abs() > 0.1 {
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
                    if dy.abs() > 0.1 {
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
fn run_xobject(
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
    fs.decompress();
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
fn flush_line(
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
fn decode_bytes_fallback(bytes: &[u8]) -> Option<String> {
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
fn is_encrypted(bytes: &[u8]) -> bool {
    let text = String::from_utf8_lossy(bytes);
    text.contains("/Encrypt")
        && (text.contains("/StdID") || text.contains("/O ") || text.contains("/U "))
}

/// Raw content stream parser: extracts string literals from BT/ET blocks.
fn decode_content_raw(content: &[u8]) -> String {
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

// ── CV Text Parser ──────────────────────────────────────────────────────────

/// Known section headers (EN + FR) mapped to internal section names.
const SECTION_HEADERS: &[(&str, &str)] = &[
    // Experience
    ("experience", "experience"),
    ("experiences", "experience"),
    ("work experience", "experience"),
    ("work history", "experience"),
    ("employment", "experience"),
    ("professional experience", "experience"),
    ("parcours professionnel", "experience"),
    ("expérience professionnelle", "experience"),
    ("expérience", "experience"),
    // Plural "Expériences" — as distinct a heading in the wild as the
    // singular form; French resume templates use both interchangeably.
    ("expériences", "experience"),
    ("emplois", "experience"),
    // Education
    ("education", "education"),
    ("academic background", "education"),
    ("academic experience", "education"),
    ("formation", "education"),
    // Plural "Formations" — same rationale as "Expériences" above.
    ("formations", "education"),
    ("éducation", "education"),
    ("parcours académique", "education"),
    // Skills
    ("skills", "skills"),
    ("technical skills", "skills"),
    ("competencies", "skills"),
    ("core competencies", "skills"),
    ("compétences", "skills"),
    ("compétences techniques", "skills"),
    ("compétence", "skills"),
    ("compétences clés", "skills"),
    // Projects
    ("projects", "projects"),
    ("personal projects", "projects"),
    ("side projects", "projects"),
    ("projets", "projects"),
    ("projets personnels", "projects"),
    // Certifications
    ("certifications", "certifications"),
    ("certificates", "certifications"),
    ("licenses", "certifications"),
    ("certificats", "certifications"),
    // Languages
    ("languages", "languages"),
    ("langues", "languages"),
    // A combined "Languages & Interests" heading is common enough in French
    // templates (interests/hobbies get a couple of lines tacked onto the
    // same section as languages, rather than their own heading) that it's
    // worth recognizing directly rather than letting it fall through
    // unrecognized and bleed into whatever section came before it.
    ("langues et centres d'intérêt", "languages"),
    ("langues et centres d'intérêts", "languages"),
    // Sections we recognize but intentionally don't import into any CV field
    // yet — mapping them here just stops their content from bleeding into
    // whatever the previous real section was (e.g. "OTHERS"/"INTERESTS"
    // text getting appended onto Certifications).
    ("summary", "ignore"),
    ("professional summary", "ignore"),
    ("résumé", "ignore"),
    ("profil", "ignore"),
    ("values", "ignore"),
    ("valeurs", "ignore"),
    ("other", "ignore"),
    ("others", "ignore"),
    ("autres", "ignore"),
    ("interests", "ignore"),
    ("hobbies", "ignore"),
    ("centres d'intérêt", "ignore"),
    ("random skills", "ignore"),
];

/// Regex-ish helpers (no regex crate — keep it simple).
pub(crate) fn extract_email(text: &str) -> Option<String> {
    for word in text.split_whitespace() {
        let w = word.trim_matches(['<', '>', ',', ';', '(', ')']);
        if w.contains('@') && w.contains('.') && !w.starts_with('@') && !w.ends_with('.') {
            let email: String = w
                .chars()
                .filter(|c| {
                    c.is_alphanumeric()
                        || *c == '@'
                        || *c == '.'
                        || *c == '_'
                        || *c == '-'
                        || *c == '+'
                })
                .collect();
            if email.contains('@') && email.contains('.') {
                return Some(email);
            }
        }
    }
    None
}

pub(crate) fn extract_phone(text: &str) -> Option<String> {
    // Scan the whole text for phone-like patterns
    let cleaned: String = text
        .chars()
        .map(|c| {
            if c.is_ascii_digit() || c == '+' {
                c
            } else {
                ' '
            }
        })
        .collect();
    // Look for sequences of 7-15 digits (with optional leading +)
    let words: Vec<&str> = cleaned.split_whitespace().collect();
    if words.is_empty() {
        return None;
    }
    // Build a candidate: start with + if original text has it
    let has_plus = text.contains('+');
    let digits: String = words
        .join("")
        .chars()
        .filter(|c| c.is_ascii_digit())
        .collect();
    if digits.len() >= 7 && digits.len() <= 15 {
        return if has_plus {
            Some(format!("+{}", digits))
        } else {
            Some(digits)
        };
    }
    None
}

pub(crate) fn extract_urls(text: &str) -> (Option<String>, Option<String>, Option<String>) {
    let mut linkedin = None;
    let mut github = None;
    let mut website = None;
    for word in text.split_whitespace() {
        let w = word.trim_matches(['<', '>', ',', ';', '(', ')']);
        if w.is_empty() {
            continue;
        }
        if linkedin.is_none() && (w.contains("linkedin.com") || w.contains("linkedin.")) {
            linkedin = Some(w.to_string());
        } else if github.is_none() && w.contains("github.com") {
            // Require the actual profile/repo domain (github.com), not any
            // domain that merely contains "github." — e.g. a personal site
            // hosted at "name.github.io" is a website, not a GitHub profile.
            github = Some(w.to_string());
        } else if website.is_none()
            && w != linkedin.as_deref().unwrap_or("")
            && (w.starts_with("http://")
                || w.starts_with("https://")
                || w.starts_with("www.")
                || looks_like_bare_domain(w))
        {
            website = Some(w.to_string());
        }
    }
    (linkedin, github, website)
}

/// Heuristic check for a bare domain/URL with no scheme or "www." prefix,
/// e.g. "falltrades.github.io/engineering" — common when a contact line uses
/// an icon glyph instead of "http(s)://" before the URL.
fn looks_like_bare_domain(w: &str) -> bool {
    let host = w.split('/').next().unwrap_or(w);
    if !host.contains('.') || host.starts_with('.') || host.ends_with('.') {
        return false;
    }
    if host.contains('@') {
        return false;
    }
    let known_tlds = [
        ".com", ".io", ".dev", ".net", ".org", ".me", ".fr", ".co", ".app", ".tech", ".xyz",
        ".info", ".site",
    ];
    known_tlds
        .iter()
        .any(|tld| host.to_lowercase().ends_with(tld))
        && host
            .chars()
            .all(|c| c.is_alphanumeric() || c == '.' || c == '-')
}

/// Detect which section a header line belongs to.
/// Max length of the same-line remainder after "Header:"/"Header —" for it
/// to still count as a section heading. Real section headers occasionally
/// carry a short same-line note (e.g. "Skills: React, Go, AWS"), but a body
/// line that merely happens to *start* with a word that's also a section
/// keyword — e.g. our own renderer's "Other:" sub-label for uncategorized
/// skills, followed by the full comma-separated skills list — is not a
/// heading and must not swallow the rest of the section as if it were one.
const SECTION_HEADER_INLINE_CONTENT_LIMIT: usize = 40;

fn detect_section(line: &str) -> Option<&'static str> {
    let trimmed = line.trim();
    let lower = trimmed.to_lowercase();
    for (header, section) in SECTION_HEADERS {
        if lower == *header {
            return Some(section);
        }
        for sep in [":", " —"] {
            let prefix = format!("{header}{sep}");
            if let Some(rest) = lower.strip_prefix(&prefix) {
                if rest.trim().len() <= SECTION_HEADER_INLINE_CONTENT_LIMIT {
                    return Some(section);
                }
            }
        }
    }
    None
}

/// True for a short, plain, standalone line that's plausibly a job title
/// sitting on its own line right after a "Company · Location  Start – End"
/// header row (see the comment in `parse_experiences` where this is used).
/// Deliberately conservative: only used to disambiguate a layout question
/// for the line immediately following a freshly-detected date range, so
/// false negatives just fall back to the older role-first interpretation
/// rather than misfiring on unrelated content further down the page.
fn looks_like_bare_role_line(line: &str) -> bool {
    let trimmed = line.trim();
    if trimmed.is_empty() || trimmed.chars().count() > 100 {
        return false;
    }
    if trimmed.starts_with(['•', '·', '-', '–', '*', '▸', '▪']) {
        return false;
    }
    let lower = trimmed.to_lowercase();
    if lower.starts_with("project ")
        || lower.starts_with("situation")
        || lower.starts_with("tasks")
        || lower.starts_with("techs")
        || lower.starts_with("tools")
    {
        return false;
    }
    // A real role title is short and standalone, not a sentence — bail if
    // it contains ". " (mid-sentence period) suggesting running prose.
    if trimmed.contains(". ") {
        return false;
    }
    // A real role title is capitalized ("Architecte DevOps",
    // "Administrateur système WebOps") — it's never the wrapped remainder
    // of a sentence that started on the PREVIOUS (now-scrolled-off) line,
    // which — in French and English alike — almost always resumes on a
    // lowercase word (e.g. "...concernant les" wrapping onto "volets
    // sécurité et conformité"). That wrapped-tail case is otherwise
    // indistinguishable from a genuine short title by every check above
    // (short, no bullet marker, no mid-sentence period, no date) — it was
    // previously mistaken for the NEXT job's role, silently swallowing the
    // real role line into that job's body text instead. Checking the
    // first letter's case catches it without needing to know anything
    // about what came before.
    if trimmed.chars().next().is_some_and(|c| c.is_lowercase()) {
        return false;
    }
    extract_date_range_from_end(trimmed).is_none()
        && extract_standalone_date_range(trimmed).is_none()
}

/// Split a "Company · Location" (or "Company, Location" / "Company |
/// Location") string into its two parts. Falls back to treating the whole
/// string as the company with an empty location if no separator is found.
fn split_company_and_location(text: &str) -> (String, String) {
    for sep in [" · ", " | ", ", "] {
        if let Some(pos) = text.find(sep) {
            return (
                text[..pos].trim().to_string(),
                text[pos + sep.len()..].trim().to_string(),
            );
        }
    }
    (text.trim().to_string(), String::new())
}

/// Find a date range at the END of an experience line.
/// Returns (start_date, end_date) and the separator used.
/// E.g. "Software Engineer at Acme - Jan 2021 - Present" → ("Jan 2021", "Present")
/// Also handles a start date with no separator of its own before it, only
/// whitespace — e.g. this app's own "Company · Location  December 2024 –
/// February 2026" row layout (company/location and the date range aren't
/// dash-separated at all; only the two dates are).
fn extract_date_range_from_end(line: &str) -> Option<(String, String)> {
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
fn tokenize_with_spans(line: &str) -> Vec<(usize, usize, &str)> {
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
fn find_date_range_span(line: &str) -> Option<(usize, usize, String, String)> {
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
fn extract_trailing_date_range_from_title(line: &str) -> Option<(String, String, String)> {
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
fn extract_date_range(line: &str) -> Option<(String, String)> {
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

/// Extract a name from the first few lines. Heuristic: first non-empty line
/// that doesn't look like a header/section/contact info.
fn guess_name(lines: &[&str]) -> Option<String> {
    // Defensive belt-and-suspenders alongside the ToUnicodeMap fix above:
    // strip any stray control characters before validating the line as a
    // name, rather than requiring the *entire* line to already be clean.
    // A single leftover corrupted byte (font-encoding edge cases aren't
    // fully eliminable) would otherwise fail the alphabetic check below
    // and cause the whole name line to be skipped — falling through to
    // the next candidate line, e.g. the job title, and silently
    // replacing the person's name with their title.
    let cleaned: Vec<String> = lines
        .iter()
        .map(|l| l.chars().filter(|c| !c.is_control()).collect::<String>())
        .collect();
    for line in cleaned.iter().take(5) {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if detect_section(trimmed).is_some() {
            continue;
        }
        if extract_email(trimmed).is_some() || extract_phone(trimmed).is_some() {
            continue;
        }
        if trimmed.starts_with("http")
            || trimmed.starts_with("www")
            || trimmed.starts_with("linkedin")
        {
            continue;
        }
        // Likely a name: 2-4 words, mostly alphabetic
        let words: Vec<&str> = trimmed.split_whitespace().collect();
        if words.len() >= 2
            && words.len() <= 5
            && words.iter().all(|w| {
                w.chars().all(|c| {
                    c.is_alphabetic()
                        || c == '.'
                        || c == '-'
                        || c == '\''
                        || c == 'é'
                        || c == 'è'
                        || c == 'ê'
                        || c == 'ë'
                        || c == 'à'
                        || c == 'â'
                        || c == 'ç'
                        || c == 'ô'
                        || c == 'ù'
                        || c == 'û'
                        || c == 'ü'
                        || c == 'ï'
                        || c == 'î'
                })
            })
        {
            return Some(trimmed.to_string());
        }
    }
    None
}

/// Try to identify a professional title from early lines (after name).
fn guess_title(lines: &[&str]) -> Option<String> {
    let name_line_idx = lines.iter().take(5).position(|l| {
        let t = l.trim();
        !t.is_empty()
            && extract_email(t).is_none()
            && extract_phone(t).is_none()
            && !t.starts_with("http")
    });
    let start_after = name_line_idx.map(|i| i + 1).unwrap_or(0);

    let title_keywords = [
        "engineer",
        "developer",
        "architect",
        "manager",
        "lead",
        "director",
        "consultant",
        "analyst",
        "scientist",
        "designer",
        "ingénieur",
        "développeur",
        "architecte",
        "manager",
        "chef",
        "directeur",
        "consultant",
        "analyste",
        "scientifique",
        "concepteur",
    ];

    for line in &lines[start_after..lines.len().min(start_after + 4)] {
        let trimmed = line.trim();
        let lower = trimmed.to_lowercase();
        if title_keywords.iter().any(|kw| lower.contains(kw)) {
            return Some(trimmed.to_string());
        }
    }
    None
}

/// Split text into sections based on detected headers.
fn split_into_sections(text: &str) -> Vec<(&str, Vec<String>)> {
    let lines: Vec<&str> = text.lines().collect();
    let mut sections: Vec<(&str, Vec<String>)> = Vec::new();
    let mut current_section = "header";
    let mut current_lines = Vec::new();

    for i in 0..lines.len() {
        let trimmed = lines[i].trim();
        if trimmed.is_empty() {
            continue;
        }

        // "Compétences" alone is a recognized (French) synonym for the
        // Skills section — correctly so, since most resumes that use it
        // mean exactly that. But some resumes instead use it as the first
        // half of an unrelated two-line label, "Compétences Globales"
        // ("Global Competencies" — a soft-skills self-rating box, often
        // sitting in a sidebar next to Experience, distinct from that same
        // resume's *actual* skills section elsewhere). Bail out of
        // treating it as a header in that specific case — a false
        // section-boundary here doesn't just mislabel a couple of lines,
        // it truncates whatever section was legitimately still open
        // (commonly Experience) right in the middle of it.
        let is_competences_globales_false_positive = trimmed.to_lowercase() == "compétences"
            && lines[i + 1..]
                .iter()
                .map(|l| l.trim())
                .find(|l| !l.is_empty())
                .is_some_and(|next| next.to_lowercase() == "globales");

        if is_competences_globales_false_positive {
            current_lines.push(trimmed.to_string());
            continue;
        }

        // Recover from a section state that's drifted away from
        // "experience" due to an interrupting sidebar (e.g. a self-rating
        // "Values"/"Core Competencies" box, or a per-job "Tools" list)
        // whose own headers matched real section keywords and permanently
        // flipped `current_section` — with nothing in a purely
        // header-driven state machine to ever flip it back. Concretely:
        // once such a sidebar drags `current_section` to "skills" (or a
        // similar unrelated section), every later job's role, company,
        // and full narrative silently end up filed under "skills" for the
        // rest of the document, rather than as their own Experience
        // entries — a bigger loss than the sidebar mixing into "skills"
        // in the first place.
        //
        // A standalone icon-prefixed date range (this resume's own job-
        // header row shape — see `extract_standalone_date_range`) is a
        // strong, low-false-positive signal that we've reached a new
        // job's header, wherever `current_section` currently claims to
        // be. When we see one outside "experience", recover this job's
        // role+company by scanning backward through whatever accumulated
        // in `current_lines` for the last two lines that don't look like
        // sidebar tool/skill bleed (a category label, a bullet, or a
        // "<tool> N+ yrs" line) — skipping right over an interposed Tools
        // sidebar to reach the actual title+company lines that preceded
        // it. Everything else pending is flushed to the old section as
        // usual (it's mostly genuine skill/tag content anyway).
        //
        // This can produce more than one ("experience", ...) tuple in the
        // returned list (one per resumption); callers that key off section
        // name must merge same-named tuples rather than assume each name
        // appears once — see `merge_duplicate_sections`.
        if current_section != "experience" && extract_standalone_date_range(trimmed).is_some() {
            let mut idx = current_lines.len();
            let mut recovered: Vec<String> = Vec::new();
            while idx > 0 && recovered.len() < 2 {
                idx -= 1;
                if looks_like_tool_bleed_line(&current_lines[idx]) {
                    continue;
                }
                recovered.push(current_lines[idx].clone());
            }
            if recovered.len() == 2 {
                // A recovered pair naming a *project* sub-header (e.g.
                // "Project 2: CITADEL – Platform Engineering"), rather
                // than a job's actual role+company, means we've landed on
                // one of that job's *project* date lines, not the job's
                // own header — `current_lines` at this point is still
                // just as jumbled as it was going in, so don't commit to
                // a bogus "experience" entry here. Leave current_section
                // as-is and keep accumulating; the job's real role+company
                // lines are further back than this scan reached, and a
                // later, real job-header date line will find them once
                // this project's own content also becomes part of the
                // (still wrong) pending run.
                let looks_like_project_subheader = recovered
                    .iter()
                    .any(|l| l.trim_start().to_lowercase().starts_with("project"));
                if !looks_like_project_subheader {
                    recovered.reverse();
                    if !current_lines.is_empty() || current_section != "header" {
                        sections.push((current_section, std::mem::take(&mut current_lines)));
                    }
                    current_section = "experience";
                    current_lines = recovered;
                    current_lines.push(trimmed.to_string());
                    continue;
                }
            }
        }

        if let Some(section) = detect_section(trimmed) {
            if !current_lines.is_empty() || current_section != "header" {
                sections.push((current_section, std::mem::take(&mut current_lines)));
            }
            current_section = section;
        } else {
            current_lines.push(trimmed.to_string());
        }
    }
    if !current_lines.is_empty() {
        sections.push((current_section, current_lines));
    }
    sections
}

/// Merges tuples that share the same section name into one, preserving
/// each name's first-occurrence position and concatenating their lines in
/// order. `split_into_sections` can legitimately emit more than one tuple
/// for the same name — most commonly "experience" when its
/// resumption-after-interruption recovery (see the comment on that logic)
/// fires more than once — and every downstream consumer that does
/// `match section { "experience" => cv.experiences = parse_experiences(lines), ... }`
/// would otherwise silently keep only the *last* such tuple's content,
/// discarding all the earlier ones it just went to the trouble of
/// recovering.
fn merge_duplicate_sections<'a>(
    sections: Vec<(&'a str, Vec<String>)>,
) -> Vec<(&'a str, Vec<String>)> {
    let mut merged: Vec<(&'a str, Vec<String>)> = Vec::new();
    for (name, lines) in sections {
        if let Some(existing) = merged.iter_mut().find(|(n, _)| *n == name) {
            existing.1.extend(lines);
        } else {
            merged.push((name, lines));
        }
    }
    merged
}

/// A single-line, lookahead-free approximation of "this looks like sidebar
/// tool/skill bleed" — used by `split_into_sections`'s experience-resumption
/// recovery (see the comment above its call site) to scan backward past an
/// interposed Tools/skills sidebar and find the real role+company lines
/// that preceded it. Deliberately permissive: a false positive here just
/// means the backward scan keeps looking a little further, which is
/// harmless, whereas a false negative could grab a stray tool name as if
/// it were a job title.
fn looks_like_tool_bleed_line(line: &str) -> bool {
    let trimmed = line.trim();
    if trimmed.starts_with(['•', '·', '-', '–', '*', '▸', '▪']) {
        return true;
    }
    harvest_skill_segments(trimmed).is_some() || is_bare_years_marker(trimmed)
}

/// Month names (English + French) used to sanity-check that a token really
/// looks like the start of a date, not just any word.
const MONTH_NAMES: &[&str] = &[
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

fn looks_like_date_token(s: &str) -> bool {
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
fn extract_standalone_date_range(line: &str) -> Option<(String, String, Option<String>)> {
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
        i += 1;
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
const MONTH_ABBREVIATIONS: &[&str] = &[
    "jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "sept", "oct", "nov", "dec",
    "janv", "févr", "fevr", "mars", "avr", "juil", "juin", "aout", "août", "déc",
];

fn looks_like_date_token_loose(s: &str) -> bool {
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
fn extract_standalone_date_range_loose(line: &str) -> Option<(String, String)> {
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

/// Keywords that signal an "institution" line (as opposed to a degree- or
/// field-of-study line) in an education entry.
const INSTITUTION_KEYWORDS: &[&str] = &[
    "university",
    "université",
    "universite",
    "école",
    "ecole",
    "institut",
    "institute",
    "college",
    "collège",
    "faculty",
    "faculté",
    "faculte",
    "lycée",
    "lycee",
    // "IUT" (Institut Universitaire de Technologie) is the common French
    // abbreviation — sufficiently common and unambiguous as a school name
    // opener that it's checked separately below rather than folded into
    // this substring list (a bare 3-letter acronym is too easy to
    // false-positive on if matched anywhere in the line; see
    // `looks_like_institution_line`).
];

fn looks_like_institution_line(line: &str) -> bool {
    let lower = line.to_lowercase();
    lower.starts_with("iut ")
        || lower == "iut"
        || INSTITUTION_KEYWORDS.iter().any(|kw| lower.contains(kw))
}

/// Keywords that open a degree line ("Licence Professionnelle...", "BTS
/// Services...", "Master of Science...") — used to detect where a NEW
/// education entry starts even when nothing else (a date range) marks the
/// boundary. Some resumes list every degree with no dates at all, in which
/// case `parse_education`'s only other entry-boundary signals never fire,
/// and every line — across every degree — piles into a single buffer that
/// then gets mis-split as one garbled entry (see the call site).
const DEGREE_KEYWORDS: &[&str] = &[
    "licence",
    "bachelor",
    "master",
    "mba",
    "bts",
    "dut",
    "phd",
    "ph.d",
    "doctorate",
    "doctorat",
    "baccalauréat",
    "baccalaureat",
    "diplôme",
    "diplome",
    "diploma",
    "magistère",
    "magistere",
    "associate degree",
    "certificat",
];

fn looks_like_degree_line(line: &str) -> bool {
    let lower = line.trim().to_lowercase();
    DEGREE_KEYWORDS.iter().any(|kw| lower.starts_with(kw))
}

/// Build one Education entry from a buffer of plain lines that preceded a
/// date range. Layout: [degree line] [optional field-of-study line(s),
/// which may wrap] [institution line(s), which may also wrap across a
/// trailing city/country line]. The institution is identified by keyword
/// (e.g. "University"); everything between the degree and that point is the
/// field of study. If no institution keyword is found, the last line is
/// used as a fallback institution.
fn build_education_from_buffer(
    buffer: &[String],
    start_year: String,
    end_year: String,
) -> Option<Education> {
    if buffer.is_empty() {
        return None;
    }
    // Institution-first layout: "University of X, Location" then a degree
    // line, then (elsewhere) the date — the reverse order from the
    // degree-first layout this function otherwise assumes. Identified by
    // content (an institution keyword), not position, since which comes
    // first varies by PDF/renderer. Delegate to the dedicated
    // institution-first builder rather than duplicating its degree/field
    // splitting here.
    if looks_like_institution_line(&buffer[0]) {
        return build_education_institution_first(
            buffer[0].clone(),
            start_year,
            end_year,
            &buffer[1..],
        );
    }
    // The degree line may itself embed the field of study on a single line,
    // e.g. "Bachelor of Science in Computer Science" or "Licence en Droit".
    let first = &buffer[0];
    let lower_first = first.to_lowercase();
    let (degree, embedded_field) = if let Some(pos) = first.find(" in ") {
        (
            first[..pos].trim().to_string(),
            Some(first[pos + 4..].trim().to_string()),
        )
    } else if let Some(pos) = lower_first.find(" en ") {
        (
            first[..pos].trim().to_string(),
            Some(first[pos + 4..].trim().to_string()),
        )
    } else {
        (first.clone(), None)
    };

    let rest = &buffer[1..];
    let inst_idx = rest.iter().position(|l| looks_like_institution_line(l));
    let (field_lines, institution_lines): (Vec<String>, Vec<String>) = match inst_idx {
        Some(idx) => {
            // Lines AFTER the institution line default to being folded
            // into `institution_lines` too, on the assumption that they're
            // a wrapped continuation of the institution's own name/address
            // (e.g. a trailing city/country). But a resume can just as
            // easily put the field-of-study line AFTER the institution
            // instead of before it (layouts vary) — recognizable because,
            // unlike an address continuation, it starts lowercase (a
            // specialization clause, e.g. this app's own French "option
            // Solutions ...", vs. a capitalized proper noun like "Boston,
            // MA"). Stop folding as soon as one of those appears, and
            // treat it — and everything after — as field instead. Without
            // this, a trailing field-after-institution line permanently
            // merged into the institution name (and the field itself came
            // out empty).
            let mut inst_end = idx + 1;
            while inst_end < rest.len()
                && rest[inst_end]
                    .chars()
                    .next()
                    .is_some_and(|c| c.is_uppercase())
            {
                inst_end += 1;
            }
            let mut field: Vec<String> = rest[..idx].to_vec();
            field.extend(rest[inst_end..].iter().cloned());
            (field, rest[idx..inst_end].to_vec())
        }
        None if rest.is_empty() => (Vec::new(), Vec::new()),
        None => (
            rest[..rest.len() - 1].to_vec(),
            rest[rest.len() - 1..].to_vec(),
        ),
    };

    let mut field_parts: Vec<String> = embedded_field
        .into_iter()
        .filter(|f| !f.is_empty())
        .collect();
    field_parts.extend(field_lines.iter().cloned());

    Some(Education {
        id: uuid::Uuid::new_v4().to_string(),
        institution: institution_lines.join(" ").trim().to_string(),
        degree: LocalizedText::same(degree),
        field: LocalizedText::same(field_parts.join(" ").trim()),
        start_year,
        end_year,
        ..Default::default()
    })
}

/// Common "block label" prefixes used in structured resume bullet groups
/// (e.g. "Situation & Tasks: ...", "Techs: Kubernetes, Docker, ..."). A line
/// starting with one of these should always be treated as the start of a new
/// block, never as a wrapped continuation of the previous bullet.
const BLOCK_LABEL_PREFIXES: &[&str] = &[
    "situation",
    "context",
    "task",
    "action",
    "result",
    "achievement",
    "techs",
    "tech stack",
    "technologies",
    "duties",
    "duty",
];

pub(crate) fn is_project_header(line: &str) -> bool {
    let lower = line.to_lowercase();
    lower.starts_with("project") || lower.starts_with("projet")
}

fn is_context_label(line: &str) -> bool {
    let lower = line.to_lowercase();
    let Some(colon_idx) = lower.find(':') else {
        return false;
    };
    let prefix = lower[..colon_idx].trim();
    // Keep this reasonably short so we don't mistake a long sentence that
    // merely contains a colon for a block label.
    prefix.len() <= 30 && BLOCK_LABEL_PREFIXES.iter().any(|p| prefix.starts_with(p))
}

/// True for any line that should never be swallowed as a wrapped bullet
/// continuation — either a "Project N: ..." sub-entry header, or a
/// "Situation:"/"Techs:"/etc. context label.
fn looks_like_block_label(line: &str) -> bool {
    is_project_header(line) || is_context_label(line)
}

fn ends_with_terminal_punct(s: &str) -> bool {
    s.trim_end().ends_with(['.', '!', '?', ':'])
}

/// Parse experience section lines into Experience entries.
/// Parse experience section lines into Experience entries. Also returns any
/// sidebar tool/skill list entries harvested along the way (see the
/// "is_skill_bleed_line" handling below) — these never become part of any
/// bullet, so they can't be recovered by a later bullet-scanning pass.
fn parse_experiences(lines: &[String]) -> (Vec<Experience>, Vec<Skill>) {
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
                    name: format!("{} {}", trimmed, next_trimmed.unwrap()),
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
                (next_line.unwrap().to_string(), company, location)
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
            && !ends_with_terminal_punct(&current_bullets.last().unwrap().en)
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
const TOOLS_LABEL_PREFIXES: &[&str] = &["techs", "tech stack", "technologies"];

/// Commit one plain line to either the tools accumulator (if it's a
/// "Techs: ..." line or a continuation of one) or the context accumulator
/// (merging into the previous entry if it's a wrapped continuation of an
/// unfinished sentence), matching the logic used while a project is
/// actively being built. Used both for immediate commits and for lines
/// that age out of the recent_plain lookback buffer.
fn commit_pending_line(context: &mut Vec<String>, tools_text: &mut String, line: &str) {
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
    if !context.is_empty()
        && !ends_with_terminal_punct(context.last().unwrap())
        && !is_context_label(line)
        && !is_project_header(line)
    {
        let last = context.last_mut().unwrap();
        last.push(' ');
        last.push_str(line);
    } else {
        context.push(line.to_string());
    }
}

/// Commit every line still pending in the recent_plain lookback buffer, in
/// order, then empty it.
fn flush_pending_lines(
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
fn looks_like_stray_heading(line: &str) -> bool {
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
fn flush_project(
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
fn extract_trailing_date_range_loose(line: &str) -> Option<(String, String, String)> {
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

/// Build an Education entry for the "institution (+ dates) comes first,
/// degree/field line(s) follow" layout — the reverse order from
/// `build_education_from_buffer`, which expects the degree line first.
/// `trailing` is whatever plain lines were collected after the
/// institution+date row and before the next entry started.
fn build_education_institution_first(
    institution: String,
    start_year: String,
    end_year: String,
    trailing: &[String],
) -> Option<Education> {
    if institution.is_empty() && trailing.is_empty() {
        return None;
    }
    let (degree, embedded_field) = match trailing.first() {
        Some(first) => {
            let lower_first = first.to_lowercase();
            if let Some(pos) = first.find(" · ") {
                (
                    first[..pos].trim().to_string(),
                    Some(first[pos + 3..].trim().to_string()),
                )
            } else if let Some(pos) = first.find(" in ") {
                (
                    first[..pos].trim().to_string(),
                    Some(first[pos + 4..].trim().to_string()),
                )
            } else if let Some(pos) = lower_first.find(" en ") {
                (
                    first[..pos].trim().to_string(),
                    Some(first[pos + 4..].trim().to_string()),
                )
            } else {
                (first.clone(), None)
            }
        }
        None => (String::new(), None),
    };
    let mut field_parts: Vec<String> = embedded_field
        .into_iter()
        .filter(|f| !f.is_empty())
        .collect();
    field_parts.extend(trailing.iter().skip(1).cloned());

    Some(Education {
        id: uuid::Uuid::new_v4().to_string(),
        institution: institution.trim_end_matches(['·', '|']).trim().to_string(),
        degree: LocalizedText::same(degree.trim_end_matches(['·', '|']).trim()),
        field: LocalizedText::same(field_parts.join(" ").trim()),
        start_year,
        end_year,
        ..Default::default()
    })
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
fn rejoin_fragmented_date_lines(lines: &[String]) -> Vec<String> {
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
    let mut i = 0;
    while i < lines.len() {
        let line = &lines[i];
        if is_lone_icon_glyph(line) {
            let next = lines.get(i + 1);
            if next.is_some_and(|n| is_lone_month(n) || starts_with_year(n)) {
                // Confirmed decorative calendar icon directly preceding a
                // date fragment — safe to drop.
                i += 1;
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
                i += 2;
                continue;
            }
            i += 1;
            continue;
        }
        if is_lone_month(line) {
            if let Some(next) = lines.get(i + 1) {
                if starts_with_year(next) {
                    out.push(format!("{} {}", line.trim(), next.trim()));
                    i += 2;
                    continue;
                }
            }
        }
        if starts_with_bare_year_then_dash(line) {
            if let Some(next) = lines.get(i + 1) {
                if let Some(month) = month_after_optional_icon(next) {
                    out.push(format!("{} {}", month, line.trim()));
                    i += 2;
                    continue;
                }
            }
        }
        out.push(line.clone());
        i += 1;
    }
    out
}

fn parse_education(lines: &[String]) -> Vec<Education> {
    let lines = rejoin_fragmented_date_lines(lines);
    let lines = lines.as_slice();
    let mut educations = Vec::new();
    let mut buffer: Vec<String> = Vec::new();
    // Set when `extract_trailing_date_range_loose` matches an
    // "Institution, Location  Start – End" row: the institution/dates are
    // already known, but this layout's degree/field line(s) haven't been
    // seen yet — they're the plain lines that follow, collected into
    // `buffer` same as usual. Flushed via `build_education_institution_first`
    // (institution-first field order) rather than
    // `build_education_from_buffer` (degree-first) once the *next* entry
    // starts or the lines run out.
    let mut pending: Option<(String, String, String)> = None; // (institution, start, end)

    let flush_pending = |pending: &mut Option<(String, String, String)>,
                         buffer: &mut Vec<String>,
                         educations: &mut Vec<Education>| {
        if let Some((institution, start, end)) = pending.take() {
            if let Some(edu) = build_education_institution_first(institution, start, end, buffer) {
                educations.push(edu);
            }
            buffer.clear();
        }
    };

    for line in lines {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        if let Some((before, start, end)) = extract_trailing_date_range_loose(trimmed) {
            // A new institution+date row always starts a new entry — flush
            // whatever was pending (institution-first layout) or buffered
            // (degree-first layout, dates never found for it) first.
            flush_pending(&mut pending, &mut buffer, &mut educations);
            if !buffer.is_empty() {
                if let Some(edu) =
                    build_education_from_buffer(&buffer, String::new(), String::new())
                {
                    educations.push(edu);
                }
                buffer.clear();
            }
            pending = Some((before, start, end));
            continue;
        }

        if let Some((start, end)) = extract_standalone_date_range_loose(trimmed) {
            // Degree-first layout: buffer already holds [degree, field?,
            // institution]; this standalone date line completes it. Not
            // expected to coincide with a pending institution-first entry,
            // but flush that first too if it somehow does, so nothing is
            // silently dropped.
            flush_pending(&mut pending, &mut buffer, &mut educations);
            if let Some(edu) = build_education_from_buffer(&buffer, start, end) {
                educations.push(edu);
            }
            buffer.clear();
            continue;
        }

        // Year-only line like "2017" (no separator on this line at all,
        // e.g. start and end years printed on their own lines).
        if trimmed.len() == 4 && trimmed.chars().all(|c| c.is_ascii_digit()) {
            flush_pending(&mut pending, &mut buffer, &mut educations);
            if let Some(edu) =
                build_education_from_buffer(&buffer, trimmed.to_string(), String::new())
            {
                educations.push(edu);
            }
            buffer.clear();
            continue;
        }

        // A new degree line (e.g. "BTS Services Informatiques aux
        // Organisations (SIO)") starting while `buffer` already holds a
        // complete-looking prior entry (its own degree line, at
        // `buffer[0]`, plus at least one institution line further down) —
        // or symmetrically, a new INSTITUTION line starting while `buffer`
        // already holds an institution-first entry (institution at
        // `buffer[0]`, degree line further down; this app's own renderer
        // outputs institution before degree, the reverse of how the
        // source resume ordered them, so a round-2 reimport of our own
        // PDF hits this ordering even though round-1 hit the other one) —
        // means we've reached the NEXT entry with no date range ever
        // having marked the boundary — some resumes list every degree
        // with no dates at all. Flush what's pending as its own entry
        // first, rather than letting this line and everything after it
        // pile into the same buffer, where `build_education_from_buffer`
        // has no way to know two entries are in there and mis-splits the
        // lot into one garbled entry (wrong institution, wrong field,
        // duplicated separators on every re-render).
        let starts_new_degree_first_entry = !buffer.is_empty()
            && looks_like_degree_line(&buffer[0])
            && looks_like_degree_line(trimmed)
            && buffer[1..].iter().any(|l| looks_like_institution_line(l));
        let starts_new_institution_first_entry = !buffer.is_empty()
            && looks_like_institution_line(&buffer[0])
            && looks_like_institution_line(trimmed)
            && buffer[1..].iter().any(|l| looks_like_degree_line(l));
        if starts_new_degree_first_entry || starts_new_institution_first_entry {
            flush_pending(&mut pending, &mut buffer, &mut educations);
            if let Some(edu) = build_education_from_buffer(&buffer, String::new(), String::new()) {
                educations.push(edu);
            }
            buffer.clear();
        }

        buffer.push(trimmed.to_string());
    }

    // Trailing buffer/pending entry with no following date range: still
    // record it rather than silently dropping a final entry.
    if pending.is_some() {
        flush_pending(&mut pending, &mut buffer, &mut educations);
    } else if let Some(edu) = build_education_from_buffer(&buffer, String::new(), String::new()) {
        educations.push(edu);
    }

    educations
}

/// Parse skills section lines — typically comma-separated or one per line.
/// Category labels this app's own renderer prefixes a skills line with
/// (see `SkillCategory::label` in models/cv.rs and `render_skills` in
/// renderer.rs, which emits `"{label}: skill, skill, …"` per category).
/// Longest-first so e.g. "platforms & infrastructure" is tried before a
/// hypothetical shorter prefix that could partially match it.
///
/// Includes the pre-6-category label strings too (same rationale as
/// `SkillCategory`'s `#[serde(alias = ...)]`, see its doc comment): a PDF
/// exported before that migration still literally has "Framework:",
/// "Tool:", "Cloud & Infrastructure:", "Soft Skill:", "Other Skills:"
/// printed as section headers, and re-importing it should still recognize
/// those, mapped onto their closest surviving category, rather than
/// failing to categorize that skill at all.
const SKILL_CATEGORY_LABELS: &[(&str, SkillCategory)] = &[
    (
        "platforms & infrastructure",
        SkillCategory::PlatformsInfrastructure,
    ),
    (
        "cloud & infrastructure", // pre-migration label
        SkillCategory::PlatformsInfrastructure,
    ),
    ("automation & devops", SkillCategory::AutomationDevOps),
    ("other skills", SkillCategory::AutomationDevOps), // pre-migration label
    ("programming", SkillCategory::Programming),
    ("soft skill", SkillCategory::Programming), // pre-migration label
    ("monitoring", SkillCategory::Monitoring),
    ("middleware", SkillCategory::Middleware),
    ("framework", SkillCategory::Programming), // pre-migration label
    ("database", SkillCategory::Database),
    ("tool", SkillCategory::AutomationDevOps), // pre-migration label
];

pub(crate) fn parse_skills(lines: &[String]) -> Vec<Skill> {
    let mut skills = Vec::new();

    // Group the section's physical lines into per-category blocks — a new
    // block starts at a line beginning with a known category prefix (see
    // `SKILL_CATEGORY_LABELS`), or implicitly at the very first line.
    //
    // Within a block, decide how to treat line breaks:
    //   - If ANY line in the block contains a comma AND comma-splitting
    //     the joined block produces tag-shaped segments (short — see
    //     `MAX_TAG_WORDS` below), the whole block is a flowing
    //     comma-separated paragraph, exactly what this app's own
    //     renderer emits for a skills category (`render_skills` joins all
    //     of a category's skills into one "{label}: a, b, c" text run).
    //     Chromium then wraps that paragraph at arbitrary word
    //     boundaries when printing to PDF — including in the middle of a
    //     multi-word skill name, e.g. "Version Control" wraps as
    //     "Version" / "Control 5+ yrs". Splitting each physical line
    //     independently (the old behavior) turned that single skill into
    //     two ("Version" and "Control 5+ yrs"), and re-exporting then
    //     rendered a spurious comma between them that wasn't in the
    //     source — so here we rejoin the block into one string with
    //     spaces before splitting on commas.
    //   - If NO line in the block contains a comma, OR the block's
    //     content is full-sentence competency bullets rather than short
    //     tags (a sidebar of "Piloter la gestion des vulnérabilités
    //     (détection, analyse et remédiation)"-style bullet points, each
    //     wrapped across 2-3 physical lines, uses commas as ordinary
    //     prose punctuation *within* one bullet, not as separators
    //     *between* skills — comma-splitting that shreds one bullet into
    //     a dozen word-fragments, and joining the whole block with
    //     spaces first, as the paragraph case does, produces one
    //     enormous run-on "skill" spanning everything up to the first
    //     bullet that happens to contain a comma), each *logical* bullet
    //     — physical lines re-merged where the PDF wrapped one bullet
    //     across several rows, via `merge_wrapped_skill_lines` — becomes
    //     its own skill entry, comma and all.
    let mut blocks: Vec<Vec<&str>> = Vec::new();
    for line in lines {
        let lower = line.to_lowercase();
        let starts_new_block = blocks.is_empty()
            || SKILL_CATEGORY_LABELS
                .iter()
                .any(|(label, _)| lower.starts_with(&format!("{label}:")));
        if starts_new_block {
            blocks.push(vec![line.as_str()]);
        } else {
            blocks.last_mut().unwrap().push(line.as_str());
        }
    }

    // A comma-split segment longer than this doesn't look like a skill
    // tag ("GitLab-CI 3+ yrs", "Version Control 5+ yrs") any more — it
    // looks like a fragment of a sentence. Real-world tag categories in
    // this app top out around 4-5 words; real competency-bullet
    // fragments run 10+ words even for the *shortest* fragment between
    // two commas, so there's a wide, safe margin between the two.
    const MAX_TAG_WORDS: usize = 8;

    for block in blocks {
        let joined = block.join(" ");
        let looks_like_tag_list = joined.contains(',')
            && joined
                .split(',')
                .all(|segment| segment.split_whitespace().count() <= MAX_TAG_WORDS);
        if looks_like_tag_list {
            parse_skill_line(&joined, &mut skills);
        } else if block.iter().any(|l| l.contains(',')) {
            for logical_line in merge_wrapped_skill_lines(&block) {
                push_whole_skill_line(&logical_line, &mut skills);
            }
        } else {
            for line in block {
                parse_skill_line(line, &mut skills);
            }
        }
    }
    skills
}

/// Re-merges a competency-bullet block's physical lines back into logical
/// bullets, undoing the PDF's mid-bullet line wrapping (see
/// `parse_skills`'s comment on why this block shape needs that instead of
/// comma-splitting). A physical line is treated as the wrapped
/// continuation of the previous one — not a new bullet — when either:
///   - it doesn't start with an uppercase letter (a genuine new bullet in
///     this style always opens with a capitalized word — an infinitive
///     verb in French resumes, a capitalized noun/acronym in English
///     ones — while a line broken mid-phrase continues in lowercase, or
///     with digits/punctuation, e.g. "cloud et" / "on-premise", "…
///     Connect," / "2FA)"); or
///   - the previous line ends with a trailing comma, which unambiguously
///     means the sentence isn't finished yet regardless of how the next
///     line starts — this also catches the rarer case of a wrap landing
///     right before an acronym, e.g. "… suivi de roadmap," / "OKR et
///     KPI", which the capitalization check alone would miss.
fn merge_wrapped_skill_lines(lines: &[&str]) -> Vec<String> {
    let mut merged: Vec<String> = Vec::new();
    for &line in lines {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let starts_uppercase = trimmed.chars().next().is_some_and(|c| c.is_uppercase());
        let prev_ends_with_comma = merged
            .last()
            .is_some_and(|prev: &String| prev.trim_end().ends_with(','));
        let is_continuation = !merged.is_empty() && (!starts_uppercase || prev_ends_with_comma);
        if is_continuation {
            let prev = merged.last_mut().unwrap();
            prev.push(' ');
            prev.push_str(trimmed);
        } else {
            merged.push(trimmed.to_string());
        }
    }
    merged
}

/// Parse one logical (already line-wrap-resolved) skills line/paragraph:
/// strip this app's own "{Category}: " prefix if present, split the rest
/// on comma and other common delimiters, and push each non-empty item as
/// a `Skill`. See `parse_skills` for how lines are grouped into blocks
/// before reaching here.
fn parse_skill_line(line: &str, skills: &mut Vec<Skill>) {
    let (category, rest) = strip_skill_category_prefix(line);

    // Also split on common delimiters
    let mut normalized = rest.replace(" | ", ",");
    normalized = normalized.replace(" · ", ",");
    normalized = normalized.replace(" – ", ",");
    normalized = normalized.replace(" - ", ",");

    for item in normalized.split(',') {
        push_skill_entry(item, category.clone(), skills);
    }
}

/// Like `parse_skill_line`, but for one already-delimited logical bullet
/// from a competency-bullet block (see `parse_skills`'s comment) — adds it
/// as a single skill entry without also comma-splitting it, since here the
/// commas are ordinary sentence punctuation *within* the bullet, not
/// separators *between* skills. Still strips a leading category-label
/// prefix and a leading bullet marker, same as the comma-splitting path,
/// so a stray "Other Skills: " or "• " at the start of a bullet is handled
/// consistently either way.
fn push_whole_skill_line(line: &str, skills: &mut Vec<Skill>) {
    let (category, rest) = strip_skill_category_prefix(line);
    push_skill_entry(rest, category, skills);
}

/// Strips this app's own "{Category}: " prefix, if present, and returns
/// the matched category alongside the remaining text — without this,
/// re-importing our own exported PDF bakes the category label into the
/// FIRST skill's name (e.g. "Automation & DevOps: CI/CD"), and the next
/// export prepends the category label again on top of that, compounding
/// into "Automation & DevOps: Automation & DevOps: …" a little further
/// with every import/export round trip.
fn strip_skill_category_prefix(line: &str) -> (SkillCategory, &str) {
    let lower = line.to_lowercase();
    for (label, cat) in SKILL_CATEGORY_LABELS {
        let prefix = format!("{label}:");
        if lower.starts_with(&prefix) {
            return (cat.clone(), line[prefix.len()..].trim_start());
        }
    }
    (SkillCategory::default(), line)
}

fn push_skill_entry(item: &str, category: SkillCategory, skills: &mut Vec<Skill>) {
    let trimmed = item.trim().trim_start_matches(['•', '·', '-']);
    let trimmed = trimmed.trim();
    if trimmed.is_empty() || trimmed.len() < 2 {
        return;
    }
    // Skip lines that look like headers
    let lower = trimmed.to_lowercase();
    if lower == "skills"
        || lower == "compétences"
        || lower == "technical skills"
        || lower == "compétences techniques"
    {
        return;
    }
    skills.push(Skill {
        id: uuid::Uuid::new_v4().to_string(),
        name: trimmed.to_string(),
        category,
        level: SkillLevel::Intermediate,
    });
}

/// Parse projects section lines.
fn parse_projects(lines: &[String]) -> Vec<Project> {
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

/// Parse certifications section lines.
/// Parse certifications section lines.
///
/// A single certification's details are commonly spread across several
/// lines — name, year, issuing body, date range — the same
/// "several-lines-per-entry, ending in a standalone date range" layout as
/// Education. Before this fix, every line became its own bogus
/// Certification entry (e.g. one real "ITIL: Foundation certification
/// (2011), PeopleCert, Aug 2018 – No Expiration Date" turned into 4 separate
/// nonsensical entries).
pub(crate) fn parse_certifications(lines: &[String]) -> Vec<Certification> {
    let mut certs = Vec::new();
    let mut buffer: Vec<String> = Vec::new();

    // Re-join physical PDF lines that are really one wrapped "·"-joined
    // logical line before splitting on "·" below. When this app's own
    // render output joins several certifications into one "A · B · C · ..."
    // line (see the flatten step's own comment) and that line is long
    // enough to wrap in the PDF, the wrap point becomes an ordinary space
    // between two words with no "·" of its own — e.g. "...Kubernetes ·
    // Opérer" / "Kubernetes · Cisco..." wrapping mid-name, splitting
    // "Opérer Kubernetes" in two. Naively treating each physical line
    // independently then re-joins the pieces with a *spurious* "·" that
    // was never in the source text, permanently splitting one
    // certification into two — and since the mis-split state renders with
    // an extra "·" of its own, re-importing again keeps compounding it.
    // Continuing to merge forward with a plain space for as long as we're
    // still inside a "·"-joined run (i.e. the accumulated line so far
    // already contains "·") reconstructs the original single line
    // regardless of where the PDF happened to wrap it.
    let mut rejoined: Vec<String> = Vec::new();
    for line in lines {
        if let Some(last) = rejoined.last_mut() {
            if (last as &String).contains(" · ") {
                last.push(' ');
                last.push_str(line.trim());
                continue;
            }
        }
        rejoined.push(line.trim().to_string());
    }

    // Flatten any line that already bundles multiple " · "-joined parts
    // (name, year, issuer, date range) into separate pseudo-lines first —
    // when a certification's full text is short enough to not wrap, this
    // app's own renderer output (and others using the same " · "
    // convention) can produce one single already-merged line rather than
    // one line per part, which the buffer-accumulation loop below can't
    // otherwise tell apart. Also drops empty/pure-punctuation segments
    // (e.g. a trailing ",") rather than treating them as real content.
    let flattened: Vec<String> = rejoined
        .iter()
        .flat_map(|line| {
            if line.contains(" · ") {
                line.split(" · ")
                    .map(|s| s.trim().to_string())
                    .filter(|s| s.chars().filter(|c| c.is_alphanumeric()).count() >= 2)
                    .collect::<Vec<_>>()
            } else {
                vec![line.clone()]
            }
        })
        .collect();

    for line in &flattened {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        if let Some((start, end)) = extract_standalone_date_range_loose(trimmed) {
            if let Some(cert) = build_certification_from_buffer(&buffer, Some((start, end))) {
                certs.push(cert);
            }
            buffer.clear();
            continue;
        }

        buffer.push(trimmed.to_string());
    }

    if certs.is_empty() && buffer.len() > 2 {
        // No date range was found anywhere in this section, so nothing
        // ever signaled where one certification ends and the next
        // begins — every line piled into this one `buffer`, which used to
        // become a single Certification with the first line as `name` and
        // everything else squashed into `issuer` via " · " joins (e.g.
        // four separate Kubernetes trainings, each its own line in the
        // source resume, ending up as one certification named "Formation
        // Kubernetes" with the other three folded into its "issuer"
        // field). A resume that lists several certifications one per line
        // with no dates at all is common enough that "one per line" is a
        // far more useful — and far less garbled — default than "every
        // line in the section is secretly one record"; each line becomes
        // its own certification instead. (This only applies once the
        // *whole* section turns out to have no date-bounded entries at
        // all — `certs.is_empty()` — so a section that mixes dated and
        // dateless certifications is untouched, and a short 1–2 line
        // buffer, more likely a single name+issuer pair than two
        // unrelated certifications, still merges as before.)
        for line in &buffer {
            if let Some(cert) = build_certification_from_buffer(std::slice::from_ref(line), None) {
                certs.push(cert);
            }
        }
    } else if let Some(cert) = build_certification_from_buffer(&buffer, None) {
        certs.push(cert);
    }

    certs
}

/// Build one Certification from a buffer of plain lines (name, optionally a
/// bare year, optionally an issuing body) plus an optional trailing date
/// range. A bare 4-digit year gets folded onto the name in parentheses
/// (e.g. "ITIL: Foundation certification (2011)"). The issuer (if a second
/// buffer line is present) and the date range are kept in their own
/// fields — matching the model's separate `issuer`/`date` fields — rather
/// than joined into `name`. Joining them into `name` used to be harmless
/// on a first import, but re-exporting our own PDF unconditionally
/// appended "· {issuer}, {date}" again on top (see render_certifications),
/// so a name that already contained the issuer/date from a previous
/// import would end up with it twice, compounding by one more repetition
/// on every subsequent round trip.
fn build_certification_from_buffer(
    buffer: &[String],
    date_range: Option<(String, String)>,
) -> Option<Certification> {
    if buffer.is_empty() {
        return None;
    }

    let mut name = buffer[0].clone();
    let mut issuer_parts: Vec<String> = Vec::new();
    for extra in &buffer[1..] {
        if extra.len() == 4 && extra.chars().all(|c| c.is_ascii_digit()) {
            name.push_str(&format!(" ({extra})"));
        } else {
            issuer_parts.push(extra.clone());
        }
    }
    let date = date_range
        .map(|(start, end)| format!("{start} – {end}"))
        .unwrap_or_default();

    Some(Certification {
        id: uuid::Uuid::new_v4().to_string(),
        name,
        issuer: issuer_parts.join(" · "),
        date,
        ..Default::default()
    })
}

/// A line that's purely proficiency-dot decoration (e.g. "○ ○ ○ ○ ○"), with
/// no actual text. Some resume templates render language proficiency as a
/// row of dot/circle glyphs — filled vs. unfilled to show the level — but
/// that fill/unfill distinction is drawn as vector graphics (colored
/// shapes), not as distinguishable text characters, so it's invisible to
/// text extraction. Rather than fabricate a language entry out of these
/// decorative glyphs, we filter them out entirely.
fn is_dots_only(s: &str) -> bool {
    let trimmed = s.trim();
    !trimmed.is_empty()
        && trimmed
            .chars()
            .all(|c| c == '○' || c == '●' || c == '•' || c.is_whitespace())
}

/// Parse languages section lines.
/// True when `line` carries no explicit proficiency marker (no "Name -
/// Level", "Name (Level)", "Name, Level", or trailing rating dots) and the
/// following line reads like descriptive prose rather than another short
/// language entry. This is the shape of an "Interests" blurb that's
/// tacked onto a combined "Languages & Interests" heading (see
/// `SECTION_HEADERS`'s "langues et centres d'intérêt" mapping): a short
/// standalone heading word (e.g. "Musique") immediately followed by a
/// full sentence describing the hobby. A genuine bare-word language line
/// (some resumes just list "English" / "French" / "Vietnamese" with no
/// separator at all — see `parse_languages_filters_dot_only_lines`) is
/// never followed by that shape, since its neighbors are more short bare
/// words, not prose — so this only fires on the real boundary.
fn looks_like_interest_heading(line: &str, next: Option<&String>) -> bool {
    let has_proficiency_marker =
        line.contains(" - ") || line.contains(" (") || line.contains(", ") || line.contains(':');
    if has_proficiency_marker {
        return false;
    }
    match next {
        Some(next) => {
            let next = next.trim();
            next.len() > 50 || next.matches(',').count() >= 2 || ends_with_terminal_punct(next)
        }
        None => false,
    }
}

pub(crate) fn parse_languages(lines: &[String]) -> Vec<Language> {
    let mut langs = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() || is_dots_only(trimmed) {
            continue;
        }
        if !langs.is_empty() && looks_like_interest_heading(trimmed, lines.get(i + 1)) {
            // Everything from here on is the trailing "& Interests" half
            // of a combined section heading, not more languages — stop
            // rather than mis-parse a hobby blurb as a language entry.
            break;
        }
        // This importer's own renderer packs every language onto a
        // single line as repeated "Name (Level)" segments (e.g.
        // "Français (Native / Bilingual) Anglais (Conversational)") —
        // re-importing a CV this importer generated needs to split that
        // back apart into separate entries, or every language after the
        // first is silently dropped (only one name/level pair is ever
        // extracted per line below). Detect that shape — 2 or more
        // parenthesized groups on one line — and split on it first;
        // every other format this function handles (dash, colon, dot-
        // rating) has at most one paren group per line and falls
        // through unaffected.
        if trimmed.matches('(').count() >= 2 {
            for segment in split_paren_segments(trimmed) {
                if let Some(lang) = parse_single_language_entry(&segment) {
                    langs.push(lang);
                }
            }
            continue;
        }
        if let Some(lang) = parse_single_language_entry(trimmed) {
            langs.push(lang);
        }
    }
    langs
}

/// Splits a line into segments, breaking right after each balanced
/// "(...)" group closes — e.g. "Français (Native / Bilingual) Anglais
/// (Conversational)" becomes ["Français (Native / Bilingual)", "Anglais
/// (Conversational)"]. Any trailing text with no closing paren (not
/// expected for the renderer's own format, but kept defensively) forms
/// a final segment on its own.
fn split_paren_segments(line: &str) -> Vec<String> {
    let chars: Vec<char> = line.chars().collect();
    let mut segments = Vec::new();
    let mut depth = 0i32;
    let mut start = 0usize;
    for (i, &c) in chars.iter().enumerate() {
        match c {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth <= 0 {
                    let seg: String = chars[start..=i].iter().collect();
                    let seg = seg.trim().to_string();
                    if !seg.is_empty() {
                        segments.push(seg);
                    }
                    start = i + 1;
                }
            }
            _ => {}
        }
    }
    let tail: String = chars[start..].iter().collect();
    let tail = tail.trim().to_string();
    if !tail.is_empty() {
        segments.push(tail);
    }
    segments
}

/// Parses one "Name - Level" / "Name (Level)" / "Name : Level" / bare
/// "Name" language entry. Shared by both the single-language-per-line
/// path and the multi-segment path above.
fn parse_single_language_entry(trimmed: &str) -> Option<Language> {
    let lower = trimmed.to_lowercase();
    let level =
        if lower.contains("native") || lower.contains("bilingue") || lower.contains("maternelle") {
            LanguageLevel::Native
        } else if lower.contains("professional")
            || lower.contains("professionnel")
            || lower.contains("fluent")
            || lower.contains("courant")
        {
            LanguageLevel::Professional
        } else {
            LanguageLevel::Conversational
        };

    // Split "French - Native", "French (Native)", or the French
    // "Anglais : Technique" colon style — using whichever separator
    // occurs *earliest* in the line, not whichever is checked first.
    // A fixed priority order (checking " (" before " : ", say) picks
    // the wrong split point whenever a line has more than one kind of
    // separator, e.g. "Anglais : Technique (niveau B1, ...)" has both
    // " : " and " (" — checking " (" first would keep "Anglais :
    // Technique" as the name instead of just "Anglais".
    let name = [" - ", " (", " : ", ", "]
        .iter()
        .filter_map(|sep| trimmed.find(sep))
        .min()
        .map(|pos| trimmed[..pos].trim().to_string())
        .unwrap_or_else(|| trimmed.to_string());
    // Strip a trailing rating-dot cluster glued onto the same line as
    // the name (e.g. "English ○ ○ ○ ○ ○") — a proficiency-dial
    // rendered as text lands right after the name with no separator
    // `is_dots_only` (which only catches a dots-only *line*) can
    // recognize, so without this the dots end up baked into the
    // name itself.
    let name = name
        .trim_end_matches([' ', '○', '●', '•'])
        .trim()
        .to_string();

    if name.is_empty() {
        None
    } else {
        Some(Language {
            id: uuid::Uuid::new_v4().to_string(),
            name,
            level,
        })
    }
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
fn find_duplicate_job_boundary(stray: &[String]) -> Option<usize> {
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

fn reclaim_stray_experience_content(
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

/// Parse extracted text into a LifetimeCV.
pub fn parse_cv(text: &str) -> LifetimeCV {
    let lines: Vec<&str> = text.lines().collect();
    let sections =
        merge_duplicate_sections(reclaim_stray_experience_content(split_into_sections(text)));

    let mut cv = LifetimeCV::default();

    // Extract personal info from header (text before first section)
    let header_lines: Vec<&str> = sections
        .iter()
        .filter(|(s, _)| *s == "header")
        .flat_map(|(_, lines)| lines.iter().map(|s| s.as_str()))
        .collect();

    // Name
    if let Some(name) = guess_name(&header_lines) {
        cv.personal.name = name;
    }

    // Title
    if let Some(title) = guess_title(&lines) {
        cv.personal.title = LocalizedText::same(title);
    }

    // Contact info from header
    let header_text = header_lines.join(" ");
    if let Some(email) = extract_email(&header_text) {
        cv.personal.email = email;
    }
    if let Some(phone) = extract_phone(&header_text) {
        cv.personal.phone = phone;
    }
    let (linkedin, github, website) = extract_urls(&header_text);
    if let Some(li) = linkedin {
        cv.personal.linkedin = li;
    }
    if let Some(gh) = github {
        cv.personal.github = gh;
    }
    if let Some(web) = website {
        cv.personal.website = web;
    }

    // Skills harvested from sidebar tool/skill lines caught (and never
    // turned into bullets) while parsing Experience — see parse_experiences.
    let mut harvested_from_parsing: Vec<Skill> = Vec::new();

    // Process each section
    for (section, lines) in &sections {
        match *section {
            "experience" => {
                let (exps, harvested) = parse_experiences(lines);
                cv.experiences = exps;
                harvested_from_parsing = harvested;
            }
            "education" => {
                cv.education = parse_education(lines);
            }
            "skills" => {
                cv.skills = parse_skills(lines);
            }
            "projects" => {
                cv.projects = parse_projects(lines);
            }
            "certifications" => {
                cv.certifications = parse_certifications(lines);
            }
            "languages" => {
                cv.languages = parse_languages(lines);
            }
            _ => {}
        }
    }

    // Multi-column PDFs can interleave a sidebar's tool/skill list into the
    // Experience section's bullet stream, so some bullets are really
    // stray skill entries rather than accomplishments. Rather than try to
    // prevent this geometrically (attempted and found to be unreliable —
    // see decode_operations/run_operations history), detect it by content:
    // these bled-in lines have a distinctive, very specific shape ("<tool
    // name> N+ yrs" repeated) that a genuine accomplishment bullet
    // essentially never has. Most of this is now caught during
    // parse_experiences itself (harvested_from_parsing, above) before it
    // can ever corrupt role/company or context detection; this second pass
    // is a safety net for anything that still slipped through as a real
    // bullet. Either way, it turns a data-corruption bug into a bonus:
    // previously-missing detailed tool/skill entries (not just the
    // top-level category summary) end up in Skills.
    let harvested = harvested_from_parsing
        .into_iter()
        .chain(harvest_skills_from_experiences(&mut cv.experiences));
    for skill in harvested {
        if !cv
            .skills
            .iter()
            .any(|s| s.name.eq_ignore_ascii_case(&skill.name))
        {
            cv.skills.push(skill);
        }
    }

    resolve_project_skill_ids(&mut cv);

    cv
}

/// Resolves every `ExperienceProject.skill_ids` from the raw tool NAME
/// strings `flush_project` temporarily staged there (see its call site's
/// comment) into real `Skill.id`s, now that `cv.skills` is finalized.
///
/// Matching is case-insensitive exact-name only — no fuzzy matching, no
/// creating a new `Skill` for a name that doesn't already exist in
/// `cv.skills`. A parsed tool name with no match is simply dropped, not
/// kept as free text: this mirrors the editor's own "strict" tag picker,
/// which only ever lets you attach a project to a skill that's already in
/// `cv.skills` — so a freshly-imported CV and a manually-edited one end up
/// with the same invariant (every `skill_ids` entry is always a real,
/// resolvable skill), rather than import quietly being a looser, parallel
/// path that can produce data the editor itself could never create.
fn resolve_project_skill_ids(cv: &mut LifetimeCV) {
    let skills = cv.skills.clone();
    for exp in &mut cv.experiences {
        for proj in &mut exp.projects {
            proj.skill_ids = proj
                .skill_ids
                .iter()
                .filter_map(|raw_name| {
                    skills
                        .iter()
                        .find(|s| s.name.eq_ignore_ascii_case(raw_name))
                        .map(|s| s.id.clone())
                })
                .collect();
        }
    }
}

/// Scan every bullet in every Experience/project for the "<tool> N+ yrs"
/// (repeated) pattern; remove matching bullets and return the harvested
/// entries as proper Skills.
fn harvest_skills_from_experiences(experiences: &mut [Experience]) -> Vec<Skill> {
    let mut harvested = Vec::new();
    for exp in experiences.iter_mut() {
        for proj in exp.projects.iter_mut() {
            let mut kept = Vec::with_capacity(proj.bullets.len());
            for bullet in proj.bullets.drain(..) {
                if let Some(segments) = harvest_skill_segments(&bullet.en) {
                    for seg in segments {
                        harvested.push(Skill {
                            id: uuid::Uuid::new_v4().to_string(),
                            name: seg,
                            category: SkillCategory::default(),
                            level: SkillLevel::Intermediate,
                        });
                    }
                } else {
                    kept.push(bullet);
                }
            }
            proj.bullets = kept;
        }
    }
    harvested
}

/// True if `line` is *only* a "<N>+yrs" years-experience marker with no
/// name attached — e.g. "2+yrs" (one fused token, exactly how
/// Input_Resume.pdf's own sidebar typesets it — verified directly against
/// its glyph positions, no space before "yrs") or "2+ yrs" (two tokens, in
/// case some other source resume spaces it out instead).
/// `harvest_skill_segments` requires the name and marker on the *same*
/// line — genuinely true for most sidebar tag rows, but not always: a name
/// that's left-aligned and a "N+yrs" badge that's right-aligned in a
/// fixed-width sidebar column can have glyph "top" coordinates that differ
/// by more than SAME_ROW_Y_EPSILON purely from sub-pixel baseline drift
/// between the two differently-positioned spans, even though they're the
/// same visual row (also verified directly — this is exactly what happens
/// to "Kustomize" / "2+yrs" in Input_Resume.pdf's TOOLS sidebar) — landing
/// the marker on its own separate reconstructed PDF line, split from its
/// name. This recognizes that marker-only line so the name immediately
/// before it (see the lookahead using this, above) can still be matched up
/// with it instead of neither ever being caught at all.
fn is_bare_years_marker(line: &str) -> bool {
    let is_years_word = |t: &str| {
        let tl = t.to_lowercase();
        tl == "yrs" || tl == "yr" || tl == "years" || tl == "year"
    };
    let digits_plus = |t: &str| -> bool {
        t.ends_with('+') && t.len() > 1 && t[..t.len() - 1].chars().all(|c| c.is_ascii_digit())
    };
    match line.split_whitespace().collect::<Vec<_>>().as_slice() {
        // Fused single token, e.g. "2+yrs".
        [one] => {
            let tl = one.to_lowercase();
            ["+yrs", "+years", "+yr", "+year"].iter().any(|suffix| {
                tl.strip_suffix(suffix)
                    .map(|digits| !digits.is_empty() && digits.chars().all(|c| c.is_ascii_digit()))
                    .unwrap_or(false)
            })
        }
        // Two tokens, e.g. "2+" "yrs".
        [num, unit] => digits_plus(num) && is_years_word(unit),
        _ => false,
    }
}

/// If `token` ends with "<digits>+" (1 or 2 digits) with at least one
/// non-digit character immediately before those digits — e.g.
/// "Kustomize2+" — splits it into the name part and the marker part:
/// `("Kustomize", "2+")`. Returns None for a token that's *only* digits and
/// '+' (like plain "2+"): that's already handled directly as its own token
/// by harvest_skill_segments's marker scan, without needing a split.
///
/// This is a real, verified pattern — not a guess: Input_Resume.pdf's own
/// TOOLS sidebar genuinely typesets some entries this way (checked
/// directly against its glyph positions — "Kustomize" and "2+" share one
/// contiguous run with no space between them, while there IS a real space
/// before "yrs"). harvest_skill_segments's marker scan needs the "<N>+"
/// marker as its own whitespace-delimited token, so without this split
/// these entries — and the sentence they happen to land next to, since
/// they're stray sidebar bleed rather than a deliberate line break — never
/// get recognized as tool/skill entries at all.
///
/// Deliberately conservative about the digit run: only 1-2 digits count
/// (a realistic "years of experience" value). A longer run is far more
/// likely to be a genuine part of a product name/version — e.g. "iOS17+"
/// or "Log4j2023+" — so those are left untouched rather than risking a
/// false split.
fn split_fused_name_and_marker(token: &str) -> Option<(&str, &str)> {
    let before_plus = token.strip_suffix('+')?;
    let digit_bytes = before_plus
        .bytes()
        .rev()
        .take_while(u8::is_ascii_digit)
        .count();
    if digit_bytes == 0 || digit_bytes > 2 {
        return None;
    }
    let digit_start = before_plus.len() - digit_bytes;
    if digit_start == 0 {
        return None; // the whole token before '+' is just digits — plain "2+"
    }
    // Safe to slice here: everything from digit_start to the end of `token`
    // is single-byte ASCII (the digit run plus the trailing '+'), so
    // digit_start can't land inside a multi-byte character.
    let name_part = &before_plus[..digit_start];
    if !name_part.chars().any(|c| c.is_alphabetic()) {
        return None;
    }
    Some((name_part, &token[digit_start..]))
}

/// Detect a line that's really a run of "<tool name> N+ yrs" segments (e.g.
/// "GitLab-CI 3+ yrs GitHub Actions 2+ yrs Jenkins 1+ yrs") rather than a
/// genuine accomplishment bullet, and split it into individual "<tool> N+
/// yrs" skill entries. Returns None if the line doesn't contain any such
/// "N+ yrs" marker at all.
fn harvest_skill_segments(line: &str) -> Option<Vec<String>> {
    let mut tokens: Vec<&str> = line.split_whitespace().collect();
    if tokens.is_empty() {
        return None;
    }

    // Pre-split any token fusing a tool name directly onto its own "N+"
    // marker with no space at all (see split_fused_name_and_marker's doc
    // comment) into two tokens, so the marker scan below — which expects
    // the "<N>+" marker as its own token — still recognizes it.
    let mut i = 0;
    while i < tokens.len() {
        if let Some((name_part, marker_part)) = split_fused_name_and_marker(tokens[i]) {
            tokens.splice(i..=i, [name_part, marker_part]);
            i += 2;
        } else {
            i += 1;
        }
    }

    // A marker is either two tokens "3+" "yrs"/"years", or one token
    // "3+yrs"/"3+years". `marker_end` is the token index of the LAST token
    // of the marker; `marker_start` is the FIRST.
    let is_years_word = |t: &str| {
        let tl = t.to_lowercase();
        tl == "yrs" || tl == "yr" || tl == "years" || tl == "year"
    };
    let digits_plus = |t: &str| -> bool {
        t.ends_with('+') && t.len() > 1 && t[..t.len() - 1].chars().all(|c| c.is_ascii_digit())
    };

    let mut markers: Vec<(usize, usize)> = Vec::new(); // (start_idx, end_idx) inclusive
    for i in 0..tokens.len() {
        if is_years_word(tokens[i]) && i > 0 && digits_plus(tokens[i - 1]) {
            markers.push((i - 1, i));
            continue;
        }
        let tl = tokens[i].to_lowercase();
        for suffix in ["+yrs", "+years", "+yr", "+year"] {
            if let Some(digits) = tl.strip_suffix(suffix) {
                if !digits.is_empty() && digits.chars().all(|c| c.is_ascii_digit()) {
                    markers.push((i, i));
                    break;
                }
            }
        }
    }

    if markers.is_empty() {
        return None;
    }

    let mut out = Vec::new();
    let mut start = 0;
    for (marker_start, marker_end) in markers {
        if marker_start < start {
            continue; // overlapping/malformed, skip defensively
        }
        let name = tokens[start..marker_start].join(" ").trim().to_string();
        let years = tokens[marker_start..=marker_end].join(" ");
        // The length cap is a safety net specifically for the fused-token
        // split above: it relies on stray sidebar bleed consistently
        // landing on its own short reconstructed PDF line (verified true
        // for every case examined so far — see split_fused_name_and_marker
        // and the skill-bleed handling above), never actually fused into a
        // long genuine sentence. If that assumption is ever wrong for some
        // other document, this stops it from swallowing a whole paragraph
        // as a bogus "skill name" instead of just failing to harvest that
        // one entry.
        if !name.is_empty() && name.len() <= 60 {
            out.push(format!("{name} {years}"));
        }
        start = marker_end + 1;
    }
    if out.is_empty() {
        None
    } else {
        Some(out)
    }
}

/// Main entry point: extract text from PDF bytes and parse into a LifetimeCV.
///
/// LinkedIn's own "Save to PDF" profile export uses a structurally
/// different layout from the resumes `parse_cv`'s heuristics were built
/// and tuned against (this app's own renderer, and human-authored
/// single-column resumes generally) — a sidebar column that appears
/// *before* the person's own name in the raw text, multi-line job
/// headers with no icon glyph, and a standalone "N years M months"
/// tenure line. Rather than bend the generic parser to also cover that
/// very different shape (risking regressions in the extensively-tested
/// generic path), route to a dedicated parser — see
/// `linkedin_import::is_linkedin_export` for the detection fingerprint.
pub fn import_pdf(bytes: &[u8]) -> Result<LifetimeCV, String> {
    let text = extract_text(bytes)?;
    if text.trim().is_empty() {
        return Err("No text could be extracted from the PDF".to_string());
    }
    let cv = if crate::services::linkedin_import::is_linkedin_export(&text) {
        crate::services::linkedin_import::parse_linkedin_cv(&text)
    } else {
        parse_cv(&text)
    };
    Ok(cv)
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// Regression test for the "glued text" bug: PDFs (commonly produced by
    /// design tools like Canva/Figma) position each word/field as its own
    /// `Tj` run via `Td`, with no literal space characters, and use `TJ`
    /// kerning numbers instead of spaces between some runs. Before the fix,
    /// text extraction concatenated everything with zero separation,
    /// producing e.g. "TOOLSCI/CDGitLab-CI3+yrsGitHubActions2+yrs" — which is
    /// exactly the kind of mangled text that ended up misparsed into the
    /// wrong CV fields (Title, LinkedIn, GitHub, etc).
    #[test]
    fn run_operations_inserts_spaces_and_newlines_for_positioned_runs() {
        use lopdf::content::Operation;
        use lopdf::Dictionary;

        let ops = vec![
            Operation::new("BT", vec![]),
            Operation::new(
                "Tf",
                vec![Object::Name(b"F1".to_vec()), Object::Integer(10)],
            ),
            Operation::new("Td", vec![Object::Integer(50), Object::Integer(700)]),
            Operation::new("Tj", vec![Object::string_literal("TOOLS")]),
            Operation::new("Td", vec![Object::Integer(0), Object::Integer(-14)]), // new line
            Operation::new("Tj", vec![Object::string_literal("CI/CD")]),
            Operation::new("Td", vec![Object::Integer(40), Object::Integer(0)]), // same line, new run
            Operation::new("Tj", vec![Object::string_literal("GitLab-CI")]),
            Operation::new("Td", vec![Object::Integer(30), Object::Integer(0)]), // same line, new run
            Operation::new("Tj", vec![Object::string_literal("3+yrs")]),
            Operation::new("Td", vec![Object::Integer(-70), Object::Integer(-14)]), // new line
            Operation::new("Tj", vec![Object::string_literal("GitHub Actions")]),
            Operation::new(
                "TJ",
                vec![Object::Array(vec![
                    Object::string_literal(""),
                    Object::Integer(-250), // kerning gap standing in for a space
                    Object::string_literal("2+yrs"),
                ])],
            ),
            Operation::new("ET", vec![]),
        ];

        let doc = Document::new();
        let resources = Dictionary::new();
        let encodings = std::collections::BTreeMap::new();
        let mut visited = Vec::new();
        let mut lines: Vec<PositionedLine> = Vec::new();
        run_operations(
            &doc,
            &ops,
            &resources,
            &encodings,
            Matrix::identity(),
            &mut visited,
            &mut lines,
        );
        let texts: Vec<&str> = lines.iter().map(|l| l.text.as_str()).collect();

        assert_eq!(
            texts,
            vec!["TOOLS", "CI/CD GitLab-CI 3+yrs", "GitHub Actions 2+yrs",]
        );
    }

    /// Regression test for the idempotence bug where a font's ligature
    /// glyph ("ﬀ", U+FB00) — rendered as a single character position in
    /// the PDF, but whose ToUnicode CMap decodes it to the 2-character
    /// string "ff" — was mistaken for a multi-character *run*, tripping
    /// the word-gap heuristic into inserting a bogus space on both
    /// sides. This turned "offboarding" into "off boarding" every time
    /// our own rendered PDF got re-imported. Chunk count for the
    /// word-gap heuristic must come from the number of glyph *codes* in
    /// the operand, not `chars().count()` of the decoded text.
    #[test]
    fn run_operations_ligature_glyph_does_not_insert_spurious_space() {
        use lopdf::content::Operation;
        use lopdf::{Dictionary, StringFormat};

        let ops = vec![
            Operation::new("BT", vec![]),
            Operation::new(
                "Tf",
                vec![Object::Name(b"F1".to_vec()), Object::Integer(10)],
            ),
            Operation::new("Td", vec![Object::Integer(50), Object::Integer(700)]),
            Operation::new("Tj", vec![Object::string_literal("o")]),
            Operation::new("Td", vec![Object::Integer(6), Object::Integer(0)]), // same line, contiguous
            // Single glyph (one byte code), but its ToUnicode CMap maps
            // it to a 2-character string, like a real "ﬀ" ligature glyph.
            Operation::new("Tj", vec![Object::String(vec![1u8], StringFormat::Literal)]),
            Operation::new("Td", vec![Object::Integer(7), Object::Integer(0)]), // same line, contiguous
            Operation::new("Tj", vec![Object::string_literal("boarding")]),
            Operation::new("ET", vec![]),
        ];

        let doc = Document::new();
        let resources = Dictionary::new();
        let mut encodings: std::collections::BTreeMap<Vec<u8>, ToUnicodeMap> =
            std::collections::BTreeMap::new();
        let mut map = std::collections::HashMap::new();
        map.insert(1u32, "ff".to_string());
        encodings.insert(b"F1".to_vec(), ToUnicodeMap { code_bytes: 1, map });
        let mut visited = Vec::new();
        let mut lines: Vec<PositionedLine> = Vec::new();
        run_operations(
            &doc,
            &ops,
            &resources,
            &encodings,
            Matrix::identity(),
            &mut visited,
            &mut lines,
        );
        let texts: Vec<&str> = lines.iter().map(|l| l.text.as_str()).collect();

        assert_eq!(texts, vec!["offboarding"]);
    }

    #[test]
    fn extract_email_basic() {
        assert_eq!(
            extract_email("contact me at john@example.com please"),
            Some("john@example.com".to_string())
        );
    }

    #[test]
    fn extract_email_angle_brackets() {
        assert_eq!(
            extract_email("<jane.doe@corp.fr>"),
            Some("jane.doe@corp.fr".to_string())
        );
    }

    #[test]
    fn extract_phone_with_plus() {
        assert_eq!(
            extract_phone("call me at +33 6 12 34 56 78"),
            Some("+33612345678".to_string())
        );
    }

    #[test]
    fn extract_phone_local() {
        assert_eq!(
            extract_phone("tel: 0612345678"),
            Some("0612345678".to_string())
        );
    }

    #[test]
    fn extract_urls_linkedin() {
        let (li, gh, web) = extract_urls("https://linkedin.com/in/john https://github.com/john");
        assert_eq!(li, Some("https://linkedin.com/in/john".to_string()));
        assert_eq!(gh, Some("https://github.com/john".to_string()));
        assert!(web.is_none());
    }

    #[test]
    fn guess_name_simple() {
        let lines = vec!["John Smith", "john@example.com", "+33 6 00 00 00"];
        assert_eq!(guess_name(&lines), Some("John Smith".to_string()));
    }

    #[test]
    fn guess_name_skips_email_line() {
        let lines = vec!["john@example.com", "Jane Doe", "+33 6 00 00 00"];
        assert_eq!(guess_name(&lines), Some("Jane Doe".to_string()));
    }

    #[test]
    fn guess_title_engineer() {
        let lines = vec!["John Smith", "Senior Rust Engineer", "john@example.com"];
        assert_eq!(
            guess_title(&lines),
            Some("Senior Rust Engineer".to_string())
        );
    }

    #[test]
    fn detect_section_experience() {
        assert_eq!(detect_section("Experience"), Some("experience"));
        assert_eq!(detect_section("Work Experience"), Some("experience"));
        assert_eq!(
            detect_section("Expérience professionnelle"),
            Some("experience")
        );
    }

    #[test]
    fn detect_section_education() {
        assert_eq!(detect_section("Education"), Some("education"));
        assert_eq!(detect_section("Formation"), Some("education"));
    }

    #[test]
    fn detect_section_skills() {
        assert_eq!(detect_section("Skills"), Some("skills"));
        assert_eq!(detect_section("Compétences"), Some("skills"));
    }

    #[test]
    fn extract_date_range_dash() {
        let result = extract_date_range("Jan 2021 - Present");
        assert_eq!(
            result,
            Some(("Jan 2021".to_string(), "Present".to_string()))
        );
    }

    #[test]
    fn extract_date_range_en_dash() {
        let result = extract_date_range("Software Engineer · Acme Corp – 2020 – 2024");
        assert!(result.is_some());
        let (_, end) = result.unwrap();
        assert_eq!(end, "2024");
    }

    #[test]
    fn parse_cv_full_sample() {
        let text = r#"
John Smith
Senior Rust Engineer
john@example.com
+33 6 12 34 56 78
linkedin.com/in/johnsmith
github.com/johnsmith

Experience
Software Engineer at Acme Corp - Jan 2021 - Present
• Built distributed systems using Rust
• Reduced latency by 40%

Junior Developer at Beta Ltd - Jun 2019 - Dec 2020
• Developed web applications with React

Education
MSc in Computer Science - MIT - 2017 - 2019
BSc in Computer Science - Stanford - 2013 - 2017

Skills
Rust, PostgreSQL, Kubernetes, React, TypeScript

Languages
French - Native
English - Professional
"#;
        let cv = parse_cv(text);
        assert_eq!(cv.personal.name, "John Smith");
        assert_eq!(cv.personal.email, "john@example.com");
        assert_eq!(cv.personal.phone, "+33612345678");
        assert!(cv.personal.linkedin.contains("linkedin.com"));
        assert!(cv.personal.github.contains("github.com"));
        assert!(!cv.experiences.is_empty());
        assert!(!cv.education.is_empty());
        assert!(!cv.skills.is_empty());
        assert!(!cv.languages.is_empty());
    }

    #[test]
    fn parse_cv_minimal() {
        let text = "Jane Doe\nDeveloper\njane@test.com\n\nSkills\nRust, Python\n";
        let cv = parse_cv(text);
        assert_eq!(cv.personal.name, "Jane Doe");
        assert_eq!(cv.personal.email, "jane@test.com");
        assert_eq!(cv.skills.len(), 2);
    }

    #[test]
    fn parse_skills_comma_separated() {
        let lines = vec!["Rust, Python, JavaScript, TypeScript".to_string()];
        let skills = parse_skills(&lines);
        assert_eq!(skills.len(), 4);
        assert_eq!(skills[0].name, "Rust");
    }

    /// Regression test for the idempotence bug where re-importing our own
    /// rendered PDF split a two-word skill name into two bogus skills.
    /// Our renderer emits a whole skills category as one flowing
    /// comma-separated paragraph ("Other Skills: a, b, c, ..."), which
    /// Chromium then wraps at arbitrary word boundaries when printing —
    /// including in the middle of a compound skill name, so "Version
    /// Control 5+ yrs" can land as "...Version" / "Control 5+ yrs...".
    /// Splitting each physical line independently (the old behavior)
    /// turned that into two separate skills, "Version" and "Control 5+
    /// yrs", and re-exporting then rendered a comma between them that
    /// was never in the source. Lines within one category block must be
    /// rejoined before splitting on commas.
    #[test]
    fn parse_skills_rejoins_compound_skill_wrapped_across_lines() {
        let lines = vec![
            "Other Skills: CI/CD 4+ yrs, Secrets Management 2+ yrs, Version".to_string(),
            "Control 5+ yrs, Artifact Management 7+ yrs".to_string(),
        ];
        let skills = parse_skills(&lines);
        let names: Vec<&str> = skills.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(
            names,
            vec![
                "CI/CD 4+ yrs",
                "Secrets Management 2+ yrs",
                "Version Control 5+ yrs",
                "Artifact Management 7+ yrs",
            ]
        );
    }

    /// Companion regression test: a human resume's one-skill-per-line
    /// sidebar layout (no commas anywhere) must NOT be blindly merged
    /// into one blob by the same rejoin logic — each line is still a
    /// complete, standalone skill entry there.
    #[test]
    fn parse_skills_keeps_one_skill_per_line_when_no_commas_present() {
        let lines = vec![
            "CI/CD 4+ yrs".to_string(),
            "Infrastructure as Code 3+ yrs".to_string(),
            "Configuration Management 4+ yrs".to_string(),
        ];
        let skills = parse_skills(&lines);
        let names: Vec<&str> = skills.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(
            names,
            vec![
                "CI/CD 4+ yrs",
                "Infrastructure as Code 3+ yrs",
                "Configuration Management 4+ yrs",
            ]
        );
    }

    #[test]
    fn parse_languages_with_levels() {
        let lines = vec![
            "French - Native".to_string(),
            "English - Professional".to_string(),
            "Spanish - Conversational".to_string(),
        ];
        let langs = parse_languages(&lines);
        assert_eq!(langs.len(), 3);
        assert_eq!(langs[0].level, LanguageLevel::Native);
        assert_eq!(langs[1].level, LanguageLevel::Professional);
        assert_eq!(langs[2].level, LanguageLevel::Conversational);
    }

    /// Regression test: some resume templates render proficiency as a row of
    /// dot/circle glyphs on their own line(s). Those must not become bogus
    /// "language" entries.
    #[test]
    fn parse_languages_filters_dot_only_lines() {
        let lines = vec![
            "English".to_string(),
            "○ ○ ○ ○ ○".to_string(),
            "French".to_string(),
            "○ ○ ○ ○ ○".to_string(),
            "Vietnamese".to_string(),
            "○ ○ ○".to_string(),
            "○".to_string(),
        ];
        let langs = parse_languages(&lines);
        assert_eq!(
            langs.len(),
            3,
            "expected only the 3 real languages, got: {:?}",
            langs.iter().map(|l| &l.name).collect::<Vec<_>>()
        );
        assert_eq!(langs[0].name, "English");
        assert_eq!(langs[1].name, "French");
        assert_eq!(langs[2].name, "Vietnamese");
    }

    /// Regression test: a certification's name/year/issuer/date range spread
    /// across several lines must become ONE Certification entry, not one
    /// bogus entry per line — and issuer/date must land in their own
    /// fields (not get joined into `name`), so re-exporting doesn't bolt
    /// the same issuer/date onto the name a second time (see
    /// render_certifications, which shows them separately from `name`).
    #[test]
    fn parse_certifications_merges_multi_line_entry() {
        let lines = vec![
            "ITIL: Foundation certification".to_string(),
            "2011".to_string(),
            "PeopleCert".to_string(),
            "\u{0011} Aug 2018 – No Expiration Date".to_string(),
        ];
        let certs = parse_certifications(&lines);
        assert_eq!(
            certs.len(),
            1,
            "expected exactly 1 merged certification, got: {:?}",
            certs.iter().map(|c| &c.name).collect::<Vec<_>>()
        );
        assert_eq!(certs[0].name, "ITIL: Foundation certification (2011)");
        assert_eq!(certs[0].issuer, "PeopleCert");
        assert_eq!(certs[0].date, "Aug 2018 – No Expiration Date");
    }

    /// Regression test for the "Techs: chip list gets eaten by an unrelated
    /// bullet" bug. tools_row_html (renderer.rs) renders each chip as its
    /// own flex item, which print-to-PDF fragments into one PDF line per
    /// chip (plus one for the "Techs:" label, and one per separating
    /// comma) — this simulates exactly that fragmentation, with an
    /// unterminated (no trailing period) prior bullet immediately before
    /// it, which is what triggers the wrapped-bullet-continuation merge
    /// that was swallowing the whole list. Every tool name must survive as
    /// its own entry in `tools`, and the unrelated prior bullet must come
    /// through completely unmodified — not extended with any of this
    /// content.
    #[test]
    fn parse_experiences_techs_chip_list_survives_fragmentation_after_open_bullet() {
        let lines = vec![
            "Some Role at Acme - Jan 2021 - Present".to_string(),
            "• Cloud AWS1+ yrs".to_string(), // unterminated — no trailing period
            "TECHS:".to_string(),
            "Openstack".to_string(),
            ",".to_string(),
            "Scaleway".to_string(),
            ",".to_string(),
            "Debian".to_string(),
        ];
        let (exps, _skills) = parse_experiences(&lines);
        assert_eq!(exps.len(), 1);
        // No "Project N:" header appears in `lines`, but flush_project()
        // always emits a (possibly unnamed) project to carry whatever
        // bullets/tools/context accumulated — bullets never land directly
        // on the Experience itself, only inside exps[i].projects[j].
        assert_eq!(exps[0].projects.len(), 1);
        let proj = &exps[0].projects[0];
        assert_eq!(
            proj.bullets.len(),
            1,
            "expected exactly the one original bullet, got: {:?}",
            proj.bullets.iter().map(|b| &b.en).collect::<Vec<_>>()
        );
        assert_eq!(
            proj.bullets[0].en, "Cloud AWS1+ yrs",
            "the unrelated prior bullet must not have absorbed any tools-list content"
        );
        assert_eq!(
            proj.skill_ids,
            vec!["Openstack", "Scaleway", "Debian"],
            "every tool name must survive as its own entry (staged in \
             skill_ids pre-resolution — see flush_project's comment), got: {:?}",
            proj.skill_ids
        );
    }

    #[test]
    fn parse_experiences_with_bullets() {
        let lines = vec![
            "Software Engineer at Acme - Jan 2021 - Present".to_string(),
            "• Built APIs".to_string(),
            "• Reduced latency".to_string(),
            "Junior Dev at Beta - 2019 - 2020".to_string(),
            "• Made websites".to_string(),
        ];
        let (exps, _skills) = parse_experiences(&lines);
        assert_eq!(exps.len(), 2);
        assert_eq!(exps[0].role.en, "Software Engineer");
        assert_eq!(exps[0].company, "Acme");
        assert_eq!(exps[0].start_date, "Jan 2021");
        assert_eq!(exps[0].end_date, "Present");
        assert!(!exps[0].id.is_empty());
    }

    /// Regression test: layout (b) — "Company · Location - Start - End" on
    /// one line with the role on its OWN following line. The location picked
    /// up by `split_company_and_location` must survive onto the Experience
    /// (covers the `location` field of the populated struct literal).
    #[test]
    fn parse_experiences_layout_b_location_from_company_line() {
        let lines = vec![
            "Acme Corp · Paris, France - Jan 2021 - Present".to_string(),
            "Software Engineer".to_string(),
            "• Built APIs".to_string(),
        ];
        let (exps, _skills) = parse_experiences(&lines);
        assert_eq!(exps.len(), 1);
        assert_eq!(exps[0].role.en, "Software Engineer");
        assert_eq!(exps[0].company, "Acme Corp");
        assert_eq!(exps[0].location, "Paris, France");
        assert_eq!(exps[0].start_date, "Jan 2021");
        assert_eq!(exps[0].end_date, "Present");
    }

    /// Regression test: a common CV layout puts role, company, and dates on
    /// three SEPARATE lines (rather than one "Role - Date - Date" line).
    /// Before this fix, this pattern was never recognized at all and the
    /// whole Experience section imported empty.
    #[test]
    fn parse_experiences_three_line_role_company_dates() {
        let lines = vec![
            "Platform Engineer (contractual)".to_string(),
            "DTNUM/SDAN/BFO".to_string(),
            "\u{0011} December 2024 – February 2026 ½ Paris, France".to_string(),
            "– Implemented GitOps deployment for the platform.".to_string(),
            "Site Reliability Engineer".to_string(),
            "DTNUM/SDAN/BFO".to_string(),
            "\u{0011} January 2024 – November 2024 ½ Paris, France".to_string(),
            "– Structured SRE practices.".to_string(),
        ];
        let (exps, _skills) = parse_experiences(&lines);
        assert_eq!(exps.len(), 2);
        assert_eq!(exps[0].role.en, "Platform Engineer (contractual)");
        assert_eq!(exps[0].company, "DTNUM/SDAN/BFO");
        assert_eq!(exps[0].start_date, "December 2024");
        assert_eq!(exps[0].end_date, "February 2026");
        assert!(!exps[0].id.is_empty(), "experience id must be non-empty");
        assert_eq!(exps[1].role.en, "Site Reliability Engineer");
        assert_eq!(exps[1].company, "DTNUM/SDAN/BFO");
        assert!(!exps[1].id.is_empty(), "experience id must be non-empty");
    }

    /// Regression test: a "Project N: ..." sub-entry inside a job may have its
    /// own standalone date range. That must NOT be mistaken for a new job —
    /// it should stay attached to the same experience.
    ///
    /// Also regression-tests: a bullet that doesn't end in terminal
    /// punctuation (e.g. one ending in a version number, "* Kubernetes:
    /// 1.20.2 → 1.23.8") must NOT swallow the next "Project N: ..." header
    /// line as a continuation. If it does, that project's own date range
    /// gets misattributed as a brand-new spurious job entry.
    #[test]
    fn project_header_after_non_terminal_bullet_does_not_spawn_spurious_job() {
        let lines = vec![
            "Site Reliability Engineer".to_string(),
            "Sirius".to_string(),
            "\u{0011} October 2022 – January 2024 ½ Bangkok, Thailand".to_string(),
            "Project 2: GitLab Administration".to_string(),
            "\u{0011} February 2023 – December 2023".to_string(),
            "– Migrated GitLab Runners with version".to_string(),
            "upgrades:".to_string(),
            "* Kubernetes: 1.20.2 → 1.23.8".to_string(),
            "Project 3: R&D – Cloud Migration".to_string(),
            "\u{0011} October 2022 – December 2023".to_string(),
            "– Automated migration of 274 nodes.".to_string(),
        ];
        let (exps, _skills) = parse_experiences(&lines);
        assert_eq!(
            exps.len(),
            1,
            "expected exactly one job, got: {:?}",
            exps.iter().map(|e| &e.role.en).collect::<Vec<_>>()
        );
        assert_eq!(exps[0].role.en, "Site Reliability Engineer");
        assert_eq!(exps[0].company, "Sirius");
    }

    /// Regression test: the standalone-date-range line ("Role\nCompany\nDates
    /// Location") also carries a trailing location, which must end up on
    /// the Experience — and, critically, a LATER job's date range must
    /// still correctly start a NEW experience even though an EARLIER job
    /// had its own "Project N:" sub-entries (this is exactly the bug fixed
    /// alongside proper project-name attachment: naively checking "is a
    /// project currently open" instead of "was the immediately preceding
    /// line a project header" caused every job after the first one to be
    /// silently swallowed).
    #[test]
    fn parse_experiences_captures_location_and_still_splits_later_jobs() {
        let lines = vec![
            "Platform Engineer".to_string(),
            "Acme Corp".to_string(),
            "\u{0011} December 2024 – February 2026 ½ Paris, France".to_string(),
            "Project 1: Migration".to_string(),
            "\u{0011} January 2025 – June 2025".to_string(),
            "– Did the migration.".to_string(),
            "Site Reliability Engineer".to_string(),
            "Globex".to_string(),
            "\u{0011} January 2020 – November 2024 ½ Bangkok, Thailand".to_string(),
            "– Kept things running.".to_string(),
        ];
        let (exps, _skills) = parse_experiences(&lines);
        assert_eq!(
            exps.len(),
            2,
            "expected both jobs, got: {:?}",
            exps.iter().map(|e| &e.role.en).collect::<Vec<_>>()
        );
        assert_eq!(exps[0].location, "Paris, France");
        assert_eq!(exps[1].role.en, "Site Reliability Engineer");
        assert_eq!(exps[1].company, "Globex");
        assert_eq!(exps[1].location, "Bangkok, Thailand");
    }

    /// Regression test: a "Project N: ..." header's name must actually be
    /// attached to the ExperienceProject that gets its bullets, and
    /// "Situation:"/"Tasks:"/etc. intro sentences must end up in the
    /// project's own `context` field (not silently discarded, and not
    /// mixed into `bullets`).
    #[test]
    fn parse_experiences_attaches_project_name_and_keeps_context() {
        let lines = vec![
            "Platform Engineer".to_string(),
            "Acme Corp".to_string(),
            "\u{0011} December 2024 – February 2026 ½ Paris, France".to_string(),
            "Project 1: Cloud Migration".to_string(),
            "\u{0011} January 2025 – June 2025".to_string(),
            "Situation: The team needed a cloud migration.".to_string(),
            "– Migrated 50 services to the cloud.".to_string(),
        ];
        let (exps, _skills) = parse_experiences(&lines);
        assert_eq!(exps.len(), 1);
        assert_eq!(exps[0].projects.len(), 1);
        assert_eq!(exps[0].projects[0].name.en, "Project 1: Cloud Migration");
        assert_eq!(
            exps[0].projects[0].context[0].en,
            "Situation: The team needed a cloud migration."
        );
        let bullet_texts: Vec<&str> = exps[0].projects[0]
            .bullets
            .iter()
            .map(|b| b.en.as_str())
            .collect();
        assert_eq!(bullet_texts, vec!["Migrated 50 services to the cloud."]);
    }

    /// Regression test: a wrapped multi-line "Situation & Tasks: ..." intro
    /// paragraph must be merged into ONE coherent context sentence, not
    /// left as several disconnected one-line fragments (each PDF line is a
    /// visually-wrapped row, not a separate sentence).
    #[test]
    fn parse_experiences_merges_wrapped_context_paragraph() {
        let lines = vec![
            "Platform Engineer".to_string(),
            "Acme Corp".to_string(),
            "\u{0011} December 2024 – February 2026 ½ Paris, France".to_string(),
            "Situation & Tasks: As part of the DevOps/SRE transformation within the IT department, I".to_string(),
            "joined the Socle Team of the Cloud π Native project while also acting as the".to_string(),
            "technical lead for the CITADEL platform.".to_string(),
            "Actions taken:".to_string(),
            "– Implemented GitOps deployment.".to_string(),
        ];
        let (exps, _skills) = parse_experiences(&lines);
        assert_eq!(exps.len(), 1);
        assert_eq!(exps[0].projects.len(), 1);
        assert_eq!(
            exps[0].projects[0].context[0].en,
            "Situation & Tasks: As part of the DevOps/SRE transformation within the IT department, I joined the Socle Team of the Cloud π Native project while also acting as the technical lead for the CITADEL platform."
        );
        let bullet_texts: Vec<&str> = exps[0].projects[0]
            .bullets
            .iter()
            .map(|b| b.en.as_str())
            .collect();
        assert_eq!(bullet_texts, vec!["Implemented GitOps deployment."]);
    }

    /// Regression test: a "Techs: A, B, C." line (possibly wrapped across
    /// several PDF lines) must be parsed into project.skill_ids (staged as
    /// raw names pre-resolution — see flush_project's comment) as
    /// individual technology names, not left as raw context text.
    #[test]
    fn parse_experiences_extracts_tools_from_techs_line() {
        let lines = vec![
            "Platform Engineer".to_string(),
            "Acme Corp".to_string(),
            "\u{0011} December 2024 – February 2026 ½ Paris, France".to_string(),
            "– Migrated the platform.".to_string(),
            "Techs: Openstack, Scaleway, Debian, Kyverno, Keycloak, Vault, Redis, CNPG, Kubernetes,".to_string(),
            "Openshift, Docker, Containerd.".to_string(),
        ];
        let (exps, _skills) = parse_experiences(&lines);
        assert_eq!(exps.len(), 1);
        assert_eq!(exps[0].projects.len(), 1);
        assert_eq!(
            exps[0].projects[0].skill_ids,
            vec![
                "Openstack",
                "Scaleway",
                "Debian",
                "Kyverno",
                "Keycloak",
                "Vault",
                "Redis",
                "CNPG",
                "Kubernetes",
                "Openshift",
                "Docker",
                "Containerd",
            ]
        );
    }

    /// Regression test: a sidebar tool/skill list line that bled into
    /// Experience bullets (e.g. from a multi-column layout) must be
    /// detected by its distinctive "<tool> N+ yrs" shape, stripped out of
    /// the bullet list, and harvested as real Skill entries instead.
    #[test]
    fn harvest_skill_segments_splits_tool_year_pairs() {
        let segs =
            harvest_skill_segments("CI/CD GitLab-CI 3+ yrs GitHub Actions 2+ yrs Jenkins 1+ yrs");
        assert_eq!(
            segs,
            Some(vec![
                "CI/CD GitLab-CI 3+ yrs".to_string(),
                "GitHub Actions 2+ yrs".to_string(),
                "Jenkins 1+ yrs".to_string(),
            ])
        );

        // A genuine accomplishment bullet must not match at all.
        assert_eq!(
            harvest_skill_segments("Reduced incidents and improved platform stability via GitOps."),
            None
        );
    }

    #[test]
    fn split_fused_name_and_marker_splits_letters_digits_plus() {
        assert_eq!(
            split_fused_name_and_marker("Kustomize2+"),
            Some(("Kustomize", "2+"))
        );
        assert_eq!(
            split_fused_name_and_marker("Dynatrace10+"),
            Some(("Dynatrace", "10+"))
        );
        // Plain digits-only "2+" is NOT split — that's already handled
        // directly as its own token by harvest_skill_segments.
        assert_eq!(split_fused_name_and_marker("2+"), None);
        // 3+ digit trailing runs are left alone (more likely a version
        // number / product name than a years count).
        assert_eq!(split_fused_name_and_marker("Log4j2023+"), None);
        // No trailing '+' at all.
        assert_eq!(split_fused_name_and_marker("Kubernetes"), None);
        // '+' with nothing digit-like before it.
        assert_eq!(split_fused_name_and_marker("C++"), None);
    }

    /// Regression test for the specific real-world pattern found in
    /// Input_Resume.pdf's own TOOLS sidebar: a tool name fused directly
    /// onto its own "N+" marker with zero space between them (verified
    /// against its actual glyph positions — "Kustomize2+ yrs" is genuinely
    /// how it's typeset, not a reconstruction artifact). Before this fix,
    /// harvest_skill_segments required the "<N>+" marker to be its own
    /// clean token, so "Kustomize2+" never matched at all — the whole line
    /// bled through as if it were narrative text.
    #[test]
    fn harvest_skill_segments_splits_fused_name_and_marker() {
        assert_eq!(
            harvest_skill_segments("Kustomize2+ yrs"),
            Some(vec!["Kustomize 2+ yrs".to_string()])
        );
        // The real case from GeneratedCV.pdf: category header sharing a
        // reconstructed line with the fused entry.
        assert_eq!(
            harvest_skill_segments("TOOLS Kustomize2+ yrs"),
            Some(vec!["TOOLS Kustomize 2+ yrs".to_string()])
        );
        // Multiple fused entries on one line, matching the other observed
        // real case (Cloud category: AWS1+ yrs Scaleway1+ yrs Dynatrace2+
        // yrs).
        assert_eq!(
            harvest_skill_segments("AWS1+ yrs Scaleway1+ yrs Dynatrace2+ yrs"),
            Some(vec![
                "AWS 1+ yrs".to_string(),
                "Scaleway 1+ yrs".to_string(),
                "Dynatrace 2+ yrs".to_string(),
            ])
        );
    }

    #[test]
    fn is_bare_years_marker_matches_fused_and_spaced_forms() {
        // Fused, no space before "yrs" — exactly how Input_Resume.pdf's own
        // TOOLS sidebar typesets it (verified against its glyph positions).
        assert!(is_bare_years_marker("2+yrs"));
        assert!(is_bare_years_marker("10+years"));
        // Spaced form too, in case some other source resume does it this
        // way instead.
        assert!(is_bare_years_marker("2+ yrs"));
        // Must NOT match once a name is attached — that's
        // harvest_skill_segments's job, on the same line.
        assert!(!is_bare_years_marker("Kustomize 2+yrs"));
        assert!(!is_bare_years_marker("Kustomize"));
        assert!(!is_bare_years_marker(""));
        assert!(!is_bare_years_marker("+yrs")); // no digits at all
    }

    /// Regression test for the specific pattern found in Input_Resume.pdf's
    /// own TOOLS sidebar: a tool name and its "N+yrs" badge are the same
    /// visual row, but end up as two separate reconstructed PDF lines
    /// because their glyph "top" coordinates differ by just enough to miss
    /// SAME_ROW_Y_EPSILON (a left-aligned name vs. a right-aligned badge in
    /// a fixed-width column). Before this fix, neither line matched
    /// harvest_skill_segments on its own (the name has no marker; the
    /// marker has no name), so both bled straight through as if they were
    /// genuine narrative text.
    #[test]
    fn parse_experiences_recovers_name_and_marker_split_across_two_lines() {
        let lines = vec![
            "Some Role at Acme - Jan 2021 - Present".to_string(),
            "• A genuine accomplishment bullet that ends properly.".to_string(),
            "Kustomize".to_string(),
            "2+yrs".to_string(),
            "• Another genuine bullet, unrelated to the sidebar noise.".to_string(),
        ];
        let (exps, skills) = parse_experiences(&lines);
        assert_eq!(exps.len(), 1);
        let skill_names: Vec<&str> = skills.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(
            skill_names,
            vec!["Kustomize 2+yrs"],
            "the split name+marker must be recovered as one harvested skill"
        );
        // And critically: "Kustomize" / "2+yrs" must not show up anywhere
        // in the actual bullets — neither as their own bogus bullets nor
        // glued onto a real one.
        let all_bullet_text: String = exps[0]
            .projects
            .iter()
            .flat_map(|p| p.bullets.iter())
            .map(|b| b.en.as_str())
            .collect::<Vec<_>>()
            .join(" | ");
        assert!(!all_bullet_text.contains("Kustomize"));
        assert!(!all_bullet_text.contains("2+yrs"));
        assert!(all_bullet_text.contains("A genuine accomplishment bullet"));
        assert!(all_bullet_text.contains("Another genuine bullet"));
    }

    /// Same as the test above, but for the fused-single-line variant
    /// ("Kustomize2+ yrs" as one reconstructed PDF line, no line split at
    /// all) — the other real pattern verified against Input_Resume.pdf's
    /// own TOOLS sidebar glyph data. Confirms the fix integrates correctly
    /// through the full parse_experiences pipeline, not just at the
    /// harvest_skill_segments unit level.
    #[test]
    fn parse_experiences_recovers_fused_name_and_marker_on_one_line() {
        let lines = vec![
            "Some Role at Acme - Jan 2021 - Present".to_string(),
            "• A genuine accomplishment bullet that ends properly.".to_string(),
            "TOOLS Kustomize2+ yrs".to_string(),
            "• Another genuine bullet, unrelated to the sidebar noise.".to_string(),
        ];
        let (exps, skills) = parse_experiences(&lines);
        assert_eq!(exps.len(), 1);
        let skill_names: Vec<&str> = skills.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(
            skill_names,
            vec!["TOOLS Kustomize 2+ yrs"],
            "the fused name+marker must be recovered as one harvested skill"
        );
        let all_bullet_text: String = exps[0]
            .projects
            .iter()
            .flat_map(|p| p.bullets.iter())
            .map(|b| b.en.as_str())
            .collect::<Vec<_>>()
            .join(" | ");
        assert!(!all_bullet_text.contains("Kustomize"));
        assert!(all_bullet_text.contains("A genuine accomplishment bullet"));
        assert!(all_bullet_text.contains("Another genuine bullet"));
    }

    #[test]
    fn harvest_skills_from_experiences_strips_bled_bullets_and_populates_skills() {
        let mut experiences = vec![Experience {
            id: "1".to_string(),
            role: LocalizedText::same("Platform Engineer"),
            company: "Acme".to_string(),
            projects: vec![ExperienceProject {
                name: LocalizedText::default(),
                bullets: vec![
                    LocalizedText::same("Reduced incidents via GitOps automation."),
                    LocalizedText::same("CI/CD GitLab-CI 3+ yrs GitHub Actions 2+ yrs"),
                ],
                ..Default::default()
            }],
            ..Default::default()
        }];
        let harvested = harvest_skills_from_experiences(&mut experiences);
        assert_eq!(harvested.len(), 2);
        assert!(harvested.iter().any(|s| s.name == "CI/CD GitLab-CI 3+ yrs"));
        assert!(harvested.iter().any(|s| s.name == "GitHub Actions 2+ yrs"));
        // The genuine bullet must remain; the bled-in one must be gone.
        let remaining: Vec<&str> = experiences[0].projects[0]
            .bullets
            .iter()
            .map(|b| b.en.as_str())
            .collect();
        assert_eq!(remaining, vec!["Reduced incidents via GitOps automation."]);
    }

    #[test]
    fn parse_experiences_project_date_range_does_not_split_job() {
        let lines = vec![
            "Platform Engineer".to_string(),
            "Acme Corp".to_string(),
            "\u{0011} December 2024 – February 2026 ½ Paris, France".to_string(),
            "– Led the platform migration.".to_string(),
            "Project 1: Internal Tooling".to_string(),
            "\u{0011} February 2025 – February 2026".to_string(),
            "– Built the internal dashboard.".to_string(),
        ];
        let (exps, _skills) = parse_experiences(&lines);
        assert_eq!(
            exps.len(),
            1,
            "the project's own date range must not create a second job"
        );
        assert_eq!(exps[0].role.en, "Platform Engineer");
        assert_eq!(exps[0].company, "Acme Corp");
        // Regression: this date range used to be silently discarded
        // entirely rather than attached to the project — losing content
        // on every re-import of our own rendered PDF, since our renderer
        // draws this line and a re-import then hits this exact shape.
        let project = exps[0]
            .projects
            .iter()
            .find(|p| p.name.en == "Project 1: Internal Tooling")
            .expect("named project should be present");
        assert_eq!(project.start_date, "February 2025");
        assert_eq!(project.end_date, "February 2026");
    }

    /// Regression test for the bug found in the *next* round of idempotence
    /// testing: once a project's dates parse correctly (see the test
    /// above), our own renderer draws them inline on the SAME line as the
    /// header, i.e. "Project N: Title – Subtitle  Start – End" — and the
    /// title itself often contains its own " – " ("Cloud πNative – Socle
    /// Team"). `extract_date_range_from_end`'s fast path ("another
    /// occurrence of the same separator marks off the start too") found
    /// that internal dash and mistook it for the name/date boundary,
    /// splitting the line into a bogus new job (role = the first half of
    /// the title) instead of leaving it as one project header with its
    /// name intact and dates attached. Confirmed via a hand-built native
    /// harness against the real `pdf_import.rs` on an actual generated PDF
    /// that this exact shape appears once dates are non-empty, and that
    /// re-rendering it broke idempotence a second time.
    #[test]
    fn parse_experiences_inline_project_header_with_internal_dash_does_not_split_job() {
        let lines = vec![
            "Platform Engineer".to_string(),
            "Acme Corp".to_string(),
            "December 2024 – February 2026 ½ Paris, France".to_string(),
            "– Led the platform migration.".to_string(),
            "Project 1: Cloud πNative – Socle Team February 2025 – February 2026".to_string(),
            "– Built the internal dashboard.".to_string(),
        ];
        let (exps, _skills) = parse_experiences(&lines);
        assert_eq!(
            exps.len(),
            1,
            "an inline-dated project header with its own internal dash must not spawn a second job"
        );
        assert_eq!(exps[0].role.en, "Platform Engineer");
        assert_eq!(exps[0].company, "Acme Corp");
        let project = exps[0]
            .projects
            .iter()
            .find(|p| !p.name.en.is_empty())
            .expect("named project should be present");
        assert_eq!(project.name.en, "Project 1: Cloud πNative – Socle Team");
        assert_eq!(project.start_date, "February 2025");
        assert_eq!(project.end_date, "February 2026");
    }

    /// Regression test for the idempotence-breaking bug where a project's
    /// own icon-prefixed date range, with NO space between the icon glyph
    /// and the month name (e.g. "\u{11}February 2025 – February 2026" — as
    /// opposed to "\u{11} February 2025 – ...", which was already handled),
    /// was misread by `extract_date_range_from_end`'s fallback path: unable
    /// to recognize "\u{11}February" as the start of a date, it treated
    /// just the bare "2025" as the start and mistook the icon+month for
    /// leftover role/company text — spawning a bogus, mostly-empty new
    /// "job" (role "\u{11}February", no company) instead of attaching the
    /// date to Project 1 where it belongs. Our own renderer draws this
    /// exact icon-glued-to-month shape for a project header's date line, so
    /// this corrupted every re-import of a PDF we generated ourselves —
    /// the phantom job then got rendered as a stray visible block, and a
    /// second re-import produced yet another different result, breaking
    /// idempotence.
    #[test]
    fn parse_experiences_glued_icon_month_project_date_does_not_spawn_phantom_job() {
        let lines = vec![
            "Platform Engineer".to_string(),
            "Acme Corp".to_string(),
            "\u{0011}December 2024 – February 2026 ½ Paris, France".to_string(),
            "– Led the platform migration.".to_string(),
            "Project 1: Internal Tooling".to_string(),
            "\u{0011}February 2025 – February 2026".to_string(),
            "– Built the internal dashboard.".to_string(),
        ];
        let (exps, _skills) = parse_experiences(&lines);
        assert_eq!(
            exps.len(),
            1,
            "the project's own glued icon+month date range must not spawn a second, phantom job"
        );
        assert_eq!(exps[0].role.en, "Platform Engineer");
        assert_eq!(exps[0].company, "Acme Corp");
        let project = exps[0]
            .projects
            .iter()
            .find(|p| p.name.en == "Project 1: Internal Tooling")
            .expect("named project should be present");
        assert_eq!(project.start_date, "February 2025");
        assert_eq!(project.end_date, "February 2026");
    }

    /// Regression test: a common CV layout puts role, company, and dates on
    /// three SEPARATE lines. Before this fix, this pattern was never
    /// recognized and the whole Experience section imported empty.
    #[test]
    fn parse_experiences_three_line_role_company_dates_persisted() {
        let lines = vec![
            "Platform Engineer (contractual)".to_string(),
            "DTNUM/SDAN/BFO".to_string(),
            "\u{0011} December 2024 – February 2026 ½ Paris, France".to_string(),
            "– Implemented GitOps deployment for the platform.".to_string(),
        ];
        let (exps, _skills) = parse_experiences(&lines);
        assert_eq!(exps.len(), 1);
        assert_eq!(exps[0].role.en, "Platform Engineer (contractual)");
        assert_eq!(exps[0].company, "DTNUM/SDAN/BFO");
        assert_eq!(exps[0].start_date, "December 2024");
        assert_eq!(exps[0].end_date, "February 2026");
    }

    /// Regression test: a long bullet sentence that wraps to 2-3 lines in
    /// the source PDF must be merged back into one bullet, not truncated.
    #[test]
    fn parse_experiences_merges_wrapped_bullet_continuation_lines() {
        let lines = vec![
            "Platform Engineer (contractual)".to_string(),
            "DTNUM/SDAN/BFO".to_string(),
            "\u{0011} December 2024 – February 2026 ½ Paris, France".to_string(),
            "– Implemented GitOps deployment for the Cloud π Native Socle (ArgoCD) with regular"
                .to_string(),
            "version upgrades.".to_string(),
            "Techs: Openstack, Scaleway, Debian.".to_string(),
        ];
        let (exps, _skills) = parse_experiences(&lines);
        assert_eq!(exps.len(), 1);
        let bullets: Vec<&str> = exps[0]
            .projects
            .iter()
            .flat_map(|p| p.bullets.iter().map(|b| b.en.as_str()))
            .collect();
        assert!(bullets.contains(&"Implemented GitOps deployment for the Cloud π Native Socle (ArgoCD) with regular version upgrades."),
            "expected merged bullet, got: {:?}", bullets);
    }

    /// Regression test: a common CV layout puts the degree, a wrapped
    /// field-of-study, and a wrapped institution+country each on their own
    /// lines, followed by a standalone (often abbreviated-month) date
    /// range. Before this fix, the date always completed the WRONG entry
    /// (an off-by-one), institution/field got scrambled, and a spurious
    /// trailing entry with only dates appeared.
    #[test]
    fn parse_education_multi_line_degree_field_institution() {
        let lines = vec![
            "Magistère of Mathematics".to_string(),
            "University of Paris-sud, Orsay,".to_string(),
            "France".to_string(),
            "\u{0011} Sept 2014 – Oct 2017".to_string(),
            "Master of Mathematics (MA)".to_string(),
            "Fundamental and applied".to_string(),
            "Mathematics".to_string(),
            "University of Paris-Saclay, Orsay,".to_string(),
            "France".to_string(),
            "\u{0011} Sept 2015 – May 2017".to_string(),
        ];
        let edus = parse_education(&lines);
        assert_eq!(
            edus.len(),
            2,
            "expected exactly 2 entries, got: {:?}",
            edus.iter().map(|e| &e.degree.en).collect::<Vec<_>>()
        );

        assert_eq!(edus[0].degree.en, "Magistère of Mathematics");
        assert_eq!(
            edus[0].institution,
            "University of Paris-sud, Orsay, France"
        );
        assert_eq!(edus[0].start_year, "Sept 2014");
        assert_eq!(edus[0].end_year, "Oct 2017");

        assert_eq!(edus[1].degree.en, "Master of Mathematics (MA)");
        assert_eq!(edus[1].field.en, "Fundamental and applied Mathematics");
        assert_eq!(
            edus[1].institution,
            "University of Paris-Saclay, Orsay, France"
        );
        assert_eq!(edus[1].start_year, "Sept 2015");
        assert_eq!(edus[1].end_year, "May 2017");
    }

    /// Regression test, education-section counterpart to
    /// `parse_experiences_glued_icon_month_project_date_does_not_spawn_phantom_job`:
    /// with NO space between the icon glyph and the abbreviated month
    /// (e.g. "\u{11}Sept 2014 – Oct 2017"), `extract_trailing_date_range_loose`
    /// used to swallow the whole institution/location line as if it were
    /// "institution text ending in the bare year 2014", leaving
    /// "\u{11}Sept" to be misread as if it were itself the institution
    /// name for start="2014" (dropping the month) — corrupting the
    /// institution and its start date on any PDF (produced by a tool other
    /// than our own renderer, which happens to always put a space there)
    /// that glues the icon directly onto the month.
    #[test]
    fn parse_education_glued_icon_month_date_does_not_corrupt_institution() {
        let lines = vec![
            "Magistère of Mathematics".to_string(),
            "University of Paris-sud, Orsay,".to_string(),
            "France".to_string(),
            "\u{0011}Sept 2014 – Oct 2017".to_string(),
        ];
        let edus = parse_education(&lines);
        assert_eq!(edus.len(), 1, "expected exactly 1 entry, got: {:?}", edus);
        assert_eq!(edus[0].degree.en, "Magistère of Mathematics");
        assert_eq!(
            edus[0].institution,
            "University of Paris-sud, Orsay, France"
        );
        assert_eq!(edus[0].start_year, "Sept 2014");
        assert_eq!(edus[0].end_year, "Oct 2017");
    }

    /// Regression test: "OTHERS"/"INTERESTS" sections must not swallow
    /// whatever section came before them.
    #[test]
    fn ignore_sections_do_not_bleed_into_certifications() {
        let text =
            "CERTIFICATIONS\nITIL Foundation\nOTHERS\nDriving License B\nINTERESTS\nChess\nManga";
        let sections = split_into_sections(text);
        let certs: Vec<&String> = sections
            .iter()
            .filter(|(s, _)| *s == "certifications")
            .flat_map(|(_, l)| l.iter())
            .collect();
        assert_eq!(certs, vec!["ITIL Foundation"]);
        let ignored: Vec<&str> = sections
            .iter()
            .filter(|(s, _)| *s == "ignore")
            .map(|(s, _)| *s)
            .collect();
        assert_eq!(ignored.len(), 2);
    }

    /// Regression test for the real bug: a multi-column PDF interleaves a
    /// sidebar header (e.g. "INTERESTS") into the middle of the document,
    /// stranding entire subsequent job entries in a section that gets
    /// dropped. The reclaim pass must recover them into Experience.
    #[test]
    fn reclaim_stray_experience_content_recovers_stranded_jobs() {
        let sections: Vec<(&str, Vec<String>)> = vec![
            ("header", vec!["Vincent".to_string()]),
            (
                "experience",
                vec![
                    "Site Reliability Engineer".to_string(),
                    "Sirius".to_string(),
                    "\u{0011} October 2022 – January 2024 ½ Bangkok, Thailand".to_string(),
                    "– Maintained IaC on AWS.".to_string(),
                ],
            ),
            (
                "ignore",
                vec![
                    "INTERESTS".to_string(),
                    "Chess".to_string(),
                    "Manga".to_string(),
                    "DevOps Engineer, Database Developer".to_string(),
                    "BRED IT (Thailand) Ltd".to_string(),
                    "\u{0011} May 2021 – October 2022 ½ Bangkok, Thailand".to_string(),
                    "– L2 Linux and Mainframe support.".to_string(),
                ],
            ),
        ];
        let reclaimed = reclaim_stray_experience_content(sections);
        let exp_lines: Vec<&String> = reclaimed
            .iter()
            .filter(|(s, _)| *s == "experience")
            .flat_map(|(_, l)| l.iter())
            .collect();
        assert!(
            exp_lines
                .iter()
                .any(|l| l.as_str() == "DevOps Engineer, Database Developer"),
            "expected the stranded job to be reclaimed into experience, got: {:?}",
            exp_lines
        );
        let (exps, _skills) =
            parse_experiences(&exp_lines.iter().map(|s| s.to_string()).collect::<Vec<_>>());
        assert_eq!(
            exps.len(),
            2,
            "expected both jobs to parse, got: {:?}",
            exps.iter().map(|e| &e.role.en).collect::<Vec<_>>()
        );
        assert_eq!(exps[1].role.en, "DevOps Engineer, Database Developer");
        assert_eq!(exps[1].company, "BRED IT (Thailand) Ltd");
    }

    // ── extract_email ─────────────────────────────────────────────────────

    #[test]
    fn extract_email_returns_none_when_no_dot_after_filter() {
        assert_eq!(extract_email("user@com"), None);
    }

    #[test]
    fn extract_email_filters_trailing_dot_leaving_no_dot() {
        assert_eq!(extract_email("user@."), None);
    }

    #[test]
    fn extract_email_rejects_leading_at() {
        assert_eq!(extract_email("@example.com"), None);
    }

    #[test]
    fn extract_email_rejects_trailing_dot() {
        assert_eq!(extract_email("user@example.com."), None);
    }

    #[test]
    fn extract_email_strips_angle_brackets_and_semicolons() {
        assert_eq!(
            extract_email("<user@host.com>;"),
            Some("user@host.com".to_string())
        );
    }

    #[test]
    fn extract_email_preserves_plus_and_dash_and_underscore() {
        assert_eq!(
            extract_email("a+b-c_d@host.com"),
            Some("a+b-c_d@host.com".to_string())
        );
    }

    #[test]
    fn extract_email_none_when_only_at_sign() {
        assert_eq!(extract_email("@"), None);
    }

    #[test]
    fn extract_email_returns_none_for_empty_input() {
        assert_eq!(extract_email(""), None);
    }

    // ── extract_phone ─────────────────────────────────────────────────────

    #[test]
    fn extract_phone_6_digits_returns_none() {
        assert_eq!(extract_phone("123456"), None);
    }

    #[test]
    fn extract_phone_16_digits_returns_none() {
        assert_eq!(extract_phone("1234567890123456"), None);
    }

    #[test]
    fn extract_phone_exactly_15_digits_with_plus() {
        assert_eq!(
            extract_phone("+123456789012345"),
            Some("+123456789012345".to_string())
        );
    }

    #[test]
    fn extract_phone_no_plus_returns_digits_without_prefix() {
        assert_eq!(
            extract_phone("call 1234567890"),
            Some("1234567890".to_string())
        );
    }

    #[test]
    fn extract_phone_empty_returns_none() {
        assert_eq!(extract_phone(""), None);
    }

    #[test]
    fn extract_phone_exactly_7_digits() {
        assert_eq!(extract_phone("1234567"), Some("1234567".to_string()));
    }

    // ── extract_urls ──────────────────────────────────────────────────────

    #[test]
    fn extract_urls_linkedin_dot_only_sets_linkedin() {
        let (li, _gh, _web) = extract_urls("linkedin.com/in/john");
        assert_eq!(li, Some("linkedin.com/in/john".to_string()));
    }

    #[test]
    fn extract_urls_github_with_profile_path_sets_github() {
        let (_li, gh, _web) = extract_urls("github.com/john");
        assert_eq!(gh, Some("github.com/john".to_string()));
    }

    #[test]
    fn extract_urls_name_github_io_does_not_set_github() {
        let (_li, gh, web) = extract_urls("name.github.io");
        assert!(gh.is_none());
        assert!(web.is_some());
    }

    #[test]
    fn extract_urls_bare_domain_sets_website() {
        let (_li, _gh, web) = extract_urls("falltrades.github.io/path");
        assert_eq!(web, Some("falltrades.github.io/path".to_string()));
    }

    #[test]
    fn extract_urls_http_prefix_sets_website() {
        let (_li, _gh, web) = extract_urls("http://example.com");
        assert_eq!(web, Some("http://example.com".to_string()));
    }

    #[test]
    fn extract_urls_www_prefix_sets_website() {
        let (_li, _gh, web) = extract_urls("www.example.com");
        assert_eq!(web, Some("www.example.com".to_string()));
    }

    #[test]
    fn extract_urls_duplicate_linkedin_not_added_to_website() {
        let (_li, _gh, web) = extract_urls("linkedin.com/in/john linkedin.com/in/john");
        assert!(web.is_none());
    }

    #[test]
    fn extract_urls_all_none_for_empty() {
        let (li, gh, web) = extract_urls("");
        assert!(li.is_none());
        assert!(gh.is_none());
        assert!(web.is_none());
    }

    // ── looks_like_bare_domain ────────────────────────────────────────────

    #[test]
    fn looks_like_bare_domain_host_without_dot_false() {
        assert!(!looks_like_bare_domain("nodotcom"));
    }

    #[test]
    fn looks_like_bare_domain_starts_with_dot_false() {
        assert!(!looks_like_bare_domain(".example.com"));
    }

    #[test]
    fn looks_like_bare_domain_ends_with_dot_false() {
        assert!(!looks_like_bare_domain("example.com."));
    }

    #[test]
    fn looks_like_bare_domain_contains_at_false() {
        assert!(!looks_like_bare_domain("user@host.com"));
    }

    #[test]
    fn looks_like_bare_domain_valid_io() {
        assert!(looks_like_bare_domain("example.io"));
    }

    #[test]
    fn looks_like_bare_domain_valid_fr() {
        assert!(looks_like_bare_domain("example.fr"));
    }

    #[test]
    fn looks_like_bare_domain_unknown_tld_false() {
        assert!(!looks_like_bare_domain("example.xyz123"));
    }

    #[test]
    fn looks_like_bare_domain_with_path() {
        assert!(looks_like_bare_domain("example.com/foo"));
    }

    #[test]
    fn looks_like_bare_domain_hyphen_allowed() {
        assert!(looks_like_bare_domain("my-site.dev"));
    }

    #[test]
    fn looks_like_bare_domain_non_alnum_hyphen_dot_false() {
        assert!(!looks_like_bare_domain("my site.com"));
    }

    // ── looks_like_bare_role_line ─────────────────────────────────────────

    #[test]
    fn looks_like_bare_role_line_empty_false() {
        assert!(!looks_like_bare_role_line(""));
        assert!(!looks_like_bare_role_line("   "));
    }

    #[test]
    fn looks_like_bare_role_line_over_100_chars_false() {
        let line = "A".repeat(101);
        assert!(!looks_like_bare_role_line(&line));
    }

    #[test]
    fn looks_like_bare_role_line_starts_with_bullet_false() {
        assert!(!looks_like_bare_role_line("• Engineer"));
        assert!(!looks_like_bare_role_line("- Developer"));
        assert!(!looks_like_bare_role_line("* Architect"));
    }

    #[test]
    fn looks_like_bare_role_line_starts_with_project_false() {
        assert!(!looks_like_bare_role_line("Project 1: something"));
    }

    #[test]
    fn looks_like_bare_role_line_starts_with_tasks_false() {
        assert!(!looks_like_bare_role_line("Tasks: did stuff"));
    }

    #[test]
    fn looks_like_bare_role_line_starts_with_tools_false() {
        assert!(!looks_like_bare_role_line("Tools: Rust, Go"));
    }

    #[test]
    fn looks_like_bare_role_line_contains_dot_space_false() {
        assert!(!looks_like_bare_role_line("Engineer. Did things"));
    }

    #[test]
    fn looks_like_bare_role_line_lowercase_first_false() {
        assert!(!looks_like_bare_role_line("engineer"));
    }

    #[test]
    fn looks_like_bare_role_line_with_date_range_at_end_false() {
        assert!(!looks_like_bare_role_line("Engineer Jan 2021 - Feb 2022"));
    }

    #[test]
    fn looks_like_bare_role_line_clean_title_true() {
        assert!(looks_like_bare_role_line("Architecte DevOps"));
    }

    #[test]
    fn looks_like_bare_role_line_single_word_title_true() {
        assert!(looks_like_bare_role_line("Engineer"));
    }

    // ── split_company_and_location ────────────────────────────────────────

    #[test]
    fn split_company_and_location_middle_dot() {
        let (c, l) = split_company_and_location("Acme · Paris");
        assert_eq!(c, "Acme");
        assert_eq!(l, "Paris");
    }

    #[test]
    fn split_company_and_location_pipe() {
        let (c, l) = split_company_and_location("Acme | London");
        assert_eq!(c, "Acme");
        assert_eq!(l, "London");
    }

    #[test]
    fn split_company_and_location_comma() {
        let (c, l) = split_company_and_location("Acme, Berlin");
        assert_eq!(c, "Acme");
        assert_eq!(l, "Berlin");
    }

    #[test]
    fn split_company_and_location_no_sep() {
        let (c, l) = split_company_and_location("Acme");
        assert_eq!(c, "Acme");
        assert_eq!(l, "");
    }

    #[test]
    fn split_company_and_location_first_sep_wins() {
        let (c, l) = split_company_and_location("X · Y, Z");
        assert_eq!(c, "X");
        assert_eq!(l, "Y, Z");
    }

    // ── extract_date_range_from_end ───────────────────────────────────────

    #[test]
    fn extract_date_range_from_end_acme_present() {
        assert_eq!(
            extract_date_range_from_end("Acme - Jan 2021 - Present"),
            Some(("Jan 2021".to_string(), "Present".to_string()))
        );
    }

    #[test]
    fn extract_date_range_from_end_two_dates() {
        assert_eq!(
            extract_date_range_from_end("Acme - Jan 2021 - Feb 2022"),
            Some(("Jan 2021".to_string(), "Feb 2022".to_string()))
        );
    }

    #[test]
    fn extract_date_range_from_end_whitespace_sep() {
        assert_eq!(
            extract_date_range_from_end("Company France December 2024 - February 2026"),
            Some(("December 2024".to_string(), "February 2026".to_string()))
        );
    }

    #[test]
    fn extract_date_range_from_end_bare_year() {
        assert_eq!(
            extract_date_range_from_end("Acme Corp - 2021 - 2024"),
            Some(("2021".to_string(), "2024".to_string()))
        );
    }

    #[test]
    fn extract_date_range_from_end_end_over_3_words_skips() {
        assert_eq!(
            extract_date_range_from_end("Acme - Jan 2021 ABCD - Feb 2022"),
            Some(("Jan 2021 ABCD".to_string(), "Feb 2022".to_string()))
        );
    }

    #[test]
    fn extract_date_range_from_end_no_date() {
        assert!(extract_date_range_from_end("Just a plain line").is_none());
    }

    #[test]
    fn extract_date_range_from_end_en_dash_separator() {
        assert_eq!(
            extract_date_range_from_end("Acme – Jan 2021 – Feb 2022"),
            Some(("Jan 2021".to_string(), "Feb 2022".to_string()))
        );
    }

    #[test]
    fn extract_date_range_from_end_actuel_present() {
        assert_eq!(
            extract_date_range_from_end("Acme - Jan 2021 - actuel"),
            Some(("Jan 2021".to_string(), "Present".to_string()))
        );
    }

    // ── tokenize_with_spans ───────────────────────────────────────────────

    #[test]
    fn tokenize_with_spans_empty() {
        assert_eq!(tokenize_with_spans(""), vec![]);
    }

    #[test]
    fn tokenize_with_spans_whitespace_only() {
        assert_eq!(tokenize_with_spans("   "), vec![]);
    }

    #[test]
    fn tokenize_with_spans_leading_trailing_ws() {
        let result = tokenize_with_spans("  hello world  ");
        assert_eq!(result, vec![(2, 7, "hello"), (8, 13, "world")]);
    }

    #[test]
    fn tokenize_with_spans_single_token() {
        let result = tokenize_with_spans("hello");
        assert_eq!(result, vec![(0, 5, "hello")]);
    }

    #[test]
    fn tokenize_with_spans_preserves_byte_spans() {
        let line = "Jan 2021 - Feb 2022";
        let result = tokenize_with_spans(line);
        assert_eq!(result.len(), 5);
        assert_eq!(result[0], (0, 3, "Jan"));
        assert_eq!(result[1], (4, 8, "2021"));
        assert_eq!(result[2], (9, 10, "-"));
        assert_eq!(result[3], (11, 14, "Feb"));
        assert_eq!(result[4], (15, 19, "2022"));
    }

    // ── find_date_range_span ──────────────────────────────────────────────

    #[test]
    fn find_date_range_span_since_month_year() {
        let r = find_date_range_span("Role at Acme Since January 2020");
        assert!(r.is_some());
        let (start, end, s, e) = r.unwrap();
        assert_eq!(s, "January 2020");
        assert_eq!(e, "Present");
        assert_eq!(
            &"Role at Acme Since January 2020"[start..end],
            "Since January 2020"
        );
    }

    #[test]
    fn find_date_range_span_depuis_bare_year() {
        let r = find_date_range_span("Depuis 2019");
        assert!(r.is_some());
        let (_, _, s, e) = r.unwrap();
        assert_eq!(s, "2019");
        assert_eq!(e, "Present");
    }

    #[test]
    fn find_date_range_span_month_year_dash_month_year() {
        let r = find_date_range_span("January 2021 - February 2022");
        assert!(r.is_some());
        let (_, _, s, e) = r.unwrap();
        assert_eq!(s, "January 2021");
        assert_eq!(e, "February 2022");
    }

    #[test]
    fn find_date_range_span_bare_year_dash_bare_year() {
        let r = find_date_range_span("2020 – 2024");
        assert!(r.is_some());
        let (_, _, s, e) = r.unwrap();
        assert_eq!(s, "2020");
        assert_eq!(e, "2024");
    }

    #[test]
    fn find_date_range_span_bare_year_dash_present() {
        let r = find_date_range_span("2020 - Present");
        assert!(r.is_some());
        let (_, _, s, e) = r.unwrap();
        assert_eq!(s, "2020");
        assert_eq!(e, "Present");
    }

    #[test]
    fn find_date_range_span_au_separator() {
        let r = find_date_range_span("January 2021 au February 2022");
        assert!(r.is_some());
        let (_, _, s, e) = r.unwrap();
        assert_eq!(s, "January 2021");
        assert_eq!(e, "February 2022");
    }

    #[test]
    fn find_date_range_span_to_separator() {
        let r = find_date_range_span("January 2021 to February 2022");
        assert!(r.is_some());
        let (_, _, s, e) = r.unwrap();
        assert_eq!(s, "January 2021");
        assert_eq!(e, "February 2022");
    }

    #[test]
    fn find_date_range_span_a_separator() {
        let r = find_date_range_span("January 2021 à February 2022");
        assert!(r.is_some());
        let (_, _, s, e) = r.unwrap();
        assert_eq!(s, "January 2021");
        assert_eq!(e, "February 2022");
    }

    #[test]
    fn find_date_range_span_month_year_sep_present() {
        let r = find_date_range_span("January 2021 – Present");
        assert!(r.is_some());
        let (_, _, s, e) = r.unwrap();
        assert_eq!(s, "January 2021");
        assert_eq!(e, "Present");
    }

    #[test]
    fn find_date_range_span_month_year_sep_bare_year() {
        let r = find_date_range_span("January 2021 - 2022");
        assert!(r.is_some());
        let (_, _, s, e) = r.unwrap();
        assert_eq!(s, "January 2021");
        assert_eq!(e, "2022");
    }

    #[test]
    fn find_date_range_span_no_date_none() {
        assert!(find_date_range_span("Just a plain line").is_none());
    }

    // ── extract_trailing_date_range_from_title ────────────────────────────

    #[test]
    fn extract_trailing_date_range_from_title_month_year_present() {
        assert_eq!(
            extract_trailing_date_range_from_title(
                "Project 1: Title – Subtitle  January 2021 – Present"
            ),
            Some((
                "Project 1: Title – Subtitle".to_string(),
                "January 2021".to_string(),
                "Present".to_string()
            ))
        );
    }

    #[test]
    fn extract_trailing_date_range_from_title_bare_year() {
        assert_eq!(
            extract_trailing_date_range_from_title("My Project 2020 - 2024"),
            Some((
                "My Project".to_string(),
                "2020".to_string(),
                "2024".to_string()
            ))
        );
    }

    #[test]
    fn extract_trailing_date_range_from_title_two_dates() {
        assert_eq!(
            extract_trailing_date_range_from_title("Tool X – Sub  January 2021 - February 2022"),
            Some((
                "Tool X – Sub".to_string(),
                "January 2021".to_string(),
                "February 2022".to_string()
            ))
        );
    }

    #[test]
    fn extract_trailing_date_range_from_title_empty_name_none() {
        assert!(extract_trailing_date_range_from_title("2021-2024").is_none());
    }

    #[test]
    fn extract_trailing_date_range_from_title_no_date_none() {
        assert!(extract_trailing_date_range_from_title("Just a title").is_none());
    }

    #[test]
    fn extract_trailing_date_range_from_title_end_with_three_words() {
        // `end.split_whitespace().count() > 3` must accept an end part of
        // exactly 3 words; flipping `>` to `>=` would skip it.
        assert_eq!(
            extract_trailing_date_range_from_title("My Tool 2021 - Dec 31 2022"),
            Some((
                "My Tool".to_string(),
                "2021".to_string(),
                "Dec 31 2022".to_string()
            ))
        );
    }

    // ── find_date_range_span end-of-token boundary ─────────────────────────
    //
    // These pin the `<` guards that keep a date range from indexing past the
    // last token. Each line ends its date pattern exactly at the last token
    // (no trailing token), so:
    //   - "<" correctly declines (None) — never an out-of-bounds panic;
    //   - mutating "<" to "<=" lets the guard pass and then panics on the
    //     out-of-range token read, killing the mutant.
    #[test]
    fn find_date_range_span_since_month_at_end_is_none() {
        // "Since <Month>" with no following year and no trailing token.
        assert!(find_date_range_span("Since January").is_none());
        assert!(find_date_range_span("Depuis Janvier").is_none());
    }

    #[test]
    fn find_date_range_span_since_bare_word_at_end_is_none() {
        // A bare "Since"/"Depuis" as the very last token: the `t + 1 < len`
        // guard must decline (None), not index past the last token.
        assert!(find_date_range_span("Depuis").is_none());
        assert!(find_date_range_span("Since").is_none());
        // Non-year token right after "Since" is not a date either.
        assert!(find_date_range_span("Depuis quelques").is_none());
    }

    #[test]
    fn find_date_range_span_month_year_sep_at_end_is_none() {
        // Month-Year separator with the month exactly at the last token.
        assert!(find_date_range_span("January – ").is_none());
        // Month at end, separator at end (no second date).
        assert!(find_date_range_span("January 2021 – ").is_none());
        assert!(find_date_range_span("January 2021 – February").is_none());
    }

    #[test]
    fn find_date_range_span_bare_year_sep_at_end_is_none() {
        // Bare-year separator with the year at the very last token.
        assert!(find_date_range_span("2020 – ").is_none());
        // Year at end (a single date).
        assert!(find_date_range_span("2020").is_none());
    }

    #[test]
    fn find_date_range_span_since_non_date_is_none() {
        // A "since"/"depuis" word followed by non-date tokens must not be
        // parsed as a range (mutating the range's inner `&&` to `||` would
        // wrongly accept it).
        assert!(find_date_range_span("Role Depuis Acme Corp").is_none());
        assert!(find_date_range_span("Since Acme").is_none());
    }

    // ── guess_title ────────────────────────────────────────────────────────

    #[test]
    fn guess_title_uses_line_after_first_non_header_contact() {
        // The title must be scanned starting on the line right after the
        // detected name/contact, not from the top, and must not mistake the
        // first contact/URL line for the boundary.
        assert_eq!(
            guess_title(&[
                "john@test.com",
                "http://example.com",
                "Jane Doe",
                "Senior Engineer",
                "Anything"
            ]),
            Some("Senior Engineer".to_string())
        );
    }

    #[test]
    fn guess_title_scans_up_to_four_lines_after_name() {
        assert_eq!(
            guess_title(&[
                "Jane Doe",
                "line one",
                "line two",
                "line three",
                "engineering manager",
            ]),
            Some("engineering manager".to_string())
        );
    }

    #[test]
    fn guess_title_name_line_is_not_itself_a_title() {
        // A name line that happens to contain a title keyword must not be
        // reported as the title when a following line carries the real one.
        assert_eq!(
            guess_title(&["Alice Engineer", "Architect"]),
            Some("Architect".to_string())
        );
    }

    #[test]
    fn extract_date_range_present() {
        assert_eq!(
            extract_date_range("Jan 2021 - Present"),
            Some(("Jan 2021".to_string(), "Present".to_string()))
        );
    }

    #[test]
    fn extract_date_range_bare_years() {
        assert_eq!(
            extract_date_range("2020 - 2024"),
            Some(("2020".to_string(), "2024".to_string()))
        );
    }

    #[test]
    fn extract_date_range_left_too_short() {
        assert!(extract_date_range("A - B").is_none());
    }

    #[test]
    fn extract_date_range_left_exactly_three_chars() {
        // `left.len() < 3` must accept a left of exactly 3 chars; flipping
        // `<` to `<=` would reject it.
        assert_eq!(
            extract_date_range("Jan - Feb"),
            Some(("Jan".to_string(), "Feb".to_string()))
        );
    }

    #[test]
    fn extract_date_range_to_separator() {
        assert_eq!(
            extract_date_range("Jan 2021 to Dec 2022"),
            Some(("Jan 2021".to_string(), "Dec 2022".to_string()))
        );
    }

    #[test]
    fn extract_date_range_a_separator() {
        assert_eq!(
            extract_date_range("Jan 2021 à Dec 2022"),
            Some(("Jan 2021".to_string(), "Dec 2022".to_string()))
        );
    }

    #[test]
    fn extract_date_range_au_separator() {
        assert_eq!(
            extract_date_range("Jan 2021 au Dec 2022"),
            Some(("Jan 2021".to_string(), "Dec 2022".to_string()))
        );
    }

    #[test]
    fn extract_date_range_fr_present_words() {
        assert_eq!(
            extract_date_range("Jan 2021 - actuel"),
            Some(("Jan 2021".to_string(), "Present".to_string()))
        );
        assert_eq!(
            extract_date_range("Jan 2021 - aujourd'hui"),
            Some(("Jan 2021".to_string(), "Present".to_string()))
        );
        assert_eq!(
            extract_date_range("Jan 2021 - current"),
            Some(("Jan 2021".to_string(), "Present".to_string()))
        );
    }

    // ── guess_name ────────────────────────────────────────────────────────

    #[test]
    fn guess_name_picks_first_plausible_line() {
        assert_eq!(
            guess_name(&["Alice Bob", "Engineer", "alice@test.com"]),
            Some("Alice Bob".to_string())
        );
    }

    #[test]
    fn guess_name_skips_empty() {
        assert_eq!(
            guess_name(&["", "  ", "Charlie D"]),
            Some("Charlie D".to_string())
        );
    }

    #[test]
    fn guess_name_skips_section_header() {
        assert_eq!(
            guess_name(&["Experience", "Dana F"]),
            Some("Dana F".to_string())
        );
    }

    #[test]
    fn guess_name_skips_email() {
        assert_eq!(
            guess_name(&["bob@test.com", "Eve G"]),
            Some("Eve G".to_string())
        );
    }

    #[test]
    fn guess_name_skips_phone() {
        assert_eq!(
            guess_name(&["+33 6 12 34 56 78", "Frank H"]),
            Some("Frank H".to_string())
        );
    }

    #[test]
    fn guess_name_skips_http() {
        assert_eq!(
            guess_name(&["http://example.com", "Grace I"]),
            Some("Grace I".to_string())
        );
    }

    #[test]
    fn guess_name_skips_www() {
        assert_eq!(
            guess_name(&["www.example.com", "Hank J"]),
            Some("Hank J".to_string())
        );
    }

    #[test]
    fn guess_name_skips_linkedin() {
        assert_eq!(
            guess_name(&["linkedin.com/in/john", "Iris K"]),
            Some("Iris K".to_string())
        );
    }

    #[test]
    fn guess_name_too_many_words_none() {
        assert!(guess_name(&["One Two Three Four Five Six"]).is_none());
    }

    #[test]
    fn guess_name_one_word_none() {
        assert!(guess_name(&["Engineer"]).is_none());
    }

    #[test]
    fn guess_name_accented_chars_allowed() {
        assert_eq!(
            guess_name(&["Jean-Luc François"]),
            Some("Jean-Luc François".to_string())
        );
    }

    #[test]
    fn guess_name_control_chars_cleaned() {
        assert_eq!(
            guess_name(&["John\u{0003} Smith"]),
            Some("John Smith".to_string())
        );
    }

    // ── guess_title ───────────────────────────────────────────────────────

    #[test]
    fn guess_title_finds_keyword_after_name() {
        assert_eq!(
            guess_title(&["John Smith", "Senior Engineer"]),
            Some("Senior Engineer".to_string())
        );
    }

    #[test]
    fn guess_title_finds_french_keyword() {
        assert_eq!(
            guess_title(&["Jean Dupont", "Développeur Rust"]),
            Some("Développeur Rust".to_string())
        );
    }

    #[test]
    fn guess_title_none_when_no_keyword() {
        assert!(guess_title(&["John Smith", "Nothing here"]).is_none());
    }

    #[test]
    fn guess_title_skips_email_before_name() {
        assert_eq!(
            guess_title(&["john@test.com", "Jane Doe", "Architect"]),
            Some("Architect".to_string())
        );
    }

    // ── looks_like_date_token ─────────────────────────────────────────────

    #[test]
    fn looks_like_date_token_month() {
        assert!(looks_like_date_token("January"));
        assert!(looks_like_date_token("janvier"));
        assert!(looks_like_date_token("août"));
    }

    #[test]
    fn looks_like_date_token_bare_year() {
        assert!(looks_like_date_token("2021"));
    }

    #[test]
    fn looks_like_date_token_present_word() {
        assert!(looks_like_date_token("Present"));
        assert!(looks_like_date_token("actuel"));
    }

    #[test]
    fn looks_like_date_token_leading_icon_glyph() {
        assert!(looks_like_date_token("\u{11}January"));
    }

    #[test]
    fn looks_like_date_token_non_date_false() {
        assert!(!looks_like_date_token("hello"));
    }

    #[test]
    fn looks_like_date_token_empty_after_strip_false() {
        assert!(!looks_like_date_token("!."));
    }

    #[test]
    fn looks_like_date_token_abbreviated_month_not_recognized() {
        assert!(!looks_like_date_token("Jan"));
    }

    // ── looks_like_date_token_loose ───────────────────────────────────────

    #[test]
    fn looks_like_date_token_loose_full_month_passthrough() {
        assert!(looks_like_date_token_loose("January"));
    }

    #[test]
    fn looks_like_date_token_loose_abbreviated_month() {
        assert!(looks_like_date_token_loose("Sept"));
        assert!(looks_like_date_token_loose("janv"));
    }

    #[test]
    fn looks_like_date_token_loose_with_leading_icon() {
        assert!(looks_like_date_token_loose("\u{11}Sept"));
    }

    #[test]
    fn looks_like_date_token_loose_non_date_false() {
        assert!(!looks_like_date_token_loose("hello"));
    }

    // ── looks_like_institution_line ───────────────────────────────────────

    #[test]
    fn looks_like_institution_line_iut_space() {
        assert!(looks_like_institution_line("IUT Informatique"));
    }

    #[test]
    fn looks_like_institution_line_iut_exact() {
        assert!(looks_like_institution_line("IUT"));
    }

    #[test]
    fn looks_like_institution_line_iut_lowercase() {
        assert!(looks_like_institution_line("iut paris"));
    }

    #[test]
    fn looks_like_institution_line_university_of() {
        assert!(looks_like_institution_line("University of Cambridge"));
    }

    #[test]
    fn looks_like_institution_line_ecole() {
        assert!(looks_like_institution_line("École Supérieure"));
    }

    #[test]
    fn looks_like_institution_line_college() {
        assert!(looks_like_institution_line("Community College"));
    }

    #[test]
    fn looks_like_institution_line_non_institution_false() {
        assert!(!looks_like_institution_line("Computer Science"));
    }

    // ── looks_like_degree_line ────────────────────────────────────────────

    #[test]
    fn looks_like_degree_line_licence() {
        assert!(looks_like_degree_line(
            "Licence Professionnelle Informatique"
        ));
    }

    #[test]
    fn looks_like_degree_line_master_of_science() {
        assert!(looks_like_degree_line("Master of Science in AI"));
    }

    #[test]
    fn looks_like_degree_line_bts() {
        assert!(looks_like_degree_line("BTS Services Informatiques"));
    }

    #[test]
    fn looks_like_degree_line_mba() {
        assert!(looks_like_degree_line("MBA Finance"));
    }

    #[test]
    fn looks_like_degree_line_phd() {
        assert!(looks_like_degree_line("PhD in Physics"));
    }

    #[test]
    fn looks_like_degree_line_doctorat() {
        assert!(looks_like_degree_line("Doctorat Informatique"));
    }

    #[test]
    fn looks_like_degree_line_diplome() {
        assert!(looks_like_degree_line("Diplôme d'ingénieur"));
    }

    #[test]
    fn looks_like_degree_line_certificat() {
        assert!(looks_like_degree_line("Certificat AWS"));
    }

    #[test]
    fn looks_like_degree_line_non_degree_false() {
        assert!(!looks_like_degree_line("University of Paris"));
    }

    // ── is_context_label ──────────────────────────────────────────────────

    #[test]
    fn is_context_label_with_situation_prefix() {
        assert!(is_context_label("Situation: The project needed help."));
    }

    #[test]
    fn is_context_label_with_techs_prefix() {
        assert!(is_context_label("Techs: Rust, Go, Docker."));
    }

    #[test]
    fn is_context_label_with_context_prefix() {
        assert!(is_context_label("Context: Cloud migration project."));
    }

    #[test]
    fn is_context_label_no_colon_false() {
        assert!(!is_context_label("No colon here"));
    }

    #[test]
    fn is_context_label_prefix_too_long_false() {
        assert!(!is_context_label(
            "A very long prefix that exceeds thirty chars: value"
        ));
    }

    #[test]
    fn is_context_label_unknown_prefix_false() {
        assert!(!is_context_label("RandomWord: value"));
    }

    #[test]
    fn is_context_label_lowercase_situation() {
        assert!(is_context_label("situation: details"));
    }

    // ── looks_like_tool_bleed_line ────────────────────────────────────────

    #[test]
    fn looks_like_tool_bleed_line_bullet_true() {
        assert!(looks_like_tool_bleed_line("• Docker 3+ yrs"));
    }

    #[test]
    fn looks_like_tool_bleed_line_harvestable_skill_true() {
        assert!(looks_like_tool_bleed_line("Rust 5+ yrs Go 3+ yrs"));
    }

    #[test]
    fn looks_like_tool_bleed_line_bare_years_true() {
        assert!(looks_like_tool_bleed_line("2+yrs"));
    }

    #[test]
    fn looks_like_tool_bleed_line_clean_role_false() {
        assert!(!looks_like_tool_bleed_line("Architecte DevOps"));
    }

    #[test]
    fn looks_like_tool_bleed_line_en_dash_bullet_true() {
        assert!(looks_like_tool_bleed_line("– Docker"));
    }

    // ── is_bare_years_marker ──────────────────────────────────────────────

    #[test]
    fn is_bare_years_marker_fused_two_token() {
        assert!(is_bare_years_marker("2+ yrs"));
        assert!(is_bare_years_marker("10+ years"));
        assert!(is_bare_years_marker("1+ yr"));
        assert!(is_bare_years_marker("5+ year"));
    }

    #[test]
    fn is_bare_years_marker_single_token_fused() {
        assert!(is_bare_years_marker("2+yrs"));
        assert!(is_bare_years_marker("10+years"));
    }

    #[test]
    fn is_bare_years_marker_no_digits_false() {
        assert!(!is_bare_years_marker("+yrs"));
    }

    #[test]
    fn is_bare_years_marker_empty_false() {
        assert!(!is_bare_years_marker(""));
    }

    #[test]
    fn is_bare_years_marker_three_tokens_false() {
        assert!(!is_bare_years_marker("2+ yrs extra"));
    }

    #[test]
    fn is_bare_years_marker_name_with_marker_false() {
        assert!(!is_bare_years_marker("Kustomize 2+yrs"));
    }

    // ── is_project_header ─────────────────────────────────────────────────

    #[test]
    fn is_project_header_english() {
        assert!(is_project_header("Project 1: Cloud Migration"));
    }

    #[test]
    fn is_project_header_french() {
        assert!(is_project_header("Projet 2: Migración"));
    }

    #[test]
    fn is_project_header_non_project() {
        assert!(!is_project_header("Just a regular line"));
    }

    // ── parse_projects (delete-field mutants) ─────────────────────────────

    #[test]
    fn parse_projects_name_with_description() {
        let projects = parse_projects(&[
            "My App: A tool for tracking things".to_string(),
            "• helps you stay organised".to_string(),
        ]);
        assert_eq!(projects.len(), 1);
        assert_eq!(projects[0].name, "My App");
        assert_eq!(projects[0].description.en, "A tool for tracking things");
        assert_eq!(projects[0].bullets.len(), 1);
        assert!(!projects[0].id.is_empty(), "project id must be populated");
    }

    #[test]
    fn parse_projects_bare_name() {
        let projects = parse_projects(&["Side Project".to_string()]);
        assert_eq!(projects.len(), 1);
        assert_eq!(projects[0].name, "Side Project");
        assert!(projects[0].description.en.is_empty());
        assert!(!projects[0].id.is_empty(), "project id must be populated");
    }

    #[test]
    fn parse_projects_multiple_and_bullet_context() {
        let projects = parse_projects(&[
            "Alpha: first".to_string(),
            "• bullet one".to_string(),
            "Beta".to_string(),
            "• bullet two".to_string(),
        ]);
        assert_eq!(projects.len(), 2);
        assert_eq!(projects[0].name, "Alpha");
        assert_eq!(projects[0].bullets.len(), 1);
        assert_eq!(projects[0].bullets[0].en, "bullet one");
        assert_eq!(projects[1].name, "Beta");
        assert_eq!(projects[1].bullets.len(), 1);
    }

    // ── build_education_institution_first (delete-field mutants) ──────────

    #[test]
    fn build_education_institution_first_all_fields() {
        let edu = build_education_institution_first(
            "Université Paris-Sud |".to_string(),
            "Sept 2014".to_string(),
            "Oct 2017".to_string(),
            &[
                "Master of Science in Computer Science".to_string(),
                "Algorithm Design".to_string(),
            ],
        )
        .expect("education should build");
        assert!(!edu.id.is_empty(), "education id must be populated");
        assert_eq!(edu.institution, "Université Paris-Sud");
        assert_eq!(edu.degree.en, "Master of Science");
        assert_eq!(edu.field.en, "Computer Science Algorithm Design");
        assert_eq!(edu.start_year, "Sept 2014");
        assert_eq!(edu.end_year, "Oct 2017");
    }

    #[test]
    fn build_education_institution_first_embedded_field_en() {
        let edu = build_education_institution_first(
            "MIT".to_string(),
            "2015".to_string(),
            "2019".to_string(),
            &["Bachelor of Arts in Economics".to_string()],
        )
        .expect("education should build");
        assert_eq!(edu.degree.en, "Bachelor of Arts");
        assert_eq!(edu.field.en, "Economics");
    }

    #[test]
    fn build_education_institution_first_embedded_field_fr_and_none() {
        let edu = build_education_institution_first(
            "ENS".to_string(),
            "2010".to_string(),
            "2013".to_string(),
            &["Licence en Mathématiques".to_string()],
        )
        .expect("education should build");
        assert_eq!(edu.degree.en, "Licence");
        assert_eq!(edu.field.en, "Mathématiques");

        let bare = build_education_institution_first(
            "College".to_string(),
            "2000".to_string(),
            "2004".to_string(),
            &["Diploma".to_string()],
        )
        .expect("education should build");
        assert_eq!(bare.degree.en, "Diploma");
        assert!(bare.field.en.is_empty());
    }

    #[test]
    fn build_education_institution_first_empty_returns_none() {
        assert!(build_education_institution_first(
            String::new(),
            String::new(),
            String::new(),
            &[],
        )
        .is_none());
    }

    // ── build_certification_from_buffer (delete-field mutants) ────────────

    #[test]
    fn build_certification_from_buffer_all_fields() {
        let cert = build_certification_from_buffer(
            &[
                "AWS Solutions Architect".to_string(),
                "2021".to_string(),
                "Amazon".to_string(),
                "Coursera".to_string(),
            ],
            Some(("Aug 2021".to_string(), "Aug 2023".to_string())),
        )
        .expect("certification should build");
        assert!(!cert.id.is_empty(), "certification id must be populated");
        assert_eq!(cert.name, "AWS Solutions Architect (2021)");
        assert_eq!(cert.issuer, "Amazon · Coursera");
        assert_eq!(cert.date, "Aug 2021 – Aug 2023");
    }

    /// Layout (c) of `parse_experiences`: a date-range row that carries
    /// only a leading separator and a location before the dates, e.g.
    /// "· Paris, France Jan 2024 – Nov 2024". The resulting experience has
    /// a non-empty location and start/end dates; deleting any of those
    /// struct fields must be caught.
    #[test]
    fn parse_experiences_layout_c_location_and_dates() {
        let (exps, _skills) = parse_experiences(&[
            "Software Engineer".to_string(),
            "ACME Corp".to_string(),
            "· Paris, France Jan 2024 – Nov 2024".to_string(),
        ]);
        assert_eq!(exps.len(), 1);
        assert_eq!(exps[0].location, "Paris, France");
        assert_eq!(exps[0].start_date, "Jan 2024");
        assert_eq!(exps[0].end_date, "Nov 2024");
        assert!(!exps[0].id.is_empty(), "experience id must be populated");
    }

    // ── id-population across builders (delete id-field mutants) ───────────

    #[test]
    fn built_structs_populate_ids() {
        let (exps, _skills) = parse_experiences(&[
            "Platform Engineer (contractual)".to_string(),
            "DTNUM/SDAN/BFO".to_string(),
            "Dec 2024 – Feb 2026".to_string(),
        ]);
        assert!(!exps[0].id.is_empty(), "experience id must be populated");

        let edu = build_education_from_buffer(
            &[
                "BSc".to_string(),
                "in Mathematics".to_string(),
                "Univ".to_string(),
            ],
            "2017".to_string(),
            "2020".to_string(),
        )
        .expect("education should build");
        assert!(!edu.id.is_empty(), "education id must be populated");
    }
}

/// Regression corpus: full-pipeline tests against fixture documents, each
/// modeling a real multi-column PDF-extraction failure pattern found in
/// production CVs (see fixture file headers for provenance). Unlike the
/// narrow unit tests above, which pin down one function's behavior on a
/// hand-built input, these run the *entire* `parse_cv` pipeline against a
/// realistic full document and assert on structural invariants — the
/// general properties a correct parse must have — rather than exact
/// expected values. That makes them resilient to future refactoring
/// inside the pipeline while still catching the class of bug each fixture
/// was built to reproduce, including in documents that aren't
/// byte-for-byte identical to the original.
///
/// Fixture text is anonymized: it preserves the exact structural shape
/// that triggered each bug (section-header placement, sidebar bleed
/// position, line wrapping, column-interleaving) but replaces real
/// people's names, employers, and contact details with placeholders.
/// Real, unanonymized PDFs should NOT be added to this fixture directory
/// if this repository is or could become public — see
/// `tests/fixtures/pdf_import/README.md` for the local-corpus workflow
/// for testing against real files without committing them.
#[cfg(test)]
mod regression_corpus {
    use super::*;

    fn fixture(name: &str) -> String {
        // Fixtures live at `tests/fixtures/pdf_import/<name>` relative to
        // the crate root, alongside (not inside) `src/`, matching normal
        // Rust convention for integration-test data. `include_str!` reads
        // relative to *this source file*, so we go up to the crate root
        // first.
        match name {
            "sidebar_skills_bleed.txt" => {
                include_str!("../../tests/fixtures/pdf_import/sidebar_skills_bleed.txt").to_string()
            }
            "trait_list_bleed.txt" => {
                include_str!("../../tests/fixtures/pdf_import/trait_list_bleed.txt").to_string()
            }
            "multi_language_single_line.txt" => {
                include_str!("../../tests/fixtures/pdf_import/multi_language_single_line.txt")
                    .to_string()
            }
            other => panic!("unknown fixture: {other}"),
        }
    }

    /// No skill entry should read like a sentence fragment (proper nouns
    /// and short tags only). This is the general invariant behind the
    /// "TECHNICAL SKILLS heading absorbs a stray Project's Situation/
    /// Actions/Results bullets" bug class: whatever the specific cause,
    /// the symptom is always prose ending up in the Skills list.
    fn assert_no_prose_fragments_in_skills(cv: &LifetimeCV) {
        for skill in &cv.skills {
            let word_count = skill.name.split_whitespace().count();
            assert!(
                word_count <= 8 && !skill.name.ends_with('.'),
                "skills list contains a prose fragment, not a skill tag: {:?}",
                skill.name
            );
        }
    }

    /// No (role, company) pair should appear as the header of more than
    /// one Experience entry. This is the general invariant behind the
    /// "sidebar bleed splits a job's role+company from its date, and two
    /// independent recovery mechanisms both claim it" bug class.
    fn assert_no_duplicate_experience_headers(cv: &LifetimeCV) {
        let mut seen = std::collections::HashSet::new();
        for exp in &cv.experiences {
            let key = (exp.role.en.clone(), exp.company.clone());
            assert!(
                seen.insert(key.clone()),
                "duplicate experience header appears twice: {key:?}"
            );
        }
    }

    /// Every experience's context/bullets should stay under a plausible
    /// length and not visibly run two different jobs' text together.
    /// Catches the "next job's role+company gets glued onto the tail of
    /// this job's last project's context" bleed pattern.
    fn assert_no_cross_job_bleed(cv: &LifetimeCV) {
        for exp in &cv.experiences {
            let other_roles: Vec<&str> = cv
                .experiences
                .iter()
                .map(|e| e.role.en.as_str())
                .filter(|r| *r != exp.role.en)
                .collect();
            for project in &exp.projects {
                for other_role in &other_roles {
                    if other_role.is_empty() {
                        continue;
                    }
                    for c in &project.context {
                        assert!(
                            !c.en.contains(other_role),
                            "job {:?}'s project {:?} context contains another job's \
                             role text {:?} — looks like cross-job bleed: {:?}",
                            exp.role.en,
                            project.name.en,
                            other_role,
                            c.en
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn regression_sidebar_skills_bleed_keeps_projects_under_correct_job() {
        // Real-world source: a two-column CV where a "TECHNICAL SKILLS" /
        // "TOOLS" sidebar interleaves mid-page into the main narrative
        // column, landing between a job's Project 1 and Project 2. This
        // used to (a) swallow Project 2 and Project 3's entire narrative
        // into the Skills section, mangled into pseudo-skill fragments by
        // `parse_skills`'s comma-join logic, and (b) once fixed to
        // reclaim that content back into Experience, duplicate the next
        // job's role+company header, because `split_into_sections`'s own
        // resumption recovery *also* independently recovers it.
        let cv = parse_cv(&fixture("sidebar_skills_bleed.txt"));

        assert_eq!(cv.experiences.len(), 2, "expected both jobs to parse");
        let job1 = &cv.experiences[0];
        assert_eq!(job1.role.en, "Platform Engineer (contractual)");
        assert_eq!(job1.company, "ACME/WIDGETS/QA");

        let project_names: Vec<&str> = job1
            .projects
            .iter()
            .map(|p| p.name.en.as_str())
            .filter(|n| !n.is_empty())
            .collect();
        assert_eq!(
            project_names,
            vec![
                "Project 1: Core – Socle Team",
                "Project 2: Zenith – Platform Engineering",
                "Project 3: Cross-cutting Initiatives and Strategic Support",
            ],
            "all three projects must stay nested under job 1, in order"
        );

        // The bullet that used to get orphaned mid-Skills (wrapped across
        // two physical lines, the second with no bullet marker of its
        // own) must land back in Project 1's bullets, not in Skills.
        let project1_bullets: Vec<&str> = job1.projects[1]
            .bullets
            .iter()
            .map(|b| b.en.as_str())
            .collect();
        assert!(
            project1_bullets.iter().any(|b| b.contains("ADR framework")),
            "the wrapped ADR bullet should be recovered into Project 1's \
             bullets, got: {project1_bullets:?}"
        );

        assert_no_prose_fragments_in_skills(&cv);
        assert_no_duplicate_experience_headers(&cv);
        assert_no_cross_job_bleed(&cv);
    }

    #[test]
    fn regression_trait_list_bleed_does_not_drop_bullets() {
        // Real-world source: a "RANDOM SKILLS" sidebar (personality-trait
        // tags, not real skills) interrupts a job's own Actions-taken
        // bullet list — not a sub-project's, the job's own top-level
        // list. This used to silently drop the four action bullets that
        // came *after* the interruption: they landed in the "ignore"
        // bucket (correctly, for the trait tags) but took genuine
        // content down with them, since the whole run was treated as one
        // undifferentiated block up to the next recognized boundary.
        let cv = parse_cv(&fixture("trait_list_bleed.txt"));

        assert_eq!(cv.experiences.len(), 1);
        let job = &cv.experiences[0];
        assert_eq!(job.role.en, "Site Reliability Engineer");
        assert_eq!(job.company, "Nimbus");

        let bullets: Vec<&str> = job.projects[0]
            .bullets
            .iter()
            .map(|b| b.en.as_str())
            .collect();
        for expected in [
            "Maintain IaC with Terraform.",
            "Automated repetitive tasks using Ansible, GitLab-CI, Bash.",
            "Supported developers through self-service tooling and documentation.",
            "Participated in on-call rotations, RCAs, and post-mortems.",
        ] {
            assert!(
                bullets.contains(&expected),
                "expected bullet {expected:?} to survive the RANDOM SKILLS \
                 interruption, got: {bullets:?}"
            );
        }

        // The trait tags themselves must NOT show up as bullets or
        // skills — they're genuinely not part of the CV.
        for junk in ["Jack of all Trades", "Fearless Frontliner", "Break things"] {
            assert!(
                !bullets.iter().any(|b| b.contains(junk)),
                "trait-list junk {junk:?} leaked into bullets: {bullets:?}"
            );
            assert!(
                !cv.skills.iter().any(|s| s.name.contains(junk)),
                "trait-list junk {junk:?} leaked into skills"
            );
        }

        assert_no_duplicate_experience_headers(&cv);
    }

    #[test]
    fn regression_multi_language_single_line_recovers_all_languages() {
        // Real-world source: this project's own CV renderer packs every
        // language onto a single output line as repeated "Name (Level)"
        // segments. Re-importing a CV this renderer generated used to
        // keep only the first language on any such line — a same-app
        // round-trip data-loss bug, not a third-party-PDF quirk.
        let cv = parse_cv(&fixture("multi_language_single_line.txt"));

        let langs: Vec<(&str, &LanguageLevel)> = cv
            .languages
            .iter()
            .map(|l| (l.name.as_str(), &l.level))
            .collect();
        assert_eq!(
            langs,
            vec![
                ("Français", &LanguageLevel::Native),
                ("Anglais", &LanguageLevel::Conversational),
            ]
        );
    }

    #[test]
    fn regression_competency_bullets_with_prose_commas_stay_intact() {
        // Real-world source: a sidebar of full-sentence competency bullets
        // (each a "Verb, verb, verb object" phrase, wrapped across 2-3
        // physical lines by the PDF layout), not a flat list of short
        // "Name N+ yrs" tags. The old block-join logic decided whether to
        // comma-split an entire multi-hundred-word block based only on
        // "does *any* line in it contain a comma" — true here, since these
        // are full sentences — which joined the whole block with spaces
        // and comma-split it as if every comma were a skill separator.
        // Since the first comma doesn't appear until deep into the block,
        // every short standalone tag before it (category headers, soft
        // skills) got fused into one giant run-on "skill" alongside the
        // start of the first real sentence.
        let lines: Vec<String> = [
            "Qualités humaines",
            "Curiosité",
            "Rigoureux",
            "Concevoir, déployer, sécuriser des infrastructures cloud et",
            "on-premise",
            "Administrer des bases de données MySQL/MariaDB,",
            "PostgreSQL, Elasticsearch, OpenSearch, MongoDB",
        ]
        .into_iter()
        .map(String::from)
        .collect();

        let skills = parse_skills(&lines);
        let names: Vec<&str> = skills.iter().map(|s| s.name.as_str()).collect();

        // Short standalone tags stay separate, one per entry — this part
        // already worked before the fix and must keep working.
        assert!(names.contains(&"Qualités humaines"));
        assert!(names.contains(&"Curiosité"));
        assert!(names.contains(&"Rigoureux"));

        // Each wrapped competency sentence survives as ONE entry, physical
        // line-wrap rejoined, prose commas intact — not shredded into
        // word-fragments by comma, and not fused with the unrelated short
        // tags before it.
        assert!(
            names.contains(
                &"Concevoir, déployer, sécuriser des infrastructures cloud et on-premise"
            ),
            "got: {names:?}"
        );
        assert!(
            names.contains(
                &"Administrer des bases de données MySQL/MariaDB, PostgreSQL, Elasticsearch, OpenSearch, MongoDB"
            ),
            "got: {names:?}"
        );

        // Nothing should look like the old run-on fusion of unrelated tags.
        assert!(
            !names
                .iter()
                .any(|n| n.contains("Qualités humaines Curiosité")),
            "got: {names:?}"
        );
    }

    #[test]
    fn regression_font_glyph_gap_does_not_corrupt_name_detection() {
        // Real-world source: a custom font subset assigned a ligature
        // ("fr") to a glyph ID with no ToUnicode entry at all. The old
        // single-byte fallback inserted a literal NUL character in its
        // place ("Wilfried" -> "Wil\0ied"), and `guess_name`'s alphabetic
        // check then rejected that whole line, silently falling through
        // to the next line — the job title — and using *that* as the
        // person's name instead. Test at the `guess_name` level directly
        // (no PDF needed): a stray control character in the name line
        // must not cause the title to be mistaken for the name.
        let lines = ["Wil\u{0000}ied Maillet", "Ingénieur DevOps"];
        let name = guess_name(&lines).expect("a name should still be found");
        assert_eq!(
            name, "Wilied Maillet",
            "control byte should be dropped, not corrupt the whole line"
        );
        assert_ne!(
            name, "Ingénieur DevOps",
            "name must not fall through to the job title"
        );
    }

    #[test]
    fn regression_tounicode_decode_drops_unmapped_control_bytes() {
        // Same bug, one layer lower: `ToUnicodeMap::decode`'s single-byte
        // fallback must not insert a raw control byte just because it
        // has no map entry — it should drop it, since it never really
        // represented that Latin-1 character in the first place (it was
        // an unresolved glyph ID).
        let map = ToUnicodeMap {
            code_bytes: 1,
            map: std::collections::HashMap::new(), // byte 0x00 unmapped
        };
        let decoded = map.decode(&[0x00]);
        assert!(
            decoded.is_none() || decoded.as_deref() == Some(""),
            "an unmapped control byte should decode to nothing, got: {decoded:?}"
        );
    }

    // ── ToUnicodeMap::decode / byte helpers ───────────────────────────────────

    #[test]
    fn tounicode_decode_with_unset_code_bytes_returns_none() {
        // `code_bytes == 0 || bytes.is_empty()` guards the loop; flipping
        // the `||` to `&&` would let a non-empty input fall through and
        // panic on `chunks(0)`. Asserting None kills that mutant.
        let map = ToUnicodeMap {
            code_bytes: 0,
            map: std::collections::HashMap::new(),
        };
        assert_eq!(map.decode(b"abc"), None);
    }

    #[test]
    fn tounicode_decode_stops_at_incomplete_trailing_chunk() {
        // code_bytes = 2; the input has a full 2-byte chunk followed by a
        // single trailing byte. `chunk.len() < code_bytes` must break so
        // the trailing byte is dropped (not fallback-decoded). Flipping the
        // `<` to `>` would never break and would append the trailing byte.
        let mut map = std::collections::HashMap::new();
        map.insert(0x4142, "AB".to_string());
        let map = ToUnicodeMap { code_bytes: 2, map };
        assert_eq!(map.decode(&[0x41, 0x42, 0x43]), Some("AB".to_string()));
    }

    #[test]
    fn tounicode_decode_does_not_latin1_fallback_on_full_unmapped_chunk() {
        // A full 2-byte chunk with no map entry must NOT fall into the
        // single-byte Latin-1 fallback (that is gated on `chunk.len() == 1`).
        let map = ToUnicodeMap {
            code_bytes: 2,
            map: std::collections::HashMap::new(),
        };
        // Correct: no matching 2-byte code, not a 1-byte chunk → None.
        // If `== 1` were flipped to `!= 1`, it would fallback on byte 'A'.
        assert_eq!(map.decode(&[0x41, 0x42]), None);
    }

    #[test]
    fn tounicode_decode_resolves_mapped_one_byte_codes() {
        let mut map = std::collections::HashMap::new();
        map.insert(0x41, "A".to_string());
        map.insert(0x42, "B".to_string());
        let map = ToUnicodeMap { code_bytes: 1, map };
        assert_eq!(map.decode(&[0x41, 0x42, 0x41]), Some("ABA".to_string()));
    }

    #[test]
    fn bytes_to_u32_big_endian_concat() {
        assert_eq!(bytes_to_u32(&[0x12, 0x34, 0x56, 0x78]), 0x12345678);
        assert_eq!(bytes_to_u32(&[0x01, 0x02]), 0x0102);
        assert_eq!(bytes_to_u32(&[0xFF]), 0xFF);
    }

    #[test]
    fn utf16be_bytes_to_string_decodes_pairs() {
        assert_eq!(utf16be_bytes_to_string(&[0x00, 0x41, 0x00, 0x42]), "AB");
        assert_eq!(
            utf16be_bytes_to_string(&[0x20, 0x1E, 0x00, 0x41]),
            "\u{201E}A"
        );
        assert_eq!(utf16be_bytes_to_string(&[]), "");
        // An odd trailing byte is dropped.
        assert_eq!(utf16be_bytes_to_string(&[0x00, 0x41, 0x00]), "A");
    }

    #[test]
    fn parse_hex_token_variants() {
        assert_eq!(parse_hex_token("<4142>"), Some(vec![0x41, 0x42]));
        assert_eq!(parse_hex_token(" <0042> "), Some(vec![0x00, 0x42]));
        // Odd / empty / unterminated forms are rejected.
        assert_eq!(parse_hex_token("<414>"), None);
        assert_eq!(parse_hex_token("<>"), None);
        assert_eq!(parse_hex_token("4142"), None);
        assert_eq!(parse_hex_token("<zz>"), None);
        // A trailing ']' (from a bfrange array) is not a valid closing '>'.
        assert_eq!(parse_hex_token("<4142>]"), None);
    }

    #[test]
    fn parse_tounicode_cmap_bfchar_and_bfrange() {
        let cmap_text = r"
            /CIDInit /ProcSet findresource begin
            12 dict begin
            begincmap
            /CIDSystemInfo << /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def
            /CMapName /Adobe-Identity-UCS def
            /CMapType 2 def
            1 begincodespacerange <00> <ff> endcodespacerange
            2 beginbfchar
            <41> <0041>
            <42> <0042>
            endbfchar
            1 beginbfrange
            <61> <63> <0061>
            endbfrange
            endcmap
            CMapName currentdict /CMap defineresource pop
            end
            end
        ";
        let cmap = parse_tounicode_cmap(cmap_text).expect("cmap should parse");
        assert_eq!(cmap.code_bytes, 1);
        assert_eq!(cmap.map.get(&0x41).map(String::as_str), Some("A"));
        assert_eq!(cmap.map.get(&0x42).map(String::as_str), Some("B"));
        // range 0x61..=0x63 -> a, b, c
        assert_eq!(cmap.map.get(&0x61).map(String::as_str), Some("a"));
        assert_eq!(cmap.map.get(&0x62).map(String::as_str), Some("b"));
        assert_eq!(cmap.map.get(&0x63).map(String::as_str), Some("c"));
    }

    #[test]
    fn parse_tounicode_cmap_bfrange_array_form() {
        let cmap_text = r"
            begincmap
            begincodespacerange <00> <ff> endcodespacerange
            1 beginbfrange
            <61> <63> [ <0041> <0042> <0043> ]
            endbfrange
            endcmap
        ";
        let cmap = parse_tounicode_cmap(cmap_text).expect("cmap should parse");
        assert_eq!(cmap.map.get(&0x61).map(String::as_str), Some("A"));
        assert_eq!(cmap.map.get(&0x62).map(String::as_str), Some("B"));
        assert_eq!(cmap.map.get(&0x63).map(String::as_str), Some("C"));
    }

    #[test]
    fn parse_tounicode_cmap_empty_returns_none() {
        assert!(parse_tounicode_cmap("no cmap constructs here").is_none());
    }

    /// Smoke test against real PDFs, if any are present locally. This
    /// directory is gitignored (see the fixtures README) — nothing here
    /// runs in CI unless you've dropped files in yourself. It's meant for
    /// manually checking a real problem PDF without ever committing it:
    /// drop it in `tests/fixtures/pdf_import/local/`, run
    /// `cargo test --release local_corpus_smoke_test -- --ignored --nocapture`,
    /// and it'll report which files, if any, violate the general
    /// invariants below — no per-file hand-written expectations needed.
    #[test]
    #[ignore = "only runs against locally-added, gitignored real PDFs"]
    fn local_corpus_smoke_test() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/pdf_import/local");
        let Ok(entries) = std::fs::read_dir(&dir) else {
            eprintln!(
                "no local corpus at {}; nothing to smoke-test",
                dir.display()
            );
            return;
        };
        let mut checked = 0;
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("pdf") {
                continue;
            }
            checked += 1;
            let bytes = std::fs::read(&path).expect("read fixture PDF");
            let cv = match import_pdf(&bytes) {
                Ok(cv) => cv,
                Err(e) => panic!("{}: import_pdf failed: {e}", path.display()),
            };
            assert!(
                !cv.experiences.is_empty(),
                "{}: found zero experience entries — likely a parse failure",
                path.display()
            );
            assert_no_prose_fragments_in_skills(&cv);
            assert_no_duplicate_experience_headers(&cv);
            assert_no_cross_job_bleed(&cv);
            println!(
                "{}: OK ({} experiences, {} skills, {} languages)",
                path.display(),
                cv.experiences.len(),
                cv.skills.len(),
                cv.languages.len()
            );
        }
        if checked == 0 {
            eprintln!("local corpus dir exists but has no .pdf files");
        }
    }
}
