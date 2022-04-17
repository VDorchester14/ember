use super::geometry::{
    Geometry,
    GeometryData,
    Vertex
};
use vulkano::buffer::{CpuAccessibleBuffer, BufferUsage};
use vulkano::device::Device;
use std::sync::Arc;


#[derive(Debug)]
pub struct CubeGeometry{
    pub data: GeometryData,
}

impl Geometry for CubeGeometry{
    fn create() -> Self{
        // dx here is just delta, not associated with x axis
        let dx = 0.5;

        // bottom plane
        let tl0 = Vertex::new(0.0 - dx, 0.0 + dx, 0.0 - dx);
        let tr0 = Vertex::new(0.0 + dx, 0.0 + dx, 0.0 - dx);
        let bl0 = Vertex::new(0.0 - dx, 0.0 - dx, 0.0 - dx);
        let br0 = Vertex::new(0.0 + dx, 0.0 - dx, 0.0 - dx);

        // top plane
        let tl1 = Vertex::new(0.0 - dx, 0.0 + dx, 0.0 + dx);
        let tr1 = Vertex::new(0.0 + dx, 0.0 + dx, 0.0 + dx);
        let bl1 = Vertex::new(0.0 - dx, 0.0 - dx, 0.0 + dx);
        let br1 = Vertex::new(0.0 + dx, 0.0 - dx, 0.0 + dx);

        // store verts.       0    1    2    3    4    5    6    7
        let vertices = vec![tl0, tr0, bl0, br0, tl1, tr1, bl1, br1];

        // top, front, right, back, left, bottom
        let indices = vec![
            4, 5, 7, 6, 4, 7, // top
            3, 2, 7, 2, 6, 7, // front
            7, 5, 1, 3, 7, 1, // right
            5, 4, 0, 1, 5, 0, // back
            4, 6, 2, 0, 4, 2, // left
            2, 3, 0, 1, 2, 0, // bottom
        ];

        CubeGeometry{
            data: GeometryData{
                vertices: vertices,
                indices: indices,
                vertex_buffer: None,
                index_buffer: None,
                initialized: false,
            }
        }
    }

    fn initialize(&mut self, device: Arc<Device>){
        // Vertex buffer init
        let vertex_buffer = {
            CpuAccessibleBuffer::from_iter(
                device.clone(),
                BufferUsage::all(),
                false,
                self.data.vertices.clone()
                .iter()
                .cloned(),
            )
            .unwrap()
        };

        // index buffer init
        let index_buffer = CpuAccessibleBuffer::from_iter(
            device.clone(),
            BufferUsage::all(),
            false,
            self.data.indices.clone()
            .iter()
            .cloned(),
        ).unwrap();

        self.data.vertex_buffer = Some(vertex_buffer);
        self.data.index_buffer = Some(index_buffer);
        self.data.initialized = true;
    }

    fn vertex_buffer(&self) -> Arc<CpuAccessibleBuffer<[Vertex]>> {
        self.data.vertex_buffer.clone().unwrap().clone()
    }

    fn index_buffer(&self) -> Arc<CpuAccessibleBuffer<[u16]>> {
        self.data.index_buffer.clone().unwrap().clone()
    }

    fn is_initialized(&self) -> bool {
        self.data.initialized
    }
}
