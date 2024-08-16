//! A module with the core UI elements - Application and Window.

mod application;
mod window;

use glutin::prelude::*;
use window::RawWindow;

pub use application::Application;
