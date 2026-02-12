//! EXIF metadata extraction from images.

use exif::{In, Reader, Tag, Value};
use std::fs::File;
use std::io::BufReader;
use std::path::Path;

use crate::types::ExifData;

/// Extracts EXIF metadata from image files.
pub struct MetadataExtractor;

impl MetadataExtractor {
    /// Extract EXIF data from an image file.
    ///
    /// Returns `None` if the file has no EXIF data or if extraction fails.
    /// This method is intentionally lenient - it returns partial data if available.
    pub fn extract(path: &Path) -> Option<ExifData> {
        let file = File::open(path).ok()?;
        let mut reader = BufReader::new(file);
        let exif = Reader::new().read_from_container(&mut reader).ok()?;

        let data = ExifData {
            captured_at: Self::get_datetime(&exif),
            camera_make: Self::get_string(&exif, Tag::Make),
            camera_model: Self::get_string(&exif, Tag::Model),
            gps_latitude: Self::get_gps_coord(&exif, Tag::GPSLatitude, Tag::GPSLatitudeRef),
            gps_longitude: Self::get_gps_coord(&exif, Tag::GPSLongitude, Tag::GPSLongitudeRef),
            iso: Self::get_u32(&exif, Tag::PhotographicSensitivity),
            aperture: Self::get_aperture(&exif),
            shutter_speed: Self::get_shutter_speed(&exif),
            focal_length: Self::get_focal_length(&exif),
            orientation: Self::get_u32(&exif, Tag::Orientation),
        };

        // Only return if we have at least some data
        if data.captured_at.is_some()
            || data.camera_make.is_some()
            || data.camera_model.is_some()
            || data.gps_latitude.is_some()
            || data.gps_longitude.is_some()
            || data.iso.is_some()
            || data.aperture.is_some()
            || data.shutter_speed.is_some()
            || data.focal_length.is_some()
            || data.orientation.is_some()
        {
            Some(data)
        } else {
            None
        }
    }

    /// Get a string field from EXIF data.
    fn get_string(exif: &exif::Exif, tag: Tag) -> Option<String> {
        exif.get_field(tag, In::PRIMARY).map(|f| {
            let s = f.display_value().to_string();
            // Clean up the string (remove quotes if present)
            s.trim_matches('"').to_string()
        })
    }

    /// Get a u32 field from EXIF data.
    fn get_u32(exif: &exif::Exif, tag: Tag) -> Option<u32> {
        exif.get_field(tag, In::PRIMARY)
            .and_then(|f| match &f.value {
                Value::Short(v) => v.first().map(|&x| x as u32),
                Value::Long(v) => v.first().copied(),
                _ => None,
            })
    }

    /// Get the capture datetime, preferring DateTimeOriginal over DateTime.
    fn get_datetime(exif: &exif::Exif) -> Option<String> {
        exif.get_field(Tag::DateTimeOriginal, In::PRIMARY)
            .or_else(|| exif.get_field(Tag::DateTime, In::PRIMARY))
            .map(|f| {
                let s = f.display_value().to_string();
                s.trim_matches('"').to_string()
            })
    }

    /// Get GPS coordinate, converting from degrees/minutes/seconds to decimal.
    fn get_gps_coord(exif: &exif::Exif, coord_tag: Tag, ref_tag: Tag) -> Option<f64> {
        let coord = exif.get_field(coord_tag, In::PRIMARY)?;
        let reference = exif.get_field(ref_tag, In::PRIMARY)?;

        // Parse degrees, minutes, seconds from EXIF rational values
        let degrees = Self::parse_gps_rationals(&coord.value)?;
        let ref_str = reference.display_value().to_string();

        // Apply sign based on reference (N/S for lat, E/W for lon)
        let sign = if ref_str.contains('S') || ref_str.contains('W') {
            -1.0
        } else {
            1.0
        };

        Some(sign * degrees)
    }

    /// Parse GPS rationals (degrees, minutes, seconds) to decimal degrees.
    fn parse_gps_rationals(value: &Value) -> Option<f64> {
        match value {
            Value::Rational(rationals) if rationals.len() >= 3 => {
                let degrees = rationals[0].to_f64();
                let minutes = rationals[1].to_f64();
                let seconds = rationals[2].to_f64();
                Some(degrees + minutes / 60.0 + seconds / 3600.0)
            }
            _ => None,
        }
    }

    /// Get aperture as a formatted string (e.g., "f/1.8").
    fn get_aperture(exif: &exif::Exif) -> Option<String> {
        exif.get_field(Tag::FNumber, In::PRIMARY).map(|f| {
            let s = f.display_value().to_string();
            format!("f/{}", s)
        })
    }

    /// Get shutter speed as a string (e.g., "1/1000").
    fn get_shutter_speed(exif: &exif::Exif) -> Option<String> {
        exif.get_field(Tag::ExposureTime, In::PRIMARY)
            .map(|f| f.display_value().to_string())
    }

    /// Get focal length in mm.
    fn get_focal_length(exif: &exif::Exif) -> Option<f32> {
        exif.get_field(Tag::FocalLength, In::PRIMARY)
            .and_then(|f| match &f.value {
                Value::Rational(v) => v.first().map(|r| r.to_f64() as f32),
                _ => None,
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_missing_file() {
        let result = MetadataExtractor::extract(Path::new("/nonexistent/file.jpg"));
        assert!(result.is_none());
    }
}
