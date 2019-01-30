use byteorder::ByteOrder;

const BUFLEN: usize = 256;

pub enum Decoded<T> {
    Msg(T),
    FrameNotYetComplete,
    Error(crate::Error),
}

/// A struct for decoding bytes.
///
/// This is not part of MiniTxRx itself because we do not want to require
/// access to resources when decoding bytes.
pub struct Decoder {
    buf: [u8; BUFLEN],
    state: FramedReaderState,
}

impl Decoder {
    pub fn new() -> Self {
        Self {
            buf: [0; BUFLEN],
            state: FramedReaderState::Empty,
        }
    }

    pub fn consume<T>(&mut self, byte: u8) -> Decoded<T>
        where
            for<'de> T: serde::de::Deserialize<'de>,
    {
        let (new_state, result) = match self.state {
            FramedReaderState::Empty => (FramedReaderState::ReadingHeader(byte), Ok(None)),
            FramedReaderState::ReadingHeader(byte0) => {
                let buf: [u8; 2] = [byte0, byte];
                let len = ::byteorder::LittleEndian::read_u16(&buf);
                if (len as usize) > BUFLEN {
                    (FramedReaderState::Error, Err(crate::Error::TooLong))
                } else {
                    let rms = ReadingMessageState { len: len, idx: 0 };
                    (FramedReaderState::ReadingMessage(rms), Ok(None))
                }
            }
            FramedReaderState::ReadingMessage(ref rms) => {
                let (msg_len, mut idx) = (rms.len, rms.idx);
                self.buf[idx as usize] = byte;
                idx += 1;
                if idx < msg_len {
                    let rms = ReadingMessageState {
                        len: msg_len,
                        idx: idx,
                    };
                    (FramedReaderState::ReadingMessage(rms), Ok(None))
                } else if idx == msg_len {
                    let result = &self.buf[0..(idx as usize)];
                    (FramedReaderState::Empty, Ok(Some(result)))
                } else {
                    // Frame langer than expected.
                    // Theoretically it is impossible to get here, so we panic.
                    panic!("frame larger than expected");
                }
            }
            FramedReaderState::Error => (FramedReaderState::Error, Err(crate::Error::PreviousError)),
        };
        self.state = new_state;
        match result {
            Ok(Some(buf)) => {
                match ssmarshal::deserialize(buf) {
                    Ok((msg, _nbytes)) => Decoded::Msg(msg),
                    Err(e) => Decoded::Error(e.into()),
                }
            },
            Ok(None) => {
                Decoded::FrameNotYetComplete
            },
            Err(e) => {
                Decoded::Error(e)
            }
        }
    }
}

struct ReadingMessageState {
    len: u16, // the length when full
    idx: u16, // the current length
}

enum FramedReaderState {
    Empty,
    ReadingHeader(u8),
    ReadingMessage(ReadingMessageState),
    Error,
}
