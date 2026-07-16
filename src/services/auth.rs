// ── Google Identity Services (GIS) OAuth2 ─────────────────────────────────────
//
// Uses the Google-hosted `google.accounts.oauth2` JS library to obtain an
// access token. The library handles the full auth flow client-side so no
// `client_secret` is needed.

#[cfg(target_arch = "wasm32")]
const TOKEN_KEY: &str = "cv_generator_google_token";

// ── Token listeners (WASM only) ──────────────────────────────────────────────

#[cfg(target_arch = "wasm32")]
use std::sync::OnceLock;

#[cfg(target_arch = "wasm32")]
static TOKEN_CLIENT: OnceLock<wasm_bindgen::JsValue> = OnceLock::new();

#[cfg(target_arch = "wasm32")]
use std::cell::RefCell;

#[cfg(target_arch = "wasm32")]
thread_local! {
    static TOKEN_LISTENERS: RefCell<Vec<Box<dyn FnMut(&str)>>> = const { RefCell::new(Vec::new()) };
}

#[cfg(target_arch = "wasm32")]
pub fn on_token_received(cb: Box<dyn FnMut(&str)>) {
    TOKEN_LISTENERS.with(|listeners| {
        listeners.borrow_mut().push(cb);
    });
}

// ── Token storage ────────────────────────────────────────────────────────────

#[cfg(target_arch = "wasm32")]
pub fn get_token() -> Option<String> {
    use gloo_storage::{LocalStorage, Storage};
    LocalStorage::get(TOKEN_KEY).ok()
}

#[cfg(target_arch = "wasm32")]
pub fn set_token(token: &str) {
    use gloo_storage::{LocalStorage, Storage};
    if token.is_empty() {
        LocalStorage::delete(TOKEN_KEY);
    } else {
        let _ = LocalStorage::set(TOKEN_KEY, token);
    }
}

#[cfg(target_arch = "wasm32")]
pub fn clear_token() {
    use gloo_storage::{LocalStorage, Storage};
    LocalStorage::delete(TOKEN_KEY);
}

#[cfg(not(target_arch = "wasm32"))]
pub fn get_token() -> Option<String> {
    None
}
#[cfg(not(target_arch = "wasm32"))]
pub fn set_token(_token: &str) {}
#[cfg(not(target_arch = "wasm32"))]
pub fn clear_token() {}

// ── OAuth flow (WASM — GIS library) ──────────────────────────────────────────

/// Initialise the Google Identity Services library by injecting its script tag.
/// Call once on app startup.
#[cfg(target_arch = "wasm32")]
pub fn init() {
    use wasm_bindgen::JsCast;
    let doc = web_sys::window()
        .expect("no window in WASM")
        .document()
        .expect("no document in WASM");

    if get_gis_oauth2().is_ok() {
        return;
    }

    if let Ok(script) = doc.create_element("script") {
        if let Ok(s) = script.dyn_into::<web_sys::HtmlScriptElement>() {
            s.set_src("https://accounts.google.com/gsi/client");
            s.set_defer(true);
            if let Some(body) = doc.body() {
                let _ = body.append_child(&s);
            }
        }
    }
}

#[cfg(target_arch = "wasm32")]
fn get_gis_oauth2() -> Result<wasm_bindgen::JsValue, wasm_bindgen::JsValue> {
    let google = js_sys::Reflect::get(&js_sys::global(), &"google".into())?;
    let accounts = js_sys::Reflect::get(&google, &"accounts".into())?;
    js_sys::Reflect::get(&accounts, &"oauth2".into())
}

/// Trigger the Google sign-in flow using the GIS Token Client.
/// Calls `on_token_received` listeners with the access token on success.
#[cfg(target_arch = "wasm32")]
pub fn start_oauth(client_id: &str, _redirect_uri: &str) {
    use wasm_bindgen::JsCast;

    if let Some(tc) = TOKEN_CLIENT.get() {
        if let Some(f) = js_sys::Reflect::get(tc, &"requestAccessToken".into())
            .ok()
            .and_then(|v| v.dyn_into::<js_sys::Function>().ok())
        {
            let _ = f.call0(tc);
        }
        return;
    }

    if get_gis_oauth2().is_err() {
        return;
    }

    use wasm_bindgen::prelude::Closure;

    let cb = Closure::wrap(Box::new(move |resp: wasm_bindgen::JsValue| {
        if let Some(token) = js_sys::Reflect::get(&resp, &"access_token".into())
            .ok()
            .and_then(|t| t.as_string())
        {
            set_token(&token);
            TOKEN_LISTENERS.with(|listeners| {
                for cb in listeners.borrow_mut().iter_mut() {
                    cb(&token);
                }
            });
        }
    }) as Box<dyn FnMut(wasm_bindgen::JsValue)>);

    let config = js_sys::Object::new();
    let _ = js_sys::Reflect::set(&config, &"client_id".into(), &client_id.into());
    let _ = js_sys::Reflect::set(
        &config,
        &"scope".into(),
        &"https://www.googleapis.com/auth/drive.appdata".into(),
    );
    let _ = js_sys::Reflect::set(&config, &"callback".into(), cb.as_ref());

    if let Ok(oauth2) = get_gis_oauth2() {
        if let Some(f) = js_sys::Reflect::get(&oauth2, &"initTokenClient".into())
            .ok()
            .and_then(|v| v.dyn_into::<js_sys::Function>().ok())
        {
            if let Ok(tc) = f.call1(&oauth2, &config) {
                cb.forget();
                TOKEN_CLIENT.set(tc.clone()).ok();
                if let Some(f) = js_sys::Reflect::get(&tc, &"requestAccessToken".into())
                    .ok()
                    .and_then(|v| v.dyn_into::<js_sys::Function>().ok())
                {
                    let _ = f.call0(&tc);
                }
            }
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn init() {}

#[cfg(not(target_arch = "wasm32"))]
pub fn start_oauth(_client_id: &str, _redirect_uri: &str) {}

/// Handle OAuth redirect — no-op with GIS library (no redirect flow used).
pub fn handle_oauth_redirect() {}

// ── UI helper ─────────────────────────────────────────────────────────────────

pub fn mask_token(t: &str) -> String {
    if t.len() > 8 {
        format!("{}…{}", &t[..4], &t[t.len() - 4..])
    } else {
        "••••".to_string()
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mask_token_long() {
        assert_eq!(mask_token("abcdefghijklmnop"), "abcd…mnop");
    }

    #[test]
    fn mask_token_short() {
        assert_eq!(mask_token("abc"), "••••");
    }

    #[test]
    fn get_set_clear_token_native_stubs() {
        set_token("test");
        assert!(get_token().is_none());
        clear_token();
    }
}
