use crate::renderer;
use crate::core::window::Window;
use crate::core::*;

use glutin::config::{Config, ConfigTemplateBuilder};
use glutin_winit::DisplayBuilder;
use std::collections::HashMap;
use std::process;
use winit::application::ApplicationHandler;
use winit::error::EventLoopError;
use winit::event::{ElementState, KeyEvent, Modifiers, StartCause, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::window::WindowId;

/// A module with known application exit codes.
mod exit_codes {
    /// An error thrown by the OS.
    pub const OS_ERROR: i32 = 1;
    /// An operation is not supported by the rendering backend.
    pub const OP_NOT_SUPPORTED: i32 = 2;
    /// An error with the event loop.
    pub const EVENT_LOOP_ERROR: i32 = 3;
}

/// An application, the main entrypoint of the program.
pub struct Application {
    event_loop: EventLoop<()>,
    application: ApplicationInternal,
    initial_raw_window: RawWindow,
}

/// An internal struct handling OS event when the application is run.
struct ApplicationInternal {
    gl_config: Config,
    window_indices: HashMap<WindowId, usize>, // Normally if the EventLoop.ControlFlow is not Poll,
    windows: Vec<Window>,                     // there should just be a HashSet<WindowId, Window>
    keyboard_modifiers: Modifiers,
}

impl Application {
    /// Creates a new application.
    pub fn new() -> Self {
        let event_loop = EventLoop::new().expect("Failed to create event loop");

        let template = ConfigTemplateBuilder::new().with_transparency(true);

        /// A comparator for finding a config with the smallest [number of samples](GlConfig::num_samples).
        fn min_transparency(c1: Config, c2: Config) -> Config {
            let transparency1 = c1.supports_transparency().unwrap_or(false);
            let transparency2 = c2.supports_transparency().unwrap_or(false);
            if (transparency1 && !transparency2) || c1.num_samples() < c2.num_samples() {
                c1
            } else {
                c2
            }
        }

        let (raw_window, gl_config) = DisplayBuilder::new()
            .with_window_attributes(Some(Window::default_attrs()))
            .build(&event_loop, template, |configs| {
                configs.reduce(min_transparency).unwrap()
            })
            .expect("Could not create OpenGL config");
        let raw_window = raw_window.expect("Could not create window with OpenGL context");

        event_loop.set_control_flow(winit::event_loop::ControlFlow::Poll);

        Application {
            event_loop,
            application: ApplicationInternal {
                gl_config,
                window_indices: HashMap::new(),
                windows: Vec::new(),
                keyboard_modifiers: Modifiers::default(),
            },
            initial_raw_window: raw_window,
        }
    }

    /// Runs the application on the calling thread.
    pub fn run(mut self) -> ! {
        let Application {
            event_loop,
            ref mut application,
            initial_raw_window,
        } = self;
        application.open_first_window("Rust Skia Template", initial_raw_window);
        match event_loop.run_app(application) {
            Ok(_) => process::exit(0),
            Err(e) => match e {
                EventLoopError::NotSupported(e) => {
                    eprintln!("Operation not supported: {e}");
                    process::exit(exit_codes::OP_NOT_SUPPORTED)
                }
                EventLoopError::Os(e) => {
                    eprintln!("OS error: {e}");
                    process::exit(exit_codes::OS_ERROR)
                } 
                EventLoopError::RecreationAttempt => {
                    eprintln!("Event loop cannot be recreated!");
                    process::exit(exit_codes::EVENT_LOOP_ERROR)
                }
                EventLoopError::ExitFailure(code) => {
                    eprintln!("Unknown error with code: {code}");
                    process::exit(code)
                },
            }
        }
        
    }
}

impl Default for Application {
    fn default() -> Self {
        Application::new()
    }
}

impl ApplicationInternal {
    /// Opens the first window when the application is run.
    fn open_first_window(&mut self, title: &str, initial_raw_window: RawWindow) {
        let window = Window::from_initial_raw(title, initial_raw_window, &self.gl_config);
        self.window_indices.insert(window.id(), 0);
        self.windows.push(window);
    }

    /// Opens a new window.
    fn open_window(&mut self, title: &str, event_loop: &ActiveEventLoop) {
        let window = Window::new(title, event_loop, &self.gl_config);
        self.window_indices.insert(window.id(), self.windows.len());
        self.windows.push(window);
    }
}

impl ApplicationHandler for ApplicationInternal {
    // Stuff only for rendering the example animation. Can be safely removed in an actual application.
    fn new_events(&mut self, _: &ActiveEventLoop, cause: StartCause) {
        if let StartCause::Poll { .. } = cause {
            if !self.windows.is_empty() {
                self.windows[0].request_redraw()
            }
        }
    }

    fn resumed(&mut self, _event_loop: &ActiveEventLoop) {}

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        // Stuff only for rendering the example animation. Can be safely removed in an actual application.
        let frame_start = std::time::Instant::now();
        let window_count = self.windows.len();

        let window_index = match self.window_indices.get(&window_id) {
            Some(index) => *index,
            None => return,
        };
        let window = &mut self.windows[window_index];
        match event {
            WindowEvent::ActivationTokenDone { .. } => {}
            WindowEvent::Resized(physical_size) => window.resize(physical_size),
            WindowEvent::Moved(_) => {}
            WindowEvent::CloseRequested => {
                // window.make_current();
                self.window_indices.remove(&window_id);
                self.windows.remove(window_index);
                if self.windows.is_empty() {
                    event_loop.exit();
                    return;
                }
                for i in window_index..self.windows.len() {
                    let window = &mut self.windows[i];
                    let id = window.id();
                    self.window_indices.insert(id, i);
                }
            }
            WindowEvent::Destroyed => {}
            WindowEvent::DroppedFile(_) => {}
            WindowEvent::HoveredFile(_) => {}
            WindowEvent::HoveredFileCancelled => {}
            WindowEvent::Focused(_) => {}
            WindowEvent::KeyboardInput {
                event:
                    KeyEvent {
                        logical_key,
                        state: ElementState::Released,
                        repeat: false,
                        ..
                    },
                ..
            } => {
                if logical_key == "q" {
                    self.window_event(event_loop, window_id, WindowEvent::CloseRequested);
                } else if logical_key == "a" {
                    let title = format!("Window {}", window_count);
                    self.open_window(title.as_str(), event_loop);
                }
            }
            WindowEvent::KeyboardInput { .. } => {}
            WindowEvent::ModifiersChanged(new_mods) => self.keyboard_modifiers = new_mods,
            WindowEvent::Ime(_) => {}
            WindowEvent::CursorMoved { .. } => {}
            WindowEvent::CursorEntered { .. } => {}
            WindowEvent::CursorLeft { .. } => {}
            WindowEvent::MouseWheel { .. } => {}
            WindowEvent::MouseInput { .. } => {}
            WindowEvent::PinchGesture { .. } => {}
            WindowEvent::PanGesture { .. } => {}
            WindowEvent::DoubleTapGesture { .. } => {}
            WindowEvent::RotationGesture { .. } => {}
            WindowEvent::TouchpadPressure { .. } => {}
            WindowEvent::AxisMotion { .. } => {}
            WindowEvent::Touch(_) => {}
            WindowEvent::ScaleFactorChanged { .. } => {}
            WindowEvent::ThemeChanged(_) => {}
            WindowEvent::Occluded(_) => {}
            WindowEvent::RedrawRequested => {
                // Stuff only for rendering the example animation. Can be safely removed in an actual application.
                let frame_duration = std::time::Duration::from_secs_f64(1.0 / 60.0);
                if frame_start - window.previous_frame_start > frame_duration {
                    window.previous_frame_start = frame_start;
                    window.frame += 1;
                    let frame = window.frame;
                    window.reset_canvas(skia_safe::Color::WHITE);
                    window.draw(|canvas| {
                        renderer::render_frame(frame % 360, 60, 60, canvas);
                    });
                }
                let next_window_index = window_index + 1;
                if next_window_index < window_count {
                    self.windows[next_window_index].request_redraw();
                }
            }
        }
    }
}
