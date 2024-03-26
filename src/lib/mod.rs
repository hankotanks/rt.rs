#![feature(type_alias_impl_trait)]

mod state;

mod size;
pub(crate) use size::Size;

mod pipelines;
pub(crate) use pipelines::{Pipeline, PipelineBuilder};

mod vertex;
pub(crate) use vertex::Vertex;
pub(crate) use vertex::{INDICES, CLIP_SPACE_EXTREMA};

#[allow(unused_imports)]
use std::{sync, panic};

#[cfg(target_arch="wasm32")]
use wasm_bindgen::prelude::*;

use winit::{event, keyboard};
use winit::event_loop::EventLoop;
use winit::window::WindowBuilder;

// Handle platform-dependent failure states
cfg_if::cfg_if! {
    if #[cfg(target_arch="wasm32")] {
        type Failed = JsValue;

        #[allow(non_snake_case)]
        #[track_caller]
        pub(crate) fn FAILURE<E>(err: E) -> Result<(), JsValue>
            where E: Into<anyhow::Error> {
            let at = panic::Location::caller();
            let err = Into::<anyhow::Error>::into(err);

            log::error!("[{at}] {err}");

            Err(JsValue::UNDEFINED)
        }
    } else {
        type Failed = anyhow::Error;

        #[allow(non_snake_case)]
        #[track_caller]
        pub(crate) fn FAILURE<E>(err: E) -> Result<(), Failed> 
            where E: Into<Failed> { 

            Err(Into::<Failed>::into(err)) 
        }
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

#[cfg(target_arch="wasm32")]
mod web {
    use std::{fmt, error};

    use winit::dpi::PhysicalSize;
    use winit::window::Window;
    use winit::platform::web::WindowExtWebSys;

    use wasm_bindgen::JsCast;

    #[derive(Debug)]
    pub(super) struct WebError { op: &'static str, }
    
    pub(super) const WEB_ERROR_APPEND: WebError = WebError { op: "append", };
    pub(super) const WEB_ERROR_SELECT: WebError = WebError { op: "select", };

    pub(super) const WEB_ERROR_PROP: WebError = WebError { 
        op: "access properties of document or the",
    };

    pub(super) const WEB_ERROR_CANVAS: WebError = WebError { op: "create", };
    pub(super) const WEB_ERROR_WINDOW: WebError = WebError { 
        op: "access window. Can't modify the",
    };
    
    impl fmt::Display for WebError {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "Unable to {} HTML canvas element", self.op)
        }
    }
    
    impl error::Error for WebError {
        fn source(&self) -> Option<&(dyn error::Error + 'static)> {
            None
        }
    
        fn cause(&self) -> Option<&dyn error::Error> {
            self.source()
        }
    }
    
    pub(super) fn web_dim() -> anyhow::Result<(web_sys::Window, PhysicalSize<u32>)> {
        let dom = web_sys::window()
            .ok_or(WEB_ERROR_WINDOW)?;
    
        let size = PhysicalSize {
            width: dom.inner_width()
                .map_err(|_| WEB_ERROR_PROP)?
                .as_f64()
                .ok_or(WEB_ERROR_PROP)? as u32,
            height: dom.inner_height()
                .map_err(|_| WEB_ERROR_PROP)?
                .as_f64()
                .ok_or(WEB_ERROR_PROP)? as u32,
        };
    
        Ok((dom, size))
    }
    
    pub(super) fn web_resize(dom: web_sys::Window, size: PhysicalSize<u32>) -> anyhow::Result<()> {
        let doc = dom.document()
            .ok_or(WEB_ERROR_PROP)?;
    
        let canvas = doc.get_element_by_id("rtrs")
            .ok_or(WEB_ERROR_SELECT)?;
    
        let canvas = canvas.dyn_into::<web_sys::HtmlCanvasElement>()
            .ok()
            .ok_or(WEB_ERROR_SELECT)?;
    
        canvas.set_width(size.width);
        canvas.set_height(size.height);
    
        Ok(())
    }

    pub(super) fn web_canvas(window: &Window) -> anyhow::Result<()> {
        let (dom, size) = web_dim()?;
    
        let _ = window.request_inner_size(size);
    
        let doc = dom.document()
            .ok_or(WEB_ERROR_PROP)?;
    
        let elem = window.canvas().ok_or(WEB_ERROR_CANVAS)?;
        let elem = web_sys::Element::from(elem);

        elem.remove_attribute("style")
            .ok()
            .ok_or(WEB_ERROR_PROP)?;
    
        let elem = elem.dyn_into::<web_sys::HtmlCanvasElement>()
            .ok()
            .ok_or(WEB_ERROR_SELECT)?;
    
        elem.set_width(size.width);
        elem.set_height(size.height);
        elem.set_id("rtrs");
    
        doc.get_elements_by_tag_name("body")
            .item(0)
            .ok_or(WEB_ERROR_SELECT)?
            .append_child(&elem.into())
            .ok()
            .ok_or(WEB_ERROR_APPEND)?;
    
        Ok(())
    }
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

    let event_loop = match EventLoop::new() {
        Ok(event_loop) => event_loop,
        Err(e) => {
            return FAILURE(e);
        },
    };

    #[allow(unused_mut)]  
    let window = WindowBuilder::new()
        .build(&event_loop)
        .unwrap();

    let window = sync::Arc::new(window);

    #[cfg(target_arch = "wasm32")] {
        if let Err(err) = web::web_canvas(&window) {
            return FAILURE(err);
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

    let mut state = match state::State::new(window.clone()).await {
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
    
    let result = event_loop.run(move |event, target| {
        // If `status` != None by the end of the loop, program terminates
        let mut status: Option<anyhow::Error> = None;

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
            match web::web_dim() {
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
                Err(e) => {
                    status = Some(Into::<anyhow::Error>::into(e));
                }
            }
        }

        // Handle this frame's event
        match event {
            event::Event::WindowEvent { event, window_id, .. }
                if window_id == window.clone().id() => match event {
                
                event::WindowEvent::CloseRequested | //
                event::WindowEvent::KeyboardInput {
                    event: event::KeyEvent {
                        state: event::ElementState::Pressed,
                        logical_key: keyboard::Key::Named(keyboard::NamedKey::Escape), ..
                    }, ..
                } => target.exit(),
                event::WindowEvent::Resized(physical_size) //
                    if size != Some(physical_size) => {

                    size = Some(physical_size);
                    size_instant = chrono::Local::now();
                },
                event::WindowEvent::RedrawRequested => {
                    match state.render() {
                        Ok(_) => { /*  */ },
                        Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                            let size = state.size();
    
                            state.resize(size);
                        },
                        Err(err) => //
                            // NOTE: Not strictly necessary to bail on Timeout...
                            status = Some(Into::<anyhow::Error>::into(err)),
                    }
                }
                _ => { /*  */ },
                // NOTE: event::WindowEvent::ScaleFactorChanged no longer
                // triggers a resize event
            },
            _ => { /*  */ },
        }

        /* Handle resize and update logic */ {
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
                                if let Err(e) = web::web_resize(dom, size) {
                                    status = Some(Into::<anyhow::Error>::into(e));
                                }; size_init = size;
                            },
                            None => {
                                status = Some(Into::<anyhow::Error>::into(web::WEB_ERROR_PROP));
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
            window.clone().request_redraw();
        }

        if let Some(err) = status.take() {
            // [src/lib/mod.rs:264:20]
            let loc = format!("{}:{}:{}", 
                std::file!(), std::line!(), std::column!());

            // We put the location in front to be 
            // consistent with `FAILURE` behavior
            log::error!("[{loc}] {err}");

            target.exit();
        }
    });

    match result {
        Ok(_) => Ok(()),
        Err(e) => FAILURE(e),
    }
}

