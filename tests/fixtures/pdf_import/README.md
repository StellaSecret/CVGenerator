# pdf_import regression fixtures

This directory holds **anonymized text fixtures** used by the regression
tests in `src/services/pdf_import.rs` (see `mod regression_corpus` near the
end of that file). Each fixture reproduces the exact structural shape of a
real multi-column PDF-extraction bug — sidebar-heading placement,
line-wrapping, column interleaving — with real names, emails, phone
numbers, and employers replaced by placeholders.

## Why text, not PDF binaries

Two reasons:

1. **Privacy.** Real resumes contain real people's contact details. A
   fixture PDF derived from an actual person's CV should never be
   committed to a repo that is or could become public, even a private
   one — git history is forever, and access scope changes over time.
   Anonymizing the *text* that `extract_text` would have produced gets
   the same test value without that risk.
2. **Precision.** These bugs live in `split_into_sections`,
   `reclaim_stray_experience_content`, `parse_languages`, and friends —
   all of which operate on already-extracted text. A fixture that's
   already text exercises exactly that layer, directly, with no PDF
   library or font-rendering variability in the way. `parse_cv(text) ->
   LifetimeCV` is the whole pipeline under test.

The one bug that lives a layer lower than text — the `ToUnicodeMap`
font-glyph-gap issue — is tested with a hand-built byte array directly
against `ToUnicodeMap::decode`, for the same reason: no PDF needed to
pin down that behavior precisely.

## Adding a new fixture when you find a new bug

1. Get the raw extracted text for the problem PDF (e.g. via a small local
   `extract_text` harness, or ask Claude to build one — see the
   conversation that produced this suite for an example).
2. Trim it down to the smallest slice that still reproduces the bug —
   usually one job header, the interrupting sidebar content, and enough
   surrounding structure (a `Project N:` after it, a following job header,
   whatever the bug needs to manifest). Shorter fixtures are easier to
   reason about when a future regression breaks them.
3. Replace names, emails, phone numbers, and company names with
   placeholders (`Jordan Rivera`, `jordan.rivera@example.com`,
   `ACME/WIDGETS/QA`, etc.) — keep dates, section-header wording, and line
   breaks exactly as extracted, since those are what trigger the bug.
4. Save it as `tests/fixtures/pdf_import/<short_bug_name>.txt`.
5. Add a test to `mod regression_corpus` in `pdf_import.rs`: load it with
   `include_str!`, run `parse_cv`, and assert on the specific thing that
   broke — plus, where it applies, one of the general invariant helpers
   (`assert_no_prose_fragments_in_skills`,
   `assert_no_duplicate_experience_headers`, `assert_no_cross_job_bleed`)
   so the test also catches *similar* future regressions, not just an
   exact byte-for-byte replay.
6. Before committing the fix, temporarily revert it and confirm the new
   test actually fails — a regression test that can't fail isn't testing
   anything. (`cargo test --release <test_name>`.)

## Testing against real PDFs locally (not committed)

For a broader smoke test against real-world files without putting anyone's
PII in git: create a `tests/fixtures/pdf_import/local/` directory (already
in `.gitignore` — see below), drop real PDFs in there, and point a local
script or test at it. A reasonable baseline smoke test for anything in
that folder: `import_pdf` shouldn't panic, should find at least one
experience entry, and the parsed CV should pass
`assert_no_prose_fragments_in_skills` /
`assert_no_duplicate_experience_headers`. That catches wholesale breakage
without needing hand-written per-file expectations, and the files never
leave your machine.

Add this to `.gitignore` if it isn't already there:

```
tests/fixtures/pdf_import/local/
```
