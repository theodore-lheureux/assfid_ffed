use anyhow::Result;

pub trait ImageCapture: Send + Sync {
    fn capture(&self) -> Result<Vec<u8>>;
}

impl ImageCapture for Box<dyn ImageCapture> {
    fn capture(&self) -> Result<Vec<u8>> {
        (**self).capture()
    }
}
