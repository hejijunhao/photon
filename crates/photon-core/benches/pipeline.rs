//! Benchmarks for the Photon image processing pipeline.
//!
//! Run with: cargo bench -p photon-core
//!
//! Benchmarks that require ONNX models on disk (e2e, batch) skip gracefully
//! if models are not found, so CI runs without model files.

use std::path::Path;
use std::sync::Arc;

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use image::DynamicImage;
use photon_core::config::{LimitsConfig, ThumbnailConfig};

fn fixture_path(name: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/images")
        .join(name)
}

// ---------------------------------------------------------------------------
// Existing benchmarks (fixed API paths)
// ---------------------------------------------------------------------------

fn benchmark_content_hash(c: &mut Criterion) {
    let path = fixture_path("test.png");
    if !path.exists() {
        eprintln!("Skipping content_hash benchmark: test fixture not found");
        return;
    }

    c.bench_function("content_hash_blake3", |b| {
        b.iter(|| {
            let _ = photon_core::Hasher::content_hash(black_box(&path));
        })
    });
}

fn benchmark_perceptual_hash(c: &mut Criterion) {
    let img = DynamicImage::new_rgb8(256, 256);
    let hasher = photon_core::Hasher::new();

    c.bench_function("perceptual_hash", |b| {
        b.iter(|| {
            let _ = hasher.perceptual_hash(black_box(&img));
        })
    });
}

fn benchmark_decode(c: &mut Criterion) {
    let path = fixture_path("test.png");
    if !path.exists() {
        eprintln!("Skipping decode benchmark: test fixture not found");
        return;
    }

    let bytes = std::fs::read(&path).unwrap();
    let decoder = photon_core::ImageDecoder::new(LimitsConfig::default());
    let rt = tokio::runtime::Runtime::new().unwrap();

    c.bench_function("decode_image", |b| {
        b.iter(|| {
            let _ =
                rt.block_on(decoder.decode_from_bytes(black_box(bytes.clone()), black_box(&path)));
        })
    });
}

fn benchmark_thumbnail(c: &mut Criterion) {
    let img = DynamicImage::new_rgb8(1920, 1080);
    let generator = photon_core::ThumbnailGenerator::new(ThumbnailConfig::default());

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
            let _ = photon_core::MetadataExtractor::extract(black_box(&path));
        })
    });
}

// ---------------------------------------------------------------------------
// New benchmarks (Phase 5)
// ---------------------------------------------------------------------------

/// Benchmark the scoring hot path: 68K × 768 matrix-vector multiply.
///
/// Uses raw ndarray — the exact BLAS operation that `TagScorer::score()` performs.
fn benchmark_score(c: &mut Criterion) {
    use ndarray::{Array1, Array2};

    let n = 68_000;
    let dim = 768;

    // Synthetic label bank matrix (N × 768) — values don't matter for throughput.
    let matrix = Array2::from_shape_fn((n, dim), |(i, j)| ((i * dim + j) as f32 * 0.0001).sin());
    // Synthetic image embedding (unit-ish vector).
    let image = Array1::from_shape_fn(dim, |j| (j as f32 * 0.001).cos());

    c.bench_function("score_68k_matvec", |b| {
        b.iter(|| {
            let scores = matrix.dot(black_box(&image));
            black_box(scores);
        })
    });
}

/// Benchmark image preprocessing at 224×224 (default model size).
fn benchmark_preprocess_224(c: &mut Criterion) {
    let img = DynamicImage::new_rgb8(4032, 3024);

    c.bench_function("preprocess_224", |b| {
        b.iter(|| {
            let _ = photon_core::preprocess_image(black_box(&img), 224);
        })
    });
}

/// Benchmark image preprocessing at 384×384 (high-quality model size).
fn benchmark_preprocess_384(c: &mut Criterion) {
    let img = DynamicImage::new_rgb8(4032, 3024);

    c.bench_function("preprocess_384", |b| {
        b.iter(|| {
            let _ = photon_core::preprocess_image(black_box(&img), 384);
        })
    });
}

/// End-to-end single-image processing benchmark.
///
/// Requires ONNX models on disk — skips if not found.
fn benchmark_process_e2e(c: &mut Criterion) {
    let config = photon_core::Config::default();

    if !photon_core::EmbeddingEngine::model_exists(&config.embedding, &config.model_dir()) {
        eprintln!("Skipping e2e benchmark: ONNX models not found");
        return;
    }

    let path = fixture_path("dog.jpg");
    if !path.exists() {
        eprintln!("Skipping e2e benchmark: dog.jpg fixture not found");
        return;
    }

    let rt = tokio::runtime::Runtime::new().unwrap();
    let mut processor = photon_core::ImageProcessor::new(&config);
    if processor.load_embedding(&config).is_err() {
        eprintln!("Skipping e2e benchmark: failed to load embedding model");
        return;
    }

    c.bench_function("process_e2e_dog_jpg", |b| {
        b.iter(|| {
            let _ = rt.block_on(processor.process(black_box(&path)));
        })
    });
}

/// Batch throughput benchmark: 4 images processed concurrently.
///
/// Requires ONNX models on disk — skips if not found.
fn benchmark_batch_throughput(c: &mut Criterion) {
    use futures_util::stream::{self, StreamExt};

    let config = photon_core::Config::default();

    if !photon_core::EmbeddingEngine::model_exists(&config.embedding, &config.model_dir()) {
        eprintln!("Skipping batch benchmark: ONNX models not found");
        return;
    }

    let paths: Vec<std::path::PathBuf> = ["dog.jpg", "beach.jpg", "car.jpg", "test.png"]
        .iter()
        .map(|n| fixture_path(n))
        .filter(|p| p.exists())
        .collect();

    if paths.is_empty() {
        eprintln!("Skipping batch benchmark: no fixtures found");
        return;
    }

    let rt = tokio::runtime::Runtime::new().unwrap();
    let mut processor = photon_core::ImageProcessor::new(&config);
    if processor.load_embedding(&config).is_err() {
        eprintln!("Skipping batch benchmark: failed to load embedding model");
        return;
    }

    let processor = Arc::new(processor);
    let options = Arc::new(photon_core::ProcessOptions::default());

    c.bench_function("batch_4_images", |b| {
        b.iter(|| {
            rt.block_on(async {
                let results: Vec<_> = stream::iter(paths.iter())
                    .map(|path| {
                        let proc = Arc::clone(&processor);
                        let opts = Arc::clone(&options);
                        async move { proc.process_with_options(path, &opts).await }
                    })
                    .buffer_unordered(4)
                    .collect()
                    .await;
                black_box(results);
            });
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
    benchmark_score,
    benchmark_preprocess_224,
    benchmark_preprocess_384,
    benchmark_process_e2e,
    benchmark_batch_throughput,
);
criterion_main!(benches);
