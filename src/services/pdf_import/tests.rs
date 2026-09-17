use super::*;
use lopdf::{Document, Object};

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

/// Pins the Td/TD same-line-vs-new-line boundary at exactly `ty.abs() ==
/// 0.1` (the threshold itself, not just values comfortably above/below
/// it) — a `>` flipped to `>=` here would wrongly start a new line right
/// at the boundary instead of treating it as still the same line.
#[test]
fn run_operations_td_boundary_ty_of_exactly_0_1_stays_on_same_line() {
    use lopdf::content::Operation;
    use lopdf::Dictionary;

    let ops = vec![
        Operation::new("BT", vec![]),
        Operation::new(
            "Tf",
            vec![Object::Name(b"F1".to_vec()), Object::Integer(10)],
        ),
        Operation::new("Td", vec![Object::Integer(50), Object::Integer(700)]),
        Operation::new("Tj", vec![Object::string_literal("alpha")]),
        // Exactly the threshold, not comfortably past it.
        Operation::new("Td", vec![Object::Integer(20), Object::Real(0.1)]),
        Operation::new("Tj", vec![Object::string_literal("beta")]),
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
        vec!["alpha beta"],
        "ty.abs() == 0.1 exactly must NOT cross the new-line threshold, got {texts:?}"
    );
}

/// Same boundary pin as the Td/TD test above, but for the `Tm` (set text
/// matrix) operator's `dy` comparison, which is a structurally different
/// mutant site even though it uses the same `> 0.1` threshold.
#[test]
fn run_operations_tm_boundary_dy_of_exactly_0_1_stays_on_same_line() {
    use lopdf::content::Operation;
    use lopdf::Dictionary;

    let ops = vec![
        Operation::new("BT", vec![]),
        Operation::new(
            "Tf",
            vec![Object::Name(b"F1".to_vec()), Object::Integer(10)],
        ),
        Operation::new(
            "Tm",
            vec![
                Object::Integer(1),
                Object::Integer(0),
                Object::Integer(0),
                Object::Integer(1),
                Object::Integer(50),
                Object::Integer(0),
            ],
        ),
        Operation::new("Tj", vec![Object::string_literal("alpha")]),
        // Exactly the threshold: 0.1f32 widened to f64, not the f64
        // literal 0.1 (which lopdf's f32 storage can't hit exactly).
        Operation::new(
            "Tm",
            vec![
                Object::Integer(1),
                Object::Integer(0),
                Object::Integer(0),
                Object::Integer(1),
                Object::Integer(70),
                Object::Real(0.1),
            ],
        ),
        Operation::new("Tj", vec![Object::string_literal("beta")]),
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
        vec!["alpha beta"],
        "dy.abs() == 0.1 exactly must NOT cross the new-line threshold, got {texts:?}"
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

// ── decode_content_raw ───────────────────────────────────────────────────────

/// Bytes inside a `BT`/`ET` block that are neither a `(...)` string literal
/// nor a `<...>` hex string (e.g. operator keywords, numbers, whitespace)
/// must simply be skipped one byte at a time so the scan still reaches the
/// following string literal and the closing `ET`. This pins the tail
/// `else { i += 1; }` advance inside the BT/ET loop: a mutant that stalls
/// or jumps that index would either hang or skip/duplicate content.
#[test]
fn decode_content_raw_skips_unrecognized_bytes_between_bt_and_et() {
    let content = b"BT /F1 12 Tf 50 700 Td (Hello) Tj ET";
    let text = decode_content_raw(content);
    assert_eq!(text, "Hello");
}

/// Multiple unrelated operator tokens (numbers, names, bare keywords)
/// between two string literals must all be skipped without corrupting or
/// dropping either literal.
#[test]
fn decode_content_raw_skips_multiple_unrecognized_tokens_between_literals() {
    let content = b"BT (One) 12 0 0 12 50 700 Tm /F2 10 Tf (Two) Tj ET";
    let text = decode_content_raw(content);
    assert_eq!(text, "One Two");
}

/// Content bytes that appear *before* any `BT` marker (or between an `ET`
/// and the next `BT`) must be scanned byte-by-byte without ever being
/// mistaken for text. This pins the outer `else { i += 1; }` advance used
/// while searching for the next `BT` marker.
#[test]
fn decode_content_raw_skips_bytes_outside_bt_et_blocks() {
    let content = b"q 1 0 0 1 0 0 cm Q BT (Only this) Tj ET S";
    let text = decode_content_raw(content);
    assert_eq!(text, "Only this");
}

/// Two separate BT/ET blocks separated by non-text operators: the scan
/// must advance past the gap and pick up the second block's literal too.
#[test]
fn decode_content_raw_handles_multiple_bt_et_blocks() {
    let content = b"BT (First) Tj ET 0 0 0 rg BT (Second) Tj ET";
    let text = decode_content_raw(content);
    assert_eq!(text, "First Second");
}

/// A `BT`/`ET` block with no string literals at all (only operator noise)
/// must terminate cleanly and contribute no text -- this only happens if
/// every non-literal byte in between is actually advanced past.
#[test]
fn decode_content_raw_empty_block_produces_no_text() {
    let content = b"BT 1 0 0 1 50 700 Tm /F1 12 Tf ET";
    let text = decode_content_raw(content);
    assert_eq!(text, "");
}

// ── run_operations: stateful operators ───────────────────────────────────────
//
// The text-content tests above only feed BT/Tf/Td/Tj/TJ/ET. The content
// stream's other operators (q/Q/cm CTM save/restore, BT reset, T* newline,
// Tm matrix, the `'` text-show operator, Do XObject invocation) mutate
// parser state that these tests pin directly — including the positions
// recorded on each PositionedLine, and the exact threshold boundaries of
// the same-line word-gap heuristics — so a deleted match arm or flipped
// comparison cannot silently pass.

fn run_ops(ops: Vec<lopdf::content::Operation>) -> Vec<PositionedLine> {
    let doc = Document::new();
    let resources = lopdf::Dictionary::new();
    let encodings = std::collections::BTreeMap::new();
    let mut visited = Vec::new();
    let mut lines = Vec::new();
    run_operations(
        &doc,
        &ops,
        &resources,
        &encodings,
        Matrix::identity(),
        &mut visited,
        &mut lines,
    );
    lines
}

fn cm_op(x: i64, y: i64) -> lopdf::content::Operation {
    lopdf::content::Operation::new(
        "cm",
        vec![
            Object::Integer(1),
            Object::Integer(0),
            Object::Integer(0),
            Object::Integer(1),
            Object::Integer(x),
            Object::Integer(y),
        ],
    )
}

fn tm_op(f_y: i64) -> lopdf::content::Operation {
    lopdf::content::Operation::new(
        "Tm",
        vec![
            Object::Integer(1),
            Object::Integer(0),
            Object::Integer(0),
            Object::Integer(1),
            Object::Integer(0),
            Object::Integer(f_y),
        ],
    )
}

fn tstr(s: &str) -> Object {
    Object::string_literal(s)
}

/// The `q`/`Q`/`cm` state-saving operators must compose against the CTM
/// stack's CURRENT top and return to the saved entry on `Q` — and a `Q`
/// on an already-restored (single-entry) stack must be a no-op. Pins the
/// deleted-arm mutants for all three plus the `len() > 1` guard flips on
/// `Q` (a `> 1` → `>= 1` flip pops the last real entry, so the trailing
/// `cm` then can't touch anything and the second line lands at the base).
#[test]
fn run_operations_ctm_save_restore_and_guard() {
    let lines = run_ops(vec![
        lopdf::content::Operation::new("q", vec![]),
        cm_op(100, 0),
        lopdf::content::Operation::new("Q", vec![]),
        lopdf::content::Operation::new("Tj", vec![tstr("A")]),
        lopdf::content::Operation::new("T*", vec![]),
        lopdf::content::Operation::new("q", vec![]),
        lopdf::content::Operation::new("q", vec![]),
        cm_op(50, 0),
        lopdf::content::Operation::new("Q", vec![]),
        lopdf::content::Operation::new("Q", vec![]),
        lopdf::content::Operation::new("Q", vec![]),
        cm_op(25, 0),
        lopdf::content::Operation::new("Tj", vec![tstr("B")]),
        lopdf::content::Operation::new("T*", vec![]),
    ]);
    assert_eq!(lines.len(), 2, "got: {lines:?}");
    assert_eq!(lines[0].text, "A");
    assert_eq!(lines[0].x, 0.0, "A must land at base, got: {lines:?}");
    assert_eq!(lines[1].text, "B");
    assert_eq!(lines[1].x, 25.0, "B must land at +25, got: {lines:?}");
}

/// A vertical `Tm` shift must flush the accumulated line; a shifted-offset
/// `Tm` does not. Pins the deleted-`Tm`-arm and the `dy.abs() > 0.1` →
/// `==`/`<` flips on the flush branch.
#[test]
fn run_operations_tm_vertical_shift_flushes_line() {
    let lines = run_ops(vec![
        lopdf::content::Operation::new("Tj", vec![tstr("A")]),
        tm_op(-100),
        lopdf::content::Operation::new("Tj", vec![tstr("B")]),
        lopdf::content::Operation::new("T*", vec![]),
    ]);
    assert_eq!(lines.len(), 2, "got: {lines:?}");
    assert_eq!(lines[0].text, "A");
    assert_eq!(lines[1].text, "B");
}

/// `dy.abs() > 0.1` in the `Tm` arm must stay strict: a zero-shift `Tm` is
/// same-line, and a single-glyph previous run must not trigger the
/// pending-space heuristic (`last_run_chars > 1` stays strict too — kills
/// the `>=` flip, which would inject a space after every single glyph).
#[test]
fn run_operations_tm_zero_shift_single_glyph_no_space() {
    let lines = run_ops(vec![
        lopdf::content::Operation::new("Tj", vec![tstr("A")]),
        tm_op(0),
        lopdf::content::Operation::new("Tj", vec![tstr("B")]),
    ]);
    assert_eq!(lines.len(), 1, "got: {lines:?}");
    assert_eq!(lines[0].text, "AB");
}

/// A multi-glyph run followed by a same-line `Tm` gets a synthetic space
/// (`last_run_chars > 1` on the pending-space branch) — pins the `>` →
/// `==`/`<` flips there.
#[test]
fn run_operations_tm_same_line_run_gap() {
    let lines = run_ops(vec![
        lopdf::content::Operation::new("Tj", vec![tstr("AB")]),
        tm_op(0),
        lopdf::content::Operation::new("Tj", vec![tstr("C")]),
    ]);
    assert_eq!(lines.len(), 1, "got: {lines:?}");
    assert_eq!(lines[0].text, "AB C");
}

/// `dy = new_tm.f - text_matrix.f` must be a subtraction: an equal-`f`
/// `Tm` drifts by 0 (no flush), while a `+` (50 + 50 = 100) or `/`
/// (50 / 50 = 1) mutant turns that into a phantom flush.
#[test]
fn run_operations_tm_dy_is_subtraction() {
    let lines = run_ops(vec![
        lopdf::content::Operation::new("Tj", vec![tstr("A")]),
        tm_op(50), // dy = 50 - 0 → flush line "A"
        lopdf::content::Operation::new("Tj", vec![tstr("B")]),
        tm_op(50), // dy = 50 - 50 = 0 → same line
        lopdf::content::Operation::new("Tj", vec![tstr("C")]),
    ]);
    assert_eq!(lines.len(), 2, "expected [A, BC], got: {lines:?}");
    assert_eq!(lines[0].text, "A");
    assert_eq!(lines[1].text, "BC");
}

/// `BT` must flush the pending accumulated line before resetting the text
/// matrix — not silently merge the runs that span it.
#[test]
fn run_operations_bt_flushes_pending_line() {
    let lines = run_ops(vec![
        lopdf::content::Operation::new("Tj", vec![tstr("A")]),
        lopdf::content::Operation::new("BT", vec![]),
        lopdf::content::Operation::new("Tj", vec![tstr("B")]),
    ]);
    let texts: Vec<&str> = lines.iter().map(|l| l.text.as_str()).collect();
    assert_eq!(texts, vec!["A", "B"]);
}

/// `T*` must start a new visual line (flush the pending run).
#[test]
fn run_operations_tstar_starts_new_line() {
    let lines = run_ops(vec![
        lopdf::content::Operation::new("Tj", vec![tstr("A")]),
        lopdf::content::Operation::new("T*", vec![]),
        lopdf::content::Operation::new("Tj", vec![tstr("B")]),
    ]);
    let texts: Vec<&str> = lines.iter().map(|l| l.text.as_str()).collect();
    assert_eq!(texts, vec!["A", "B"]);
}

/// The `'` text-showing operator writes its string operand as its own line.
#[test]
fn run_operations_apostrophe_shows_text_line() {
    let lines = run_ops(vec![lopdf::content::Operation::new("'", vec![tstr("X")])]);
    let texts: Vec<&str> = lines.iter().map(|l| l.text.as_str()).collect();
    assert_eq!(texts, vec!["X"]);
}

/// A `Do` invocation must recurse into the named Form XObject and extract
/// its content-stream text.
#[test]
fn run_operations_do_invokes_form_xobject() {
    use lopdf::Stream;

    let mut doc = Document::new();
    let form_id = doc.add_object(Object::Stream(Stream::new(
        {
            let mut d = lopdf::Dictionary::new();
            d.set(b"Subtype", Object::Name(b"Form".to_vec()));
            d
        },
        b"BT /F1 12 Tf 0 20 Td (XOBJ TEXT) Tj ET".to_vec(),
    )));
    let mut xobjs = lopdf::Dictionary::new();
    xobjs.set(b"Fm1", Object::Reference(form_id));
    let mut resources = lopdf::Dictionary::new();
    resources.set(b"XObject", Object::Dictionary(xobjs));

    let encodings = std::collections::BTreeMap::new();
    let mut visited = Vec::new();
    let mut lines = Vec::new();
    run_operations(
        &doc,
        &[lopdf::content::Operation::new(
            "Do",
            vec![Object::Name(b"Fm1".to_vec())],
        )],
        &resources,
        &encodings,
        Matrix::identity(),
        &mut visited,
        &mut lines,
    );
    let texts: Vec<&str> = lines.iter().map(|l| l.text.as_str()).collect();
    assert_eq!(texts, vec!["XOBJ TEXT"]);
}

/// The `Tj` pending-space guard is a conjunction: with no pending same-line
/// jump, a fresh word run must NOT get a synthetic space (an `&&` → `||`
/// mutant would inject one).
#[test]
fn run_operations_tj_space_guard_is_conjunction() {
    let lines = run_ops(vec![
        lopdf::content::Operation::new("Tj", vec![tstr("AB")]),
        lopdf::content::Operation::new("Tj", vec![tstr("C")]),
    ]);
    assert_eq!(lines.len(), 1, "got: {lines:?}");
    assert_eq!(lines[0].text, "ABC");
}

/// A small negative TJ kerning number (above the word-gap threshold) is not
/// a space: the `n < -threshold && have_text` guard stays a conjunction
/// (kills the `&&` → `||` flip) and keeps its leading minus sign (kills the
/// `delete -` flip, which would let `-5 < 180` through).
#[test]
fn run_operations_tj_small_negative_kerning_is_not_space() {
    let lines = run_ops(vec![
        lopdf::content::Operation::new("Tj", vec![tstr("AB")]),
        lopdf::content::Operation::new(
            "TJ",
            vec![Object::Array(vec![Object::Integer(-5), tstr("CD")])],
        ),
    ]);
    assert_eq!(lines.len(), 1, "got: {lines:?}");
    assert_eq!(lines[0].text, "ABCD");
}

/// A TJ kerning number exactly at the word-gap threshold is not a space —
/// the `n < -TJ_WORD_GAP_THRESHOLD` comparison stays strict.
#[test]
fn run_operations_tj_kerning_at_threshold_is_not_space() {
    let lines = run_ops(vec![
        lopdf::content::Operation::new("Tj", vec![tstr("AB")]),
        lopdf::content::Operation::new(
            "TJ",
            vec![Object::Array(vec![Object::Integer(-180), tstr("CD")])],
        ),
    ]);
    assert_eq!(lines.len(), 1, "got: {lines:?}");
    assert_eq!(lines[0].text, "ABCD");
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
        "Situation & Tasks: As part of the DevOps/SRE transformation within the IT department, I"
            .to_string(),
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
        "Techs: Openstack, Scaleway, Debian, Kyverno, Keycloak, Vault, Redis, CNPG, Kubernetes,"
            .to_string(),
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

#[test]
fn parse_projects_recognizes_dash_and_asterisk_bullets() {
    // "•" is already covered above — this pins the other two `||`
    // branches in the bullet-prefix chain specifically. With `||`->`&&`,
    // no single prefix could ever satisfy all four checks at once, so
    // NOTHING would be recognized as a bullet at all.
    let projects = parse_projects(&[
        "Alpha".to_string(),
        "- dash bullet".to_string(),
        "* star bullet".to_string(),
    ]);
    assert_eq!(projects.len(), 1);
    assert_eq!(projects[0].bullets.len(), 2);
    assert_eq!(projects[0].bullets[0].en, "dash bullet");
    assert_eq!(projects[0].bullets[1].en, "star bullet");
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

// Pins the `pos + 3` byte offset used for the " · " (middle-dot) degree/
// field separator — untested until now. "·" (U+00B7) is a 2-byte UTF-8
// character, so " · " is 4 bytes total; `+ 3` deliberately lands on the
// separator's own trailing space (relying on the immediate `.trim()` to
// clean it up) rather than `+ 4`. That's not itself a bug, but it does
// mean a `+` mutated to `-`/`*` here is the only thing standing between
// this working and either a wrong split or an out-of-bounds/non-char-
// boundary panic.
#[test]
fn build_education_institution_first_embedded_field_middle_dot() {
    let edu = build_education_institution_first(
        "MIT".to_string(),
        "2015".to_string(),
        "2019".to_string(),
        &["Bachelor of Arts · Economics".to_string()],
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
    assert!(
        build_education_institution_first(String::new(), String::new(), String::new(), &[],)
            .is_none()
    );
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
    // "Univ" (not "University") matches no INSTITUTION_KEYWORDS entry, so
    // this hits the `None => ...` fallback arm (last line = institution,
    // everything before = field), not the `Some(idx) => ...` arm. Asserts
    // that split exactly, which also pins the `None if rest.is_empty()`
    // guard: mutated to `true`, this non-empty-rest case would wrongly
    // take the empty-institution/empty-field arm instead.
    assert_eq!(edu.institution, "Univ");
    assert_eq!(edu.field.en, "in Mathematics");
}

// Pins the exact byte offset used to split an embedded "X in Y"/"X en Y"
// degree line, rather than just checking the education entry built at
// all — a `pos + 4` ("in "/"en " are both 4 bytes) miscounted as `pos - 4`
// or `pos * 4` produces a wrong split (or, for `* 4`, an out-of-bounds
// slice panic) that a looser assertion wouldn't catch.
#[test]
fn build_education_from_buffer_splits_embedded_in_degree_and_field() {
    let edu = build_education_from_buffer(
        &["Bachelor of Science in Computer Science".to_string()],
        "2017".to_string(),
        "2020".to_string(),
    )
    .expect("education should build");
    assert_eq!(edu.degree.en, "Bachelor of Science");
    assert_eq!(edu.field.en, "Computer Science");
}

#[test]
fn build_education_from_buffer_splits_embedded_en_degree_and_field() {
    let edu = build_education_from_buffer(
        &["Licence en Droit".to_string()],
        "2017".to_string(),
        "2020".to_string(),
    )
    .expect("education should build");
    assert_eq!(edu.degree.en, "Licence");
    assert_eq!(edu.field.en, "Droit");
}

// Pins `inst_end`'s STARTING value (`idx + 1`, not `idx`) specifically.
// `looks_like_institution_line` matches by case-insensitive keyword
// containment, not by requiring the line to start uppercase — so a
// lowercase-starting institution line (unusual, but real: an OCR'd PDF
// or an all-lowercase-styled resume) both satisfies `looks_like_
// institution_line` AND fails the loop's own `is_uppercase` check on its
// character. A `+` mutated to `*` here makes `inst_end` start at
// `idx` instead of `idx + 1`; since the loop condition is checked BEFORE
// incrementing, that's the difference between the institution line
// itself being included in `institution_lines` (correct) or immediately
// excluded and folded into `field` instead (wrong).
#[test]
fn build_education_from_buffer_inst_end_starts_past_the_institution_line() {
    let edu = build_education_from_buffer(
        &["BSc".to_string(), "the university of paris".to_string()],
        "2017".to_string(),
        "2020".to_string(),
    )
    .expect("education should build");
    assert_eq!(edu.institution, "the university of paris");
    assert_eq!(edu.field.en, "");
}

// ── parse_cv section wiring ──────────────────────────────────────────────────
//
// `parse_projects`/`parse_certifications` themselves are already unit
// tested above with direct line-array input — these two instead pin the
// *wiring* in `parse_cv`'s section-header match: that a "Projects"/
// "Certifications" header in real extracted text actually routes its
// lines to those parsers and lands in `cv.projects`/`cv.certifications`,
// not just that the parsers work in isolation.
#[test]
fn parse_cv_routes_projects_section_into_cv_projects() {
    let text = "Jane Doe\n\nProjects\nSide Tracker: personal habit tracker\n• built with Rust\n";
    let cv = parse_cv(text);
    assert_eq!(
        cv.projects.len(),
        1,
        "expected the Projects section to populate cv.projects, got {:?}",
        cv.projects.iter().map(|p| &p.name).collect::<Vec<_>>()
    );
    assert_eq!(cv.projects[0].name, "Side Tracker");
}

#[test]
fn parse_cv_routes_certifications_section_into_cv_certifications() {
    let text = "Jane Doe\n\nCertifications\nAWS Certified Solutions Architect\n2022\nAmazon\n";
    let cv = parse_cv(text);
    assert_eq!(
        cv.certifications.len(),
        1,
        "expected the Certifications section to populate cv.certifications, got {:?}",
        cv.certifications
            .iter()
            .map(|c| &c.name)
            .collect::<Vec<_>>()
    );
}

/// Regression test for `parse_cv`'s harvested-skill merge: skills recovered
/// from sidebar tool/skill bleed inside the Experience section must land in
/// `cv.skills` unless a case-insensitive duplicare already exists — pinning
/// the `!` on `if !cv.skills.iter().any(...)` (a `delete !` mutant flips it
/// to "only push a duplicate", silently dropping every harvested entry when
/// the list is otherwise empty).
#[test]
fn parse_cv_merges_harvested_skill_lines_into_cv_skills() {
    let text = "Jane Doe\nDeveloper\n\nExperience\nSome Role at Acme - Jan 2021 - Present\n• A genuine accomplishment bullet.\nTOOLS Kustomize2+ yrs\n• Another genuine bullet.\n\nSkills\nKubernetes\n";
    let cv = parse_cv(text);
    let names: Vec<&str> = cv.skills.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(
        names,
        vec!["Kubernetes", "TOOLS Kustomize 2+ yrs"],
        "harvested skill from Experience must be merged into cv.skills, got: {names:?}"
    );
}

// ── resolve_project_skill_ids ────────────────────────────────────────────────

#[test]
fn resolve_project_skill_ids_maps_skill_names_to_ids_case_insensitively() {
    let mut cv = LifetimeCV {
        skills: vec![Skill {
            id: "s-rust".to_string(),
            name: "Rust".to_string(),
            ..Default::default()
        }],
        experiences: vec![Experience {
            projects: vec![ExperienceProject {
                // Deliberately different case from the skill's own name,
                // to pin the `eq_ignore_ascii_case` matching specifically.
                skill_ids: vec!["rust".to_string()],
                ..Default::default()
            }],
            ..Default::default()
        }],
        ..Default::default()
    };
    resolve_project_skill_ids(&mut cv);
    assert_eq!(
        cv.experiences[0].projects[0].skill_ids,
        vec!["s-rust".to_string()],
        "the raw name \"rust\" must resolve to the real skill id \"s-rust\""
    );
}

#[test]
fn resolve_project_skill_ids_drops_names_with_no_matching_skill() {
    let mut cv = LifetimeCV {
        skills: vec![],
        experiences: vec![Experience {
            projects: vec![ExperienceProject {
                skill_ids: vec!["nonexistent".to_string()],
                ..Default::default()
            }],
            ..Default::default()
        }],
        ..Default::default()
    };
    resolve_project_skill_ids(&mut cv);
    assert!(
        cv.experiences[0].projects[0].skill_ids.is_empty(),
        "an unresolvable raw skill name must be dropped, not kept as-is: {:?}",
        cv.experiences[0].projects[0].skill_ids
    );
}

// ── sections.rs mutation-coverage tests ──────────────────────────────
//
// A final standalone date-range row (this app's own renderer shape) used
// by the split_into_sections resumption-recovery tests below.
const SECTIONS_STANDALONE_DATE: &str = "\u{0011} May 2021 – October 2022 ½ Bangkok, Thailand";

#[test]
fn extract_urls_linkedin_dot_non_com_domain_sets_linkedin() {
    // Kills `||` -> `&&` at extract_urls's linkedin check: a domain that
    // contains "linkedin." but is not "linkedin.com" (e.g. linkedin.fr)
    // must still be recognized as the LinkedIn URL via the second clause.
    let (li, _gh, web) = extract_urls("linkedin.fr/in/john");
    assert_eq!(li, Some("linkedin.fr/in/john".to_string()));
    assert_eq!(web, None);
}

#[test]
fn extract_urls_https_prefix_sets_website() {
    // Kills `||` -> `&&` between the https:// and www. clauses: a bare
    // "https://" URL (which never also starts with "www.") must still be
    // recognized as the website.
    let (_li, _gh, web) = extract_urls("https://example.com");
    assert_eq!(web, Some("https://example.com".to_string()));
}

#[test]
fn detect_section_inline_content_at_limit_is_header() {
    // Kills `<=` -> `>` on SECTION_HEADER_INLINE_CONTENT_LIMIT: exactly the
    // limit (40) still counts as a header, one past it does not.
    let at_limit = format!("Skills:{}", "a".repeat(40));
    assert_eq!(detect_section(&at_limit), Some("skills"));
    let over_limit = format!("Skills:{}", "a".repeat(41));
    assert_eq!(detect_section(&over_limit), None);
}

#[test]
fn looks_like_bare_role_line_exactly_100_chars_true() {
    // Kills `>` -> `>=` on the max-length guard: exactly 100 chars is still
    // a plausible role line; only >100 is rejected.
    assert!(looks_like_bare_role_line(&"A".repeat(100)));
}

#[test]
fn split_into_sections_globales_swallow_only_after_competences() {
    // Kills the `==` -> `!=` on `trimmed.to_lowercase() == "compétences"`
    // (413) and the `&&` -> `||` joining it to the "globales" lookahead
    // (414): a NORMAL header immediately followed by "globales" must still
    // open its section, not be swallowed by the false-positive guard.
    let sections = split_into_sections("Experience\nGlobales\nEngineer");
    assert_eq!(
        sections,
        vec![(
            "experience",
            vec!["Globales".to_string(), "Engineer".to_string()]
        )]
    );
}

#[test]
fn split_into_sections_competences_globales_is_not_a_header() {
    // "Compétences" immediately followed by "Globales" is the false-positive
    // two-line label, not a real Skills header, so no skills section may be
    // opened. This kills the bounds/negation/equality mutants on lines
    // 413/414/417/418 (lookahead index arithmetic, the `!l.is_empty()`
    // negation, and the `== "globales"` check).
    let sections = split_into_sections("Experience\nRust dev\nCompétences\nGlobales\nGo");
    assert!(
        !sections.iter().any(|(s, _)| *s == "skills"),
        "Compétences Globales must not open a skills section, got: {:?}",
        sections
    );
    assert_eq!(sections.len(), 1);
    assert_eq!(sections[0].0, "experience");
}

#[test]
fn split_into_sections_recovers_last_role_company_before_date() {
    // When a standalone job-header date line is found outside "experience",
    // the recovery scan must pick the LAST two non-bleed lines (the genuine
    // role+company immediately preceding it) — not the first two, and not
    // every line. Kills the `!=`/`&&`/boundary mutants on 455/458/465.
    let text = format!("SKILLS\nRole A\nCompany A\nRole B\nCompany B\n{SECTIONS_STANDALONE_DATE}");
    let sections = split_into_sections(&text);
    let exp = sections
        .iter()
        .find(|(s, _)| *s == "experience")
        .unwrap_or_else(|| panic!("expected a recovered experience section, got: {sections:?}"));
    assert_eq!(exp.1[0], "Role B");
    assert_eq!(exp.1[1], "Company B");
    assert_eq!(exp.1[2], SECTIONS_STANDALONE_DATE);
}

#[test]
fn split_into_sections_no_recovery_with_fewer_than_two_preceding_lines() {
    // Kills `>` -> `>=` on the `idx > 0` walk guard: with only one preceding
    // non-bleed line the scan must stop at the start of the buffer instead
    // of decrementing past index 0 (which would underflow and panic).
    let text = format!("SKILLS\nDevOps Engineer\n{SECTIONS_STANDALONE_DATE}");
    let sections = split_into_sections(&text);
    assert!(
        !sections.iter().any(|(s, _)| *s == "experience"),
        "a single preceding line is not a role+company pair, got: {:?}",
        sections
    );
}

#[test]
fn split_into_sections_recovery_keeps_buffered_lines_in_old_section() {
    // Before switching to the recovered experience entry, the accumulated
    // lines must be flushed to the section they belonged to. With no section
    // header before them yet, that is the initial "header" section. Kills
    // the `!`-deletion on 483:24 and the `||` -> `&&` on 483:50.
    let text = format!("Role A\nCompany A\nRole B\nCompany B\n{SECTIONS_STANDALONE_DATE}");
    let sections = split_into_sections(&text);
    let header = sections
        .iter()
        .find(|(s, _)| *s == "header")
        .unwrap_or_else(|| {
            panic!("expected the buffered lines in a header section, got: {sections:?}")
        });
    assert_eq!(
        header.1,
        vec![
            "Role A".to_string(),
            "Company A".to_string(),
            "Role B".to_string(),
            "Company B".to_string()
        ]
    );
}

#[test]
fn split_into_sections_consecutive_headers_keep_empty_non_header_section() {
    // Kills the `!=` -> `==` on the `current_section != "header"` clause of
    // the boundary guard (495): an empty section that is not "header" must
    // still be emitted when the next header arrives.
    let sections = split_into_sections("SKILLS\nEXPERIENCE");
    assert_eq!(sections.len(), 1, "got: {sections:?}");
    assert_eq!(sections[0].0, "skills");
    assert!(sections[0].1.is_empty());
}

// ── dates.rs mutation-coverage tests ─────────────────────────────────

fn lines_of(parts: &[&str]) -> Vec<String> {
    parts.iter().map(|s| s.to_string()).collect()
}

#[test]
fn extract_date_range_from_end_empty_before_start_is_not_a_range() {
    // Kills `&&` -> `||` on the fast-path emptiness guard (38): when the
    // same separator repeats but the text before the first one is empty,
    // the fast path must be rejected and the fallback (bare leading year)
    // must produce the start instead.
    assert_eq!(
        extract_date_range_from_end(" - Jan 2021 - Present"),
        Some(("2021".to_string(), "Present".to_string()))
    );
}

#[test]
fn extract_date_range_from_end_end_exactly_three_words_proceeds() {
    // Kills `>` -> `==`/`>=` on the "end is too long" guard (63): exactly
    // three words is still a valid end, not trailing junk.
    assert_eq!(
        extract_date_range_from_end("Acme France December 2024 – February 2026 (Remote)"),
        Some((
            "December 2024".to_string(),
            "February 2026 (Remote)".to_string()
        ))
    );
}

#[test]
fn extract_date_range_from_end_loose_month_requires_all_four_conditions() {
    // Kills the `&&` -> `||` flips in the abbreviated-month branch
    // (92/93/94): a non-month penultimate token must fall through to the
    // bare-year branch, not be read as a start month.
    assert_eq!(
        extract_date_range_from_end("Acme France Rust 2024 – February 2026"),
        Some(("2024".to_string(), "February 2026".to_string()))
    );
}

#[test]
fn extract_date_range_from_end_abbrev_month_before_real_background() {
    // Kills `-` -> `/` on the abbreviation branch's `words[..len-2]` split
    // (97): the background check must include the whole pre-month prefix.
    assert_eq!(
        extract_date_range_from_end("1 · Paris Dec 2024 – February 2026"),
        Some(("Dec 2024".to_string(), "February 2026".to_string()))
    );
}

#[test]
fn extract_date_range_from_end_non_4_digit_year_is_not_a_date() {
    // Kills `&&` -> `||` on the bare-year branch guard (109): a token that
    // is not 4 digits long must not be accepted as a bare year.
    assert_eq!(
        extract_date_range_from_end("Acme France 20x4 – February 2026"),
        None
    );
}

#[test]
fn extract_date_range_from_end_bare_year_with_short_background() {
    // Kills `-` -> `/` on the bare-year branch's `words[..len-1]` split
    // (110): the background check must include every word before the year.
    assert_eq!(
        extract_date_range_from_end("1 · Paris Acme 2024 – February 2026"),
        Some(("2024".to_string(), "February 2026".to_string()))
    );
}

#[test]
fn extract_trailing_date_range_loose_basic() {
    // Kills the function-is-None replacement (547) plus the `+`/skip/negation
    // mutants on the separator-length, present/year guard and the `>`-limit
    // guard (551/555/563).
    assert_eq!(
        extract_trailing_date_range_loose("University X, City Sept 2014 – Oct 2017"),
        Some((
            "University X, City".to_string(),
            "Sept 2014".to_string(),
            "Oct 2017".to_string()
        ))
    );
}

#[test]
fn extract_trailing_date_range_loose_present() {
    // Kills the `!`-deletion on the `!is_present` guard (555:16): an
    // open-ended "Present" end must still be accepted.
    assert_eq!(
        extract_trailing_date_range_loose("University X Sept 2014 – Present"),
        Some((
            "University X".to_string(),
            "Sept 2014".to_string(),
            "Present".to_string()
        ))
    );
}

#[test]
fn extract_trailing_date_range_loose_end_exactly_three_words_proceeds() {
    // Kills `>` -> `==`/`>=` on the length guard (563): exactly three words
    // is a valid end.
    assert_eq!(
        extract_trailing_date_range_loose("University X Sept 2014 – Oct 2017 (Remote)"),
        Some((
            "University X".to_string(),
            "Sept 2014".to_string(),
            "Oct 2017 (Remote)".to_string()
        ))
    );
}

#[test]
fn extract_trailing_date_range_loose_loose_month_requires_both_conditions() {
    // Kills `&&` -> `||` on the loose-month branch (569): a non-month
    // penultimate token must fall through to the bare-year branch.
    assert_eq!(
        extract_trailing_date_range_loose("Acme France Rust 2024 – February 2026"),
        Some((
            "Acme France Rust".to_string(),
            "2024".to_string(),
            "February 2026".to_string()
        ))
    );
}

#[test]
fn extract_trailing_date_range_loose_non_4_digit_year_is_not_a_date() {
    // Kills the `||`/`==` flips on the bare-year guard (576): a 5-digit run
    // must not be treated as a year.
    assert_eq!(
        extract_trailing_date_range_loose("Acme France 20245 – February 2026"),
        None
    );
}

#[test]
fn extract_trailing_date_range_loose_bare_year_with_short_background() {
    // Kills `-` -> `/` on the bare-year branch's split (577).
    assert_eq!(
        extract_trailing_date_range_loose("1 · Paris Acme 2024 – February 2026"),
        Some((
            "1 · Paris Acme".to_string(),
            "2024".to_string(),
            "February 2026".to_string()
        ))
    );
}

#[test]
fn rejoin_fragmented_date_lines_drops_icon_before_month_and_joins_year() {
    // Kills the icon-arm deletion and the `!`-deletion in the glyph check
    // (610), the empty-check negation (616) and the `||` -> `&&` in the
    // month-name/abbreviation membership check (618).
    let out = rejoin_fragmented_date_lines(&lines_of(&["\u{0011}", "Sept", "2014 – Oct 2017"]));
    assert_eq!(out, vec!["Sept 2014 – Oct 2017".to_string()]);
}

#[test]
fn rejoin_fragmented_date_lines_reverse_order_month_after_year_range() {
    // Kills the `!`-deletion in month_after_optional_icon (628), the
    // `>=` -> `<` length guard (637) and the always-false guard replacement
    // (648:28 false).
    let out = rejoin_fragmented_date_lines(&lines_of(&["2014 – Oct 2017", "Sept"]));
    assert_eq!(out, vec!["Sept 2014 – Oct 2017".to_string()]);
}

#[test]
fn rejoin_fragmented_date_lines_lone_month_then_year_range() {
    // Also pins the `>=` -> `<` length guard (637) via the forward order.
    let out = rejoin_fragmented_date_lines(&lines_of(&["Sept", "2014 – Oct 2017"]));
    assert_eq!(out, vec!["Sept 2014 – Oct 2017".to_string()]);
}

#[test]
fn rejoin_fragmented_date_lines_non_year_first_token_is_not_joined() {
    // Kills the always-true guard replacement (648:28 true) and the `&&` ->
    // `||` in the 4-digit/all-digit guard (648:45): a non-numeric first
    // token must not trigger the reverse-order join.
    let input = lines_of(&["abcd – Oct 2017", "Sept"]);
    assert_eq!(rejoin_fragmented_date_lines(&input), input);
}

#[test]
fn rejoin_fragmented_date_lines_five_digit_first_token_is_not_joined() {
    // Kills the `==` -> `!=` in the 4-digit guard (648:40): a 5-digit first
    // token is not a year prefix for this pattern.
    let input = lines_of(&["20145 – Oct 2017", "Sept"]);
    assert_eq!(rejoin_fragmented_date_lines(&input), input);
}

// ── push_skill_entry ──────────────────────────────────────────────────────

#[test]
fn push_skill_entry_rejects_a_single_character() {
    let mut skills = Vec::new();
    push_skill_entry("a", SkillCategory::default(), &mut skills);
    assert!(
        skills.is_empty(),
        "a 1-char entry must be rejected by `trimmed.len() < 2`, got {skills:?}"
    );
}

#[test]
fn push_skill_entry_accepts_exactly_two_characters() {
    // Pins the `< 2` boundary itself, not just "short vs long" in general
    // — `==`/`<=` mutants would both wrongly reject exactly-2-char input.
    let mut skills = Vec::new();
    push_skill_entry("Go", SkillCategory::default(), &mut skills);
    assert_eq!(skills.len(), 1, "a 2-char entry must be accepted");
    assert_eq!(skills[0].name, "Go");
}

#[test]
fn push_skill_entry_rejects_header_lines_case_insensitively() {
    // A single one of the four header strings deliberately doesn't match
    // any of the other three — under a `||`->`&&` mutation on this chain,
    // NO input could ever satisfy all four equalities simultaneously, so
    // nothing would ever be recognized as a header at all.
    for header in [
        "Skills",
        "COMPÉTENCES",
        "Technical Skills",
        "compétences techniques",
    ] {
        let mut skills = Vec::new();
        push_skill_entry(header, SkillCategory::default(), &mut skills);
        assert!(
            skills.is_empty(),
            "header line {header:?} must be skipped, not added as a skill: {skills:?}"
        );
    }
}

#[test]
fn push_skill_entry_accepts_a_normal_skill_name() {
    let mut skills = Vec::new();
    push_skill_entry("• Rust", SkillCategory::default(), &mut skills);
    assert_eq!(skills.len(), 1);
    assert_eq!(skills[0].name, "Rust");
}

// ── parse_education boundary/entry-split coverage ─────────────────────────

#[test]
fn parse_education_year_only_line_requires_exactly_four_digits() {
    let lines = vec![
        "BSc".to_string(),
        "MIT".to_string(),
        "202".to_string(),
        "2017".to_string(),
    ];
    let edus = parse_education(&lines);
    assert_eq!(
        edus.len(),
        1,
        "expected exactly one entry, got {:?}",
        edus.iter().map(|e| &e.degree.en).collect::<Vec<_>>()
    );
    assert_eq!(edus[0].start_year, "2017");
    assert!(
        edus[0].institution.contains("202")
            || edus[0].field.en.contains("202")
            || edus[0].degree.en.contains("202"),
        "the 3-digit line must be folded into buffered text, not treated \
         as a (premature, wrong) year-only trigger: {edus:?}"
    );
}

#[test]
fn parse_education_year_only_line_requires_all_ascii_digits() {
    let lines = vec![
        "BSc".to_string(),
        "acme".to_string(),
        "MIT".to_string(),
        "2017".to_string(),
    ];
    let edus = parse_education(&lines);
    assert_eq!(
        edus.len(),
        1,
        "a 4-char non-digit line must not be treated as a year-only \
         trigger, got {:?}",
        edus.iter().map(|e| &e.degree.en).collect::<Vec<_>>()
    );
    assert_eq!(edus[0].start_year, "2017");
}

#[test]
fn parse_education_starts_new_degree_first_entry_flushes_without_date_range() {
    let lines = vec![
        "Master of Science".to_string(),
        "University X".to_string(),
        "Bachelor of Science".to_string(),
        "University Y".to_string(),
    ];
    let edus = parse_education(&lines);
    assert_eq!(
        edus.len(),
        2,
        "a new degree line while the buffer already holds a complete \
         degree+institution entry must flush it as its own entry, got {:?}",
        edus.iter().map(|e| &e.degree.en).collect::<Vec<_>>()
    );
    assert_eq!(edus[0].degree.en, "Master of Science");
    assert_eq!(edus[1].degree.en, "Bachelor of Science");
}

#[test]
fn parse_education_flushes_buffered_degree_first_entry_before_institution_date_row() {
    let lines = vec![
        "Bachelor of Science".to_string(),
        "MIT".to_string(),
        "Some College – Jan 2015 – Jun 2018".to_string(),
    ];
    let edus = parse_education(&lines);
    assert_eq!(
        edus.len(),
        2,
        "the buffered degree-first entry must be flushed, not silently \
         dropped, when an institution+date row starts before it ever got \
         a date range of its own: got {:?}",
        edus.iter().map(|e| &e.degree.en).collect::<Vec<_>>()
    );
    assert_eq!(edus[0].degree.en, "Bachelor of Science");
}

// ── looks_like_stray_heading ────────────────────────────────────────────────

#[test]
fn looks_like_stray_heading_two_words_all_caps_is_true() {
    assert!(looks_like_stray_heading("TOOLS SKILLS"));
}

#[test]
fn looks_like_stray_heading_three_words_is_false() {
    // Pins the `words.len() > 2` half of the early-return guard: with
    // `||`->`&&`, an all-caps 3-word line couldn't ever satisfy BOTH
    // `is_empty()` and `len() > 2` at once, so it would wrongly fall
    // through and return true instead of the correct false.
    assert!(!looks_like_stray_heading("TOOLS SKILLS USED"));
}

#[test]
fn looks_like_stray_heading_mixed_case_is_false() {
    // Pins the `&&` between has_letters and all-uppercase: with `||`, a
    // mixed-case (but letter-containing) short line would wrongly count.
    assert!(!looks_like_stray_heading("Tools Skills"));
}

#[test]
fn looks_like_stray_heading_digits_only_is_false() {
    // Pins the same `&&` from the other side: `has_letters` is false here,
    // and `.all()` on the (now-empty, since there are no alphabetic
    // chars) uppercase-filter is vacuously true — with `||`, `false ||
    // true` wrongly returns true instead of the correct false.
    assert!(!looks_like_stray_heading("123"));
}

// ── commit_pending_line ─────────────────────────────────────────────────────

#[test]
fn commit_pending_line_separates_successive_tools_entries_with_a_space() {
    let mut context = Vec::new();
    let mut tools_text = String::new();
    commit_pending_line(&mut context, &mut tools_text, "Techs: Rust");
    commit_pending_line(&mut context, &mut tools_text, "Docker");
    assert_eq!(
        tools_text, "Techs: Rust Docker",
        "successive tools_text appends must be space-separated, not run \
         together, and the first append must not have a leading space"
    );
}

// ── flush_project ────────────────────────────────────────────────────────

fn flush_project_context(context_lines: &[&str]) -> Vec<String> {
    let mut exp = Experience::default();
    let mut name: Option<String> = Some("proj".to_string());
    let mut start_date = String::new();
    let mut end_date = String::new();
    let mut context: Vec<String> = context_lines.iter().map(|s| s.to_string()).collect();
    let mut tools_text = String::new();
    let mut bullets = Vec::new();
    flush_project(
        &mut exp,
        &mut name,
        &mut start_date,
        &mut end_date,
        &mut context,
        &mut tools_text,
        &mut bullets,
    );
    exp.projects[0]
        .context
        .iter()
        .map(|c| c.en.clone())
        .collect()
}

#[test]
fn flush_project_drops_a_stray_all_caps_heading_from_context() {
    let kept = flush_project_context(&["TOOLS", "Built a thing."]);
    assert_eq!(kept, vec!["Built a thing.".to_string()]);
}

#[test]
fn flush_project_drops_a_short_bare_label_from_context() {
    let kept = flush_project_context(&["Situation:", "Built a thing."]);
    assert_eq!(kept, vec!["Built a thing.".to_string()]);
}

#[test]
fn flush_project_bare_label_length_boundary_is_inclusive_at_40() {
    // Exactly 40 chars (including the trailing ':') must still be
    // dropped as a bare label — pins `<= BARE_LABEL_MAX_LEN` specifically
    // against a `>` mutation, which would wrongly keep it.
    let exactly_40 = "x".repeat(39) + ":";
    assert_eq!(exactly_40.chars().count(), 40);
    let kept = flush_project_context(&[&exactly_40, "Built a thing."]);
    assert_eq!(kept, vec!["Built a thing.".to_string()]);
}

#[test]
fn flush_project_keeps_a_long_colon_ending_sentence() {
    // 41 chars: one over the boundary, so this must be KEPT (it's the
    // "long sentence that happens to end in a colon" case the doc
    // comment on BARE_LABEL_MAX_LEN describes, not a bare label).
    let exactly_41 = "x".repeat(40) + ":";
    assert_eq!(exactly_41.chars().count(), 41);
    let kept = flush_project_context(&[&exactly_41]);
    assert_eq!(kept, vec![exactly_41]);
}

// ── Test helpers ─────────────────────────────────────────────────────────────

fn long_prose() -> String {
    "Lorem ipsum dolor sit amet, consectetur adipiscing elit sed do eiusmod tempor".to_string()
}

// ── parse_certifications (645, 670, 703) ─────────────────────────────────────

#[test]
fn parse_certifications_flattened_two_alnum_segments_are_kept() {
    // 645: `>= 2` in the flattening filter must keep a segment with
    // exactly 2 alphanumeric characters. Under `>=`→`<`, "C2" (count 2)
    // would be dropped, changing the issuer field.
    let lines = vec!["K8s · C2 · Bar · Jan 2020 – Dec 2021".to_string()];
    let certs = parse_certifications(&lines);
    assert_eq!(certs.len(), 1);
    assert_eq!(certs[0].name, "K8s");
    assert_eq!(
        certs[0].issuer, "C2 · Bar",
        "the 2-alnum segment 'C2' must survive the flattening filter"
    );
}

#[test]
fn parse_certifications_two_dateless_lines_merges_as_one() {
    // 670: `buffer.len() > 2` boundary at 2: exactly 2 dateless lines with
    // no dates anywhere must be merged as a single cert (name + issuer),
    // not split per-line. Under `>`→`==` or `>`→`>=`, the condition
    // becomes true and the lines are wrongly split into 2 certs.
    let lines = vec!["AWS Certified".to_string(), "Amazon".to_string()];
    let certs = parse_certifications(&lines);
    assert_eq!(
        certs.len(),
        1,
        "exactly-2 dateless lines must merge into one cert, got {:?}",
        certs.iter().map(|c| &c.name).collect::<Vec<_>>()
    );
    assert_eq!(certs[0].name, "AWS Certified");
    assert_eq!(certs[0].issuer, "Amazon");
}

#[test]
fn parse_certifications_three_dateless_lines_splits_per_line() {
    // 670: > 2: three dateless lines must be split one-per-line (3 certs).
    // Under `>`→`<`, the guard flips to false and the lines merge as 1.
    let lines = vec![
        "AWS Certified".to_string(),
        "GCP Certified".to_string(),
        "Azure Certified".to_string(),
    ];
    let certs = parse_certifications(&lines);
    assert_eq!(
        certs.len(),
        3,
        "three dateless lines should become three separate certs, got {:?}",
        certs.iter().map(|c| &c.name).collect::<Vec<_>>()
    );
    assert_eq!(certs[0].name, "AWS Certified");
    assert_eq!(certs[1].name, "GCP Certified");
    assert_eq!(certs[2].name, "Azure Certified");
}

#[test]
fn parse_certifications_deferred_when_section_has_dated_entries() {
    // 670: `&&`→`||` — when the section already produced a dated cert
    // AND there are >2 trailing dateless lines, the deferred path should
    // merge them as one cert. Under `||`, it wrongly splits per-line.
    let lines = vec![
        "AWS Certified".to_string(),
        "Jan 2020 – Dec 2021".to_string(),
        "Foo".to_string(),
        "Bar".to_string(),
        "Baz".to_string(),
    ];
    let certs = parse_certifications(&lines);
    assert_eq!(
        certs.len(),
        2,
        "must produce 1 dated cert + 1 merged trailing cert, got {:?}",
        certs
            .iter()
            .map(|c| (&c.name, &c.issuer))
            .collect::<Vec<_>>()
    );
    assert_eq!(certs[0].name, "AWS Certified");
    assert_eq!(certs[1].name, "Foo");
}

#[test]
fn parse_certifications_dateless_buffer_with_year_only_is_single_record() {
    // 703: `&&`→`||` in the "bare-year in trailing lines" check. A 4-digit
    // non-digit line (like a bare year) signals a single record. Under
    // `||`, a 4-char non-digit line would wrongly trigger the merged path.
    let lines = vec![
        "AWS Certified".to_string(),
        "20ab".to_string(),
        "Amazon".to_string(),
    ];
    let certs = parse_certifications(&lines);
    assert_eq!(
        certs.len(),
        3,
        "a 4-char NON-digit line must NOT trigger the bare-year merge, got {:?}",
        certs.iter().map(|c| &c.name).collect::<Vec<_>>()
    );
}

// ── build_certification_from_buffer (747) ────────────────────────────────────

#[test]
fn build_certification_from_buffer_only_folds_real_digit_year() {
    // 747: `&&`→`||` — a 4-char NON-digit line (no `is_ascii_digit`) must
    // be folded into `issuer`, not appended to `name` as a "(year)".
    let cert = build_certification_from_buffer(
        &[
            "AWS Certified".to_string(),
            "abcd".to_string(),
            "Amazon".to_string(),
        ],
        None,
    )
    .expect("cert should build");
    assert_eq!(
        cert.name, "AWS Certified",
        "a non-digit 4-char line must become an issuer part, not a year on the name"
    );
    assert_eq!(cert.issuer, "abcd · Amazon");
}

// ── is_dots_only (774, 775, 778) ────────────────────────────────────────────

#[test]
fn is_dots_only_pure_dots_true() {
    assert!(is_dots_only("○ ○ ○"));
}

#[test]
fn is_dots_only_mixed_dot_types_true() {
    // 778: the three `||`→`&&` mutants each break for at least one char
    // in this mixed set — each glyph class is only true on the branch it
    // tests, and the `&&` mutants demand ALL branches simultaneously.
    assert!(is_dots_only("● ○ ● •"));
}

#[test]
fn is_dots_only_non_dots_false() {
    assert!(!is_dots_only("English"));
    assert!(!is_dots_only(""));
}

// ── looks_like_interest_heading (795, 796, 803) ─────────────────────────────

#[test]
fn looks_like_interest_heading_prose_following_bare_word_is_true() {
    // 795: `replace fn return with false` — any true-returning call kills it.
    let next = long_prose();
    assert!(looks_like_interest_heading("Musique", Some(&next)));
    assert!(!looks_like_interest_heading("Musique", None));
}

#[test]
fn looks_like_interest_heading_recognizes_dash_as_marker() {
    // 796: first `||` (between " - " and " (") flipped to `&&` would make
    // a dash-only line no longer count as having a proficiency marker.
    assert!(!looks_like_interest_heading(
        "French - Native",
        Some(&long_prose())
    ));
}

#[test]
fn looks_like_interest_heading_recognizes_comma_as_marker() {
    // 796: second `||` (between " (" and ", ") flipped to `&&`.
    assert!(!looks_like_interest_heading(
        "French, Native",
        Some(&long_prose())
    ));
}

#[test]
fn looks_like_interest_heading_recognizes_colon_as_marker() {
    // 796: third `||` (between ", " and ':') flipped to `&&`.
    assert!(!looks_like_interest_heading(
        "Français : courant",
        Some(&long_prose())
    ));
}

#[test]
fn looks_like_interest_heading_long_next_alone_is_enough() {
    // 803: first `||` in the next-match clause (len>50 || count>=2).
    // Only len>50 is true here — kills `||`→`&&` on that operator.
    assert!(looks_like_interest_heading("Musique", Some(&long_prose())));
}

#[test]
fn looks_like_interest_heading_two_commas_alone_are_enough() {
    // 803: second `||` in the next-match clause (count>=2 || punct).
    // Only count>=2 is true here — kills `||`→`&&` on that operator.
    assert!(looks_like_interest_heading(
        "Musique",
        Some(&"a, b, c, d".to_string())
    ));
}

#[test]
fn looks_like_interest_heading_next_of_exactly_50_chars_is_not_prose() {
    // 803: `>`→`==` and `>`→`>=` — exactly 50 chars (not >50) must be
    // treated as non-prose (short), returning false.
    let next = "abcdefghij".repeat(5); // exactly 50 characters
    assert_eq!(next.len(), 50);
    assert!(!looks_like_interest_heading("Musique", Some(&next)));
}

// ── parse_languages interest-heading boundary (816) ──────────────────────────

#[test]
fn parse_languages_stops_at_interests_blurb_after_real_language() {
    // 816: `delete !` in `!langs.is_empty()` — when `langs` is empty
    // (i=0), a lookalike interest-heading line would wrongly cause a
    // break instead of being parsed; when `langs` is non-empty, the
    // `+`→`-` or `+`→`*` on `lines.get(i + 1)` reads the prev/same
    // line instead of next and the heading is no longer recognized,
    // allowing the blurb to be parsed as a language entry.
    let lines = vec!["English".to_string(), "Musique".to_string(), long_prose()];
    let langs = parse_languages(&lines);
    assert_eq!(
        langs.len(),
        1,
        "only 'English' should be kept; 'Musique' + prose = interest blurb, got {:?}",
        langs.iter().map(|l| &l.name).collect::<Vec<_>>()
    );
    assert_eq!(langs[0].name, "English");
}

// ── split_paren_segments (861, 878) ─────────────────────────────────────────

#[test]
fn split_paren_segments_nested_parens_first_segment_includes_outer_parens() {
    // 861: delete the '(' match arm (or `+=`→`-=`/`*=`) — without '('
    // incrementing depth, every ')' immediately closes the current
    // segment at the first ')', splitting nested parens differently.
    let segs = split_paren_segments("A (B (C) D) E");
    assert_eq!(segs, vec!["A (B (C) D)", "E"]);
}

#[test]
fn split_paren_segments_keeps_trailing_text_after_last_close_paren() {
    // 878: delete `!` in `if !tail.is_empty()` — would skip the
    // non-empty trailing segment "Anglais".
    let segs = split_paren_segments("Français (Native / Bilingual) Anglais");
    assert_eq!(segs, vec!["Français (Native / Bilingual)", "Anglais"]);
}

// ── parse_single_language_entry level markers (890, 894, 895) ────────────────

#[test]
fn parse_single_language_entry_bilingue_yields_native() {
    // 890: ||→&& on the second || (between bilingue and maternelle).
    // "bilingue" as the sole truth makes that `&&` false, wrongly
    // dropping the entry to Conversational.
    let lang = parse_single_language_entry("Français (Bilingue)");
    let lang = lang.expect("should parse");
    assert_eq!(lang.level, LanguageLevel::Native);
}

#[test]
fn parse_single_language_entry_maternelle_yields_native() {
    let lang = parse_single_language_entry("Français (Maternelle)");
    let lang = lang.expect("should parse");
    assert_eq!(lang.level, LanguageLevel::Native);
}

#[test]
fn parse_single_language_entry_fluent_yields_professional() {
    // 894: ||→&& between "professionnel" and "fluent".
    let lang = parse_single_language_entry("Anglais (Fluent)");
    let lang = lang.expect("should parse");
    assert_eq!(lang.level, LanguageLevel::Professional);
}

#[test]
fn parse_single_language_entry_courant_yields_professional() {
    // 895: ||→&& between "fluent" and "courant".
    let lang = parse_single_language_entry("Anglais (Courant)");
    let lang = lang.expect("should parse");
    assert_eq!(lang.level, LanguageLevel::Professional);
}

// ── is_bare_years_marker (989, 1002) ────────────────────────────────────────

#[test]
fn is_bare_years_marker_digits_without_plus_false() {
    // 989:26: `&&`→`||` between `ends_with('+')` and `len > 1` — "22"
    // (no '+') would wrongly make `digits_plus` return true.
    assert!(!is_bare_years_marker("22 yrs"));
}

#[test]
fn is_bare_years_marker_plus_without_digits_false() {
    // 989:37: `>`→`>=` and 989:41: `&&`→`||` — "+" (len 1, no digits
    // before '+') would wrongly pass the guard.
    assert!(!is_bare_years_marker("+ yrs"));
}

#[test]
fn is_bare_years_marker_digits_without_plus_two_token_false() {
    // 1002:41: `&&`→`||` in the [num, unit] arm — "22" (digits_plus
    // false) OR "yrs" (is_years_word true) would wrongly match.
    assert!(!is_bare_years_marker("22 yrs"));
}

// ── harvest_skill_segments (1088–1132) ──────────────────────────────────────

#[test]
fn harvest_skill_segments_two_token_yrs_variant() {
    // 1088: `==`→`!=` on "yrs" in is_years_word.
    assert_eq!(
        harvest_skill_segments("Docker 3+ yrs"),
        Some(vec!["Docker 3+ yrs".to_string()])
    );
}

#[test]
fn harvest_skill_segments_two_token_yr_variant() {
    // 1088: `==`→`!=` on "yr".
    assert_eq!(
        harvest_skill_segments("Docker 3+ yr"),
        Some(vec!["Docker 3+ yr".to_string()])
    );
}

#[test]
fn harvest_skill_segments_two_token_years_variant() {
    // 1088: `==`→`!=` on "years"; also kills the `||`→`&&` at 1088:35
    // (between "yr" and "years") — "years" relies solely on the 3rd
    // clause; flipping it to `&&` makes the check false.
    assert_eq!(
        harvest_skill_segments("Docker 3+ years"),
        Some(vec!["Docker 3+ years".to_string()])
    );
}

#[test]
fn harvest_skill_segments_two_token_year_variant() {
    // 1088: `==`→`!=` on "year"; also kills the `||`→`&&` at 1088:52
    // (between "years" and "year").
    assert_eq!(
        harvest_skill_segments("Docker 3+ year"),
        Some(vec!["Docker 3+ year".to_string()])
    );
}

#[test]
fn harvest_skill_segments_digits_plus_requires_plus_suffix() {
    // 1091:26/37/41 — `digits_plus` guards: a bare "22" or "+" without a
    // trailing '+' must NOT pass; "22" OR'd instead of AND'd at any guard
    // position flips the result.
    assert_eq!(harvest_skill_segments("Something 22 yrs"), None);
    assert_eq!(harvest_skill_segments("Something + yrs"), None);
}

#[test]
fn harvest_skill_segments_years_word_at_index_zero_ignored() {
    // 1096:37: `&&`→`||` would short-circuit with i=0 and attempt to
    // read `tokens[i-1]` (panic); 1096:42: `>`→`>=` same effect.
    assert_eq!(harvest_skill_segments("yrs 3+"), None);
}

#[test]
fn harvest_skill_segments_fused_marker_requires_digits() {
    // 1103:20: delete `!` in `!digits.is_empty()` — would skip valid
    // fused markers like "3+yrs"; 1103:39: `&&`→`||` — would let non-
    // digit strings like "3a+yrs" pass as markers.
    assert_eq!(
        harvest_skill_segments("Docker 3+yrs"),
        Some(vec!["Docker 3+yrs".to_string()])
    );
    assert_eq!(harvest_skill_segments("Docker 3a+yrs"), None);
}

#[test]
fn harvest_skill_segments_slash_instead_of_minus_index() {
    // 1097: `-`→`/` on `tokens[i - 1]` — becomes `tokens[i / 1]` which
    // is the years-word itself, not the digit-plus token.
    assert_eq!(
        harvest_skill_segments("Docker 3+ yrs"),
        Some(vec!["Docker 3+ yrs".to_string()])
    );
}

#[test]
fn harvest_skill_segments_overlapping_markers_produce_no_output() {
    // 1118: `<`→`==`/`<=` and 1132: `&&`→`||` — when markers start at
    // index 0, the first marker's name is empty and must be skipped;
    // overlapping/duplicate markers with empty names must still produce
    // no skill entries (not a panic or spurious push).
    assert_eq!(harvest_skill_segments("3+ yrs 2+ yrs"), None);
}

// ── parse_experiences: targeted mutation-kill tests ──────────────────────────
//
// The mutants below are all in the `parse_experiences`/`reclaim_stray_
// experience_content`/`find_duplicate_job_boundary` family and were reported
// MISSED by `cargo mutants -f src/services/pdf_import/experience.rs` (this
// file has no `exclude_re` in .github/workflows/mutants.yml — every mutant
// here is meant to be genuinely killed by a test, not excluded).

/// 36:48: `line.split_whitespace().count() > 20` replaced with `==`, `<`,
/// or `>=`. Only reachable for a line whose date range isn't already at the
/// very end (so `extract_date_range_from_end` misses it) but which does
/// have one findable mid-line by `find_date_range_span` with trailing text
/// after it (e.g. "- CDI - City"), matching this function's own doc-comment
/// example. At exactly 20 words the line must still be normalized (dropping
/// the trailing "- CDI - City"); at 21 words it must be left untouched.
#[test]
fn parse_experiences_midline_date_word_count_boundary() {
    // 10 filler words + "- January 2020 - February 2021 - CDI - City" (10 more
    // whitespace-separated tokens) = 20 words exactly.
    let at_boundary = "W1 W2 W3 W4 W5 W6 W7 W8 W9 W10 - January 2020 - February 2021 - CDI - City";
    assert_eq!(at_boundary.split_whitespace().count(), 20);
    let (exps, _skills) = parse_experiences(&[at_boundary.to_string()]);
    assert_eq!(
        exps.len(),
        1,
        "20-word mid-line date range must be normalized into a job, got: {:?}",
        exps.iter().map(|e| &e.role.en).collect::<Vec<_>>()
    );
    assert_eq!(exps[0].start_date, "January 2020");
    assert_eq!(exps[0].end_date, "February 2021");
    assert!(
        !exps[0].role.en.contains("CDI") && !exps[0].company.contains("CDI"),
        "trailing '- CDI - City' must have been dropped by normalization, got role={:?} company={:?}",
        exps[0].role.en,
        exps[0].company
    );

    // 21 words (one extra filler) must be left completely alone: no
    // recognizable trailing date range, so no job is produced at all.
    let over_boundary =
        "W1 W2 W3 W4 W5 W6 W7 W8 W9 W10 W11 - January 2020 - February 2021 - CDI - City";
    assert_eq!(over_boundary.split_whitespace().count(), 21);
    let (exps2, _skills2) = parse_experiences(&[over_boundary.to_string()]);
    assert_eq!(
        exps2.len(),
        0,
        "21-word mid-line date range must NOT be normalized, got: {:?}",
        exps2.iter().map(|e| &e.role.en).collect::<Vec<_>>()
    );
}

/// 56:77: `before.chars().filter(|c| c.is_alphabetic()).count() < 2`
/// replaced with `<=`. With exactly 2 alphabetic chars before the mid-line
/// date, the line must still be normalized (real text, not just a
/// decorative icon glyph).
#[test]
fn parse_experiences_midline_date_before_text_two_letters_boundary() {
    let line = "AB - January 2020 - February 2021 - CDI";
    let (exps, _skills) = parse_experiences(&[line.to_string()]);
    assert_eq!(
        exps.len(),
        1,
        "before-date text with 2 alphabetic chars must be normalized, got: {:?}",
        exps.iter().map(|e| &e.role.en).collect::<Vec<_>>()
    );
    assert_eq!(exps[0].start_date, "January 2020");
    assert_eq!(exps[0].end_date, "February 2021");
}

/// 191:32: `skip_until = i + 2;` (name-before-bare-marker skill harvest)
/// replaced with `-` (panics immediately via `usize` underflow at i=0, and
/// deterministically at any i since it always underflows relative to
/// itself... more precisely catches any i) or `*` (identity-ish at small i,
/// but observably wrong at i>0: `i * 2 != i + 2` for any i != 2). Placing
/// the harvested pair at i=1 (not i=0) makes both mutants observably wrong
/// rather than accidentally coincidental.
#[test]
fn parse_experiences_skill_bleed_marker_skip_advances_exactly_two_lines() {
    let lines = vec![
        "Placeholder".to_string(),
        "GitLab-CI".to_string(),
        "3+ yrs".to_string(),
        "Foo Corp - Jan 2020 - Feb 2021".to_string(),
    ];
    let (exps, skills) = parse_experiences(&lines);
    assert!(
        skills.iter().any(|s| s.name == "GitLab-CI 3+ yrs"),
        "expected the name+marker pair to be harvested as a skill, got: {:?}",
        skills.iter().map(|s| &s.name).collect::<Vec<_>>()
    );
    assert_eq!(exps.len(), 1);
    // If the marker line "3+ yrs" wasn't properly skipped, it would leak
    // into recent_plain and get wrongly claimed as this job's role instead
    // of "Placeholder" (before_date_peek "Foo Corp" contains none of the
    // role/company separator markers, so layout (d) consults recent_plain).
    assert_eq!(exps[0].role.en, "Placeholder");
    assert_eq!(exps[0].company, "Foo Corp");
}

/// 255/256/257/258:17: each `||` in the 5-way
/// `before_date_peek.contains(" at "/" chez "/" · "/" | "/", ")` chain
/// replaced with `&&`. With exactly ONE of the five markers present, the
/// correct (all-`||`) chain is true (bypassing layout (d)'s recent_plain
/// claim), while any single `&&` mutation drags the whole chain false for
/// that marker alone, wrongly claiming the preceding bare-role line.
#[test]
fn parse_experiences_prev_plain_role_guard_kills_on_any_single_separator() {
    let cases = [
        (
            "Global Corp at Client - Jan 2020 - Feb 2021",
            "Global Corp",
            "Client",
        ), // 255 (" at ")
        (
            "Global Corp chez Client - Jan 2020 - Feb 2021",
            "Global Corp",
            "Client",
        ), // 256 (" chez ")
        (
            "Global Corp · Client - Jan 2020 - Feb 2021",
            "Global Corp",
            "Client",
        ), // 257 (" · ")
        (
            "Global Corp | Client - Jan 2020 - Feb 2021",
            "Global Corp",
            "Client",
        ), // 258 (" | ")
    ];
    for (header, expect_role, expect_company) in cases {
        let lines = vec![
            "Some Role".to_string(),
            header.to_string(),
            "• Did stuff".to_string(),
        ];
        let (exps, _skills) = parse_experiences(&lines);
        assert_eq!(exps.len(), 1, "header: {header}");
        assert_eq!(
            exps[0].role.en, expect_role,
            "header: {header} -- if this is \"Some Role\" instead, the \
             prev_plain_role guard wrongly claimed the preceding bare-role \
             line despite the separator being present"
        );
        assert_eq!(exps[0].company, expect_company, "header: {header}");
    }
}

/// 359:46: `unambiguous_role_first = before_date.contains(" at ") ||
/// before_date.contains(" chez ")` replaced with `&&`; and 367:17: the
/// sibling `!unambiguous_role_first && next_line...bare_role` replaced
/// with `||`. Both are killed by the same scenario: a header containing
/// " chez " (unambiguous role-first marker) immediately followed by a
/// line that also happens to look like a bare role line. Correct code
/// must NOT treat this as layout (b) (next-line-is-the-role) since the
/// header is already unambiguous; either mutation makes it do so anyway.
#[test]
fn parse_experiences_unambiguous_chez_header_is_not_overridden_by_layout_b() {
    let lines = vec![
        "Global Corp chez Client - Jan 2020 - Feb 2021".to_string(),
        "Ingénieur Logiciel".to_string(),
        "• Did stuff".to_string(),
    ];
    let (exps, _skills) = parse_experiences(&lines);
    assert_eq!(exps.len(), 1);
    assert_eq!(
        exps[0].role.en, "Global Corp",
        "expected the unambiguous ' chez ' split to win; got role {:?} \
         (layout (b) wrongly took over)",
        exps[0].role.en
    );
    assert_eq!(exps[0].company, "Client");
}

/// 387:37 (`pos + 6` for " chez "), 393:37 (`pos + 3` for " · "), 399:37
/// (`pos + 3` for " | "), 405:37 (`pos + 2` for ", ") each replaced with
/// `-` or `*`. Each byte-offset must land exactly past its separator so
/// the company text doesn't retain a stray leading character. Uses a
/// bullet as the next line (not a bare role) so layout (b) never
/// intercepts, and an empty recent_plain so layout (d) never intercepts
/// either -- isolating each `rfind`+offset branch directly.
#[test]
fn parse_experiences_role_company_separator_byte_offsets() {
    let cases = [
        (
            "Jean Dupont chez Acme Corp - Jan 2020 - Feb 2021",
            "Jean Dupont",
            "Acme Corp",
        ),
        (
            "Jane Smith · Globex - Jan 2020 - Feb 2021",
            "Jane Smith",
            "Globex",
        ),
        (
            "Jane Smith | Globex - Jan 2020 - Feb 2021",
            "Jane Smith",
            "Globex",
        ),
        (
            "Jane Smith, Initech - Jan 2020 - Feb 2021",
            "Jane Smith",
            "Initech",
        ),
    ];
    for (header, expect_role, expect_company) in cases {
        let lines = vec![header.to_string(), "• Did stuff".to_string()];
        let (exps, _skills) = parse_experiences(&lines);
        assert_eq!(exps.len(), 1, "header: {header}");
        assert_eq!(exps[0].role.en, expect_role, "header: {header}");
        assert_eq!(exps[0].company, expect_company, "header: {header}");
    }
}

/// 412:32: `skip_until = i + 2;` (bare-role-consumed layout) replaced with
/// `-` (panics via underflow when the first job is at index 0) or `*`
/// (leaves the consumed role line unskipped, letting it leak into
/// recent_plain and get wrongly claimed by the NEXT job's layout (d)).
#[test]
fn parse_experiences_consumed_role_line_skip_advances_exactly_two_lines() {
    let lines = vec![
        "Acme Corp - Jan 2021 - Present".to_string(),
        "Software Engineer".to_string(),
        "Random Corp - Jan 2019 - Dec 2020".to_string(),
        "• bullet for job2".to_string(),
    ];
    let (exps, _skills) = parse_experiences(&lines);
    assert_eq!(
        exps.len(),
        2,
        "expected two jobs, got: {:?}",
        exps.iter().map(|e| &e.role.en).collect::<Vec<_>>()
    );
    assert_eq!(exps[0].role.en, "Software Engineer");
    assert_eq!(exps[0].company, "Acme Corp");
    // If "Software Engineer" wasn't properly skipped, it leaks into
    // recent_plain and gets wrongly claimed as job2's role (job2's header
    // "Random Corp" contains none of the separator markers, so layout (d)
    // consults recent_plain).
    assert_eq!(exps[1].role.en, "Random Corp");
    assert_eq!(exps[1].company, "");
    assert_eq!(exps[1].projects[0].bullets.len(), 1);
    assert_eq!(exps[1].projects[0].bullets[0].en, "bullet for job2");
}

/// 502/505/506:17: each `||` in the 7-way bullet-marker
/// `starts_with(...)` chain replaced with `&&`. With exactly ONE marker
/// present per test, the correct (all-`||`) chain recognizes it as a
/// bullet; any single `&&` mutation on that marker's operator drags the
/// whole chain false, so the line is wrongly treated as plain text
/// instead of a bullet.
#[test]
fn parse_experiences_bullet_marker_guard_kills_on_any_single_marker() {
    let cases = [
        ("- Did the ascii-dash thing", "Did the ascii-dash thing"), // 502
        ("▸ Did the triangle thing", "Did the triangle thing"),     // 505
        ("▪ Did the square thing", "Did the square thing"),         // 506
    ];
    for (bullet_line, expect_text) in cases {
        let lines = vec![
            "Some Role at Some Co - Jan 2020 - Feb 2021".to_string(),
            bullet_line.to_string(),
        ];
        let (exps, _skills) = parse_experiences(&lines);
        assert_eq!(exps.len(), 1, "bullet line: {bullet_line}");
        assert_eq!(
            exps[0].projects[0].bullets.len(),
            1,
            "expected {bullet_line:?} to be recognized as a bullet, got bullets: {:?}",
            exps[0].projects[0]
                .bullets
                .iter()
                .map(|b| &b.en)
                .collect::<Vec<_>>()
        );
        assert_eq!(
            exps[0].projects[0].bullets[0].en, expect_text,
            "bullet line: {bullet_line}"
        );
    }
}

/// 923:15: `while idx > 0 && recovered.len() < 2` replaced with `>=`
/// (always true for `usize`). With only ONE recoverable (non-bleed) line
/// in `stray`, the correct loop stops after recovering just that one line
/// and returns `None` (fewer than 2 recovered); the `>=` mutant loops one
/// extra time re-visiting index 0, pushes it a second time, and wrongly
/// returns `Some`.
#[test]
fn find_duplicate_job_boundary_single_line_returns_none() {
    let stray = vec!["Software Engineer".to_string()];
    assert_eq!(find_duplicate_job_boundary(&stray), None);
}

// ── reclaim_stray_experience_content: targeted mutation-kill tests ──────────

/// 1016:32: `lines[i - 1]` replaced with `lines[i / 1]` (reads the
/// trigger's OWN date line instead of the preceding line). Also kills
/// 1037:19 (the `i - 1` index assignment in the `i < 2` branch, exercised
/// here since the trigger sits at index 1): mutating that to `i + 1` or
/// `i / 1` changes which suffix gets reclaimed. The date line here starts
/// with a bullet-marker character ('–'), so if it's ever misread as the
/// "prev" line, `prev_is_plausible`'s bullet check correctly rejects it —
/// making the misread observable as "no reclaim happened" instead of a
/// silently-identical result. Also incidentally reaches 1057 (`idx > 0` ->
/// `idx >= 0` in the backward-bullet-walk): the resulting idx is 0, so the
/// `>=` mutant re-enters the loop and panics on `lines[idx - 1]`'s
/// underflow.
#[test]
fn reclaim_stray_experience_content_reads_correct_preceding_line() {
    let sections: Vec<(&str, Vec<String>)> = vec![
        ("experience", vec!["Existing Job".to_string()]),
        (
            "ignore",
            vec![
                "Company Name Here".to_string(),
                "– January 2020 – February 2021".to_string(),
            ],
        ),
    ];
    let reclaimed = reclaim_stray_experience_content(sections);
    let ignore_lines: Vec<&String> = reclaimed
        .iter()
        .filter(|(s, _)| *s == "ignore")
        .flat_map(|(_, l)| l.iter())
        .collect();
    assert!(
        ignore_lines.is_empty(),
        "expected the whole stranded pair to be reclaimed, ignore left with: {:?}",
        ignore_lines
    );
    let exp_lines: Vec<&String> = reclaimed
        .iter()
        .filter(|(s, _)| *s == "experience")
        .flat_map(|(_, l)| l.iter())
        .collect();
    assert!(
        exp_lines.iter().any(|l| l.as_str() == "Company Name Here"),
        "expected 'Company Name Here' to be reclaimed, got: {:?}",
        exp_lines
    );
}

/// 1018:17: the first `&&` in `prev_is_plausible` (`!prev.is_empty() &&
/// prev.len() <= 100 && ...`) replaced with `||`. Due to precedence this
/// makes the whole check short-circuit true whenever `prev` is merely
/// non-empty, regardless of length. A `prev` line over 100 chars must be
/// rejected by the real check (no reclaim happens).
#[test]
fn reclaim_stray_experience_content_rejects_overlong_prev_line() {
    let long_prev = "L".repeat(150);
    let sections: Vec<(&str, Vec<String>)> = vec![
        ("experience", vec!["Existing Job".to_string()]),
        (
            "ignore",
            vec![
                long_prev.clone(),
                "January 2020 – February 2021".to_string(),
            ],
        ),
    ];
    let reclaimed = reclaim_stray_experience_content(sections);
    let exp_lines: Vec<&String> = reclaimed
        .iter()
        .filter(|(s, _)| *s == "experience")
        .flat_map(|(_, l)| l.iter())
        .collect();
    assert_eq!(
        exp_lines,
        vec!["Existing Job"],
        "an over-100-char prev line must not be reclaimed as a job header"
    );
    let ignore_lines: Vec<&String> = reclaimed
        .iter()
        .filter(|(s, _)| *s == "ignore")
        .flat_map(|(_, l)| l.iter())
        .collect();
    assert_eq!(
        ignore_lines.len(),
        2,
        "the ignore section must be untouched"
    );
}

/// 1019:17: the second `&&` in `prev_is_plausible` (`... && !prev.
/// starts_with([bullets])`) replaced with `||`. This makes the whole
/// check short-circuit true whenever `prev` is short and non-empty,
/// regardless of it being a bullet line. A `prev` that starts with a
/// bullet marker must be rejected (no reclaim happens).
#[test]
fn reclaim_stray_experience_content_rejects_bullet_prev_line() {
    let sections: Vec<(&str, Vec<String>)> = vec![
        ("experience", vec!["Existing Job".to_string()]),
        (
            "ignore",
            vec![
                "– Bullet-like prev line".to_string(),
                "January 2020 – February 2021".to_string(),
            ],
        ),
    ];
    let reclaimed = reclaim_stray_experience_content(sections);
    let exp_lines: Vec<&String> = reclaimed
        .iter()
        .filter(|(s, _)| *s == "experience")
        .flat_map(|(_, l)| l.iter())
        .collect();
    assert_eq!(
        exp_lines,
        vec!["Existing Job"],
        "a bullet-prefixed prev line must not be reclaimed as a job header"
    );
    let ignore_lines: Vec<&String> = reclaimed
        .iter()
        .filter(|(s, _)| *s == "ignore")
        .flat_map(|(_, l)| l.iter())
        .collect();
    assert_eq!(
        ignore_lines.len(),
        2,
        "the ignore section must be untouched"
    );
}

/// 1027:37: `lines[i - 2]` replaced with `lines[i / 2]` (reads the wrong
/// line for `prev2`), and 1032:23: the `i - 2` assignment itself replaced
/// with `i / 2`. Chosen so `i - 2 != i / 2` (i=6: 4 vs 3) and the two
/// candidate lines have different plausibility, so either mutation
/// produces an observably different reclaimed slice.
#[test]
fn reclaim_stray_experience_content_two_line_back_uses_correct_index() {
    let sections: Vec<(&str, Vec<String>)> = vec![
        ("experience", vec!["Existing Job".to_string()]),
        (
            "ignore",
            vec![
                "Foo".to_string(),                          // 0
                "Bar".to_string(),                          // 1
                "Baz".to_string(),                          // 2
                "".to_string(), // 3 (lines[i/2] when i=6 -- implausible if misread)
                "Real Role Line".to_string(), // 4 (lines[i-2] -- the correct prev2)
                "Real Company Line".to_string(), // 5 (prev1)
                "January 2020 – February 2021".to_string(), // 6 (trigger, i=6)
            ],
        ),
    ];
    let reclaimed = reclaim_stray_experience_content(sections);
    let exp_lines: Vec<&String> = reclaimed
        .iter()
        .filter(|(s, _)| *s == "experience")
        .flat_map(|(_, l)| l.iter())
        .collect();
    assert_eq!(
        exp_lines,
        vec![
            "Existing Job",
            "Real Role Line",
            "Real Company Line",
            "January 2020 – February 2021"
        ],
        "expected the reclaim to start exactly at the 2-lines-back role line"
    );
}

/// 1029:21: the first `&&` in the `prev2` plausibility check replaced
/// with `||` (precedence makes it short-circuit true on non-empty alone,
/// ignoring length). An over-100-char prev2 must be rejected, falling
/// back to the 1-line-back (`i - 1`) reclaim instead.
#[test]
fn reclaim_stray_experience_content_rejects_overlong_prev2_line() {
    let long_prev2 = "L".repeat(150);
    let sections: Vec<(&str, Vec<String>)> = vec![
        ("experience", vec!["Existing Job".to_string()]),
        (
            "ignore",
            vec![
                long_prev2.clone(),
                "Real Company Line".to_string(),
                "January 2020 – February 2021".to_string(),
            ],
        ),
    ];
    let reclaimed = reclaim_stray_experience_content(sections);
    let exp_lines: Vec<&String> = reclaimed
        .iter()
        .filter(|(s, _)| *s == "experience")
        .flat_map(|(_, l)| l.iter())
        .collect();
    assert_eq!(
        exp_lines,
        vec![
            "Existing Job",
            "Real Company Line",
            "January 2020 – February 2021"
        ],
        "an over-100-char prev2 must be rejected, falling back to 1-line-back reclaim"
    );
}

/// 1030:21: the second `&&` in the `prev2` plausibility check replaced
/// with `||` (short-circuits true on non-empty-and-short alone, ignoring
/// the bullet-prefix check). A bullet-prefixed prev2 must be rejected,
/// falling back to the 1-line-back reclaim instead.
#[test]
fn reclaim_stray_experience_content_rejects_bullet_prev2_line() {
    let sections: Vec<(&str, Vec<String>)> = vec![
        ("experience", vec!["Existing Job".to_string()]),
        (
            "ignore",
            vec![
                "– Bullet-like prev2 line".to_string(),
                "Real Company Line".to_string(),
                "January 2020 – February 2021".to_string(),
            ],
        ),
    ];
    let reclaimed = reclaim_stray_experience_content(sections);
    let exp_lines: Vec<&String> = reclaimed
        .iter()
        .filter(|(s, _)| *s == "experience")
        .flat_map(|(_, l)| l.iter())
        .collect();
    assert_eq!(
        exp_lines,
        vec![
            "Existing Job",
            "Real Company Line",
            "January 2020 – February 2021"
        ],
        "a bullet-prefixed prev2 must be rejected, falling back to 1-line-back reclaim"
    );
}

/// 1034:23: the `i - 1` assignment in the "prev2 not plausible" else
/// branch replaced with `i + 1` or `i / 1`. Covered by the two tests
/// above (`..._rejects_overlong_prev2_line` /
/// `..._rejects_bullet_prev2_line`): both take this else branch, and both
/// assert the reclaimed slice starts exactly at `i - 1` ("Real Company
/// Line"), which a `+1`/`/1` mutation would shift or shrink.
#[test]
fn reclaim_stray_experience_content_prev2_fallback_index_is_i_minus_one() {
    let sections: Vec<(&str, Vec<String>)> = vec![
        ("experience", vec!["Existing Job".to_string()]),
        (
            "ignore",
            vec![
                "".to_string(),                             // implausible prev2 (empty)
                "Good Prev Line".to_string(),               // prev1, i - 1
                "January 2020 – February 2021".to_string(), // trigger, i = 2
            ],
        ),
    ];
    let reclaimed = reclaim_stray_experience_content(sections);
    let exp_lines: Vec<&String> = reclaimed
        .iter()
        .filter(|(s, _)| *s == "experience")
        .flat_map(|(_, l)| l.iter())
        .collect();
    assert_eq!(
        exp_lines,
        vec![
            "Existing Job",
            "Good Prev Line",
            "January 2020 – February 2021"
        ],
        "expected the reclaim to start exactly at i - 1"
    );
}

/// 1073:28: `idx > 1` (second guard in the "skills"/"ignore" backward
/// bullet-walk) replaced with `idx >= 1`. With idx landing on exactly 1
/// after the primary split, the correct code stops there; the `>= 1`
/// mutant re-enters the loop body and panics on `lines[idx - 2]`'s
/// `usize` underflow.
#[test]
fn reclaim_stray_backward_walk_stops_at_idx_one() {
    let sections: Vec<(&str, Vec<String>)> = vec![
        ("experience", vec!["Existing Job".to_string()]),
        (
            "ignore",
            vec![
                "".to_string(),               // 0: implausible prev2 -> forces i-1 branch
                "Team Lead Role".to_string(), // 1: prev1, plausible -> reclaim starts here
                "January 2020 – February 2021".to_string(), // 2: trigger, i = 2, idx = i - 1 = 1
            ],
        ),
    ];
    let reclaimed = reclaim_stray_experience_content(sections);
    let exp_lines: Vec<&String> = reclaimed
        .iter()
        .filter(|(s, _)| *s == "experience")
        .flat_map(|(_, l)| l.iter())
        .collect();
    assert_eq!(
        exp_lines,
        vec![
            "Existing Job",
            "Team Lead Role",
            "January 2020 – February 2021"
        ]
    );
}

/// 1074:67: `lines[idx - 1]` (tool-bleed check in the backward
/// bullet-walk) replaced with `lines[idx / 1]` (reads the element AT idx
/// instead of just before it). Constructed so the correct read (a plain
/// non-bleed line) and the misread (a genuine tool-bleed line) have
/// different `looks_like_tool_bleed_line` results, flipping whether the
/// walk steps back at all.
#[test]
fn reclaim_stray_backward_walk_tool_bleed_check_reads_idx_minus_one() {
    let sections: Vec<(&str, Vec<String>)> = vec![
        ("experience", vec!["Existing Job".to_string()]),
        (
            "skills",
            vec![
                "Alpha filler".to_string(),   // 0
                "Bravo filler".to_string(),   // 1
                "This is a very long line of ordinary prose describing responsibilities in extensive detail well past a hundred characters".to_string(), // 2: implausible prev2 (too long), NOT tool-bleed
                "GitLab-CI 3+ yrs".to_string(), // 3: prev1, plausible AND tool-bleed
                "January 2020 – February 2021".to_string(), // 4: trigger, i = 4, idx = i - 1 = 3
            ],
        ),
    ];
    let reclaimed = reclaim_stray_experience_content(sections);
    let exp_lines: Vec<&String> = reclaimed
        .iter()
        .filter(|(s, _)| *s == "experience")
        .flat_map(|(_, l)| l.iter())
        .collect();
    // Correct: idx=3; lines[idx-1]=lines[2] (long prose) is NOT a
    // tool-bleed line, so the walk's second condition can be true and it
    // stays at idx=3 (lines[idx-2]=lines[1]="Bravo filler" doesn't start
    // with a bullet, so the walk doesn't actually step back here -- the
    // point is which line gets *checked*, observable via the panic-free,
    // unmutated result below).
    assert_eq!(
        exp_lines,
        vec![
            "Existing Job",
            "GitLab-CI 3+ yrs",
            "January 2020 – February 2021"
        ]
    );
}

/// 1075:38: `lines[idx - 2]` (bullet check in the backward bullet-walk)
/// replaced with `lines[idx / 2]` or `lines[idx + 2]`. Chosen with idx=5
/// so `idx - 2 = 3 != idx / 2 = 2`, and the two candidate lines have
/// different bullet-prefix status, so the mutation changes whether the
/// walk steps back at all.
#[test]
fn reclaim_stray_backward_walk_bullet_check_reads_idx_minus_two() {
    let sections: Vec<(&str, Vec<String>)> = vec![
        ("experience", vec!["Existing Job".to_string()]),
        (
            "skills",
            vec![
                "Filler0".to_string(), // 0
                "Filler1".to_string(), // 1
                "Filler2".to_string(), // 2 (lines[idx/2] when idx=5 -- NOT a bullet)
                "– continuing a wrapped bullet from earlier in the list".to_string(), // 3 (lines[idx-2] -- IS a bullet)
                "This is a very long line of ordinary prose describing responsibilities in extensive detail well past a hundred characters".to_string(), // 4: implausible prev2 (too long), NOT tool-bleed
                "GitLab-CI 3+ yrs".to_string(), // 5: prev1, plausible AND tool-bleed
                "January 2020 – February 2021".to_string(), // 6: trigger, i = 6, idx = i - 1 = 5
            ],
        ),
    ];
    let reclaimed = reclaim_stray_experience_content(sections);
    let exp_lines: Vec<&String> = reclaimed
        .iter()
        .filter(|(s, _)| *s == "experience")
        .flat_map(|(_, l)| l.iter())
        .collect();
    // Correct: idx starts at 5; lines[idx-1]=lines[4] is not tool-bleed
    // (true), lines[idx-2]=lines[3] DOES start with a bullet (true) ->
    // steps back to idx=3. At idx=3: lines[idx-1]=lines[2] not a bullet
    // (skip first if); lines[idx-1]=lines[2] not tool-bleed (true),
    // lines[idx-2]=lines[1]="Filler1" doesn't start with a bullet (false)
    // -> stops at idx=3.
    assert_eq!(
        exp_lines,
        vec![
            "Existing Job",
            "– continuing a wrapped bullet from earlier in the list",
            "This is a very long line of ordinary prose describing responsibilities in extensive detail well past a hundred characters",
            "GitLab-CI 3+ yrs",
            "January 2020 – February 2021",
        ]
    );
}
