//! Exact (SHA-256) and perceptual (aHash / dHash / pHash) hashing.
//!
//! Perceptual hashes are 64-bit fingerprints of what an image looks like. Two images
//! that look alike have hashes that differ in only a few bits, measured by
//! [`hamming`] distance, even if their bytes are completely different (resized,
//! recompressed, converted to another format).

use image::imageops::{self, FilterType};
use image::GrayImage;
use sha2::{Digest, Sha256};
use std::io::Read;

pub fn sha256_bytes(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

pub fn sha256_reader(mut reader: impl Read) -> std::io::Result<String> {
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 1 << 16];
    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

/// Number of differing bits between two hashes.
pub fn hamming(a: u64, b: u64) -> u32 {
    (a ^ b).count_ones()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PerceptualHashes {
    pub ahash: u64,
    pub dhash: u64,
    pub phash: u64,
}

/// Computes all three perceptual hashes from a grayscale image of any size.
pub fn perceptual_hashes(gray: &GrayImage) -> PerceptualHashes {
    PerceptualHashes {
        ahash: ahash(gray),
        dhash: dhash(gray),
        phash: phash(gray),
    }
}

/// Average hash: shrink to 8x8, each bit = pixel brighter than the mean.
pub fn ahash(gray: &GrayImage) -> u64 {
    let small = imageops::resize(gray, 8, 8, FilterType::Triangle);
    let mean = small.pixels().map(|p| p.0[0] as u32).sum::<u32>() / 64;
    small.pixels().enumerate().fold(0u64, |acc, (i, p)| {
        acc | (((p.0[0] as u32 > mean) as u64) << i)
    })
}

/// Difference hash: shrink to 9x8, each bit = pixel brighter than its right neighbour.
/// Captures gradients, so it is robust to overall brightness changes.
pub fn dhash(gray: &GrayImage) -> u64 {
    let small = imageops::resize(gray, 9, 8, FilterType::Triangle);
    let mut hash = 0u64;
    for y in 0..8 {
        for x in 0..8 {
            let left = small.get_pixel(x, y).0[0];
            let right = small.get_pixel(x + 1, y).0[0];
            hash |= ((left > right) as u64) << (y * 8 + x);
        }
    }
    hash
}

/// DCT hash: shrink to 32x32, take the 8x8 lowest-frequency DCT coefficients,
/// each bit = coefficient above the median. Captures overall structure and is the
/// most robust of the three to resizing and recompression.
pub fn phash(gray: &GrayImage) -> u64 {
    const N: usize = 32;
    const K: usize = 8;
    let small = imageops::resize(gray, N as u32, N as u32, FilterType::Lanczos3);

    let mut px = [[0f64; N]; N];
    for (x, y, p) in small.enumerate_pixels() {
        px[y as usize][x as usize] = p.0[0] as f64;
    }

    // cos_table[u][x] = cos((2x + 1) * u * pi / 2N), the DCT-II basis.
    let mut cos_table = [[0f64; N]; K];
    for (u, row) in cos_table.iter_mut().enumerate() {
        for (x, c) in row.iter_mut().enumerate() {
            *c = (((2 * x + 1) * u) as f64 * std::f64::consts::PI / (2 * N) as f64).cos();
        }
    }

    // Separable 2D DCT, but only for the K x K low-frequency corner we need.
    let mut rows = [[0f64; K]; N]; // rows[y][u] = sum_x px[y][x] * cos[u][x]
    for y in 0..N {
        for u in 0..K {
            rows[y][u] = (0..N).map(|x| px[y][x] * cos_table[u][x]).sum();
        }
    }
    let mut coeffs = [0f64; K * K];
    for v in 0..K {
        for u in 0..K {
            coeffs[v * K + u] = (0..N).map(|y| rows[y][u] * cos_table[v][y]).sum();
        }
    }

    // The DC term (index 0) is just average brightness; exclude it from the median.
    let mut ac: Vec<f64> = coeffs[1..].to_vec();
    ac.sort_by(|a, b| a.total_cmp(b));
    let median = ac[ac.len() / 2];

    coeffs
        .iter()
        .enumerate()
        .skip(1)
        .fold(0u64, |acc, (i, &c)| acc | (((c > median) as u64) << i))
}

/// Pixel-level difference between two pictures, robust to resizing and recompression.
///
/// Both images are squashed to 512x512, lightly blurred (to cancel out compression noise and
/// resampling differences), and split into 8x8-pixel patches. The score is the mean absolute
/// difference (0-255) of the *most different* patch. Re-encoded or resized copies differ a
/// little everywhere, so every patch scores low. Pictures with real changes, like two
/// screenshots of one window with different text, match almost everywhere but differ
/// strongly in a few patches, which drives the score up.
pub fn local_difference(a: &GrayImage, b: &GrayImage) -> f64 {
    const SIZE: u32 = 512;
    const PATCH: u32 = 8;
    let prep =
        |g: &GrayImage| imageops::blur(&imageops::resize(g, SIZE, SIZE, FilterType::Triangle), 1.0);
    let (a, b) = (prep(a), prep(b));
    let (pa, pb) = (a.as_raw(), b.as_raw());
    let mut worst = 0u32;
    for py in 0..SIZE / PATCH {
        for px in 0..SIZE / PATCH {
            let mut sum = 0u32;
            for y in py * PATCH..(py + 1) * PATCH {
                let row = (y * SIZE) as usize;
                for x in px * PATCH..(px + 1) * PATCH {
                    let i = row + x as usize;
                    sum += pa[i].abs_diff(pb[i]) as u32;
                }
            }
            worst = worst.max(sum);
        }
    }
    worst as f64 / (PATCH * PATCH) as f64
}

/// Sharpness score: variance of the 4-neighbour Laplacian on a fixed 256x256 thumbnail.
/// Blurry images have weak edges and therefore a low score. Normalising the size makes
/// scores comparable between copies of the same picture at different resolutions.
pub fn sharpness(gray: &GrayImage) -> f64 {
    const S: u32 = 256;
    let img = imageops::resize(gray, S, S, FilterType::Triangle);
    let at = |x: u32, y: u32| img.get_pixel(x, y).0[0] as f64;
    let mut sum = 0.0;
    let mut sum_sq = 0.0;
    let mut n = 0.0;
    for y in 1..S - 1 {
        for x in 1..S - 1 {
            let lap = at(x - 1, y) + at(x + 1, y) + at(x, y - 1) + at(x, y + 1) - 4.0 * at(x, y);
            sum += lap;
            sum_sq += lap * lap;
            n += 1.0;
        }
    }
    let mean = sum / n;
    sum_sq / n - mean * mean
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{DynamicImage, Luma, RgbImage};

    fn scene(w: u32, h: u32) -> GrayImage {
        // A smooth, structured picture: diagonal gradient with a bright disc.
        GrayImage::from_fn(w, h, |x, y| {
            let (fx, fy) = (x as f64 / w as f64, y as f64 / h as f64);
            let disc = ((fx - 0.35).powi(2) + (fy - 0.4).powi(2)).sqrt() < 0.18;
            Luma([if disc { 240 } else { ((fx + fy) * 100.0) as u8 }])
        })
    }

    fn other_scene(w: u32, h: u32) -> GrayImage {
        GrayImage::from_fn(w, h, |x, y| {
            let stripes = (((x as f64 / w as f64) * 6.0) as u32).is_multiple_of(2);
            let fy = y as f64 / h as f64;
            Luma([if stripes {
                (fy * 200.0) as u8
            } else {
                255 - (fy * 200.0) as u8
            }])
        })
    }

    #[test]
    fn sha256_matches_known_vector() {
        assert_eq!(
            sha256_bytes(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(sha256_reader(&b"abc"[..]).unwrap(), sha256_bytes(b"abc"));
    }

    #[test]
    fn resized_copy_has_near_identical_hashes() {
        let big = perceptual_hashes(&scene(1200, 900));
        let small = perceptual_hashes(&scene(400, 300));
        assert!(hamming(big.phash, small.phash) <= 4, "phash too far");
        assert!(hamming(big.dhash, small.dhash) <= 6, "dhash too far");
        assert!(hamming(big.ahash, small.ahash) <= 6, "ahash too far");
    }

    #[test]
    fn jpeg_recompression_keeps_hashes_close() {
        let original = scene(800, 600);
        let mut jpeg = Vec::new();
        DynamicImage::ImageLuma8(original.clone())
            .write_to(
                &mut std::io::Cursor::new(&mut jpeg),
                image::ImageFormat::Jpeg,
            )
            .unwrap();
        let decoded = image::load_from_memory(&jpeg).unwrap().to_luma8();
        let (a, b) = (perceptual_hashes(&original), perceptual_hashes(&decoded));
        assert!(hamming(a.phash, b.phash) <= 4);
    }

    #[test]
    fn different_pictures_are_far_apart() {
        let a = perceptual_hashes(&scene(800, 600));
        let b = perceptual_hashes(&other_scene(800, 600));
        assert!(hamming(a.phash, b.phash) > 16, "phash should differ");
    }

    #[test]
    fn blur_lowers_sharpness() {
        let rgb = RgbImage::from_fn(512, 512, |x, y| {
            let v = if (x / 8 + y / 8) % 2 == 0 { 255 } else { 0 };
            image::Rgb([v, v, v])
        });
        let sharp = DynamicImage::ImageRgb8(rgb);
        let blurred = sharp.blur(4.0);
        assert!(sharpness(&sharp.to_luma8()) > 2.0 * sharpness(&blurred.to_luma8()));
    }
}
