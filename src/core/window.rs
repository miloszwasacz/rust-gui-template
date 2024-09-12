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
use skia_safe::{scalar, Canvas, ColorType, Surface};
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
    num_samples: usize,
    stencil_size: usize,
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

        let mut window = Window {
            raw,
            gl,
            skia,

            // Stuff only for rendering the example animation. Can be safely removed in an actual application.
            frame: 0,
            previous_frame_start: std::time::Instant::now(),
        };
        window.update_scale_factor();
        window
    }

    /// Returns the window's unique ID.
    pub fn id(&self) -> WindowId {
        self.raw.id()
    }

    /// Resets the canvas to its initial state ([Matrix](skia_safe::Matrix) and [Clip](Canvas::local_clip_bounds))
    /// and [clears](Canvas::clear) it with the `background` color.
    pub fn reset_canvas(&mut self, background: impl Into<skia_safe::Color4f>) {
        self.skia.surface.canvas().restore_to_count(0);
        self.skia.surface.canvas().reset_matrix();
        self.update_scale_factor();
        self.skia.surface.canvas().clear(background);
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
        self.skia.resize_surface(new_size);
    }

    /// Updates the scale factor of the window's canvas.
    fn update_scale_factor(&mut self) {
        let scale_factor = self.raw.scale_factor() as scalar;
        self.skia
            .surface
            .canvas()
            .scale((scale_factor, scale_factor));
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

        let num_samples = gl_config.num_samples() as usize;
        let stencil_size = gl_config.stencil_size() as usize;

        let surface = Self::create_surface(
            &mut direct_ctx,
            raw_window.inner_size(),
            num_samples,
            stencil_size,
        );

        Skia {
            surface,
            direct_ctx,
            num_samples,
            stencil_size,
        }
    }

    fn create_surface(
        direct_ctx: &mut DirectContext,
        size: PhysicalSize<u32>,
        num_samples: usize,
        stencil_size: usize,
    ) -> Surface {
        let size = (
            size.width.try_into().expect("Could not convert width"),
            size.height.try_into().expect("Could not convert height"),
        );
        let fb_info = unsafe {
            let mut fboid = 0;
            gl::GetIntegerv(gl::FRAMEBUFFER_BINDING, &mut fboid);

            FramebufferInfo {
                fboid: fboid.try_into().unwrap(),
                format: Format::RGBA8.into(),
                ..Default::default()
            }
        };
        let target = backend_render_targets::make_gl(size, num_samples, stencil_size, fb_info);

        wrap_backend_render_target(
            direct_ctx,
            &target,
            SurfaceOrigin::BottomLeft,
            ColorType::RGBA8888,
            None,
            None,
        )
        .expect("Could not create Skia surface")
    }

    fn resize_surface(&mut self, size: PhysicalSize<u32>) {
        self.surface = Self::create_surface(
            &mut self.direct_ctx,
            size,
            self.num_samples,
            self.stencil_size,
        );
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
