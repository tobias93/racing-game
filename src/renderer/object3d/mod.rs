use anyhow::Result;
use std::sync::Arc;
use vulkano::buffer::CpuAccessibleBuffer;
use vulkano::device::Device;

pub mod vertex;

pub fn triangle(device: &Arc<Device>) -> Result<Arc<CpuAccessibleBuffer<[vertex::Vertex]>>> {
    use crate::renderer::object3d::vertex::Vertex;
    use vulkano::buffer::{BufferUsage, CpuAccessibleBuffer};

    let vertex_buffer = CpuAccessibleBuffer::from_iter(
        Arc::clone(device),
        BufferUsage::all(),
        false,
        [
            Vertex {
                position: [-0.5, -0.25],
            },
            Vertex {
                position: [0.0, 0.5],
            },
            Vertex {
                position: [0.25, -0.1],
            },
        ]
        .iter()
        .cloned(),
    )?;
    Ok(vertex_buffer)
}
