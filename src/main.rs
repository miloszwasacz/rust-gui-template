use rust_gui_template::core::Application;

fn main() {
    run_example();
}

/// Runs the example application.
pub fn run_example() {
    let app = Application::new();
    app.run();
}