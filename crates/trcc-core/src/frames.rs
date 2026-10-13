//! Builds the list of ready-to-send JPEG frames for a still image or a GIF.

use std::io::Cursor;
use std::path::Path;
use std::time::Duration;

use image::codecs::gif::GifDecoder;
use image::{AnimationDecoder, DynamicImage, ImageDecoder};

use crate::Error;
use crate::render::{self, Fit, Rotation};

/// Caps that keep a long or crafted GIF from using unbounded memory.
pub const MAX_FRAMES: usize = 600;
const MAX_TOTAL_JPEG_BYTES: usize = 96 * 1024 * 1024;
/// Browsers treat GIF delays under 20 ms as 100 ms; doing the same keeps
/// "0 delay" GIFs from spinning the CPU.
const MIN_DELAY: Duration = Duration::from_millis(20);
const DEFAULT_DELAY: Duration = Duration::from_millis(100);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    pub jpeg: Vec<u8>,
    pub delay: Duration,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameOptions {
    pub width: u32,
    pub height: u32,
    pub fit: Fit,
    pub rotation: Rotation,
    pub quality: u8,
}

pub fn from_file(path: &Path, opts: FrameOptions) -> Result<Vec<Frame>, Error> {
    from_bytes(&render::read_limited(path)?, opts)
}

pub fn from_bytes(bytes: &[u8], opts: FrameOptions) -> Result<Vec<Frame>, Error> {
    if bytes.starts_with(b"GIF8") {
        let frames = gif_frames(bytes, opts)?;
        if !frames.is_empty() {
            return Ok(frames);
        }
    }
    let img = render::decode(bytes)?;
    Ok(vec![Frame {
        jpeg: encode(&img, opts)?,
        delay: Duration::MAX,
    }])
}

fn encode(img: &DynamicImage, opts: FrameOptions) -> Result<Vec<u8>, Error> {
    let prepared = render::prepare(img, opts.width, opts.height, opts.fit, opts.rotation);
    render::encode_jpeg(&prepared, opts.quality)
}

fn gif_frames(bytes: &[u8], opts: FrameOptions) -> Result<Vec<Frame>, Error> {
    let mut decoder = GifDecoder::new(Cursor::new(bytes))?;
    decoder.set_limits(render::limits())?;
    let mut out = Vec::new();
    let mut total = 0usize;
    for frame in decoder.into_frames() {
        if out.len() == MAX_FRAMES {
            return Err(Error::TooManyFrames);
        }
        let frame = frame?;
        let (num, den) = frame.delay().numer_denom_ms();
        let ms = num.checked_div(den).unwrap_or(0);
        let delay = match Duration::from_millis(u64::from(ms)) {
            d if d < MIN_DELAY => DEFAULT_DELAY,
            d => d,
        };
        let jpeg = encode(&DynamicImage::ImageRgba8(frame.into_buffer()), opts)?;
        total += jpeg.len();
        if total > MAX_TOTAL_JPEG_BYTES {
            return Err(Error::ImageTooLarge);
        }
        out.push(Frame { jpeg, delay });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::codecs::gif::GifEncoder;
    use image::{Delay, Rgba, RgbaImage};

    const OPTS: FrameOptions = FrameOptions {
        width: 48,
        height: 48,
        fit: Fit::Fill,
        rotation: Rotation::R0,
        quality: 80,
    };

    fn gif(delays_ms: &[u32]) -> Vec<u8> {
        let mut out = Vec::new();
        {
            let mut enc = GifEncoder::new(&mut out);
            for (i, &ms) in delays_ms.iter().enumerate() {
                let shade = u8::try_from(i * 40 % 255).unwrap();
                let img = RgbaImage::from_pixel(16, 16, Rgba([shade, 0, 0, 255]));
                let f = image::Frame::from_parts(img, 0, 0, Delay::from_numer_denom_ms(ms, 1));
                enc.encode_frame(f).unwrap();
            }
        }
        out
    }

    #[test]
    fn still_image_is_one_frame() {
        let mut png = Vec::new();
        DynamicImage::ImageRgb8(image::RgbImage::new(10, 10))
            .write_to(&mut Cursor::new(&mut png), image::ImageFormat::Png)
            .unwrap();
        let frames = from_bytes(&png, OPTS).unwrap();
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].delay, Duration::MAX);
    }

    #[test]
    fn gif_keeps_frames_and_delays() {
        let frames = from_bytes(&gif(&[50, 120, 300]), OPTS).unwrap();
        let delays: Vec<_> = frames.iter().map(|f| f.delay.as_millis()).collect();
        assert_eq!(delays, [50, 120, 300]);
        assert!(frames.iter().all(|f| f.jpeg.starts_with(&[0xFF, 0xD8])));
    }

    #[test]
    fn tiny_gif_delays_are_slowed_down() {
        let frames = from_bytes(&gif(&[0, 10]), OPTS).unwrap();
        assert!(frames.iter().all(|f| f.delay == DEFAULT_DELAY));
    }

    #[test]
    fn too_many_frames_is_refused() {
        let delays = vec![50; MAX_FRAMES + 1];
        assert!(matches!(
            from_bytes(&gif(&delays), OPTS),
            Err(Error::TooManyFrames)
        ));
    }
}
