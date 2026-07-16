// Library crate: pure Rust, no Dioxus, no WASM deps.
// `cargo test --lib` targets this crate exclusively, which means:
//   - tests compile and run on any native host (Linux CI, Mac, Windows)
//   - the full Dioxus/WASM dependency tree is never pulled in during testing
//   - the binary (main.rs + views/) imports from here via `cv_generator::`

pub mod i18n_core;
pub mod models;
pub mod services;
