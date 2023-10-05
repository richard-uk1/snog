//! This crate provides a wrapper for vello that adheres to the KISS philosophy.
//!
//! # Goals
//!
//! - keep it simple, stupid
//! - Be as thin as possible, to make it easy to keep in sync with vello
//!
//! # Non-goals
//!
//! - Do everything that vello does - if you want that then use vello!
//! - Multiple windows
//!
//! Having said that, if you want a feature that isn't implemented, and you can implement it in a
//! way that is *simple to use*, then feel free to PR.
//!
//! # Todo
//!
//! - Text
//!
//! # Name
//!
//! The word 'snog' is as an informal name for a sloppy kiss in the UK. The code in the crate may
//! or may not be sloppy.
use std::ops::{Deref, DerefMut};
pub use vello::{kurbo, peniko, SceneBuilder, SceneFragment};
use vello::{
    kurbo::{Affine, Point, Size},
    peniko::Color,
    util::{RenderContext, RenderSurface},
    Renderer, RendererOptions, Scene,
};
use winit::{
    dpi::{LogicalPosition, PhysicalPosition},
    event::{Event as WEvent, ModifiersState, MouseScrollDelta, WindowEvent},
    event_loop::EventLoop,
    window::Window,
};
#[doc(inline)]
pub use winit::{
    event::{ElementState, MouseButton, VirtualKeyCode},
    event_loop::ControlFlow,
};

/// Events that you can use to update your internal state.
#[non_exhaustive]
#[derive(Debug)]
pub enum Event {
    /// The 'x' at the top of the screen was pressed, or a request was made to close the window in
    /// some other way.
    CloseRequested,
    CursorMoved {
        pos: Point,
    },
    MouseInput {
        state: ElementState,
        button: MouseButton,
    },
    MouseWheel {
        delta: f64,
    },
    KeyboardInput {
        state: ElementState,
        keycode: VirtualKeyCode,
    },
    /// The window was resized or the scale factor changed.
    Resized {
        screen: Screen,
    },
    ModifiersChanged(ModifiersState),
}

impl Event {
    fn from_winit_window(evt: WindowEvent, screen: Screen) -> Option<Self> {
        match evt {
            WindowEvent::CloseRequested => Some(Self::CloseRequested),
            WindowEvent::CursorMoved { position, .. } => {
                let p: LogicalPosition<f64> = position.to_logical(screen.scale_factor);
                Some(Event::CursorMoved {
                    pos: Point::new(p.x, p.y),
                })
            }
            WindowEvent::MouseInput { state, button, .. } => {
                Some(Event::MouseInput { state, button })
            }
            WindowEvent::MouseWheel { delta, .. } => {
                // TODO bit of a hack to unify lines and pixels (assumes 20px line height)
                match delta {
                    MouseScrollDelta::PixelDelta(PhysicalPosition { y, .. }) => {
                        Some(Event::MouseWheel {
                            delta: (y / 20.).ceil(),
                        })
                    }
                    MouseScrollDelta::LineDelta(_, y) => {
                        Some(Event::MouseWheel { delta: y.into() })
                    }
                }
            }
            WindowEvent::KeyboardInput { input, .. } => {
                input.virtual_keycode.map(|keycode| Event::KeyboardInput {
                    keycode,
                    state: input.state,
                })
            }
            WindowEvent::ModifiersChanged(state) => Some(Event::ModifiersChanged(state)),
            WindowEvent::Resized { .. } | WindowEvent::ScaleFactorChanged { .. } => {
                Some(Event::Resized { screen })
            }
            _ => None,
        }
    }
}

#[derive(Debug, Copy, Clone)]
pub struct Screen {
    phy_size: Size,
    scale_factor: f64,
}

impl Screen {
    /// Logical screen size
    pub fn size(&self) -> Size {
        Size {
            width: self.phy_size.width / self.scale_factor,
            height: self.phy_size.height / self.scale_factor,
        }
    }

    /// The screen scale factor. 2 = hidpi
    pub fn scale(&self) -> f64 {
        self.scale_factor
    }
}

pub struct RenderCtx<'a> {
    scene_builder: &'a mut SceneBuilder<'a>,
    screen: Screen,
}

impl<'a> RenderCtx<'a> {
    pub fn screen(&self) -> Screen {
        self.screen
    }
}

impl<'a> Deref for RenderCtx<'a> {
    type Target = SceneBuilder<'a>;
    fn deref(&self) -> &Self::Target {
        &self.scene_builder
    }
}

impl<'a> DerefMut for RenderCtx<'a> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.scene_builder
    }
}

pub trait AppLogic {
    fn render<'a>(&'a mut self, cx: &'a mut RenderCtx<'a>);
    fn event(&mut self, event: Event, cf: &mut ControlFlow) {
        if matches!(event, Event::CloseRequested) {
            *cf = ControlFlow::Exit;
        }
    }
}

pub struct App<T> {
    logic: T,
    screen: Option<Screen>,
}

impl<T: 'static + Default> App<T> {
    pub fn new() -> Self {
        Self::new_with_data(T::default())
    }
}

impl<T: 'static> App<T> {
    pub fn new_with_data(user_data: T) -> Self {
        Self {
            logic: user_data,
            screen: None,
        }
    }
}

impl<T: AppLogic + 'static> App<T> {
    pub fn run(mut self) {
        let event_loop = EventLoop::new();
        let mut render_cx = RenderContext::new().unwrap();

        let mut renderers: Vec<Option<Renderer>> = vec![];

        let mut cached_window = None;
        let mut scene = Scene::new();
        let mut fragment = SceneFragment::new();

        let mut render_state: Option<RenderState> = None;

        event_loop.run(move |event, event_loop, control_flow| match event {
            WEvent::Resumed => {
                let Option::None = render_state else { return };
                let window = cached_window
                    .take()
                    .unwrap_or_else(|| create_window(event_loop));
                let size = window.inner_size();
                let surface_future = render_cx.create_surface(&window, size.width, size.height);
                // We need to block here, in case a Suspended event appeared
                let Ok(surface) = pollster::block_on(surface_future) else {
                    *control_flow = ControlFlow::ExitWithCode(1);
                    return;
                };
                render_state = {
                    let render_state = RenderState { window, surface };
                    renderers.resize_with(render_cx.devices.len(), || None);
                    let id = render_state.surface.dev_id;
                    renderers[id].get_or_insert_with(|| {
                        Renderer::new(
                            &render_cx.devices[id].device,
                            &RendererOptions {
                                surface_format: Some(render_state.surface.format),
                                timestamp_period: 1.,
                                use_cpu: false,
                            },
                        )
                        .expect("Couldn't create renderer")
                    });
                    Some(render_state)
                };
                *control_flow = ControlFlow::Poll;
            }
            WEvent::Suspended => {
                eprintln!("Suspending");
                // When we suspend, we need to remove the `wgpu` Surface
                if let Some(render_state) = render_state.take() {
                    cached_window = Some(render_state.window);
                }
                *control_flow = ControlFlow::Wait;
            }
            WEvent::MainEventsCleared => {
                if let Some(render_state) = &mut render_state {
                    render_state.window.request_redraw();
                }
            }
            WEvent::RedrawRequested(_) => {
                let Some(render_state) = &mut render_state else {
                    return;
                };
                let width = render_state.surface.config.width;
                let height = render_state.surface.config.height;
                let device_handle = &render_cx.devices[render_state.surface.dev_id];

                let mut builder = SceneBuilder::for_fragment(&mut fragment);

                // https://github.com/linebender/vello/issues/291
                // TODO remove after issue is resolved.
                {
                    let brush = vello::peniko::Brush::Solid(Color::BLACK);
                    builder.fill(
                        vello::peniko::Fill::NonZero,
                        Affine::IDENTITY,
                        &brush,
                        None,
                        &vello::kurbo::Rect::new(0., 0., 10., 10.),
                    );
                }
                let s = self.screen.unwrap_or(Screen {
                    phy_size: Size::new(width as f64, height as f64),
                    scale_factor: 1.,
                });
                let mut ctx = RenderCtx {
                    scene_builder: &mut builder,
                    screen: s,
                };
                self.logic.render(&mut ctx);

                // If the user specifies a base color in the CLI we use that. Otherwise we use any
                // color specified by the scene. The default is black.
                let render_params = vello::RenderParams {
                    base_color: Color::BLACK,
                    width,
                    height,
                };
                let mut builder = SceneBuilder::for_scene(&mut scene);
                // We apply scaling to the fragment to account for screen scale factor
                let scale = self.screen.map(|s| {
                    let s = s.scale_factor;
                    Affine::scale(s)
                });
                builder.append(&fragment, scale);
                let surface_texture = render_state
                    .surface
                    .surface
                    .get_current_texture()
                    .expect("failed to get surface texture");
                vello::block_on_wgpu(
                    &device_handle.device,
                    renderers[render_state.surface.dev_id]
                        .as_mut()
                        .unwrap()
                        .render_to_surface_async(
                            &device_handle.device,
                            &device_handle.queue,
                            &scene,
                            &surface_texture,
                            &render_params,
                        ),
                )
                .expect("failed to render to surface");
                surface_texture.present();
                device_handle.device.poll(wgpu::Maintain::Poll);
            }
            WEvent::WindowEvent { event, window_id } => {
                let Some(render_state) = &mut render_state else {
                    return;
                };
                if render_state.window.id() != window_id {
                    return;
                }

                match &event {
                    WindowEvent::Resized(size) => {
                        let phy_size = Size::new(size.width as f64, size.height as f64);
                        if let Some(s) = self.screen.as_mut() {
                            // We short-circuit here to avoid emitting a resize event when the size
                            // hasn't changed
                            if s.phy_size == phy_size {
                                return;
                            }
                            s.phy_size = phy_size
                        } else {
                            self.screen = Some(Screen {
                                phy_size,
                                scale_factor: 1.,
                            })
                        }
                        render_cx.resize_surface(
                            &mut render_state.surface,
                            size.width,
                            size.height,
                        );
                        render_state.window.request_redraw();
                    }
                    WindowEvent::ScaleFactorChanged {
                        scale_factor,
                        new_inner_size,
                    } => {
                        self.screen = Some(Screen {
                            phy_size: Size::new(
                                new_inner_size.width as f64,
                                new_inner_size.height as f64,
                            ),
                            scale_factor: *scale_factor,
                        });

                        render_cx.resize_surface(
                            &mut render_state.surface,
                            new_inner_size.width,
                            new_inner_size.height,
                        );
                    }
                    _ => (),
                }

                if let Some(screen) = self.screen {
                    if let Some(evt) = Event::from_winit_window(event, screen) {
                        self.logic.event(evt, control_flow);
                    }
                }
            }
            _ => (),
        });
    }
}

// Copied from with_init example (as is a lot of other stuff in this code)
struct RenderState {
    // SAFETY: We MUST drop the surface before the `window`, so the fields
    // must be in this order
    surface: RenderSurface,
    window: Window,
}

fn create_window(event_loop: &winit::event_loop::EventLoopWindowTarget<()>) -> Window {
    use winit::{dpi::LogicalSize, window::WindowBuilder};
    WindowBuilder::new()
        .with_inner_size(LogicalSize::new(1044, 800))
        .with_resizable(true)
        .with_title("Snog")
        .build(&event_loop)
        .unwrap()
}
