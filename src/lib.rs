//! Main template for a Rust GUI library.
#![warn(missing_docs)]

mod renderer;
pub mod core;

/// Runs the example application.
pub fn run_example() {
    let app = core::Application::new();
    app.run();
}
