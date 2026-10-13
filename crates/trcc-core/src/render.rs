//! Turns an image file into a frame the screen can show.

use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;

use image::codecs::jpeg::JpegEncoder;
use image::imageops::{self, FilterType};
use image::{DynamicImage, ImageReader, Limits, RgbImage};

use crate::Error;

/// Files and decoded images above these sizes are refused before decoding,
/// so a crafted image cannot exhaust memory.
pub const MAX_FILE_BYTES: u64 = 32 * 1024 * 1024;
pub const MAX_DIMENSION: u32 = 8192;
const MAX_ALLOC: u64 = 256 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Fit {
    /// Cover the whole screen, cropping the edges.
    #[default]
    Fill,
    /// Show the whole image with black bars.
    Fit,
    /// Stretch to the screen, ignoring aspect ratio.
    Stretch,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Rotation {
    #[default]
    R0,
    R90,
    R180,
    R270,
}

impl Rotation {
    pub fn from_degrees(deg: u16) -> Result<Self, Error> {
        match deg {
            0 => Ok(Self::R0),
            90 => Ok(Self::R90),
            180 => Ok(Self::R180),
            270 => Ok(Self::R270),
            _ => Err(Error::InvalidRotation(deg)),
        }
    }
}

fn limits() -> Limits {
    let mut l = Limits::default();
    l.max_image_width = Some(MAX_DIMENSION);
    l.max_image_height = Some(MAX_DIMENSION);
    l.max_alloc = Some(MAX_ALLOC);
    l
}

pub fn load(path: &Path) -> Result<DynamicImage, Error> {
    let file = File::open(path)?;
    let size = file.metadata()?.len();
    if size > MAX_FILE_BYTES {
        return Err(Error::ImageTooLarge);
    }
    let mut bytes = Vec::with_capacity(usize::try_from(size).unwrap_or(0));
    BufReader::new(file)
        .take(MAX_FILE_BYTES + 1)
        .read_to_end(&mut bytes)?;
    decode(&bytes)
}

pub fn decode(bytes: &[u8]) -> Result<DynamicImage, Error> {
    if bytes.len() as u64 > MAX_FILE_BYTES {
        return Err(Error::ImageTooLarge);
    }
    let mut reader = ImageReader::new(std::io::Cursor::new(bytes)).with_guessed_format()?;
    reader.limits(limits());
    reader.decode().map_err(Error::from)
}

/// Scales, crops and rotates `img` to exactly `width` x `height`.
#[must_use]
pub fn prepare(
    img: &DynamicImage,
    width: u32,
    height: u32,
    fit: Fit,
    rotation: Rotation,
) -> RgbImage {
    let img = match rotation {
        Rotation::R0 => img.clone(),
        Rotation::R90 => img.rotate90(),
        Rotation::R180 => img.rotate180(),
        Rotation::R270 => img.rotate270(),
    };
    let rgb = img.to_rgb8();
    match fit {
        Fit::Stretch => imageops::resize(&rgb, width, height, FilterType::CatmullRom),
        Fit::Fill => {
            let scaled =
                DynamicImage::ImageRgb8(rgb).resize_to_fill(width, height, FilterType::CatmullRom);
            scaled.to_rgb8()
        }
        Fit::Fit => {
            let scaled = DynamicImage::ImageRgb8(rgb)
                .resize(width, height, FilterType::CatmullRom)
                .to_rgb8();
            let mut canvas = RgbImage::new(width, height);
            let x = i64::from((width - scaled.width()) / 2);
            let y = i64::from((height - scaled.height()) / 2);
            imageops::replace(&mut canvas, &scaled, x, y);
            canvas
        }
    }
}

pub fn encode_jpeg(img: &RgbImage, quality: u8) -> Result<Vec<u8>, Error> {
    if !(1..=100).contains(&quality) {
        return Err(Error::InvalidQuality(quality));
    }
    let mut out = Vec::new();
    JpegEncoder::new_with_quality(&mut out, quality).encode_image(img)?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{ImageFormat, Rgb};

    fn wide() -> DynamicImage {
        let mut img = RgbImage::from_pixel(200, 100, Rgb([255, 0, 0]));
        img.put_pixel(0, 0, Rgb([0, 255, 0]));
        DynamicImage::ImageRgb8(img)
    }

    #[test]
    fn every_fit_produces_screen_size() {
        for fit in [Fit::Fill, Fit::Fit, Fit::Stretch] {
            let out = prepare(&wide(), 480, 480, fit, Rotation::R0);
            assert_eq!(out.dimensions(), (480, 480), "{fit:?}");
        }
    }

    #[test]
    fn fit_adds_black_bars() {
        let out = prepare(&wide(), 480, 480, Fit::Fit, Rotation::R0);
        assert_eq!(out.get_pixel(240, 10), &Rgb([0, 0, 0]));
        assert_eq!(out.get_pixel(240, 240)[0], 255);
    }

    #[test]
    fn fill_has_no_bars() {
        let out = prepare(&wide(), 480, 480, Fit::Fill, Rotation::R0);
        assert!(out.get_pixel(240, 2)[0] > 200);
    }

    #[test]
    fn rotation_from_degrees() {
        assert_eq!(Rotation::from_degrees(270).unwrap(), Rotation::R270);
        assert!(Rotation::from_degrees(45).is_err());
    }

    #[test]
    fn rotate_90_turns_wide_into_tall() {
        let out = prepare(&wide(), 100, 200, Fit::Stretch, Rotation::R90);
        assert_eq!(out.dimensions(), (100, 200));
    }

    #[test]
    fn jpeg_roundtrip() {
        let img = prepare(&wide(), 480, 480, Fit::Fill, Rotation::R0);
        let jpg = encode_jpeg(&img, 90).unwrap();
        assert_eq!(&jpg[..2], &[0xFF, 0xD8]);
        let back = decode(&jpg).unwrap();
        assert_eq!((back.width(), back.height()), (480, 480));
        assert!(encode_jpeg(&img, 0).is_err());
    }

    #[test]
    fn oversized_dimensions_are_refused() {
        // A real 1x(MAX+1) PNG: tiny file, but the header declares a huge image.
        let img = image::GrayImage::new(1, MAX_DIMENSION + 1);
        let mut png = Vec::new();
        img.write_to(&mut std::io::Cursor::new(&mut png), ImageFormat::Png)
            .unwrap();
        assert!(decode(&png).is_err());
    }

    #[test]
    fn garbage_is_refused() {
        assert!(decode(b"not an image").is_err());
    }
}
