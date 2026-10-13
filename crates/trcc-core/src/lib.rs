//! Core library for Thermalright AIO LCD screens: protocol, image preparation
//! and USB transport.

pub mod config;
pub mod device;
pub mod frames;
pub mod player;
pub mod protocol;
pub mod render;
pub mod usb;

pub use device::{Lcd, Transport};
pub use render::{Fit, Rotation};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("no supported screen found (looking for USB 87AD:70DB)")]
    NotFound,
    #[error("unexpected handshake reply: {0}")]
    BadHandshake(&'static str),
    #[error("this screen is not supported yet (PM {pm}, SUB {sub}); please open a device report")]
    UnsupportedPanel { pm: u8, sub: u8 },
    #[error("frame payload of {0} bytes is outside the allowed range")]
    PayloadSize(usize),
    #[error("payload is not a JPEG image")]
    NotJpeg,
    #[error("USB write stopped after {sent} of {expected} bytes")]
    ShortWrite { sent: usize, expected: usize },
    #[error("image file is too large")]
    ImageTooLarge,
    #[error("rotation must be 0, 90, 180 or 270 degrees, got {0}")]
    InvalidRotation(u16),
    #[error("the GIF has too many frames")]
    TooManyFrames,
    #[error("invalid settings: {0}")]
    Config(String),
    #[error("JPEG quality must be 1 to 100, got {0}")]
    InvalidQuality(u8),
    #[error("could not read the image: {0}")]
    Image(#[from] image::ImageError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("USB error: {0}")]
    Usb(#[from] nusb::Error),
    #[error("USB transfer failed: {0}")]
    Transfer(#[from] nusb::transfer::TransferError),
}
