# CV Generator

A fully client-side CV builder written in **Rust + Dioxus**.  
No backend. No AI. No subscription. Your data never leaves your device.

---

## What it does

**Step 1 — Build your lifetime CV**  
Enter all your experience, skills, education and projects once through a
multi-step form. This becomes your personal career database, stored in
`localStorage` (web) or a local JSON file (mobile/desktop).

**Step 2 — Preview & download**  
See your complete CV rendered as a clean, print-ready HTML document.
Click "Download PDF" to print it via the browser's native print dialog
(`Ctrl+P → Save as PDF`).

**Step 3 — Tailor to a job description**  
Paste any job posting. The app extracts keywords, scores every item in
your lifetime CV against them, and outputs a filtered CV that surfaces
only the most relevant experience — using your exact words, nothing
rewritten. A gap-analysis banner shows which keywords matched and which
are missing.

---

## Architecture

```
src/
├── main.rs               Entry point, global CV signal, nav
├── router.rs             Route enum (Home / CvEditor / CvPreview / Tailor)
├── models/
│   └── cv.rs             All data structs (LifetimeCV, Experience, Skill…)
├── services/
│   ├── storage.rs        localStorage (web) / JSON file (mobile)
│   ├── matcher.rs        JD keyword extraction + relevance scoring
│   └── renderer.rs       HTML CV template generator
└── views/
    ├── home.rs           Dashboard with completion status
    ├── cv_editor.rs      6-step form (Personal → Experience → Skills…)
    ├── cv_preview.rs     Lifetime CV preview in iframe + PDF download
    └── tailor.rs         JD input, match score, tailored CV output
```

**No AI involved.** The matching algorithm is pure keyword frequency:

1. Tokenise the JD (lowercase, remove stop words, remove words < 3 chars)
2. Build a frequency map of the top 40 keywords
3. Score each experience/skill/project by how many of those keywords
   appear in its text, weighted by keyword frequency
4. Sort by score, filter low-scorers, always include ≥ 2 experiences
5. Produce matched / missing keyword sets for the gap banner

All rendering happens in Rust — the HTML CV template is built as a
`String` and injected into an `<iframe srcdoc>` for CSS isolation.

---

## Prerequisites

```bash
# 1. Rust (stable)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# 2. Dioxus CLI
cargo install dioxus-cli

# 3. WASM target (for web)
rustup target add wasm32-unknown-unknown
```

---

## Running — Web

```bash
dx serve --platform web
# Open http://localhost:8080
```

Production build:
```bash
dx build --platform web --release
# Output in: dist/
```

---

## Running — Android

```bash
# Install Android NDK and set ANDROID_NDK_HOME, then:
dx build --platform android --release
```

The Rust codebase is identical between platforms. The only
platform-specific code is in `services/storage.rs`, where a `cfg` gate
switches between `gloo-storage` (WASM/localStorage) and a JSON file
(native).

For Android, replace the `data_path()` function in `storage.rs` with
the platform data directory from the Dioxus mobile APIs once they
stabilise (see https://dioxuslabs.com/learn/0.6/getting_started/mobile).

---

## Data storage

| Platform      | Where data lives                          |
|---------------|-------------------------------------------|
| Web           | `localStorage` key `cv_generator_lifetime_cv` |
| Desktop       | `cv_data.json` in the working directory   |
| Android       | Swap `data_path()` in `storage.rs`        |

Data is serialised as JSON via `serde`. To export/import, open DevTools
→ Application → Local Storage and copy the value.

---

## Extending

**Add a new CV section** (e.g. Publications):
1. Add the struct to `models/cv.rs` and a `Vec<Publication>` field on `LifetimeCV`
2. Add a form step in `views/cv_editor.rs`
3. Add a `render_publications()` function in `services/renderer.rs`
4. Call it from both `render_lifetime_cv` and `render_tailored_cv`

**Add AI prose suggestions later** (optional upsell):
- Add an "Improve this bullet" button on individual experience items
- Call the Anthropic API with just that bullet + the JD context
- Keep it as an opt-in enhancement, not a core dependency

---

## Roadmap ideas

- [ ] Import from LinkedIn PDF (PDF parsing in WASM via `pdf-extract`)
- [ ] Multiple CV templates (minimalist, two-column, academic)
- [ ] Export as DOCX in addition to PDF
- [ ] Offline PWA mode (service worker + manifest)
- [ ] Sync across devices via a simple Firestore document (optional backend)
