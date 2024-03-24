use crate::{Pipeline, Vertex, CONFIG};

#[allow(clippy::from_over_into)]
impl<'a> Into<Pipeline<wgpu::RenderPipeline>> for super::PipelineBuilder<'a> {
    fn into(self) -> Pipeline<wgpu::RenderPipeline> {
        let tg_layout = self.device.create_bind_group_layout(
            &wgpu::BindGroupLayoutDescriptor {
                label: None,
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { 
                                filterable: false 
                            },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None
                    }
                ],
            }
        );
    
        let tg = self.device.create_bind_group(
            &wgpu::BindGroupDescriptor {
                label: None,
                layout: &tg_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(self.view),
                    }
                ],
            }
        );
    
        let inner_layout = self.device.create_pipeline_layout(
            &wgpu::PipelineLayoutDescriptor {
                label: None,
                push_constant_ranges: &[],
                bind_group_layouts: &[
                    self.size_group_layout,
                    &tg_layout,
                ],
            }
        );
    
        let inner = self.device.create_render_pipeline(
            &wgpu::RenderPipelineDescriptor {
                label: None,
                layout: Some(&inner_layout),
                vertex: wgpu::VertexState {
                    module: self.module,
                    entry_point: "vs_main",
                    buffers: &[Vertex::description()],
                },
                fragment: Some(wgpu::FragmentState {
                    module: self.module,
                    entry_point: "fs_main",
                    targets: &[Some(wgpu::ColorTargetState {
                        format: CONFIG.format.add_srgb_suffix(),
                        blend: Some(wgpu::BlendState {
                            color: wgpu::BlendComponent::REPLACE,
                            alpha: wgpu::BlendComponent::REPLACE,
                        }),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    strip_index_format: None,
                    front_face: wgpu::FrontFace::Ccw,
                    cull_mode: Some(wgpu::Face::Back),
                    polygon_mode: wgpu::PolygonMode::Fill,
                    unclipped_depth: false,
                    conservative: false,
                },
                depth_stencil: None,
                multisample: wgpu::MultisampleState {
                    count: 1,
                    mask: !0,
                    alpha_to_coverage_enabled: false,
                },
                multiview: None,
            }
        );
    
        super::Pipeline { inner, group: tg, }
    }
}