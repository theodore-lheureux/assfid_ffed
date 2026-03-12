use anyhow::Result;
use tracing::info;

use super::traits::{IndexCalculator, VegetationIndices};

pub struct StubIndexCalculator;

impl IndexCalculator for StubIndexCalculator {
    fn calculate(&self, tiff_data: &[u8]) -> Result<VegetationIndices> {
        info!(bytes = tiff_data.len(), "Stub: computing vegetation indices");
        Ok(VegetationIndices {
            ndvi: vec![0.0],
            ndre: vec![0.0],
            red_edge: vec![0.0],
            nir: vec![0.0],
            width: 1,
            height: 1,
        })
    }

    fn serialize(&self, indices: &VegetationIndices) -> Result<Vec<u8>> {
        info!(
            width = indices.width,
            height = indices.height,
            "Stub: serializing indices"
        );
        Ok(Vec::new())
    }
}
