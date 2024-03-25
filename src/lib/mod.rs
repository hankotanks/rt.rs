#![feature(type_alias_impl_trait)]

mod state;

mod size;
pub(crate) use size::Size;

mod pipelines;
pub(crate) use pipelines::{Pipeline, PipelineBuilder};

mod vertex;
pub(crate) use vertex::Vertex;
pub(crate) use vertex::{INDICES, CLIP_SPACE_EXTREMA};

use std::{error, fmt};

#[cfg(target_arch="wasm32")]
use wasm_bindgen::prelude::*;

use winit::event;
use winit::event_loop::{ControlFlow, EventLoop};
use winit::window::WindowBuilder;

// Handle platform-dependent failure states
cfg_if::cfg_if! {
    if #[cfg(target_arch="wasm32")] {
        type Failed = JsValue;

        #[allow(non_snake_case)]
        pub(crate) fn FAILURE(err: Error) -> Result<(), JsValue> {
            log::error!("{}", err);

            Err(JsValue::symbol(Some(&format!("{:?}", err))))
        }
    } else {
        type Failed = Error;

        #[allow(non_snake_case)]
        pub(crate) fn FAILURE(e: Error) -> Result<(), Failed> { Err(e) }
    }
}

//
// CONFIG declaration

#[allow(non_snake_case)]
pub struct Config {
    pub format: wgpu::TextureFormat,
    // If `Ok`, size is result, 
    // otherwise workgroup 'tile' size is specified in the `Err` value
    pub resolution: Result<Size, u32>,
    pub fps: u32,
}

impl Config {
    pub fn wg_dim(&self) -> u32 {
        let dim = match self.resolution {
            Ok(size) => {
                let Size {
                    mut width,
                    mut height,
                } = size;

                while height != 0 {
                    let temp = width;

                    width = height;
                    height = temp % height;
                }
                
                width
            },
            Err(wg) => wg,
        };

        if dim * dim > 256 { 16 } else { dim }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self { 
            format: wgpu::TextureFormat::Rgba8Unorm,
            resolution: Err(16), // Ok(Size { width: 640, height: 480, }),
            fps: 15,
        }
    }
}

pub(crate) static CONFIG: once_cell::sync::Lazy<Config> = //
    // TODO: This could read from a config file (if present), otherwise
    once_cell::sync::Lazy::new(|| { Config::default() });

//
// Error definition

#[allow(dead_code)]
#[derive(Debug)]
pub enum Error {
    LoggerInitFailure,
    CanvasAppendFailure,
    CanvasResizeFailure,
    TimeOut,
    TextureFormatUnavailable,
    OutOfMemory,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", match self {
            Error::LoggerInitFailure => //
                "Couldn't initialize logger (wasm32)",
            Error::CanvasAppendFailure => //
                "Failed to append canvas element to DOM",
            Error::CanvasResizeFailure => //
                "Failed to resize canvas element",
            Error::TimeOut => "Surface redraw timed out",
            Error::TextureFormatUnavailable => Box::leak({
                format!("Requisite texture formats [{:?}, {:?}] could not be loaded", 
                    CONFIG.format, 
                    CONFIG.format.add_srgb_suffix()
                ).into_boxed_str()
            }),
            Error::OutOfMemory => "Ran out of memory",
        })
    }
}

impl error::Error for Error {
    fn source(&self) -> Option<&(dyn error::Error + 'static)> {
        None
    }

    fn cause(&self) -> Option<&dyn error::Error> {
        self.source()
    }
}

#[cfg(target_arch = "wasm32")]
use winit::dpi::PhysicalSize;

#[cfg(target_arch = "wasm32")]
use web_sys::HtmlCanvasElement;

#[cfg(target_arch = "wasm32")]
fn web_dim() -> anyhow::Result<(web_sys::Window, PhysicalSize<u32>)> {
    let dom = web_sys::window()
        .ok_or(Error::CanvasAppendFailure)?;

    let size = PhysicalSize {
        width: dom.inner_width()
            .ok()
            .ok_or(Error::CanvasAppendFailure)?
            .as_f64()
            .ok_or(Error::CanvasAppendFailure)? as u32,
        height: dom.inner_height()
            .ok()
            .ok_or(Error::CanvasAppendFailure)?
            .as_f64()
            .ok_or(Error::CanvasAppendFailure)? as u32,
    };

    Ok((dom, size))
}

#[cfg(target_arch = "wasm32")]
fn web_resize(dom: web_sys::Window, size: PhysicalSize<u32>) -> anyhow::Result<()> {
    let doc = dom.document()
        .ok_or(Error::CanvasAppendFailure)?;

    let canvas = doc.get_element_by_id("rtrs")
        .ok_or(Error::CanvasAppendFailure)?;

    let canvas = canvas.dyn_into::<HtmlCanvasElement>()
        .ok()
        .ok_or(Error::CanvasResizeFailure)?;

    canvas.set_width(size.width);
    canvas.set_height(size.height);

    Ok(())
}

#[cfg(target_arch = "wasm32")]
use winit::window::Window;

#[cfg(target_arch = "wasm32")]
use winit::platform::web::WindowExtWebSys;

#[cfg(target_arch = "wasm32")]
fn web_canvas(window: &mut Window) -> anyhow::Result<()> {
    let (dom, size) = web_dim()?;

    window.set_inner_size(size);

    let doc = dom.document()
        .ok_or(Error::CanvasAppendFailure)?;

    let elem = web_sys::Element::from(window.canvas());
    elem.remove_attribute("style")
        .ok()
        .ok_or(Error::CanvasAppendFailure)?;

    let elem = elem.dyn_into::<HtmlCanvasElement>()
        .ok()
        .ok_or(Error::CanvasAppendFailure)?;

    elem.set_width(size.width);
    elem.set_height(size.height);
    elem.set_id("rtrs");

    doc.get_elements_by_tag_name("body")
        .item(0)
        .ok_or(Error::CanvasAppendFailure)?
        .append_child(&elem.into())
        .ok()
        .ok_or(Error::CanvasAppendFailure)?;

    Ok(())
}

#[cfg_attr(target_arch="wasm32", wasm_bindgen(start))]
pub async fn run() -> Result<(), Failed> {
    cfg_if::cfg_if! {
        if #[cfg(target_arch = "wasm32")] {
            std::panic::set_hook(Box::new(console_error_panic_hook::hook));

            // NOTE: `console_log` crate stopped working, so I subbed it out
            wasm_logger::init(wasm_logger::Config::default())
        } else {
            env_logger::init();
        }
    }

    let event_loop = EventLoop::new();

    #[allow(unused_mut)]  
    let mut window = WindowBuilder::new()
        .build(&event_loop)
        .unwrap();

    #[cfg(target_arch = "wasm32")] {
        if let Err(_) = web_canvas(&mut window) {
            return FAILURE(Error::CanvasAppendFailure);
        }
    }

    // Keeps track of resize actions. 
    // `size_instant` keeps track of the last resize event, 
    // after the user has stopped resizing, 
    // the new size can be processed
    let mut size = None;
    let mut size_instant = chrono::Local::now();
    
    // Since we are tracking size all the time when on the web,
    // we need an extra stopgap to prevent more updates
    #[cfg(target_arch = "wasm32")]
    let mut size_init = window.inner_size();

    let mut state = match state::State::new(window).await {
        Ok(state) => state,
        Err(err) => {
            return FAILURE(err);
        },
    };

    // The number of milliseconds per frame
    let fps = (CONFIG.fps as f64).recip() * 1_000.;
    
    // Respectively: time since last update; current time
    let mut time_accum = 0.;
    let mut time_curr = chrono::Local::now();
    
    event_loop.run(move |event, _, control_flow| {
        // If `status` != None by the end of the loop, program terminates
        let mut status: Option<Error> = None;

        // Take a snapshot of the current Instant
        let time_frame_start = chrono::Local::now();

        // Accumulate time since last frame update
        time_accum += time_curr
            .signed_duration_since(time_frame_start)
            .num_milliseconds()
            .abs() as f64;

        // Update current Instant
        time_curr = time_frame_start;

        // Must listen for resizes manually on the web
        // The event handlers (unfortunately) don't work
        #[cfg(target_arch = "wasm32")] {
            match web_dim() {
                Ok((_, size_curr)) => {
                    let size_update_required = match size {
                        Some(size_temp) if size_temp != size_curr => true,
                        None if size_init != size_curr => true,
                        _ => false,
                    };

                    if size_update_required {
                        size = Some(size_curr);
                        size_instant = chrono::Local::now();
                    }
                },
                Err(_) => {
                    status = Some(Error::CanvasResizeFailure);
                }
            }
        }

        // Handle this frame's event
        match event {
            event::Event::WindowEvent { event, window_id, .. }
                if window_id == state.window().id() => match event {

                event::WindowEvent::CloseRequested | //
                event::WindowEvent::KeyboardInput {
                    input: event::KeyboardInput {
                        state: event::ElementState::Pressed,
                        virtual_keycode: Some(event::VirtualKeyCode::Escape), ..
                    }, ..
                } => *control_flow = ControlFlow::Exit,
                event::WindowEvent::Resized(physical_size) //
                    if size != Some(physical_size) => {

                    size = Some(physical_size);
                    size_instant = chrono::Local::now();
                },
                event::WindowEvent::ScaleFactorChanged { new_inner_size, .. } => //
                    state.resize(*new_inner_size),
                _ => { /*  */ },
            },
            event::Event::RedrawRequested(window_id) 
                if window_id == state.window().id() => {

                match state.render() {
                    Ok(_) => { /*  */ },
                    Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                        let size = state.size();

                        state.resize(size);
                    },
                    Err(wgpu::SurfaceError::OutOfMemory) => //
                        status = Some(Error::OutOfMemory),
                    Err(wgpu::SurfaceError::Timeout) => //
                        // NOTE: It isn't strictly necessary to bail here
                        status = Some(Error::TimeOut),
                }
            },
            event::Event::RedrawEventsCleared => {
                state.window().request_redraw();
            },
            event::Event::MainEventsCleared => {
                let time_temp = size_instant
                    .signed_duration_since(time_frame_start)
                    .num_milliseconds()
                    .abs() as f64;

                // Update flag
                let mut update_required = false;

                // Check if enough time has passed since the user resized
                if time_temp > fps {
                    // If so, resize the texture and update uniforms
                    if let Some(size) = size.take() {
                        #[cfg(target_arch = "wasm32")] {
                            match web_sys::window() {
                                Some(dom) => {
                                    if let Err(_) = web_resize(dom, size) {
                                        status = Some(Error::CanvasResizeFailure);
                                    }; size_init = size;
                                },
                                None => {
                                    status = Some(Error::CanvasResizeFailure);
                                }
                            }
                        }

                        state.resize(size);

                        // Set flag
                        update_required = true;
                    }                    
                }

                // Set the update flag if enough time has elapsed
                if time_accum >= fps {
                    time_accum -= fps;

                    // Set flag
                    update_required = true;
                }

                // Perform the update
                if update_required {
                    state.update();
                }

                // Update has been performed; now we can redraw safely
                state.window().request_redraw();
            },
            _ => { /*  */ },
        }

        if let Some(err) = status.take() {
            log::error!("{}", err);

            *control_flow = ControlFlow::Exit;
        }
    });
}

