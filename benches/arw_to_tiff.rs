use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId};
use ffed_protosat_rs::image_pipeline::{
    ArwToTiffPipeline, ConversionConfig, TiffCompression,
};
use std::io::Cursor;

fn generate_mock_raw_data(width: usize, height: usize) -> Vec<u8> {
    let mut data = Vec::new();
    for y in 0..height {
        for x in 0..width {
            let value = ((x + y) % 256) as u8;
            data.push(value);
            data.push(value);
        }
    }
    data
}

fn benchmark_conversion_sizes(c: &mut Criterion) {
    let mut group = c.benchmark_group("conversion_by_size");
    
    let sizes = vec![
        (100, 100, "100x100"),
        (500, 500, "500x500"),
        (1000, 1000, "1000x1000"),
    ];
    
    for (width, height, label) in sizes {
        let mock_data = generate_mock_raw_data(width, height);
        
        group.bench_with_input(
            BenchmarkId::from_parameter(label),
            &mock_data,
            |b, data| {
                let config = ConversionConfig::default();
                let pipeline = ArwToTiffPipeline::new(config);
                
                b.iter(|| {
                    let mut output = Cursor::new(Vec::new());
                    let _ = pipeline.convert(black_box(data), &mut output);
                });
            },
        );
    }
    
    group.finish();
}

fn benchmark_compression_methods(c: &mut Criterion) {
    let mut group = c.benchmark_group("compression_methods");
    let mock_data = generate_mock_raw_data(500, 500);
    
    let compressions = vec![
        (TiffCompression::None, "none"),
        (TiffCompression::Lzw, "lzw"),
        (TiffCompression::Deflate, "deflate"),
    ];
    
    for (compression, label) in compressions {
        group.bench_with_input(
            BenchmarkId::from_parameter(label),
            &mock_data,
            |b, data| {
                let config = ConversionConfig::builder()
                    .compression(compression)
                    .build();
                let pipeline = ArwToTiffPipeline::new(config);
                
                b.iter(|| {
                    let mut output = Cursor::new(Vec::new());
                    let _ = pipeline.convert(black_box(data), &mut output);
                });
            },
        );
    }
    
    group.finish();
}

fn benchmark_with_predictor(c: &mut Criterion) {
    let mut group = c.benchmark_group("predictor_impact");
    let mock_data = generate_mock_raw_data(500, 500);
    
    group.bench_function("lzw_no_predictor", |b| {
        let config = ConversionConfig::builder()
            .compression(TiffCompression::Lzw)
            .predictor(None)
            .build();
        let pipeline = ArwToTiffPipeline::new(config);
        
        b.iter(|| {
            let mut output = Cursor::new(Vec::new());
            let _ = pipeline.convert(black_box(&mock_data), &mut output);
        });
    });
    
    group.bench_function("lzw_with_predictor", |b| {
        let config = ConversionConfig::builder()
            .compression(TiffCompression::Lzw)
            .predictor(Some(2))
            .build();
        let pipeline = ArwToTiffPipeline::new(config);
        
        b.iter(|| {
            let mut output = Cursor::new(Vec::new());
            let _ = pipeline.convert(black_box(&mock_data), &mut output);
        });
    });
    
    group.finish();
}

fn benchmark_validation_overhead(c: &mut Criterion) {
    let mut group = c.benchmark_group("validation_overhead");
    let mock_data = generate_mock_raw_data(500, 500);
    
    group.bench_function("with_validation", |b| {
        let config = ConversionConfig::builder()
            .validate_dimensions(true)
            .build();
        let pipeline = ArwToTiffPipeline::new(config);
        
        b.iter(|| {
            let mut output = Cursor::new(Vec::new());
            let _ = pipeline.convert(black_box(&mock_data), &mut output);
        });
    });
    
    group.bench_function("without_validation", |b| {
        let config = ConversionConfig::builder()
            .validate_dimensions(false)
            .build();
        let pipeline = ArwToTiffPipeline::new(config);
        
        b.iter(|| {
            let mut output = Cursor::new(Vec::new());
            let _ = pipeline.convert(black_box(&mock_data), &mut output);
        });
    });
    
    group.finish();
}

criterion_group!(
    benches,
    benchmark_conversion_sizes,
    benchmark_compression_methods,
    benchmark_with_predictor,
    benchmark_validation_overhead
);
criterion_main!(benches);
