//! Talks to a screen over any byte transport.

use crate::Error;
use crate::protocol::{self, HandshakeReply, PACKET_LEN, Panel};

const CHUNK: usize = 16 * 1024;
const USB_PACKET: usize = 512;

/// One bulk OUT and one bulk IN endpoint. Implemented by the USB backend and
/// by a fake in tests.
pub trait Transport {
    /// Writes all of `data`. An empty slice sends a zero-length packet.
    fn write(&mut self, data: &[u8]) -> Result<(), Error>;
    /// Reads one transfer of at most `max` bytes.
    fn read(&mut self, max: usize) -> Result<Vec<u8>, Error>;
}

pub struct Lcd<T: Transport> {
    transport: T,
    info: HandshakeReply,
    panel: Panel,
}

impl<T: Transport> Lcd<T> {
    pub fn connect(mut transport: T) -> Result<Self, Error> {
        transport.write(&protocol::handshake_request())?;
        let reply = transport.read(USB_PACKET)?;
        let info = protocol::parse_handshake(&reply)?;
        let panel = info.panel()?;
        Ok(Self {
            transport,
            info,
            panel,
        })
    }

    pub fn info(&self) -> &HandshakeReply {
        &self.info
    }

    pub fn panel(&self) -> Panel {
        self.panel
    }

    pub fn send_jpeg(&mut self, jpeg: &[u8]) -> Result<(), Error> {
        if jpeg.len() < 4 || jpeg[..2] != [0xFF, 0xD8] {
            return Err(Error::NotJpeg);
        }
        let header = protocol::frame_header(self.panel, jpeg.len())?;
        let mut wire = Vec::with_capacity(PACKET_LEN + jpeg.len());
        wire.extend_from_slice(&header);
        wire.extend_from_slice(jpeg);
        for chunk in wire.chunks(CHUNK) {
            self.transport.write(chunk)?;
        }
        // A transfer that ends exactly on a packet boundary needs a zero-length
        // packet, or the device keeps waiting for more data.
        if wire.len() % USB_PACKET == 0 {
            self.transport.write(&[])?;
        }
        Ok(())
    }
}

#[cfg(test)]
pub(crate) mod fake {
    use super::*;

    #[derive(Default)]
    pub struct FakeScreen {
        pub writes: Vec<Vec<u8>>,
        pub reply: Vec<u8>,
    }

    impl FakeScreen {
        pub fn new(pm: u8) -> Self {
            let mut reply = vec![0u8; PACKET_LEN];
            reply[..4].copy_from_slice(&protocol::MAGIC);
            reply[4..12].copy_from_slice(b"SSCRM-V1");
            reply[24] = pm;
            reply[36] = 1;
            Self {
                writes: Vec::new(),
                reply,
            }
        }
    }

    impl Transport for &mut FakeScreen {
        fn write(&mut self, data: &[u8]) -> Result<(), Error> {
            self.writes.push(data.to_vec());
            Ok(())
        }
        fn read(&mut self, max: usize) -> Result<Vec<u8>, Error> {
            Ok(self.reply.iter().copied().take(max).collect())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::fake::FakeScreen;
    use super::*;

    fn jpeg(len: usize) -> Vec<u8> {
        let mut j = vec![0u8; len];
        j[0] = 0xFF;
        j[1] = 0xD8;
        j
    }

    #[test]
    fn connect_sends_handshake_and_reads_panel() {
        let mut fake = FakeScreen::new(3);
        let lcd = Lcd::connect(&mut fake).unwrap();
        assert_eq!(lcd.info().model, "SSCRM-V1");
        assert_eq!(lcd.panel().width, 480);
        drop(lcd);
        assert_eq!(fake.writes, vec![protocol::handshake_request().to_vec()]);
    }

    #[test]
    fn connect_refuses_unknown_panel() {
        let mut fake = FakeScreen::new(42);
        assert!(matches!(
            Lcd::connect(&mut fake),
            Err(Error::UnsupportedPanel { .. })
        ));
    }

    #[test]
    fn frame_is_header_plus_payload_in_chunks() {
        let mut fake = FakeScreen::new(3);
        let mut lcd = Lcd::connect(&mut fake).unwrap();
        let payload = jpeg(40_000);
        lcd.send_jpeg(&payload).unwrap();
        drop(lcd);
        let frame: Vec<u8> = fake.writes[1..].concat();
        assert_eq!(frame.len(), PACKET_LEN + payload.len());
        assert_eq!(&frame[PACKET_LEN..], payload.as_slice());
        assert!(fake.writes[1..].iter().all(|w| w.len() <= CHUNK));
        assert!(fake.writes.last().is_some_and(|w| !w.is_empty()));
    }

    #[test]
    fn zero_length_packet_on_exact_boundary() {
        let mut fake = FakeScreen::new(3);
        let mut lcd = Lcd::connect(&mut fake).unwrap();
        lcd.send_jpeg(&jpeg(USB_PACKET * 4 - PACKET_LEN)).unwrap();
        drop(lcd);
        assert!(fake.writes.last().is_some_and(Vec::is_empty));
    }

    #[test]
    fn non_jpeg_payload_is_refused() {
        let mut fake = FakeScreen::new(3);
        let mut lcd = Lcd::connect(&mut fake).unwrap();
        assert!(matches!(lcd.send_jpeg(b"PNG data"), Err(Error::NotJpeg)));
    }
}
