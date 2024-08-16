# Rust Skia GUI template

This is a template for a GUI application or library that uses [Skia](https://github.com/rust-skia/rust-skia) 
for drawing, [OpenGL](https://github.com/rust-windowing/glutin) for rendering, 
and [`winit`](https://github.com/rust-windowing/winit) for window management.

It is based on the `skia-safe`'s [_gl-window_ example](https://github.com/rust-skia/rust-skia/tree/master/skia-safe/examples/gl-window).

## How to use

This template implements two core components: `Application` and `Window`.

### Application

It is the main entrypoint of the GUI. It initializes the event loop, creates a window, and runs the app.
It is also responsible for handling the window events, such as resizing, redrawing, device input, etc.

To use the `Application`, create a new instance with `Application::new()`, and call the `run()` method.

### Window

It is a wrapper around the `winit::window::Window`, `glutin`'s OpenGl `Context` and `Surface`, and Skia's `Surface`.

To open a new window, use the `ApplicationInternal::open_window()` method.
To draw on the window's canvas, use the `Window::draw()` method.