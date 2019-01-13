use byteorder::ByteOrder;

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

const BUFLEN: usize = 256;

pub(crate) struct FramedReader {
    buf: [u8; BUFLEN],
    state: FramedReaderState,
}

impl FramedReader {
    pub(crate) fn new() -> FramedReader {
        FramedReader {
            buf: [0; BUFLEN],
            state: FramedReaderState::Empty,
        }
    }
    pub(crate) fn consume(&mut self, byte: u8) -> Result<Option<&[u8]>, ()> {
        let (new_state, result) = match self.state {
            FramedReaderState::Empty => (FramedReaderState::ReadingHeader(byte), Ok(None)),
            FramedReaderState::ReadingHeader(byte0) => {
                let buf: [u8; 2] = [byte0, byte];
                let len = ::byteorder::LittleEndian::read_u16(&buf);
                if (len as usize) > BUFLEN {
                    // panic!("len too long");
                    (FramedReaderState::Error, Err(()))
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
                    // frame longer than expected
                    // panic!("idx too large");
                    (FramedReaderState::Error, Err(()))
                }
            }
            FramedReaderState::Error => (FramedReaderState::Error, Err(())),
        };
        self.state = new_state;
        result
    }
}
