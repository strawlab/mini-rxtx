#![no_std]

mod framed_serial_reader;
mod framed_serial_sender;

use stm32nucleo_hal::prelude::*;
use stm32nucleo_hal::stm32f30x::USART2;
use stm32nucleo_hal::serial::{Rx, Tx};
use crate::framed_serial_reader::FramedReader;
use crate::framed_serial_sender::FramedSender;
use heapless::consts::U128;
use heapless::spsc::Queue;

// TODO: make generic over more than USART2
pub struct MiniTxRx {
    rx: Rx<USART2>,
    tx: Tx<USART2>,
    in_bytes: Queue<u8, U128>,
    serial_sender: FramedSender,
}

impl MiniTxRx {
    #[inline]
    pub fn new(
        tx: Tx<USART2>,
        rx: Rx<USART2>,
    ) -> Self {
        let in_bytes = Queue::new();
        let serial_sender = FramedSender::new(Queue::new());
        Self { rx, tx, in_bytes, serial_sender }
    }

    #[inline]
    pub fn dequeue(&mut self) -> Option<u8> {
        // Called with lock.
        self.in_bytes.dequeue()
    }

    #[inline]
    pub fn send_msg(&mut self, m: SerializedMsg) ->Result<(), u8> {
        // Called with lock.
        self.serial_sender.send_frame(&m.buf[0..m.n_bytes])
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
            match self.serial_sender.pump() {
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

pub struct SerializedMsg {
    buf: [u8; 32],
    n_bytes: usize,
}

#[inline]
pub fn serialize_msg<T: serde::ser::Serialize>(msg: T) -> SerializedMsg {
    let mut buf: [u8; 32] = [0; 32];
    let n_bytes = ssmarshal::serialize(&mut buf, &msg).unwrap();
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
