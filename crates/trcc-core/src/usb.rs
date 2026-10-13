//! USB backend using the `WinUSB` driver Windows binds to the screen.

use std::time::Duration;

use nusb::transfer::{Buffer, Bulk, In, Out};
use nusb::{Endpoint, MaybeFuture};

use crate::Error;
use crate::device::Transport;

pub const VENDOR_ID: u16 = 0x87AD;
pub const PRODUCT_ID: u16 = 0x70DB;
const INTERFACE: u8 = 0;
const EP_OUT: u8 = 0x01;
const EP_IN: u8 = 0x81;
const WRITE_TIMEOUT: Duration = Duration::from_secs(5);
const READ_TIMEOUT: Duration = Duration::from_secs(1);
const READ_LEN: usize = 512;

pub struct UsbTransport {
    out: Endpoint<Bulk, Out>,
    inp: Endpoint<Bulk, In>,
}

impl UsbTransport {
    /// Opens the first connected supported screen.
    pub fn open() -> Result<Self, Error> {
        let info = nusb::list_devices()
            .wait()?
            .find(|d| d.vendor_id() == VENDOR_ID && d.product_id() == PRODUCT_ID)
            .ok_or(Error::NotFound)?;
        let device = info.open().wait()?;
        let interface = device.claim_interface(INTERFACE).wait()?;
        Ok(Self {
            out: interface.endpoint::<Bulk, Out>(EP_OUT)?,
            inp: interface.endpoint::<Bulk, In>(EP_IN)?,
        })
    }
}

impl Transport for UsbTransport {
    fn write(&mut self, data: &[u8]) -> Result<(), Error> {
        let done = self
            .out
            .transfer_blocking(Buffer::from(data), WRITE_TIMEOUT);
        done.status?;
        if done.actual_len != data.len() {
            return Err(Error::ShortWrite {
                sent: done.actual_len,
                expected: data.len(),
            });
        }
        Ok(())
    }

    fn read(&mut self, max: usize) -> Result<Vec<u8>, Error> {
        let done = self
            .inp
            .transfer_blocking(Buffer::new(READ_LEN), READ_TIMEOUT);
        let buf = done.into_result()?;
        let mut v = buf.into_vec();
        v.truncate(max);
        Ok(v)
    }
}
