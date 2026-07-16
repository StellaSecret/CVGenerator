use crate::i18n;
use crate::router::Route;
use cv_generator::models::LifetimeCV;
use cv_generator::services::auth;
use cv_generator::services::drive;
use cv_generator::services::storage::save_cv;
use dioxus::prelude::*;

const BUILD_TIME_CLIENT_ID: Option<&str> = option_env!("GOOGLE_CLIENT_ID");

fn make_ok(s: &str) -> String {
    format!("✅  {s}")
}
fn make_err(s: &str) -> String {
    format!("❌  {s}")
}

#[component]
pub fn Sync() -> Element {
    let mut cv: Signal<LifetimeCV> = use_context();
    let lang: Signal<i18n::Lang> = use_context();
    let l = *lang.read();
    let mut token = use_signal(|| auth::get_token().unwrap_or_default());
    let mut status = use_signal(String::new);
    let mut loading = use_signal(|| false);

    #[cfg(target_arch = "wasm32")]
    use_hook(|| {
        let l = *lang.read();
        auth::on_token_received(Box::new(move |tok: &str| {
            token.set(tok.to_string());
            status.set(make_ok(&i18n::tr("sy_signed_in_msg", l)));
        }));
    });

    let signed_in = !token.read().is_empty();
    let has_cv = !cv.read().personal.name.is_empty();
    let has_cid = BUILD_TIME_CLIENT_ID.filter(|s| !s.is_empty()).is_some();
    let busy = *loading.read();
    let status_txt = status.read().clone();

    let t_nav = i18n::tr("nav_back", l);
    let t_title = i18n::tr("sy_title", l);
    let t_sub = i18n::tr("sy_subtitle", l);
    let t_gdrive = i18n::tr("sy_gdrive", l);
    let t_gd_desc = i18n::tr("sy_gdrive_desc", l);
    let t_signin = i18n::tr("sy_sign_in", l);
    let t_signedin = i18n::tr("sy_signed_in", l);
    let t_signedout = i18n::tr("sy_signed_out", l);
    let t_configerr = i18n::tr("sy_config_err", l);
    let t_connect = i18n::tr("sy_connecting", l);
    let t_working = i18n::tr("sy_working", l);
    let t_backup = i18n::tr("sy_backup", l);
    let t_restore = i18n::tr("sy_restore", l);
    let t_backupok = i18n::tr("sy_backup_ok", l);
    let t_restoreok = i18n::tr("sy_restore_ok", l);
    let t_local = i18n::tr("sy_local", l);
    let t_ldesc = i18n::tr("sy_local_desc", l);
    let t_export = i18n::tr("sy_export", l);
    let t_jsonok = i18n::tr("sy_json_ok", l);

    rsx! {
        div { class: "page",
            div { class: "page-back-row",
                Link { to: Route::Home {}, class: "page-back-link", "{t_nav}" }
            }
            div { class: "page-header",
                h1 { "{t_title}" }
                p { class: "subtitle", "{t_sub}" }
            }

            div { class: "sync-card",
                h2 { class: "sync-card-title",
                    span { class: "sync-card-icon", "☁️" }
                    "{t_gdrive}"
                }
                p { class: "sync-card-desc", "{t_gd_desc}" }

                div { class: "sync-google-row",
                    button {
                        class: if signed_in { "btn btn-google btn-google-done" } else { "btn btn-google" },
                        disabled: busy,
                        onclick: move |_| {
                            if signed_in {
                                auth::clear_token();
                                token.set(String::new());
                                status.set(make_ok(t_signedout));
                                return;
                            }
                            if !has_cid {
                                status.set(make_err(t_configerr));
                                return;
                            }
                            let cid = BUILD_TIME_CLIENT_ID.unwrap_or_default().to_string();
                            auth::start_oauth(&cid, "");
                        },
                        svg {
                            xmlns: "http://www.w3.org/2000/svg",
                            width: "20", height: "20", view_box: "0 0 48 48",
                            path { fill: "#EA4335", d: "M24 9.5c3.54 0 6.71 1.22 9.21 3.6l6.85-6.85C35.9 2.38 30.47 0 24 0 14.62 0 6.51 5.38 2.56 13.22l7.98 6.19C12.43 13.72 17.74 9.5 24 9.5z" }
                            path { fill: "#4285F4", d: "M46.98 24.55c0-1.57-.15-3.09-.38-4.55H24v9.02h12.94c-.58 2.96-2.26 5.48-4.78 7.18l7.73 6c4.51-4.18 7.09-10.36 7.09-17.65z" }
                            path { fill: "#FBBC05", d: "M10.53 28.59c-.48-1.45-.76-2.99-.76-4.59s.27-3.14.76-4.59l-7.98-6.19C.92 16.46 0 20.12 0 24c0 3.88.92 7.54 2.56 10.78l7.97-6.19z" }
                            path { fill: "#34A853", d: "M24 48c6.48 0 11.93-2.13 15.89-5.81l-7.73-6c-2.18 1.48-4.97 2.31-8.16 2.31-6.26 0-11.57-4.22-13.47-9.91l-7.98 6.19C6.51 42.62 14.62 48 24 48z" }
                        }
                        if busy { "{t_connect}" }
                        else if signed_in { "{t_signedin}" }
                        else { "{t_signin}" }
                    }
                    if signed_in {
                        span { class: "sync-cid-masked", "{auth::mask_token(&token.read())}" }
                    }
                }

                if signed_in {
                    div { class: "sync-drive-actions",
                        button {
                            class: "btn btn-primary",
                            disabled: busy || !has_cv,
                            onclick: move |_| {
                                let t       = token.read().clone();
                                let cv_snap = cv.read().clone();
                                loading.set(true);
                                let tok_label = t_backupok;
                                spawn(async move {
                                    match drive::drive_backup(&cv_snap, &t).await {
                                        Ok(_)  => status.set(make_ok(tok_label)),
                                        Err(e) => status.set(make_err(&e)),
                                    }
                                    loading.set(false);
                                });
                            },
                            if busy { "{t_working}" } else { "{t_backup}" }
                        }
                        button {
                            class: "btn btn-secondary",
                            disabled: busy,
                            onclick: move |_| {
                                let t = token.read().clone();
                                loading.set(true);
                                let tok_label = t_restoreok;
                                spawn(async move {
                                    match drive::drive_restore(&t).await {
                                        Ok(restored) => {
                                            save_cv(&restored);
                                            *cv.write() = restored;
                                            status.set(make_ok(tok_label));
                                        }
                                        Err(e) => status.set(make_err(&e)),
                                    }
                                    loading.set(false);
                                });
                            },
                            if busy { "{t_working}" } else { "{t_restore}" }
                        }
                    }
                }
            }

            div { class: "sync-card",
                h2 { class: "sync-card-title",
                    span { class: "sync-card-icon", "💾" }
                    "{t_local}"
                }
                p { class: "sync-card-desc", "{t_ldesc}" }
                div { class: "sync-drive-actions",
                    button {
                        class: "btn btn-outline",
                        disabled: !has_cv,
                        onclick: move |_| {
                            drive::local_export(&cv.read());
                            status.set(make_ok(t_jsonok));
                        },
                        "{t_export}"
                    }
                    ImportButton { cv, status, lang }
                }
            }

            if !status_txt.is_empty() {
                div {
                    class: if status_txt.starts_with("✅") { "sync-status sync-status-ok" }
                           else { "sync-status sync-status-err" },
                    "{status_txt}"
                }
            }
        }
    }
}

#[component]
fn ImportButton(
    cv: Signal<LifetimeCV>,
    mut status: Signal<String>,
    lang: Signal<i18n::Lang>,
) -> Element {
    #[cfg(target_arch = "wasm32")]
    {
        use wasm_bindgen::closure::Closure;
        use wasm_bindgen::JsCast;
        let l = *lang.read();
        let t_import_ok = i18n::tr("sy_import_ok", l).to_string();
        let t_import = i18n::tr("sy_import", l).to_string();
        return rsx! {
            button {
                class: "btn btn-outline",
                onclick: move |_| {
                    let window = web_sys::window().expect("window");
                    let doc    = window.document().expect("document");
                    let input  = doc.create_element("input").expect("input");
                    let _ = input.set_attribute("type",   "file");
                    let _ = input.set_attribute("accept", ".json");
                    let _ = input.set_attribute("style",  "display:none");
                    let input2 = input.clone();
                    let ok_msg2 = t_import_ok.clone();
                    let cb = Closure::<dyn FnMut()>::new(move || {
                        use js_sys::Reflect;
                        if let Some(files) = Reflect::get(&input2, &"files".into()).ok()
                            .and_then(|f| f.dyn_into::<web_sys::FileList>().ok())
                        {
                            if files.length() > 0 {
                                let file   = files.get(0).unwrap();
                                let reader = web_sys::FileReader::new().expect("FileReader");
                                let r2     = reader.clone();
                                let ok_msg3 = ok_msg2.clone();
                                let onload = Closure::<dyn FnMut()>::new(move || {
                                    if let Some(text) = r2.result().ok().and_then(|r| r.as_string()) {
                                        match drive::restore_from_json(&text) {
                                            Ok(restored) => {
                                                save_cv(&restored);
                                                *cv.write() = restored;
                                                status.set(make_ok(&ok_msg3));
                                            }
                                            Err(e) => status.set(make_err(&e)),
                                        }
                                    }
                                });
                                reader.set_onload(Some(onload.as_ref().unchecked_ref()));
                                onload.forget();
                                let _ = reader.read_as_text(file.unchecked_ref());
                            }
                        }
                    });
                    if let Some(body) = doc.body() { let _ = body.append_child(&input); }
                    input.unchecked_ref::<web_sys::EventTarget>()
                        .add_event_listener_with_callback("change", cb.as_ref().unchecked_ref()).ok();
                    input.dyn_ref::<web_sys::HtmlElement>().map(|el| el.click());
                    cb.forget();
                },
                "{t_import}"
            }
        };
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let l = *lang.read();
        let t_import = i18n::tr("sy_import", l);
        rsx! { button { class: "btn btn-outline", disabled: true, "{t_import}" } }
    }
}
