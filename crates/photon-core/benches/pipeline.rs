//! Benchmarks for the Photon image processing pipeline.
//!
//! Run with: cargo bench -p photon-core

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use image::DynamicImage;
use photon_core::config::{LimitsConfig, ThumbnailConfig};
use std::path::Path;

fn fixture_path(name: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/images")
        .join(name)
}

fn benchmark_content_hash(c: &mut Criterion) {
    let path = fixture_path("test.png");
    if !path.exists() {
        eprintln!("Skipping content_hash benchmark: test fixture not found");
        return;
    }

    c.bench_function("content_hash_blake3", |b| {
        b.iter(|| {
            let _ = photon_core::pipeline::Hasher::content_hash(black_box(&path));
        })
    });
}

fn benchmark_perceptual_hash(c: &mut Criterion) {
    let img = DynamicImage::new_rgb8(256, 256);

    c.bench_function("perceptual_hash", |b| {
        b.iter(|| {
            let _ = photon_core::pipeline::Hasher::perceptual_hash(black_box(&img));
        })
    });
}

fn benchmark_decode(c: &mut Criterion) {
    let path = fixture_path("test.png");
    if !path.exists() {
        eprintln!("Skipping decode benchmark: test fixture not found");
        return;
    }

    let decoder = photon_core::pipeline::ImageDecoder::new(LimitsConfig::default());
    let rt = tokio::runtime::Runtime::new().unwrap();

    c.bench_function("decode_image", |b| {
        b.iter(|| {
            let _ = rt.block_on(decoder.decode(black_box(&path)));
        })
    });
}

fn benchmark_thumbnail(c: &mut Criterion) {
    let img = DynamicImage::new_rgb8(1920, 1080);
    let generator = photon_core::pipeline::ThumbnailGenerator::new(ThumbnailConfig::default());

    c.bench_function("thumbnail_256px", |b| {
        b.iter(|| {
            let _ = generator.generate(black_box(&img));
        })
    });
}

fn benchmark_metadata(c: &mut Criterion) {
    let path = fixture_path("test.png");
    if !path.exists() {
        eprintln!("Skipping metadata benchmark: test fixture not found");
        return;
    }

    c.bench_function("metadata_extract", |b| {
        b.iter(|| {
            let _ = photon_core::pipeline::MetadataExtractor::extract(black_box(&path));
        })
    });
}

criterion_group!(
    benches,
    benchmark_content_hash,
    benchmark_perceptual_hash,
    benchmark_decode,
    benchmark_thumbnail,
    benchmark_metadata,
);
criterion_main!(benches);
