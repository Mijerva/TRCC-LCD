//! Wire format for the bulk-USB screens. See `docs/PROTOCOL.md`.

use crate::Error;

pub const MAGIC: [u8; 4] = [0x12, 0x34, 0x56, 0x78];
pub const PACKET_LEN: usize = 64;
pub const MAX_PAYLOAD: usize = 4 * 1024 * 1024;

const OFFSET_MODEL: usize = 4;
const MODEL_MAX_LEN: usize = 16;
const OFFSET_PM: usize = 24;
const OFFSET_SUB: usize = 36;
const OFFSET_HANDSHAKE_FLAG: usize = 56;
const OFFSET_MODE: usize = 56;
const OFFSET_PAYLOAD_LEN: usize = 60;
const FRAME_MODE: u32 = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Encoding {
    Jpeg,
}

impl Encoding {
    fn command(self) -> u32 {
        match self {
            Encoding::Jpeg => 2,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Panel {
    pub width: u32,
    pub height: u32,
    pub encoding: Encoding,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HandshakeReply {
    pub model: String,
    pub pm: u8,
    pub sub: u8,
}

impl HandshakeReply {
    /// Only profiles confirmed on real hardware are accepted, so an unknown
    /// screen fails loudly instead of being driven at a guessed resolution.
    pub fn panel(&self) -> Result<Panel, Error> {
        match (self.pm, self.sub) {
            (3, _) => Ok(Panel {
                width: 480,
                height: 480,
                encoding: Encoding::Jpeg,
            }),
            (pm, sub) => Err(Error::UnsupportedPanel { pm, sub }),
        }
    }
}

#[must_use]
pub fn handshake_request() -> [u8; PACKET_LEN] {
    let mut p = [0u8; PACKET_LEN];
    p[..4].copy_from_slice(&MAGIC);
    p[OFFSET_HANDSHAKE_FLAG] = 1;
    p
}

/// Parses the device's handshake reply. The reply is untrusted input.
pub fn parse_handshake(reply: &[u8]) -> Result<HandshakeReply, Error> {
    if reply.len() < PACKET_LEN {
        return Err(Error::BadHandshake("reply too short"));
    }
    if reply[..4] != MAGIC {
        return Err(Error::BadHandshake("wrong magic"));
    }
    let pm = reply[OFFSET_PM];
    if pm == 0 {
        return Err(Error::BadHandshake("device reported no panel"));
    }
    let model: String = reply[OFFSET_MODEL..OFFSET_MODEL + MODEL_MAX_LEN]
        .iter()
        .take_while(|&&b| b != 0)
        .map(|&b| {
            if b.is_ascii_graphic() {
                char::from(b)
            } else {
                '?'
            }
        })
        .collect();
    Ok(HandshakeReply {
        model,
        pm,
        sub: reply[OFFSET_SUB],
    })
}

pub fn frame_header(panel: Panel, payload_len: usize) -> Result<[u8; PACKET_LEN], Error> {
    if payload_len == 0 || payload_len > MAX_PAYLOAD {
        return Err(Error::PayloadSize(payload_len));
    }
    let len = u32::try_from(payload_len).map_err(|_| Error::PayloadSize(payload_len))?;
    let mut h = [0u8; PACKET_LEN];
    h[..4].copy_from_slice(&MAGIC);
    h[4..8].copy_from_slice(&panel.encoding.command().to_le_bytes());
    h[8..12].copy_from_slice(&panel.width.to_le_bytes());
    h[12..16].copy_from_slice(&panel.height.to_le_bytes());
    h[OFFSET_MODE..OFFSET_MODE + 4].copy_from_slice(&FRAME_MODE.to_le_bytes());
    h[OFFSET_PAYLOAD_LEN..].copy_from_slice(&len.to_le_bytes());
    Ok(h)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reply(model: &[u8], pm: u8, sub: u8) -> Vec<u8> {
        let mut r = vec![0u8; PACKET_LEN];
        r[..4].copy_from_slice(&MAGIC);
        r[4..4 + model.len()].copy_from_slice(model);
        r[OFFSET_PM] = pm;
        r[OFFSET_SUB] = sub;
        r
    }

    const PANEL_480: Panel = Panel {
        width: 480,
        height: 480,
        encoding: Encoding::Jpeg,
    };

    #[test]
    fn handshake_request_layout() {
        let p = handshake_request();
        assert_eq!(&p[..4], &MAGIC);
        assert_eq!(p[56], 1);
        assert_eq!(p.iter().filter(|&&b| b != 0).count(), 5);
    }

    #[test]
    fn parses_known_reply() {
        let r = parse_handshake(&reply(b"SSCRM-V1", 3, 1)).unwrap();
        assert_eq!(
            r,
            HandshakeReply {
                model: "SSCRM-V1".into(),
                pm: 3,
                sub: 1
            }
        );
        assert_eq!(r.panel().unwrap(), PANEL_480);
    }

    #[test]
    fn rejects_bad_replies() {
        assert!(parse_handshake(&[0u8; 10]).is_err());
        let mut bad_magic = reply(b"X", 3, 1);
        bad_magic[0] = 0;
        assert!(parse_handshake(&bad_magic).is_err());
        assert!(parse_handshake(&reply(b"X", 0, 0)).is_err());
    }

    #[test]
    fn unknown_panel_is_refused() {
        let r = parse_handshake(&reply(b"X", 9, 0)).unwrap();
        assert!(matches!(
            r.panel(),
            Err(Error::UnsupportedPanel { pm: 9, sub: 0 })
        ));
    }

    #[test]
    fn model_string_is_sanitized() {
        let r = parse_handshake(&reply(b"AB\x1b[2J\xff", 3, 1)).unwrap();
        assert_eq!(r.model, "AB?[2J?");
    }

    #[test]
    fn frame_header_layout() {
        let h = frame_header(PANEL_480, 21_076).unwrap();
        assert_eq!(&h[..4], &MAGIC);
        assert_eq!(u32::from_le_bytes(h[4..8].try_into().unwrap()), 2);
        assert_eq!(u32::from_le_bytes(h[8..12].try_into().unwrap()), 480);
        assert_eq!(u32::from_le_bytes(h[12..16].try_into().unwrap()), 480);
        assert!(h[16..56].iter().all(|&b| b == 0));
        assert_eq!(u32::from_le_bytes(h[56..60].try_into().unwrap()), 2);
        assert_eq!(u32::from_le_bytes(h[60..64].try_into().unwrap()), 21_076);
    }

    #[test]
    fn frame_header_rejects_bad_sizes() {
        assert!(frame_header(PANEL_480, 0).is_err());
        assert!(frame_header(PANEL_480, MAX_PAYLOAD + 1).is_err());
    }
}
