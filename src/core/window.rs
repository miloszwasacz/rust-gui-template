use self::helper::*;
use crate::core::*;

use glutin::config::Config;
use glutin::context::{ContextApi, ContextAttributesBuilder, PossiblyCurrentContext};
use glutin::display::GetGlDisplay;
use glutin::surface::{Surface as GLSurface, SurfaceAttributesBuilder, WindowSurface};
#[allow(deprecated)]
use raw_window_handle::HasRawWindowHandle;
use skia_safe::gpu::gl::{Format, FramebufferInfo, Interface};
use skia_safe::gpu::surfaces::wrap_backend_render_target;
use skia_safe::gpu::{backend_render_targets, direct_contexts, DirectContext, SurfaceOrigin};
use skia_safe::{Canvas, ColorType, Surface};
use std::ffi::CString;
use winit::dpi::{LogicalSize, PhysicalSize};
use winit::event_loop::ActiveEventLoop;
use winit::window::{WindowAttributes, WindowId};

/// A window produced by `winit`.
///
/// The window must have an OpenGL context attached, so it should only be created
/// using [`glutin_winit::DisplayBuilder::build`] or [`glutin_winit::finalize_window`]
pub(super) type RawWindow = winit::window::Window;

/// A window with a Skia canvas.
pub struct Window {
    raw: RawWindow,
    gl: OpenGL,
    skia: Skia,

    // Stuff only for rendering the example animation. Can be safely removed in an actual application.
    #[allow(missing_docs)]
    pub frame: usize,
    #[allow(missing_docs)]
    pub previous_frame_start: std::time::Instant,
}

/// Properties to  OpenGL
struct OpenGL {
    surface: GLSurface<WindowSurface>,
    ctx: PossiblyCurrentContext,
}

/// Properties required to draw with Skia.
struct Skia {
    surface: Surface,
    direct_ctx: DirectContext,
}

impl Window {
    /// Creates a new window using the [`RawWindow`] initially created along with the [`Application`].
    ///
    /// This method should only be used once when the application is first run.
    pub(super) fn from_initial_raw(
        title: &str,
        initial_raw: RawWindow,
        gl_config: &Config,
    ) -> Self {
        initial_raw.set_title(title);
        Window::from_raw(initial_raw, gl_config)
    }

    /// Creates a new window
    pub(super) fn new(title: &str, event_loop: &ActiveEventLoop, gl_config: &Config) -> Self {
        let window_attrs = Window::default_attrs();
        let raw_window = glutin_winit::finalize_window(event_loop, window_attrs, gl_config)
            .expect("Could not create window with OpenGL context");
        raw_window.set_title(title);

        Window::from_raw(raw_window, gl_config)
    }

    fn from_raw(raw: RawWindow, gl_config: &Config) -> Self {
        let gl = OpenGL::new(gl_config, &raw);
        let skia = Skia::new(&raw, gl_config);

        Window {
            raw,
            gl,
            skia,

            // Stuff only for rendering the example animation. Can be safely removed in an actual application.
            frame: 0,
            previous_frame_start: std::time::Instant::now(),
        }
    }

    /// Returns the window's unique ID.
    pub fn id(&self) -> WindowId {
        self.raw.id()
    }

    /// Draws on the window's Skia canvas using the instructions defined in `drawing`.
    pub fn draw(&mut self, mut drawing: impl FnMut(&Canvas)) {
        self.make_current();
        drawing(self.skia.surface.canvas());
        self.skia.direct_ctx.flush_and_submit();
        self.gl.surface.swap_buffers(&self.gl.ctx).unwrap();
    }

    /// Requests the window to be redrawn.
    pub(super) fn request_redraw(&self) {
        self.raw.request_redraw();
    }

    /// Resizes the window.
    pub(super) fn resize(&mut self, new_size: PhysicalSize<u32>) {
        let PhysicalSize { width, height } = new_size;
        self.gl
            .surface
            .resize(&self.gl.ctx, u32_to_nonzero(width), u32_to_nonzero(height));
    }

    /// Makes the window's OpenGL context current. Should be called before
    /// drawing on the window's canvas.
    fn make_current(&self) {
        self.gl
            .ctx
            .make_current(&self.gl.surface)
            .expect("Could not make OpenGL context current");
    }

    /// Default attributes for window creation.
    pub(super) fn default_attrs() -> WindowAttributes {
        WindowAttributes::default()
            .with_title("Rust Skia Template")
            .with_inner_size(LogicalSize::new(500, 500))
    }
}

impl Drop for Window {
    fn drop(&mut self) {
        self.make_current()
    }
}

impl OpenGL {
    fn new(config: &Config, raw_window: &RawWindow) -> Self {
        #[allow(deprecated)]
        let raw_window_handle = raw_window
            .raw_window_handle()
            .expect("Failed to retrieve RawWindowHandle");

        let not_current_ctx = unsafe {
            let create_ctx = |ctx_attrs| config.display().create_context(config, &ctx_attrs);

            let attrs = ContextAttributesBuilder::new().build(Some(raw_window_handle));
            create_ctx(attrs).unwrap_or_else(|_| {
                let fallback_attrs = ContextAttributesBuilder::new()
                    .with_context_api(ContextApi::Gles(None))
                    .build(Some(raw_window_handle));
                create_ctx(fallback_attrs).expect("Failed to create OpenGL context")
            })
        };

        let PhysicalSize { width, height } = raw_window.inner_size();

        let surface = unsafe {
            let attrs = SurfaceAttributesBuilder::<WindowSurface>::new().build(
                raw_window_handle,
                u32_to_nonzero(width),
                u32_to_nonzero(height),
            );

            config
                .display()
                .create_window_surface(config, &attrs)
                .expect("Could not create OpenGL window surface")
        };

        let ctx = not_current_ctx
            .make_current(&surface)
            .expect("Could not make OpenGL context current when setting up Skia renderer");

        OpenGL { surface, ctx }
    }
}

impl Skia {
    fn new(raw_window: &RawWindow, gl_config: &Config) -> Self {
        fn get_proc_address(gl_config: &Config, addr: &str) -> *const std::ffi::c_void {
            let addr = CString::new(addr).unwrap();
            gl_config.display().get_proc_address(&addr)
        }

        gl::load_with(|addr| get_proc_address(gl_config, addr));

        let interface = Interface::new_load_with(|addr| match addr {
            "eglGetCurrentDisplay" => std::ptr::null(),
            _ => get_proc_address(gl_config, addr),
        })
        .expect("Could not create OpenGL interface");

        let mut direct_ctx =
            direct_contexts::make_gl(interface, None).expect("Could not create direct context");

        let fb_info = unsafe {
            let mut fboid = 0;
            gl::GetIntegerv(gl::FRAMEBUFFER_BINDING, &mut fboid);

            FramebufferInfo {
                fboid: fboid.try_into().unwrap(),
                format: Format::RGBA8.into(),
                ..Default::default()
            }
        };

        let num_samples = gl_config.num_samples() as usize;
        let stencil_size = gl_config.stencil_size() as usize;

        let PhysicalSize { width, height } = raw_window.inner_size();
        let size = (
            width.try_into().expect("Could not convert width"),
            height.try_into().expect("Could not convert height"),
        );

        let target = backend_render_targets::make_gl(size, num_samples, stencil_size, fb_info);
        let surface = wrap_backend_render_target(
            &mut direct_ctx,
            &target,
            SurfaceOrigin::BottomLeft,
            ColorType::RGBA8888,
            None,
            None,
        )
        .expect("Could not create Skia surface");

        Skia {
            surface,
            direct_ctx,
        }
    }
}

mod helper {
    use std::num::NonZeroU32;

    /// Converts the `value` to a [`NonZeroU32`] if it's greater than 0,
    /// or returns [`NonZeroU32::MIN`] otherwise.
    pub fn u32_to_nonzero(value: u32) -> NonZeroU32 {
        NonZeroU32::new(value).unwrap_or(NonZeroU32::MIN)
    }
}
