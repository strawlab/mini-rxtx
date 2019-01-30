#![cfg_attr(not(feature = "std"), no_std)]

mod decoder;

pub use crate::decoder::{Decoder, Decoded};
use heapless::consts::U128;
use heapless::spsc::Queue;
use byteorder::ByteOrder;

#[derive(Debug)]
pub enum Error {
    SerializeError,
    TooLong,
    PreviousError,
    Incomplete,
    ExtraCharactersFound,
}

impl From<ssmarshal::Error> for Error {
    fn from(_orig: ssmarshal::Error) -> Error {
        Error::SerializeError
    }
}

pub trait TransmitEnabled {
    fn transmit_enabled(&self) -> bool;
}

pub struct MiniTxRx<RX,TX> {
    rx: RX,
    tx: TX,
    in_bytes: Queue<u8, U128>,
    tx_queue: Queue<u8, U128>,
}

impl<RX,TX> MiniTxRx<RX,TX>
    where
        RX: embedded_hal::serial::Read<u8>,
        TX: embedded_hal::serial::Write<u8> + TransmitEnabled,
{
    #[inline]
    pub fn new(
        tx: TX,
        rx: RX,
    ) -> Self {
        let in_bytes = Queue::new();
        let tx_queue = Queue::new();
        Self { rx, tx, in_bytes, tx_queue }
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
    pub fn send_msg(&mut self, m: SerializedMsg) ->Result<(), u8> {
        // Called with lock.
        let frame = &m.buf[0..m.total_bytes];
        for byte in frame.iter() {
            self.tx_queue.enqueue(*byte)?;
        }
        Ok(())
    }

    #[inline]
    fn pump_sender(&mut self) {
        if self.tx.transmit_enabled() {
            match self.tx_queue.dequeue() {
                Some(byte) => match self.tx.write(byte) {
                    Ok(()) => {},
                    Err(nb::Error::WouldBlock) => panic!("unreachable"), // transmit_enabled() check prevents this
                    Err(nb::Error::Other(_e)) => panic!("unreachable"), // not possible according to function definition
                },
                None => {},
            }
        }
    }

    #[inline]
    pub fn on_interrupt(&mut self) {
        // This is called inside the interrupt handler and should do as little
        // as possible.

        // We have a new byte
        match self.rx.read() {
            Ok(byte) => {
                // iprintln!(&mut resources.ITM.stim[0], "serial got byte {}", byte);
                self.in_bytes.enqueue(byte).expect("failed to enqueue byte");
            },
            Err(nb::Error::WouldBlock) => {}, // do nothing, probably task called because of Txe event
            Err(nb::Error::Other(_e)) => {
                // We have a real error. We should do something here. But what?
            },
        }

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
pub fn serialize_msg<'a,T: serde::ser::Serialize>(msg: &T, buf: &'a mut [u8]) -> Result<SerializedMsg<'a>,Error> {
    let n_bytes = ssmarshal::serialize(&mut buf[2..], msg)?;
    if n_bytes > u16::max_value() as usize {
        return Err(Error::SerializeError);
    }
    byteorder::LittleEndian::write_u16(&mut buf[0..2], n_bytes as u16);
    Ok(SerializedMsg { buf, total_bytes: n_bytes+2 })
}

/// Encode messages into `Vec<u8>`
///
/// This is not part of MiniTxRx itself because we do not want to require
/// access to resources when encoding bytes.
#[cfg(feature="std")]
pub fn serialize_msg_owned<T: serde::ser::Serialize>(msg: &T) -> Result<Vec<u8>,Error> {
    let mut dest = vec![0; 1024];
    let n_bytes = serialize_msg(msg,&mut dest)?.total_bytes;
    dest.truncate(n_bytes);
    Ok(dest)
}

#[cfg(feature="std")]
pub fn deserialize_owned<T>(buf: &[u8]) -> Result<T,Error>
    where
        for<'de> T: serde::de::Deserialize<'de>,
{
    let mut decode_buf = vec![0; 1024];
    let mut decoder = Decoder::new(&mut decode_buf);

    let mut result: Option<T> = None;

    for char_i in buf {

        if result.is_some() {
            // no more characters allowed
            return Err(Error::ExtraCharactersFound);
        }

        match decoder.consume(*char_i) {
            Decoded::Msg(msg) => {
                result = Some(msg);
            },
            Decoded::FrameNotYetComplete => {},
            Decoded::Error(e) => {
                return Err(e);
            },
        }
    }

    match result {
        Some(m) => Ok(m),
        None => Err(Error::Incomplete),
    }
}
