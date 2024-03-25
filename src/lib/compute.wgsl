struct Size { 
    width: u32, 
    height: u32 
}

@group(0) @binding(0)
var<uniform> size: Size;

@group(1) @binding(0)
var out: texture_storage_2d<rgba8unorm, write>;

@compute @workgroup_size(^@, ?@, 1)
fn main_cs(@builtin(global_invocation_id) id: vec3<u32>) {
    if(id.x < size.width && id.y < size.height) {
        let coord = vec2<i32>(i32(id.x), i32(id.y));

        if(id.x * 2u < size.width && id.y * 2u < size.height) {
            if(id.x % 2u == 0u) {
                let color = vec4<f32>(1.0, 0.0, 0.0, 0.0);
                textureStore(out, coord, color);
            } else {
                let color = vec4<f32>(0.0, 0.0, 0.0, 0.0);
                textureStore(out, coord, color);
            }
        } else if(id.x * 2u > size.width && id.y * 2u < size.height) {
            let color = vec4<f32>(0.0, 0.0, 0.0, 0.0);
            textureStore(out, coord, color);
        } else if(id.x * 2u < size.width && id.y * 2u > size.height) {
            let color = vec4<f32>(0.0, 1.0, 0.0, 0.0);
            textureStore(out, coord, color);
        } else {
            if(id.y % 2u == 0u) {
                let color = vec4<f32>(0.0, 0.0, 1.0, 0.0);
                textureStore(out, coord, color);
            } else {
                let color = vec4<f32>(0.0, 0.0, 0.0, 0.0);
                textureStore(out, coord, color);
            }
        }
    }
}