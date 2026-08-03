#[cfg(not(target_arch = "wasm32"))]
include!("native_main.rs");

#[cfg(target_arch = "wasm32")]
fn main() {}
