mod render;
pub use render::*;

mod compute;
pub use compute::*;

pub struct Pipeline<P> {
    pub inner: P,
    pub group: wgpu::BindGroup,
}

#[derive(Clone, Copy)]
pub struct PipelineBuilder<'a> {
    pub device: &'a wgpu::Device,
    pub view: &'a wgpu::TextureView,
    pub module: &'a wgpu::ShaderModule,
    pub size_group_layout: &'a wgpu::BindGroupLayout,
}