pub mod vert {
    vulkano_shaders::shader! {
        ty: "vertex",
        path: "shaders/vert.glsl",
    }
}

pub mod frag {
    vulkano_shaders::shader! {
        ty: "fragment",
        path: "shaders/frag.glsl",
    }
}
