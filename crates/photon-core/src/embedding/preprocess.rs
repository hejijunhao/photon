//! Image preprocessing for SigLIP embedding generation.
//!
//! SigLIP base-patch16-224 expects:
//! - Input size: 224×224 pixels
//! - Normalization: pixels scaled to [-1, 1] via (pixel/255 - 0.5) / 0.5
//! - Channel order: RGB
//! - Tensor layout: NCHW [batch, channels, height, width]

use image::DynamicImage;
use ndarray::Array4;

/// Number of color channels (RGB).
const CHANNELS: usize = 3;

/// SigLIP normalization mean (per-channel).
const NORM_MEAN: f32 = 0.5;

/// SigLIP normalization std (per-channel).
const NORM_STD: f32 = 0.5;

/// Preprocess an image for SigLIP inference.
///
/// Resizes to `image_size × image_size`, converts to RGB, normalizes to [-1, 1],
/// and returns an NCHW tensor suitable for ONNX Runtime.
pub fn preprocess(image: &DynamicImage, image_size: u32) -> Array4<f32> {
    let resized = image.resize_exact(
        image_size,
        image_size,
        image::imageops::FilterType::Lanczos3,
    );
    let rgb = resized.to_rgb8();

    let size = image_size as usize;
    let mut tensor = Array4::<f32>::zeros((1, CHANNELS, size, size));

    // Access raw RGB bytes and tensor slice directly to avoid per-pixel
    // bounds-checking overhead from get_pixel() and 4D ndarray indexing.
    let raw = rgb.as_raw();
    let tensor_data = tensor.as_slice_mut().unwrap();
    for (i, pixel) in raw.chunks_exact(3).enumerate() {
        let y = i / size;
        let x = i % size;
        for (c, &val) in pixel.iter().enumerate() {
            // NCHW layout: offset = c * size * size + y * size + x
            let idx = c * size * size + y * size + x;
            tensor_data[idx] = (val as f32 / 255.0 - NORM_MEAN) / NORM_STD;
        }
    }

    tensor
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{DynamicImage, RgbImage};

    #[test]
    fn test_preprocess_shape_224() {
        let img = DynamicImage::ImageRgb8(RgbImage::new(640, 480));
        let tensor = preprocess(&img, 224);
        assert_eq!(tensor.shape(), &[1, 3, 224, 224]);
    }

    #[test]
    fn test_preprocess_shape_384() {
        let img = DynamicImage::ImageRgb8(RgbImage::new(640, 480));
        let tensor = preprocess(&img, 384);
        assert_eq!(tensor.shape(), &[1, 3, 384, 384]);
    }

    #[test]
    fn test_preprocess_normalization_range() {
        // White image (255, 255, 255) -> (255/255 - 0.5) / 0.5 = 1.0
        let img =
            DynamicImage::ImageRgb8(RgbImage::from_pixel(10, 10, image::Rgb([255, 255, 255])));
        let tensor = preprocess(&img, 224);
        let max_val = tensor.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
        assert!((max_val - 1.0).abs() < 0.01);

        // Black image (0, 0, 0) -> (0/255 - 0.5) / 0.5 = -1.0
        let img = DynamicImage::ImageRgb8(RgbImage::from_pixel(10, 10, image::Rgb([0, 0, 0])));
        let tensor = preprocess(&img, 224);
        let min_val = tensor.iter().cloned().fold(f32::INFINITY, f32::min);
        assert!((min_val - (-1.0)).abs() < 0.01);
    }
}
