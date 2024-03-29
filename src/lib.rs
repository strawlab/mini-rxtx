#![cfg_attr(not(feature = "std"), no_std)]

mod decoder;

#[cfg(feature = "std")]
pub use crate::decoder::StdDecoder;
pub use crate::decoder::{Decoded, Decoder};

use byteorder::ByteOrder;
use heapless::spsc::Queue;

#[derive(Debug)]
#[cfg_attr(feature = "std", derive(thiserror::Error))]
pub enum Error {
    #[cfg_attr(feature = "std", error("serialization failed"))]
    SerializeError(ssmarshal::Error),
    #[cfg_attr(feature = "std", error("too long"))]
    TooLong,
    #[cfg_attr(feature = "std", error("already experienced an error previously"))]
    PreviousError,
    #[cfg_attr(feature = "std", error("incomplete"))]
    Incomplete,
    #[cfg_attr(feature = "std", error("extra characters found"))]
    ExtraCharactersFound,
}

#[cfg(feature = "std")]
fn _test_error_is_std() {
    // Compile-time test to ensure Error implements std::error::Error trait.
    fn implements<T: std::error::Error>() {}
    implements::<Error>();
}

impl From<ssmarshal::Error> for Error {
    fn from(orig: ssmarshal::Error) -> Error {
        Error::SerializeError(orig)
    }
}

pub struct MiniTxRx<RX, TX, const RX_SIZE: usize, const TX_SIZE: usize> {
    rx: RX,
    tx: TX,
    in_bytes: Queue<u8, RX_SIZE>,
    tx_queue: Queue<u8, TX_SIZE>,
    held_byte: Option<u8>,
}

impl<RX, TX, const RX_SIZE: usize, const TX_SIZE: usize> MiniTxRx<RX, TX, RX_SIZE, TX_SIZE>
where
    RX: embedded_hal::serial::Read<u8>,
    TX: embedded_hal::serial::Write<u8>,
{
    #[inline]
    pub fn new(tx: TX, rx: RX) -> Self {
        Self {
            rx,
            tx,
            in_bytes: Queue::new(),
            tx_queue: Queue::new(),
            held_byte: None,
        }
    }

    #[inline]
    pub fn pump(&mut self) -> Option<u8> {
        // Called with lock.

        // Pump the output queue
        self.pump_sender();

        // Pump the input queue
        self.in_bytes.dequeue()
    }

    #[inline]
    pub fn send_msg(&mut self, m: SerializedMsg) -> Result<(), u8> {
        // Called with lock.
        let frame = &m.buf[0..m.total_bytes];
        for byte in frame.iter() {
            self.tx_queue.enqueue(*byte)?;
        }
        Ok(())
    }

    // inner function called by pump_sender
    fn send_byte(&mut self, byte: u8) {
        debug_assert!(self.held_byte.is_none());
        match self.tx.write(byte) {
            Ok(()) => {}
            Err(nb::Error::WouldBlock) => self.held_byte = Some(byte),
            Err(nb::Error::Other(_e)) => panic!("unreachable"), // not possible according to function definition
        }
    }

    fn pump_sender(&mut self) {
        if let Some(byte) = self.held_byte.take() {
            self.send_byte(byte)
        }
        if self.held_byte.is_none() {
            match self.tx_queue.dequeue() {
                Some(byte) => self.send_byte(byte),
                None => {}
            }
        }
    }

    #[inline]
    pub fn on_interrupt(&mut self) -> Result<(), RX::Error> {
        // This is called inside the interrupt handler and should do as little
        // as possible.

        // We have a new byte
        match self.rx.read() {
            Ok(byte) => {
                #[cfg(feature = "print-defmt")]
                defmt::trace!("got byte {}", byte);
                self.in_bytes.enqueue(byte).expect("failed to enqueue byte");
            }
            Err(nb::Error::WouldBlock) => {} // do nothing, probably task called because of Txe event
            Err(nb::Error::Other(e)) => {
                return Err(e);
            }
        }
        Ok(())
    }

    pub fn rx(&mut self) -> &mut RX {
        &mut self.rx
    }

    pub fn tx(&mut self) -> &mut TX {
        &mut self.tx
    }
}

pub struct SerializedMsg<'a> {
    buf: &'a [u8],
    total_bytes: usize,
}

impl<'a> SerializedMsg<'a> {
    pub fn framed_slice(&self) -> &[u8] {
        &self.buf[0..self.total_bytes]
    }
}

/// Encode messages into a byte buffer.
///
/// This is not part of MiniTxRx itself because we do not want to require
/// access to resources when encoding bytes.
#[inline]
pub fn serialize_msg<'a, T: serde::ser::Serialize>(
    msg: &T,
    buf: &'a mut [u8],
) -> Result<SerializedMsg<'a>, Error> {
    let n_bytes = ssmarshal::serialize(&mut buf[2..], msg)?;
    if n_bytes > u16::max_value() as usize {
        return Err(Error::TooLong);
    }
    byteorder::LittleEndian::write_u16(&mut buf[0..2], n_bytes as u16);
    Ok(SerializedMsg {
        buf,
        total_bytes: n_bytes + 2,
    })
}

/// Encode messages into `Vec<u8>`
///
/// This is not part of MiniTxRx itself because we do not want to require
/// access to resources when encoding bytes.
#[cfg(feature = "std")]
pub fn serialize_msg_owned<T: serde::ser::Serialize>(msg: &T) -> Result<Vec<u8>, Error> {
    let mut dest = vec![0; 1024];
    let n_bytes = serialize_msg(msg, &mut dest)?.total_bytes;
    dest.truncate(n_bytes);
    Ok(dest)
}

pub fn deserialize_owned_borrowed<T>(buf: &[u8], decode_buf: &mut [u8]) -> Result<T, Error>
where
    for<'de> T: serde::de::Deserialize<'de>,
{
    let mut decoder = Decoder::new(decode_buf);

    let mut result: Option<T> = None;

    for char_i in buf {
        if result.is_some() {
            // no more characters allowed
            return Err(Error::ExtraCharactersFound);
        }

        match decoder.consume(*char_i) {
            Decoded::Msg(msg) => {
                result = Some(msg);
            }
            Decoded::FrameNotYetComplete => {}
            Decoded::Error(e) => {
                return Err(e);
            }
        }
    }

    match result {
        Some(m) => Ok(m),
        None => Err(Error::Incomplete),
    }
}

#[cfg(feature = "std")]
pub fn deserialize_owned<T>(buf: &[u8]) -> Result<T, Error>
where
    for<'de> T: serde::de::Deserialize<'de>,
{
    let mut decoder = StdDecoder::new(1024);

    let mut result: Option<T> = None;

    for char_i in buf {
        if result.is_some() {
            // no more characters allowed
            return Err(Error::ExtraCharactersFound);
        }

        match decoder.consume(*char_i) {
            Decoded::Msg(msg) => {
                result = Some(msg);
            }
            Decoded::FrameNotYetComplete => {}
            Decoded::Error(e) => {
                return Err(e);
            }
        }
    }

    match result {
        Some(m) => Ok(m),
        None => Err(Error::Incomplete),
    }
}
