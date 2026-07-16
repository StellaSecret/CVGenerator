#![allow(non_snake_case)]

mod i18n;
mod router;
mod views;

use dioxus::prelude::*;
use router::Route;

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

    if let Ok(el) = doc.create_element("script") {
        let _ = el.set_attribute(
            "src",
            "https://cdnjs.cloudflare.com/ajax/libs/html2pdf.js/0.10.1/html2pdf.bundle.min.js",
        );
        let _ = head.append_child(&el);
    }

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
