#![no_std]

mod framed_serial_reader;

use crate::framed_serial_reader::FramedReader;
use heapless::consts::U128;
use heapless::spsc::Queue;
use byteorder::ByteOrder;

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
    pub fn dequeue(&mut self) -> Option<u8> {
        // Called with lock.
        self.in_bytes.dequeue()
    }

    #[inline]
    pub fn send_msg(&mut self, m: SerializedMsg) ->Result<(), u8> {
        // Called with lock.
        let frame = &m.buf[0..m.n_bytes];
        let mut lenbuf = [0, 0];
        byteorder::LittleEndian::write_u16(&mut lenbuf, frame.len() as u16);

        for byte in lenbuf.iter() {
            self.tx_queue.enqueue(*byte)?;
        }
        for byte in frame.iter() {
            self.tx_queue.enqueue(*byte)?;
        }
        Ok(())
    }

    #[inline]
    pub fn on_interrupt(&mut self) {
        // This is called inside the interrupt handler and should do as little
        // as possible.

        // Either we are ready to send or have a new byte (or both?)
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

        if self.tx.transmit_enabled() {
            match self.tx_queue.dequeue() {
                Some(byte) => match self.tx.write(byte) {
                    Ok(()) => {},
                    Err(nb::Error::WouldBlock) => panic!("unreachable"), // transmit_enabled() check prevents this
                    Err(nb::Error::Other(_e)) => panic!("unreachable"), // not possible according to function definition
                },
                None => {},
            };
        }
    }
}

pub struct SerializedMsg<'a> {
    buf: &'a [u8],
    n_bytes: usize,
}

#[inline]
pub fn serialize_msg<'a,T: serde::ser::Serialize>(msg: T, buf: &'a mut [u8]) -> SerializedMsg<'a> {
    let n_bytes = ssmarshal::serialize(buf, &msg).unwrap();
    SerializedMsg { buf, n_bytes }
}

pub struct Decoder {
    inner: FramedReader,
}

impl Decoder {
    pub fn new() -> Self {
        Self {inner: FramedReader::new() }
    }

    pub fn consume<T>(&mut self, byte: u8) -> Decoded<T>
        where
            for<'de> T: serde::de::Deserialize<'de>,
    {
        match self.inner.consume(byte) {
            Ok(Some(buf)) => {
                let (msg, _nbytes) = ssmarshal::deserialize(buf).unwrap();
                Decoded::Msg(msg)
            },
            Ok(None) => {
                Decoded::FrameNotYetComplete
            },
            Err(_) => {
                Decoded::Error
            }
        }
    }
}

pub enum Decoded<T> {
    Msg(T),
    FrameNotYetComplete,
    Error,
}
