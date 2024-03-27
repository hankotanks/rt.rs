use std::borrow;
use std::sync;

use winit::event::WindowEvent;
use winit::window::Window;

use wgpu::util::DeviceExt as _;

use crate::PipelineBuilder;
use crate::CONFIG;
use crate::{Pipeline, Size};

struct PipelinePackage {
    compute_group: wgpu::BindGroup,
    compute_pipeline: wgpu::ComputePipeline,
    render_group: wgpu::BindGroup,
    render_pipeline: wgpu::RenderPipeline,
}

impl PipelinePackage {
    fn new(
        device: &wgpu::Device,
        size: Size,
        shader_compute: &wgpu::ShaderModule,
        shader_render: &wgpu::ShaderModule,
        size_group_layout: &wgpu::BindGroupLayout,
    ) -> Self {
        //
        // Texture Init

        let texture = device.create_texture(
            &wgpu::TextureDescriptor {
                label: None,
                size: size.into(),
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: CONFIG.format,
                usage: wgpu::TextureUsages::STORAGE_BINDING 
                     | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[
                    CONFIG.format,
                    #[cfg(target_arch = "wasm32")]
                    CONFIG.format.add_srgb_suffix(),
                ],
            }
        );

        cfg_if::cfg_if! {
            if #[cfg(target_arch = "wasm32")] {
                let texture_view_render_format = CONFIG.format;
            } else {
                let texture_view_render_format = CONFIG.format.add_srgb_suffix();
            }
        }

        let texture_view_render = texture.create_view(
            &wgpu::TextureViewDescriptor {
                label: None,
                format: Some(texture_view_render_format),
                dimension: Some(wgpu::TextureViewDimension::D2),
                aspect: wgpu::TextureAspect::All,
                base_mip_level: 0,
                mip_level_count: Some(1),
                base_array_layer: 0,
                array_layer_count: Some(1),
            }
        );

        let texture_view_compute = texture.create_view(
            &wgpu::TextureViewDescriptor {
                label: None,
                format: Some(CONFIG.format),
                dimension: Some(wgpu::TextureViewDimension::D2),
                aspect: wgpu::TextureAspect::All,
                base_mip_level: 0,
                mip_level_count: Some(1),
                base_array_layer: 0,
                array_layer_count: Some(1),
            }
        );

        //
        // Compute Pipeline

        let builder = PipelineBuilder {
            device,
            view: &texture_view_compute,
            module: shader_compute,
            size_group_layout,
        };

        let Pipeline {
            inner: compute_pipeline,
            group: compute_group, ..
        } = builder.into();

        //
        // Render Pipeline

        let builder = PipelineBuilder {
            device,
            view: &texture_view_render,
            module: shader_render,
            size_group_layout,
        };

        let Pipeline {
            inner: render_pipeline,
            group: render_group, ..
        } = builder.into();

        Self {
            compute_group,
            compute_pipeline,
            render_group,
            render_pipeline,
        }
    }
}

pub struct State {
    // WGPU interface
    device: wgpu::Device,
    queue: wgpu::Queue,
    surface: wgpu::Surface<'static>,
    surface_config: wgpu::SurfaceConfiguration,

    shader_compute: wgpu::ShaderModule,
    shader_render: wgpu::ShaderModule,

    // Bind groups & compute pass
    size_buffer: wgpu::Buffer,
    size_group_layout: wgpu::BindGroupLayout,
    size_group: wgpu::BindGroup,
    compute_group: wgpu::BindGroup,
    compute_pipeline: wgpu::ComputePipeline,
    
    // Render pass
    vertices: wgpu::Buffer,
    indices: wgpu::Buffer,
    render_group: wgpu::BindGroup,
    render_pipeline: wgpu::RenderPipeline,

    // Window
    window_size: winit::dpi::PhysicalSize<u32>,
}

impl State {
    pub(super) async fn new(
        window: sync::Arc<Window>,
    ) -> anyhow::Result<Self> {
        //
        // WGPU State Information

        let window_size = window.inner_size();

        let instance_desc = wgpu::InstanceDescriptor {
            // NOTE: Specifying `wgpu::Backends::BROWSER_WEBGPU`
            // ensures that WGPU never chooses WebGL2
            backends: {
                #[cfg(target_arch = "wasm32")]
                let backends = wgpu::Backends::BROWSER_WEBGPU;

                #[cfg(not(target_arch = "wasm32"))]
                let backends = wgpu::Backends::all();

                backends
            },
            ..Default::default()
        };

        let instance = wgpu::Instance::new(instance_desc);

        #[cfg(target_arch = "wasm32")]
        unsafe fn target() -> anyhow::Result<wgpu::SurfaceTargetUnsafe> {
            use wgpu::rwh;

            Ok(wgpu::SurfaceTargetUnsafe::RawHandle { 
                raw_display_handle: rwh::RawDisplayHandle::Web({
                    rwh::WebDisplayHandle::new()
                }),
                raw_window_handle: rwh::RawWindowHandle::Web({
                    rwh::WebWindowHandle::new(CONFIG.canvas_raw_handle)
                }),
            })
        }

        cfg_if::cfg_if! {
            if #[cfg(target_arch="wasm32")] {
                let surface_target = unsafe { target()? };

                let surface = unsafe {
                    instance.create_surface_unsafe(surface_target)?
                };
            } else {
                let surface_target = Box::new(window.clone());
                let surface_target = wgpu::SurfaceTarget::Window(surface_target);

                let surface = instance.create_surface(surface_target)?;
            }
        }

        let adapter = instance.request_adapter(
            &wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::default(),
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            }
        ).await.unwrap();

        // TODO: BGRA8UNORM_STORAGE is not necessary,
        // since Rgba8Unorm and Rgba8UnormSrgb is used instead
        let bgra8unorm_storage_enabled = adapter
            .features()
            .contains(wgpu::Features::BGRA8UNORM_STORAGE);

        let device_desc = wgpu::DeviceDescriptor {
            label: None,
            required_features: if bgra8unorm_storage_enabled {
                wgpu::Features::BGRA8UNORM_STORAGE
            } else {
                wgpu::Features::empty()
            },
            required_limits: wgpu::Limits::default(),
        };

        let (device, queue) = adapter.request_device(&device_desc, None)
            .await
            .unwrap();

        //
        // Size Buffer & Bind Groups

        let size = CONFIG.resolution
            .unwrap_or_else(|_| window_size.into());

        let size_buffer = device.create_buffer_init(
            &wgpu::util::BufferInitDescriptor {
                label: None,
                contents: bytemuck::cast_slice(&[size]),
                usage: wgpu::BufferUsages::UNIFORM 
                     | wgpu::BufferUsages::COPY_DST,
            }
        );

        let size_group_layout = device.create_bind_group_layout(
            &wgpu::BindGroupLayoutDescriptor {
                label: None,
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::all(),
                    count: None,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    }
                }]
            }
        );

        let size_group = device.create_bind_group(
            &wgpu::BindGroupDescriptor {
                label: None,
                layout: &size_group_layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: size_buffer.as_entire_binding()
                }]
            }
        );

        //
        // Surface Configuration

        let caps = surface.get_capabilities(&adapter);
        
        cfg_if::cfg_if! {
            if #[cfg(target_arch = "wasm32")] {
                let format = CONFIG.format;
            } else {
                let format = CONFIG.format.add_srgb_suffix();
            }
        }
        if !caps.formats.contains(&format) {
            anyhow::bail!(wgpu::SurfaceError::Lost);
        }

        let wgpu::SurfaceCapabilities {
            present_modes,
            alpha_modes, ..
        } = caps;

        let surface_config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: window_size.width,
            height: window_size.height,
            present_mode: present_modes[0],
            alpha_mode: alpha_modes[0],
            view_formats: vec![
                CONFIG.format, 
                #[cfg(not(target_arch = "wasm32"))]
                CONFIG.format.add_srgb_suffix(),
            ],
            desired_maximum_frame_latency: 1,
        };

        surface.configure(&device, &surface_config);

        //
        // Shader Configuration

        let shader_render = device.create_shader_module(
            wgpu::ShaderModuleDescriptor {
                label: None,
                source: wgpu::ShaderSource::Wgsl({
                    include_str!("render.wgsl").into()
                }),
            },
        );

        let shader_compute = device.create_shader_module(
            wgpu::ShaderModuleDescriptor {
                label: None,
                source: wgpu::ShaderSource::Wgsl({
                    let shader: &'static str = include_str!("compute.wgsl");

                    let wg_dim = CONFIG.wg_dim();
                    let wg_dim = format!("{}", wg_dim);

                    let shader = shader.replace("^@", &wg_dim);
                    let shader = shader.replace("?@", &wg_dim);

                    borrow::Cow::Borrowed(Box::leak(shader.into_boxed_str()))
                }),
            },
        );

        //
        // Render Pipeline

        let vertices = device.create_buffer_init(
            &wgpu::util::BufferInitDescriptor {
                label: None,
                contents: bytemuck::cast_slice(crate::CLIP_SPACE_EXTREMA),
                usage: wgpu::BufferUsages::VERTEX
            }
        );

        let indices = device.create_buffer_init(
            &wgpu::util::BufferInitDescriptor {
                label: None,
                contents: bytemuck::cast_slice(crate::INDICES),
                usage: wgpu::BufferUsages::INDEX
            }
        );

        let PipelinePackage {
            compute_group,
            compute_pipeline,
            render_group,
            render_pipeline,
        } = PipelinePackage::new(
            &device, 
            size, 
            &shader_compute, 
            &shader_render, 
            &size_group_layout
        );

        Ok(Self {
            device,
            queue,
            surface,
            surface_config,

            shader_compute,
            shader_render,

            size_buffer,
            size_group_layout,
            size_group,
            compute_group,
            compute_pipeline,

            vertices,
            indices,
            render_group,
            render_pipeline,

            window_size,
        })
    }

    pub fn resize(&mut self, size: winit::dpi::PhysicalSize<u32>) {
        if size.width > 0 && size.height > 0 {
            self.window_size = size;

            self.surface_config.width = size.width;
            self.surface_config.height = size.height;

            self.surface.configure(&self.device, &self.surface_config);

            if CONFIG.resolution.is_err() {
                let size = Into::<Size>::into(size);

                self.queue.write_buffer(
                    &self.size_buffer, 
                    0,
                    bytemuck::cast_slice(&[size])
                );
    
                let Self {
                    device,
                    shader_compute,
                    shader_render,
                    size_group_layout, ..
                } = self;
    
                let PipelinePackage {
                    compute_group,
                    compute_pipeline,
                    render_group,
                    render_pipeline,
                } = PipelinePackage::new(
                    device, 
                    size, 
                    shader_compute, 
                    shader_render, 
                    size_group_layout
                );
    
                self.compute_group = compute_group;
                self.compute_pipeline = compute_pipeline;
    
                self.render_group = render_group;
                self.render_pipeline = render_pipeline;
            }
        }
    }

    pub(super) fn size(&self) -> winit::dpi::PhysicalSize<u32> {
        self.window_size
    }

    #[allow(unused_variables, dead_code)]
    pub(super) fn input(&mut self, event: &WindowEvent) -> bool {
        false
    }

    pub(super) fn update(&mut self) {
        let mut encoder = self.device.create_command_encoder(&{
            wgpu::CommandEncoderDescriptor::default()
        });

        {
            let mut compute_pass = encoder.begin_compute_pass(&{
                wgpu::ComputePassDescriptor::default()
            });

            compute_pass.set_pipeline(&self.compute_pipeline);

            compute_pass.set_bind_group(0, &self.size_group, &[]);
            compute_pass.set_bind_group(1, &self.compute_group, &[]);

            let dim = CONFIG.wg_dim();

            let Size {
                width,
                height, ..
            } = match CONFIG.resolution {
                Ok(size) => size,
                Err(_) => Size::from(self.window_size),
            };

            compute_pass.dispatch_workgroups(
                width.div_euclid(dim), 
                height.div_euclid(dim), 
                1
            );
        }

        self.queue.submit(Some(encoder.finish()));
    }

    pub(super) fn render(&mut self) -> Result<(), wgpu::SurfaceError> {
        let output = self.surface.get_current_texture()?;

        let view = output.texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = self.device.create_command_encoder({
            &wgpu::CommandEncoderDescriptor::default()
        });

        {
            let color_attachment = wgpu::RenderPassColorAttachment {
                view: &view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: 0.1,
                        g: 0.2,
                        b: 0.3,
                        a: 1.0,
                    }),
                    store: wgpu::StoreOp::Store,
                },
            };

            let mut render_pass = encoder.begin_render_pass(
                &wgpu::RenderPassDescriptor {
                    label: None,
                    color_attachments: &[Some(color_attachment)],
                    depth_stencil_attachment: None,
                    ..Default::default()
                }
            );

            render_pass.set_pipeline(&self.render_pipeline);

            render_pass.set_bind_group(0, &self.size_group, &[]);
            render_pass.set_bind_group(1, &self.render_group, &[]);

            render_pass.set_index_buffer(
                self.indices.slice(..), 
                wgpu::IndexFormat::Uint32
            );

            render_pass.set_vertex_buffer(0, self.vertices.slice(..));

            render_pass.draw_indexed(
                0..(crate::INDICES.len() as u32), 
                0, 
                0..1
            ); 
        }

        self.queue.submit(std::iter::once(encoder.finish()));

        output.present();

        Ok(())
    }
}