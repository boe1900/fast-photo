use crate::models::ExifData;
use chrono::NaiveDateTime;
use std::fs::File;
use std::io::BufReader;
use std::path::Path;
use tracing;

/// Extract EXIF metadata from an image file
pub fn extract_exif(path: &Path) -> Option<ExifData> {
    let file = File::open(path).ok()?;
    let mut reader = BufReader::new(file);
    let exif_reader = exif::Reader::new();
    let exif = exif_reader.read_from_container(&mut reader).ok()?;

    let camera_make = get_field_string(&exif, exif::Tag::Make);
    let camera_model = get_field_string(&exif, exif::Tag::Model);
    let lens_model = get_field_string(&exif, exif::Tag::LensModel);
    let focal_length = get_field_string(&exif, exif::Tag::FocalLength);
    let aperture = get_field_string(&exif, exif::Tag::FNumber);
    let shutter_speed = get_field_string(&exif, exif::Tag::ExposureTime);
    let iso = get_field_u32(&exif, exif::Tag::PhotographicSensitivity).map(|v| v as i32);
    let orientation = get_field_u32(&exif, exif::Tag::Orientation).map(|v| v as u16);

    let taken_at =
        get_field_string(&exif, exif::Tag::DateTimeOriginal).and_then(|s| parse_exif_datetime(&s));

    let latitude = extract_gps_coord(&exif, exif::Tag::GPSLatitude, exif::Tag::GPSLatitudeRef);
    let longitude = extract_gps_coord(&exif, exif::Tag::GPSLongitude, exif::Tag::GPSLongitudeRef);

    Some(ExifData {
        camera_make,
        camera_model,
        lens_model,
        focal_length,
        aperture,
        shutter_speed,
        iso,
        taken_at,
        latitude,
        longitude,
        orientation,
    })
}

fn get_field_string(exif: &exif::Exif, tag: exif::Tag) -> Option<String> {
    exif.get_field(tag, exif::In::PRIMARY)
        .map(|f| f.display_value().with_unit(f).to_string())
}

fn get_field_u32(exif: &exif::Exif, tag: exif::Tag) -> Option<u32> {
    exif.get_field(tag, exif::In::PRIMARY)
        .and_then(|f| match &f.value {
            exif::Value::Short(v) => v.first().map(|&x| x as u32),
            exif::Value::Long(v) => v.first().copied(),
            _ => None,
        })
}

fn parse_exif_datetime(s: &str) -> Option<NaiveDateTime> {
    // EXIF format: "2024:01:15 14:30:00" or "2024-01-15 14:30:00"
    let cleaned = s.trim().trim_matches('"');
    NaiveDateTime::parse_from_str(cleaned, "%Y:%m:%d %H:%M:%S")
        .or_else(|_| NaiveDateTime::parse_from_str(cleaned, "%Y-%m-%d %H:%M:%S"))
        .ok()
}

fn extract_gps_coord(exif: &exif::Exif, coord_tag: exif::Tag, ref_tag: exif::Tag) -> Option<f64> {
    let coord_field = exif.get_field(coord_tag, exif::In::PRIMARY)?;
    let ref_field = exif.get_field(ref_tag, exif::In::PRIMARY)?;

    let rationals = match &coord_field.value {
        exif::Value::Rational(v) if v.len() >= 3 => v,
        _ => return None,
    };

    let degrees = rationals[0].to_f64();
    let minutes = rationals[1].to_f64();
    let seconds = rationals[2].to_f64();

    let mut coord = degrees + minutes / 60.0 + seconds / 3600.0;

    let ref_str = ref_field.display_value().to_string();
    if ref_str.contains('S') || ref_str.contains('W') {
        coord = -coord;
    }

    Some(coord)
}

/// Get image dimensions without loading the full image
pub fn get_image_dimensions(path: &Path) -> Option<(u32, u32)> {
    match image::image_dimensions(path) {
        Ok(dims) => Some(dims),
        Err(e) => {
            tracing::debug!("Failed to get dimensions for {:?}: {}", path, e);
            None
        }
    }
}
