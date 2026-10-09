# vendor/

## manganis (0.7.9, locally patched)

Upstream `manganis` 0.7.9 contains `compile_error!("Only 64-bit Android targets
are supported")` in `src/android/callback.rs`, which makes
`--target armv7-linux-androideabi` impossible to build. Google Play then marks
the app incompatible with 32-bit-only phones.

The only change is in `rust_callback`: on 32-bit targets the handler pointer is
taken from the low `jlong` half instead of failing to compile. 64-bit behaviour
is unchanged. Wired in via `[patch.crates-io]` in the root `Cargo.toml`.

Remove this directory and the `[patch.crates-io]` entry once a released
manganis supports 32-bit Android.
