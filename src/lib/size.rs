use winit::dpi;

#[repr(C)]
#[derive(Clone, Copy)]
#[derive(bytemuck::Pod, bytemuck::Zeroable)]
pub struct Size {
    pub width: u32,
    pub height: u32,
}

#[allow(clippy::from_over_into)]
impl Into<wgpu::Extent3d> for Size {
    fn into(self) -> wgpu::Extent3d {
        let Self {
            width, 
            height, ..
        } = self;

        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        }
    }
}

impl From<dpi::PhysicalSize<u32>> for Size {
    fn from(value: dpi::PhysicalSize<u32>) -> Self {
        let dpi::PhysicalSize {
            width,
            height,
        } = value;
        
        Self { width, height }
    }
}