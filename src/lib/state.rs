use winit::event::WindowEvent;
use winit::window::Window;

use wgpu::util::DeviceExt as _;

use crate::PipelineBuilder;
use crate::CONFIG;
use crate::Error;
use crate::{Pipeline, Size};

pub struct State {
    // WGPU interface
    device: wgpu::Device,
    queue: wgpu::Queue,
    surface: wgpu::Surface,
    surface_config: wgpu::SurfaceConfiguration,

    // Bind groups & compute pass
    size: Size,
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
    window: Window,
}

impl State {
    pub(super) async fn new(window: Window) -> anyhow::Result<Self> {
        //
        // WGPU State Information
        //

        let window_size = window.inner_size();

        let instance_desc = wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            ..Default::default()
        };

        let instance = wgpu::Instance::new(instance_desc);

        let surface = unsafe { 
            instance.create_surface(&window).unwrap()
        };

        let adapter = instance.request_adapter(
            &wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::default(),
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            }
        ).await.unwrap();

        let bgra8unorm_storage_enabled = adapter
            .features()
            .contains(wgpu::Features::BGRA8UNORM_STORAGE);

        let device_desc = wgpu::DeviceDescriptor {
            label: None,
            features: if bgra8unorm_storage_enabled {
                wgpu::Features::BGRA8UNORM_STORAGE
            } else {
                wgpu::Features::empty()
            },
            limits: if cfg!(target_arch = "wasm32") {
                wgpu::Limits::downlevel_webgl2_defaults()
            } else {
                wgpu::Limits::default()
            },
        };

        let (device, queue) = adapter.request_device(&device_desc, None)
            .await
            .unwrap();

        //
        // Size Buffer & Bind Groups
        //

        let size = CONFIG.resolution
            .unwrap_or_else(|()| window_size.into());

        let size_buffer = device.create_buffer_init(
            &wgpu::util::BufferInitDescriptor {
                label: None,
                contents: bytemuck::cast_slice(&[size]),
                usage: wgpu::BufferUsages::UNIFORM,
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
        //

        let caps = surface.get_capabilities(&adapter);

        let format = CONFIG.format.add_srgb_suffix();
        if !caps.formats.contains(&format) {
            anyhow::bail!(Error::TextureFormatUnavailable);
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
            view_formats: Vec::with_capacity(0),
        };

        surface.configure(&device, &surface_config);

        //
        // Shader Configuration
        //

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
                    include_str!("compute.wgsl").into()
                }),
            },
        );

        //
        // Texture Init
        //

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
                    CONFIG.format.add_srgb_suffix(),
                ],
            }
        );

        let texture_view_render = texture.create_view(
            &wgpu::TextureViewDescriptor {
                label: None,
                format: Some(CONFIG.format.add_srgb_suffix()),
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
        //

        let builder = PipelineBuilder {
            device: &device,
            view: &texture_view_compute,
            module: &shader_compute,
            size_group_layout: &size_group_layout,
        };

        let Pipeline {
            inner: compute_pipeline,
            group: compute_group, ..
        } = builder.into();

        //
        // Render Pipeline
        //

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

        let builder = PipelineBuilder {
            device: &device,
            view: &texture_view_render,
            module: &shader_render,
            size_group_layout: &size_group_layout,
        };

        let Pipeline {
            inner: render_pipeline,
            group: render_group, ..
        } = builder.into();

        Ok(Self {
            device,
            queue,
            surface,
            surface_config,

            size,
            size_group,
            compute_group,
            compute_pipeline,

            vertices,
            indices,
            render_group,
            render_pipeline,

            window_size,
            window,
        })
    }

    pub(super) fn window(&self) -> &Window {
        &self.window
    }

    pub fn resize(&mut self, new_size: winit::dpi::PhysicalSize<u32>) {
        if new_size.width > 0 && new_size.height > 0 {
            self.window_size = new_size;
            self.surface_config.width = new_size.width;
            self.surface_config.height = new_size.height;
            self.surface.configure(&self.device, &self.surface_config);
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

            compute_pass.dispatch_workgroups(
                self.size.width / 16,
                self.size.height / 16,
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