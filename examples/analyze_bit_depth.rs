use std::collections::HashMap;

fn main() -> anyhow::Result<()> {
    let mut decoder = tiff::decoder::Decoder::new(std::fs::File::open("output.tiff")?)?;
    let (width, height) = decoder.dimensions()?;
    
    println!("Image: {}x{} pixels", width, height);
    
    // Read the image data
    let mut decoder = tiff::decoder::Decoder::new(std::fs::File::open("output.tiff")?)?;
    let image = decoder.read_image()?;
    
    if let tiff::decoder::DecodingResult::U16(data) = image {
        let mut r_vals = HashMap::new();
        let mut g_vals = HashMap::new();
        let mut b_vals = HashMap::new();
        
        let mut r_min = u16::MAX;
        let mut r_max = u16::MIN;
        let mut g_min = u16::MAX;
        let mut g_max = u16::MIN;
        let mut b_min = u16::MAX;
        let mut b_max = u16::MIN;
        
        // Sample every pixel
        for chunk in data.chunks(3) {
            if chunk.len() == 3 {
                let r = chunk[0];
                let g = chunk[1];
                let b = chunk[2];
                
                *r_vals.entry(r).or_insert(0) += 1;
                *g_vals.entry(g).or_insert(0) += 1;
                *b_vals.entry(b).or_insert(0) += 1;
                
                r_min = r_min.min(r);
                r_max = r_max.max(r);
                g_min = g_min.min(g);
                g_max = g_max.max(g);
                b_min = b_min.min(b);
                b_max = b_max.max(b);
            }
        }
        
        println!("\nRed channel:");
        println!("  Range: {} - {} (span: {})", r_min, r_max, r_max - r_min);
        println!("  Unique values: {}", r_vals.len());
        println!("  Effective bits: {:.2}", (r_vals.len() as f64).log2());
        
        println!("\nGreen channel:");
        println!("  Range: {} - {} (span: {})", g_min, g_max, g_max - g_min);
        println!("  Unique values: {}", g_vals.len());
        println!("  Effective bits: {:.2}", (g_vals.len() as f64).log2());
        
        println!("\nBlue channel:");
        println!("  Range: {} - {} (span: {})", b_min, b_max, b_max - b_min);
        println!("  Unique values: {}", b_vals.len());
        println!("  Effective bits: {:.2}", (b_vals.len() as f64).log2());
        
        // Check how many values are at max (clipped)
        let r_clipped = r_vals.get(&65535).unwrap_or(&0);
        let g_clipped = g_vals.get(&65535).unwrap_or(&0);
        let b_clipped = b_vals.get(&65535).unwrap_or(&0);
        
        let total_pixels = width as u64 * height as u64;
        println!("\nClipping at maximum (65535):");
        println!("  Red: {} pixels ({:.2}%)", r_clipped, *r_clipped as f64 / total_pixels as f64 * 100.0);
        println!("  Green: {} pixels ({:.2}%)", g_clipped, *g_clipped as f64 / total_pixels as f64 * 100.0);
        println!("  Blue: {} pixels ({:.2}%)", b_clipped, *b_clipped as f64 / total_pixels as f64 * 100.0);
        
        // Estimate what would happen with 8-bit
        println!("\nIf converted to 8-bit:");
        println!("  Red: {} -> {} unique values (loss: {})", 
                 r_vals.len(), 
                 r_vals.keys().map(|&v| v / 256).collect::<std::collections::HashSet<_>>().len(),
                 r_vals.len() - r_vals.keys().map(|&v| v / 256).collect::<std::collections::HashSet<_>>().len());
        println!("  Green: {} -> {} unique values (loss: {})", 
                 g_vals.len(), 
                 g_vals.keys().map(|&v| v / 256).collect::<std::collections::HashSet<_>>().len(),
                 g_vals.len() - g_vals.keys().map(|&v| v / 256).collect::<std::collections::HashSet<_>>().len());
        println!("  Blue: {} -> {} unique values (loss: {})", 
                 b_vals.len(), 
                 b_vals.keys().map(|&v| v / 256).collect::<std::collections::HashSet<_>>().len(),
                 b_vals.len() - b_vals.keys().map(|&v| v / 256).collect::<std::collections::HashSet<_>>().len());
        
    } else {
        println!("Not a 16-bit image!");
    }
    
    Ok(())
}
