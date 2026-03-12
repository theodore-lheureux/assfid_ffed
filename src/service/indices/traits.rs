use anyhow::Result;

pub struct VegetationIndices {
    pub ndvi: Vec<f32>,
    pub ndre: Vec<f32>,
    pub red_edge: Vec<f32>,
    pub nir: Vec<f32>,
    pub width: usize,
    pub height: usize,
}

pub trait IndexCalculator: Send + Sync {
    fn calculate(&self, tiff_data: &[u8]) -> Result<VegetationIndices>;
    fn serialize(&self, indices: &VegetationIndices) -> Result<Vec<u8>>;
}
