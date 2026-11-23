//! The Pyde user interface, built with Leptos: `components` draw the
//! controllers in `state`, which reach the workers through `clients`.

mod app;
mod clients;
mod components;
mod editing;
mod files;
mod state;

pub use app::App;

pub fn mount() {
    leptos::mount::mount_to_body(App);
}
