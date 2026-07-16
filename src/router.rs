use dioxus::prelude::*;
use dioxus_router::Routable;

use crate::i18n;
use crate::views::{
    cv_editor::CvEditor, cv_preview::CvPreview, home::Home, sync::Sync, tailor::Tailor,
};

#[derive(Clone, PartialEq, Routable, Debug)]
pub enum Route {
    #[layout(NavLayout)]
    #[route("/")]
    Home {},
    #[route("/cv/edit")]
    CvEditor {},
    #[route("/cv/preview")]
    CvPreview {},
    #[route("/tailor")]
    Tailor {},
    #[route("/sync")]
    Sync {},
}

#[component]
fn NavLayout() -> Element {
    let mut lang = use_context::<Signal<i18n::Lang>>();
    let mut theme = use_context::<Signal<i18n::Theme>>();

    let toggle_theme = move |_: MouseEvent| {
        let t = theme();
        theme.set(t.toggle());
    };
    let toggle_lang = move |_: MouseEvent| {
        let mut l = lang();
        l = l.toggle();
        l.persist();
        lang.set(l);
    };

    rsx! {
        div { class: "app",
            header { class: "nav",
                Link { to: Route::Home {}, class: "nav-brand",
                    img { src: asset!("/assets/cv-generator-icon.svg"), class: "nav-brand-icon", alt: "" }
                    { i18n::tr("nav_brand", lang()) }
                }
                div { class: "nav-links",
                    Link { to: Route::Home {},      class: "nav-link", active_class: "nav-link-active",
                        { i18n::tr("nav_home", lang()) } }
                    Link { to: Route::CvEditor {},  class: "nav-link", active_class: "nav-link-active",
                        { i18n::tr("nav_cv", lang()) } }
                    Link { to: Route::CvPreview {}, class: "nav-link", active_class: "nav-link-active",
                        { i18n::tr("nav_preview", lang()) } }
                    Link { to: Route::Tailor {},    class: "nav-link", active_class: "nav-link-active",
                        { i18n::tr("nav_tailor", lang()) } }
                    Link { to: Route::Sync {},      class: "nav-link", active_class: "nav-link-active",
                        { i18n::tr("nav_sync", lang()) } }
                }
                div { class: "nav-toggles",
                    button { class: "nav-toggle", onclick: toggle_theme,
                        { theme().label() }
                    }
                    button { class: "nav-toggle", onclick: toggle_lang,
                        { if lang() == i18n::Lang::Fr { "EN" } else { "FR" } }
                    }
                }
            }
            main { class: "main",
                Outlet::<Route> {}
            }
        }
    }
}
