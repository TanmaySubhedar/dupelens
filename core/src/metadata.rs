//! Reads one file and produces everything the engine stores about it:
//! SHA-256, EXIF facts, pixel dimensions, perceptual hashes and sharpness.
//! Files are only ever opened for reading.

use crate::hashing::{self, PerceptualHashes};
use exif::{In, Tag, Value};
use image::{DynamicImage, ImageReader};
use std::fs::File;
use std::io::{BufReader, Cursor};
use std::path::Path;

/// Files larger than this are hashed by streaming and skip image decoding.
const MAX_IN_MEMORY: u64 = 256 * 1024 * 1024;

/// Star rating (Windows/Adobe EXIF "Rating", 0-5) at or above which a photo counts as a favourite.
const FAVORITE_MIN_RATING: u32 = 4;

/// EXIF tag 0x4746: Rating. Not among kamadak-exif's named constants.
const TAG_RATING: Tag = Tag(exif::Context::Tiff, 0x4746);

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ExifInfo {
    pub tag_count: u32,
    pub taken_at: Option<String>,
    pub favorite: bool,
    /// EXIF orientation 1-8 (1 = upright).
    pub orientation: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Analysis {
    pub sha256: String,
    pub exif: ExifInfo,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub hashes: Option<PerceptualHashes>,
    pub sharpness: Option<f64>,
}

pub fn analyze(path: &Path, size: u64) -> std::io::Result<Analysis> {
    if size > MAX_IN_MEMORY {
        let sha256 = hashing::sha256_reader(BufReader::new(File::open(path)?))?;
        let exif = read_exif(&mut BufReader::new(File::open(path)?));
        return Ok(Analysis {
            sha256,
            exif,
            width: None,
            height: None,
            hashes: None,
            sharpness: None,
        });
    }

    // One read serves hashing, EXIF and decoding.
    let bytes = std::fs::read(path)?;
    let sha256 = hashing::sha256_bytes(&bytes);
    let exif = read_exif(&mut Cursor::new(&bytes));

    let mut analysis = Analysis {
        sha256,
        exif,
        width: None,
        height: None,
        hashes: None,
        sharpness: None,
    };

    // Undecodable or corrupt images still take part in exact (SHA-256) matching.
    if crate::scanner::is_decodable(path) {
        if let Some(img) = decode(&bytes) {
            let img = apply_orientation(img, analysis.exif.orientation);
            analysis.width = Some(img.width());
            analysis.height = Some(img.height());
            // A ~512px thumbnail is plenty for hashing and much faster to process.
            let gray = img.thumbnail(512, 512).to_luma8();
            analysis.hashes = Some(hashing::perceptual_hashes(&gray));
            analysis.sharpness = Some(hashing::sharpness(&gray));
        }
    }
    Ok(analysis)
}

fn decode(bytes: &[u8]) -> Option<DynamicImage> {
    ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .ok()?
        .decode()
        .ok()
}

pub fn read_exif<R: std::io::BufRead + std::io::Seek>(reader: &mut R) -> ExifInfo {
    let Ok(exif) = exif::Reader::new().read_from_container(reader) else {
        return ExifInfo {
            orientation: 1,
            ..Default::default()
        };
    };

    let taken_at = [Tag::DateTimeOriginal, Tag::DateTimeDigitized, Tag::DateTime]
        .iter()
        .find_map(|&tag| {
            let field = exif.get_field(tag, In::PRIMARY)?;
            let Value::Ascii(ref parts) = field.value else {
                return None;
            };
            let dt = exif::DateTime::from_ascii(parts.first()?).ok()?;
            Some(format!(
                "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
                dt.year, dt.month, dt.day, dt.hour, dt.minute, dt.second
            ))
        });

    let uint = |tag| {
        exif.get_field(tag, In::PRIMARY)
            .and_then(|f| f.value.get_uint(0))
    };

    ExifInfo {
        tag_count: exif.fields().count() as u32,
        taken_at,
        favorite: uint(TAG_RATING).is_some_and(|r| r >= FAVORITE_MIN_RATING),
        orientation: uint(Tag::Orientation)
            .filter(|o| (1..=8).contains(o))
            .unwrap_or(1),
    }
}

/// Rotates/flips pixels so that a photo stored sideways with an orientation flag hashes
/// the same as a copy whose pixels were physically rotated by an editor.
pub fn apply_orientation(img: DynamicImage, orientation: u32) -> DynamicImage {
    match orientation {
        2 => img.fliph(),
        3 => img.rotate180(),
        4 => img.flipv(),
        5 => img.rotate90().fliph(),
        6 => img.rotate90(),
        7 => img.rotate270().fliph(),
        8 => img.rotate270(),
        _ => img,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::RgbImage;

    #[test]
    fn non_image_bytes_have_no_exif() {
        let info = read_exif(&mut Cursor::new(b"not an image".to_vec()));
        assert_eq!(info.tag_count, 0);
        assert_eq!(info.orientation, 1);
        assert!(info.taken_at.is_none());
    }

    #[test]
    fn orientation_6_swaps_dimensions() {
        let img = DynamicImage::ImageRgb8(RgbImage::new(40, 30));
        let rotated = apply_orientation(img, 6);
        assert_eq!((rotated.width(), rotated.height()), (30, 40));
    }

    #[test]
    fn analyze_png_reports_dimensions_and_hashes() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.png");
        RgbImage::from_fn(64, 48, |x, y| image::Rgb([x as u8 * 4, y as u8 * 5, 90]))
            .save(&path)
            .unwrap();
        let size = std::fs::metadata(&path).unwrap().len();
        let a = analyze(&path, size).unwrap();
        assert_eq!((a.width, a.height), (Some(64), Some(48)));
        assert!(a.hashes.is_some());
        assert_eq!(a.sha256.len(), 64);
    }

    #[test]
    fn corrupt_image_still_gets_sha256() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("broken.jpg");
        std::fs::write(&path, b"\xFF\xD8\xFF garbage").unwrap();
        let a = analyze(&path, 13).unwrap();
        assert!(a.hashes.is_none());
        assert_eq!(a.sha256.len(), 64);
    }
}
