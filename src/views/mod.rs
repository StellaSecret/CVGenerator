pub mod cv_editor;
pub mod cv_preview;
pub mod home;
pub mod sync;
pub mod tailor;

/// Resizes a CV preview `<iframe>` to hug its actual rendered content
/// height, instead of sitting inside a fixed-height box.
///
/// The preview iframes used to have a fixed CSS height (900px on the
/// lifetime CV preview, 820px on the tailor preview). That's fine for a
/// long CV, but tailoring is specifically the feature that *removes*
/// content (fewer projects/skills/experiences selected for a job
/// posting) — so the more effective the tailoring, the more likely the
/// actual content falls well short of that fixed box, leaving a large
/// blank strip below the CV inside the preview frame. Measuring the
/// real content height and setting it as the iframe's own height (with a
/// small buffer and a sane floor so a near-empty CV doesn't collapse to
/// an unusably short box) makes the preview's visible size track what's
/// actually on the page, the same way the print/PDF output already does
/// via `download_pdf_js`.
///
/// Called from each preview iframe's `onload`, which reliably fires again
/// every time `srcdoc` changes (e.g. re-tailoring against a new JD, or
/// toggling projects/skills in the picker) — no separate change-detection
/// needed on the Rust side.
#[allow(dead_code)]
pub(crate) fn resize_iframe_to_content_js(iframe_id: &str) -> String {
    format!(
        r#"(function(){{
        var f = document.getElementById('{iframe_id}');
        if (!f || !f.contentDocument || !f.contentDocument.documentElement) return;
        var h = f.contentDocument.documentElement.scrollHeight;
        // +4px absorbs sub-pixel rounding so a razor-thin internal
        // scrollbar doesn't appear on content that just barely fits;
        // 200px floor keeps a near-empty CV from collapsing to a sliver.
        f.style.height = Math.max(h + 4, 200) + 'px';
    }})();"#,
        iframe_id = iframe_id,
    )
}

/// Builds the JS that drives the "Download PDF" button in the CV Preview and
/// Tailor views. Kept pure (no wasm APIs) so it is unit-testable natively;
/// only the callers reach into the browser.
///
/// It produces a single, long, infinitely-scrollable page instead of the
/// usual paginated A4 print: before `print()` it measures the rendered
/// content height and injects an `@page{size:210mm <h>mm;margin:0}` rule so
/// the page is exactly as tall as the CV, with no per-sheet page breaks.
/// This keeps the "no backend, data never leaves the device" promise intact
/// — the browser's own print-to-PDF is still what produces the file.
#[allow(dead_code)]
pub(crate) fn download_pdf_js(iframe_id: &str, filename: &str) -> String {
    // Suggested filename for the browser's "Save as PDF" print destination
    // (it uses document.title, sans extension — the .pdf gets added
    // automatically).
    let title = filename.strip_suffix(".pdf").unwrap_or(filename);
    format!(
        r#"(function(){{
        var f = document.getElementById('{iframe_id}');
        if (!f || !f.contentWindow) return;
        var d = f.contentDocument, w = f.contentWindow;
        try {{ d.title = {title:?}; }} catch (e) {{}}
        // Force the on-screen layout to coincide with the print layout for a
        // moment so the measured height below equals the real print height.
        // The @media print block hides the scan/toolbox banner and tightens
        // .cv-doc's padding, and an A4 page is 210mm wide (=754px of content
        // once the 20px horizontal padding is removed), which is exactly the
        // wrapping width the print engine will use.
        var style = d.createElement('style');
        style.id = 'print-fit';
        style.textContent =
            'html,body{{margin:0!important}}' +
            '.cv-doc .toolbar,.cv-doc .gap-banner{{display:none!important}}' +
            // width (not max-width!) so the measurement can't be squeezed
            // narrower than A4 by whatever on-screen column the iframe
            // happens to sit in (e.g. the Tailor view's sidebar layout).
            // max-width is only a ceiling: if the iframe's ambient box is
            // narrower than 754px at measurement time, .cv-doc shrinks to
            // fit it, wraps far more than a real 754px-wide A4 page would,
            // and this measurement comes out much taller than the actual
            // print output — leaving a large blank strip at the bottom of
            // the exported PDF once the real print (which always uses the
            // true paper width, unconstrained by the source iframe's
            // on-screen box) renders shorter than the page we sized for it.
            '.cv-doc{{width:754px;max-width:none;padding:20px!important}}';
        d.head.appendChild(style);
        // Measure the actual content wrapper (.cv-doc) directly rather
        // than documentElement/body. In practice documentElement.scrollHeight
        // has been observed running ~3000px taller than body.scrollHeight
        // in this iframe — html measuring taller than its own body isn't
        // normal, and whatever causes it (a stray fixed-position element,
        // scrollbar-gutter reservation, a margin/box-sizing quirk — not
        // yet root-caused) was silently padding every tailored PDF with
        // that much blank trailing space, since we were taking the max of
        // the two. .cv-doc's own rendered box is what actually needs to
        // fit on the page, and getBoundingClientRect() isn't affected by
        // whatever inflates the ancestor html element.
        var cvDocEl = d.querySelector('.cv-doc');
        var contentHeightPx = cvDocEl
            ? cvDocEl.getBoundingClientRect().height
            : Math.max(d.documentElement.scrollHeight, d.body ? d.body.scrollHeight : 0);
        var heightMm = Math.ceil(contentHeightPx * 25.4 / 96);
        // TEMP DEBUG — remove once the tailored-CV blank-page-bottom issue
        // is confirmed fixed. Open devtools console before clicking
        // Download: this reports the raw scrollHeight this measurement is
        // based on, so we can tell whether the *measurement* itself is
        // inflated (bug in this code) or whether it's an accurate reading
        // of content that's genuinely taller than it looks in the on-screen
        // preview (754px A4-width reflow vs. a wider preview column).
        try {{
            console.log('[download_pdf_js]', {{
                cvDocHeightPx: contentHeightPx,
                iframe: '{iframe_id}',
                scrollHeightPx: d.documentElement.scrollHeight,
                bodyScrollHeightPx: d.body ? d.body.scrollHeight : null,
                cvDocWidth: (d.querySelector('.cv-doc') || {{}}).offsetWidth,
                heightMm: heightMm,
            }});
        }} catch (e) {{}}
        // Pad the page a little: browsers can lay the print out a couple of
        // px taller than the on-screen measurement (font hinting, subpixel
        // rounding), which would otherwise spill a sliver of content onto a
        // near-empty second page. +1% and +1mm costs a few mm of trailing
        // whitespace at most.
        var pageMm = Math.ceil(heightMm * 1.01) + 1;
        // One long scrollable page sized to fit the content. The guard stops
        // absurd sizes: Acrobat caps pages at 200in (~5080mm) and Chromium's
        // print engine has its own limits, so anything taller falls back to
        // the app's normal paginated @page{{margin:1.5cm}} print.
        if (pageMm >= 100 && pageMm <= 5000) {{
            style.textContent =
                '@page{{size:210mm ' + pageMm + 'mm;margin:0}}' + style.textContent;
        }}
        w.focus();
        w.print();
        // print() is synchronous in the browsers we target; remove the probe
        // stylesheet once the dialog closes so the screen preview is restored.
        style.remove();
    }})();"#,
        iframe_id = iframe_id,
        title = title,
    )
}

#[cfg(test)]
mod tests {
    use super::{download_pdf_js, resize_iframe_to_content_js};

    #[test]
    fn resize_iframe_to_content_js_measures_and_sets_height() {
        let js = resize_iframe_to_content_js("cv-tailor-frame");
        assert!(js.contains("getElementById('cv-tailor-frame')"));
        assert!(js.contains("f.contentDocument.documentElement.scrollHeight"));
        // Buffer + floor so a near-empty CV doesn't collapse to a sliver.
        assert!(js.contains("Math.max(h + 4, 200) + 'px'"));
        assert!(js.contains("f.style.height ="));
    }

    #[test]
    fn download_pdf_js_measures_content_and_sizes_one_long_page() {
        let js = download_pdf_js("cv-preview-frame", "Jane Smith-cv.pdf");
        // Filename suggestion, sans extension, goes into document.title.
        assert!(js.contains(r#"d.title = "Jane Smith-cv";"#));
        // Measurement in CSS px -> mm at the 96dpi convention, off the
        // actual .cv-doc content box (not documentElement/body, which can
        // run taller than the real content for reasons unrelated to it).
        assert!(js.contains("cvDocEl.getBoundingClientRect().height"));
        assert!(js.contains("contentHeightPx * 25.4 / 96"));
        // A few mm of safety padding so print-vs-screen rounding never
        // spills a sliver onto a second page.
        assert!(js.contains("Math.ceil(heightMm * 1.01) + 1"));
        // The single-page rule and its width/height/margin descriptors.
        assert!(js.contains("@page{size:210mm "));
        assert!(js.contains("+ pageMm + 'mm;margin:0}'"));
        // Rejects absurdly tall content (Acrobat's 200in page cap).
        assert!(js.contains("pageMm >= 100 && pageMm <= 5000"));
    }

    #[test]
    fn download_pdf_js_mirrors_print_layout_before_measuring() {
        let js = download_pdf_js("f", "cv.pdf");
        assert!(js.contains(".cv-doc .toolbar,.cv-doc .gap-banner{display:none!important}"));
        assert!(js.contains("html,body{margin:0!important}"));
        assert!(js.contains("style.id = 'print-fit';"));
    }

    #[test]
    fn download_pdf_js_forces_a4_width_not_just_a_max_width() {
        // Regression test: this used to be `max-width:754px`, a ceiling
        // rather than a fixed width. Inside a narrow on-screen container
        // (e.g. the Tailor view's sidebar layout), .cv-doc would shrink to
        // fit that narrower box during measurement, wrap far more than a
        // real A4-width page would, and produce a measured height much
        // taller than the actual print output — leaving a large blank
        // strip at the bottom of the exported PDF. A fixed width can't be
        // squeezed by the ambient container, regardless of how narrow the
        // iframe's on-screen box happens to be at measurement time.
        let js = download_pdf_js("f", "cv.pdf");
        assert!(js.contains(".cv-doc{width:754px;max-width:none;padding:20px!important}"));
        assert!(!js.contains(".cv-doc{max-width:754px"));
    }

    #[test]
    fn download_pdf_js_measures_cv_doc_not_document_element() {
        // Regression test: documentElement.scrollHeight was observed
        // running ~3000px taller than body.scrollHeight in the tailor
        // iframe — some ancestor-level inflation unrelated to the actual
        // CV content — and since the old code took the max of the two, it
        // silently added that much blank space to the bottom of every
        // exported PDF. Measuring .cv-doc's own bounding box sidesteps
        // whatever inflates html/body above it.
        let js = download_pdf_js("f", "cv.pdf");
        assert!(js.contains("d.querySelector('.cv-doc')"));
        assert!(js.contains("cvDocEl.getBoundingClientRect().height"));
        // Still falls back to the old measurement if .cv-doc is somehow
        // absent, rather than producing a zero/garbage page height.
        assert!(js.contains(
            "Math.max(d.documentElement.scrollHeight, d.body ? d.body.scrollHeight : 0)"
        ));
    }
}
