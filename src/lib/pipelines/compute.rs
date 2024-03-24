use crate::{Pipeline, CONFIG};

use super::PipelineBuilder;

#[allow(clippy::from_over_into)]
impl<'a> Into<Pipeline<wgpu::ComputePipeline>> for PipelineBuilder<'a> {
    fn into(self) -> Pipeline<wgpu::ComputePipeline> {
        let compute_texture_group_layout = self.device.create_bind_group_layout(
            &wgpu::BindGroupLayoutDescriptor {
                label: None,
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::StorageTexture {
                            access: wgpu::StorageTextureAccess::WriteOnly,
                            format: CONFIG.format,
                            view_dimension: wgpu::TextureViewDimension::D2,
                        },
                        count: None,
                    }
                ],
            }
        );
    
        let group = self.device.create_bind_group(
            &wgpu::BindGroupDescriptor {
                label: None,
                layout: &compute_texture_group_layout,
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
                    &compute_texture_group_layout,
                ]
            } 
        );
    
        let inner = self.device.create_compute_pipeline(
            &wgpu::ComputePipelineDescriptor {
                label: None,
                layout: Some(&inner_layout),
                module: self.module,
                entry_point: "main_cs",
            }
        );

        super::Pipeline { inner, group, }
    }
}