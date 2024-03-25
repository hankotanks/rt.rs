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

use winit::{dpi, event};
use winit::event_loop::{ControlFlow, EventLoop};
use winit::window::WindowBuilder;

// Handle platform-dependent failure states
cfg_if::cfg_if! {
    if #[cfg(target_arch="wasm32")] {
        type Failed = JsValue;

        // `wasm-bindgen` requires this signature
        const FAILURE: Result<(), JsValue> = Err(JsValue::NULL);
    } else {
        type Failed = ();

        const FAILURE: Result<(), ()> = Err(());
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
            fps: 60,
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
pub(crate) enum Error {
    LoggerInitFailure,
    CanvasAppendFailure,
    TimeOut,
    TextureFormatUnavailable,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", match self {
            Error::LoggerInitFailure => //
                "Couldn't initialize logger (wasm32)",
            Error::CanvasAppendFailure => //
                "Failed to append canvas element to DOM",
            Error::TimeOut => "Surface redraw timed out",
            Error::TextureFormatUnavailable => Box::leak({
                format!("Requisite texture formats [{:?}, {:?}] could not be loaded", 
                    CONFIG.format, 
                    CONFIG.format.add_srgb_suffix()
                ).into_boxed_str()
            }),
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

#[cfg_attr(target_arch="wasm32", wasm_bindgen(start))]
pub async fn run() -> Result<(), Failed> {
    cfg_if::cfg_if! {
        if #[cfg(target_arch = "wasm32")] {
            std::panic::set_hook(Box::new(console_error_panic_hook::hook));

            if console_log::init_with_level(log::Level::Warn).is_err() {
                eprintln!("{}", Error::LoggerInitFailure);

                return FAILURE;
            }
        } else {
            env_logger::init();
        }
    }

    let event_loop = EventLoop::new();

    let mut window = WindowBuilder::new();
    if let Err(wg) = CONFIG.resolution {
        window = window.with_min_inner_size(dpi::Size::Physical({
            dpi::PhysicalSize::new(wg, wg)
        }));
    }
        
    let window = window.build(&event_loop).unwrap();

    #[cfg(target_arch = "wasm32")] {
        use winit::dpi::PhysicalSize;
        use winit::platform::web::WindowExtWebSys;

        window.set_inner_size(PhysicalSize::new(450, 400));

        let result = web_sys::window()
            .and_then(|win| win.document())
            .and_then(|doc| {
                let canvas = web_sys::Element::from(window.canvas());

                doc.get_element_by_id("rtrs")?
                    .append_child(&canvas).ok()?;

                Some(())
            }).ok_or(Error::CanvasAppendFailure);

        if let Err(error) = result {
            eprintln!("{}", error);

            return FAILURE;
        }
    }

    let mut size = None;
    let mut size_instant = chrono::Local::now();

    let mut state = match state::State::new(window).await {
        Ok(state) => state,
        Err(error) => {
            eprintln!("{}", error);

            return FAILURE;
        },
    };

    let fps = (CONFIG.fps as f32).recip().floor() as i64 * 1000;
    
    let mut time_accum = 0;
    let mut time_curr = chrono::Local::now();

    event_loop.run(move |event, _, control_flow| { 
        let time_frame_start = chrono::Local::now();

        time_accum += time_curr
            .signed_duration_since(time_frame_start)
            .num_milliseconds();

        time_curr = time_frame_start;

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

                        state.resize(size)
                    },
                    Err(wgpu::SurfaceError::OutOfMemory) => //
                        *control_flow = ControlFlow::Exit,
                    Err(wgpu::SurfaceError::Timeout) => {
                        log::warn!("{}", Error::TimeOut);
                    },
                }
            },
            event::Event::RedrawEventsCleared => {
                state.window().request_redraw();
            },
            event::Event::MainEventsCleared => {
                let time_temp = size_instant
                    .signed_duration_since(time_frame_start)
                    .num_milliseconds();

                if time_temp > fps {
                    if let Some(size) = size.take() {
                        state.resize(size);
                    }                    
                }

                if time_accum >= fps {
                    time_accum -= fps;

                    state.update();
                }

                state.window().request_redraw();
            },
            _ => { /*  */ },
        }
    });
}

