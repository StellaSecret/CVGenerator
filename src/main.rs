#![allow(non_snake_case)]

mod i18n;
mod router;
mod views;

use dioxus::prelude::*;
use router::Route;

// Runs once on platforms rendered through wry (Android, desktop): reads the
// remembered theme from localStorage — the same key the landing and demo pages
// write — and falls back to the device's dark/light setting when there is none.
// The attribute is applied straight away so even the first frame matches the
// returned value, and the boolean result is what `Theme::detect()` cannot
// compute without web_sys.
#[cfg(not(target_arch = "wasm32"))]
const RESOLVE_THEME_JS: &str = r#"
    let stored = null;
    try { stored = localStorage.getItem("cv_gen_theme"); } catch (e) {}
    if (stored !== "light" && stored !== "dark") {
        stored = (window.matchMedia && window.matchMedia("(prefers-color-scheme: light)").matches)
            ? "light"
            : "dark";
    }
    document.documentElement.setAttribute("data-theme", stored);
    return stored === "light";
"#;

fn main() {
    #[cfg(target_arch = "wasm32")]
    {
        inject_head_resources();
    }
    dioxus::launch(App);
}

#[cfg(target_arch = "wasm32")]
fn inject_head_resources() {
    let doc = match web_sys::window().and_then(|w| w.document()) {
        Some(d) => d,
        None => return,
    };
    let head = match doc.head() {
        Some(h) => h,
        None => return,
    };

    if let Ok(el) = doc.create_element("style") {
        el.set_text_content(Some(include_str!("../assets/main.css")));
        let _ = head.append_child(&el);
    }

    if let Ok(el) = doc.create_element("link") {
        let _ = el.set_attribute("rel", "stylesheet");
        let _ = el.set_attribute(
            "href",
            "https://fonts.googleapis.com/css2?family=Inter:wght@400;500;600;700;800&display=swap",
        );
        let _ = head.append_child(&el);
    }

    // NOTE: html2pdf.js (+ html2canvas + jsPDF) previously loaded here has
    // been removed. That pipeline works by rasterizing the DOM into a
    // screenshot image and embedding that image in a PDF — producing a PDF
    // with no real text layer at all (unselectable, unsearchable, and
    // unreadable by any text-extraction tool, including this app's own PDF
    // import). We now use the browser's native print-to-PDF instead (see
    // download_pdf in cv_preview.rs / tailor.rs), which renders actual text
    // glyphs.

    if let Ok(el) = doc.create_element("link") {
        let _ = el.set_attribute("rel", "icon");
        let _ = el.set_attribute("type", "image/svg+xml");
        let _ = el.set_attribute("href", &asset!("/assets/cv-generator-icon.svg").to_string());
        let _ = head.append_child(&el);
    }

    if let Ok(el) = doc.create_element("link") {
        let _ = el.set_attribute("rel", "manifest");
        let _ = el.set_attribute("href", &asset!("/assets/manifest.json").to_string());
        let _ = head.append_child(&el);
    }
}

#[component]
fn App() -> Element {
    cv_generator::services::auth::init();

    use_context_provider(|| {
        let saved = cv_generator::services::storage::load_cv().unwrap_or_default();
        Signal::new(saved)
    });

    let lang = use_signal(i18n::Lang::detect);
    let theme = use_signal(i18n::Theme::detect);
    use_context_provider(|| lang);
    use_context_provider(|| theme);

    // Android (and the desktop renderer generally) compiles for a non-wasm
    // target, where web_sys does not exist: `Theme::detect()` cannot read
    // localStorage or matchMedia, so its result is only a placeholder. This
    // resolver performs the same detection inside the wry WebView instead.
    // The `theme_resolved` latch stops the reactive effect below from writing
    // the placeholder theme into localStorage before the resolver has read the
    // real value back — with no stored choice it must come from the device's
    // dark/light setting, and this run must win the read side of the race.
    #[cfg(not(target_arch = "wasm32"))]
    let theme_resolved = use_signal(|| false);

    #[cfg(not(target_arch = "wasm32"))]
    use_effect(move || {
        let mut theme = theme;
        let mut theme_resolved = theme_resolved;
        spawn(async move {
            // Reads no signals, so this effect fires exactly once. wry runs
            // `document::eval` through its query bridge, which wraps the script
            // in a function — hence the `return`.
            let eval_ = document::eval(RESOLVE_THEME_JS);
            let light = eval_.join::<bool>().await.unwrap_or(false);
            theme.set(if light {
                i18n::Theme::Light
            } else {
                i18n::Theme::Dark
            });
            theme_resolved.set(true);
        });
    });

    use_effect(move || {
        let t = theme();
        t.persist();
        #[cfg(target_arch = "wasm32")]
        if let Some(doc) = web_sys::window().and_then(|w| w.document()) {
            let _ = doc.document_element().map(|el| {
                let _ = el.set_attribute("data-theme", t.as_str());
            });
            if let Ok(Some(title)) = doc.query_selector("title") {
                title.set_text_content(Some("CV Generator"));
            }
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            // The wasm block above is compiled out here, so this is the only
            // path that reaches the WebView's DOM — previously `data-theme`
            // was never written on Android and the toggle only flipped its own
            // glyph. localStorage is the same store the landing and demo pages
            // use, keeping one theme across the web app and the APK.
            if !*theme_resolved.read() {
                return;
            }
            let _ = document::eval(&format!(
                "document.documentElement.setAttribute('data-theme', \"{}\");\
                 try {{ localStorage.setItem('cv_gen_theme', \"{}\"); }} catch (e) {{}}",
                t.as_str(),
                t.as_str(),
            ));
        }
    });

    use_effect(move || {
        let l = lang();
        l.persist();
    });

    rsx! {
        style { {include_str!("../assets/main.css")} }
        Router::<Route> {}
    }
}
