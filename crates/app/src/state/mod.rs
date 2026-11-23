//! The app's state, one controller per concern. Each is a bundle of signals
//! that is `Copy`, so views and event handlers can use it freely.

mod checker;
mod debounce;
mod document;
mod runner;
mod status;

pub use checker::Checker;
pub use document::Document;
pub use runner::{MAX_LINES, Runner};
pub use status::Status;
