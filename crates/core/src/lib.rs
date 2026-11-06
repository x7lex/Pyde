mod app;
mod components;
mod files;
mod highlight;
mod python;

pub use app::App;

/// Mounts the Pyde workbench into `<body>`.
pub fn mount() {
    leptos::mount::mount_to_body(App);
}
