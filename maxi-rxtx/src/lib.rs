use std::io;
use byteorder::ByteOrder;
use bytes::BytesMut;
use tokio_io::codec::{Decoder, Encoder};

#[derive(Debug)]
pub enum Error {
    Io(std::io::Error),
    EncodeDecode,
    ParseInt(std::num::ParseIntError),
}

impl From<std::io::Error> for Error {
    fn from(orig: std::io::Error) -> Error {
        Error::Io(orig)
    }
}

impl From<ssmarshal::Error> for Error {
    fn from(_orig: ssmarshal::Error) -> Error {
        Error::EncodeDecode
    }
}

impl From<std::num::ParseIntError> for Error {
    fn from(orig: std::num::ParseIntError) -> Error {
        Error::ParseInt(orig)
    }
}

pub type Result<T> = std::result::Result<T,Error>;

const HEADER_LEN: usize = 2;
struct FrameCodec {}

impl FrameCodec {
    fn new() -> Self {
        Self {}
    }
}

impl Decoder for FrameCodec {
    type Item = Vec<u8>;
    type Error = io::Error;

    fn decode(&mut self, buf: &mut BytesMut) -> io::Result<Option<Self::Item>> {
        // TODO: make more efficient by keeping state. (This is currently quite
        // inefficient, as we re-parse header again and again on each new call.)

        // peek into buffer to get header data
        let mut header: [u8; HEADER_LEN] = [0, 0];
        if let Some(byte1) = buf.get(1) {
            if let Some(byte0) = buf.get(0) { // don't really need checked get() here, but unchecked is not safe rust.
                header[0] = *byte0;
                header[1] = *byte1;
            } else {
                return Ok(None);
            }
        } else {
            return Ok(None);
        }

        // now parse header data
        let data_len = byteorder::LittleEndian::read_u16(&header) as usize;

        // do we have enough data?
        if buf.len() < (data_len + HEADER_LEN) {
            return Ok(None);
        }

        let frame = buf.split_to(HEADER_LEN + data_len); // header and data
        let (_header, data) = frame.split_at(HEADER_LEN);
        Ok(Some(data.to_vec()))
    }
}

impl Encoder for FrameCodec {
    type Item = Vec<u8>;
    type Error = io::Error;

    fn encode(&mut self, data: Self::Item, buf: &mut BytesMut) -> io::Result<()> {
        // peek into buffer to get header data
        let mut header: [u8; HEADER_LEN] = [0, 0];
        byteorder::LittleEndian::write_u16(&mut header, data.len() as u16);
        buf.extend_from_slice(&header);
        buf.extend_from_slice(&data);
        Ok(())
    }
}


/// wrap a FrameCodec into ToDevice and FromDevice types
pub struct MyCodec<FROM, TO> {
    upstream: FrameCodec,
    my_from: std::marker::PhantomData<FROM>,
    my_to: std::marker::PhantomData<TO>,
}

impl<FROM, TO> MyCodec<FROM, TO> {
    pub fn new() -> Self {
        Self {
            upstream: FrameCodec::new(),
            my_from: std::marker::PhantomData,
            my_to: std::marker::PhantomData,
        }
    }
}

impl<FROM, TO> Decoder for MyCodec<FROM, TO>
    where
        for<'de> FROM: serde::de::Deserialize<'de>,
{
    type Item = FROM;
    type Error = Error;

    fn decode(&mut self, buf: &mut BytesMut) -> Result<Option<FROM>> {
        match self.upstream.decode(buf)? {
            Some(data) => {
                match ssmarshal::deserialize(&data) {
                    Ok(msg) => Ok(Some(msg.0)),
                    Err(e) => Err(e.into()),
                }
            },
            None => {
                Ok(None)
            }
        }
    }
}

impl<FROM, TO> Encoder for MyCodec<FROM, TO>
    where
        TO: serde::ser::Serialize,
{
    type Item = TO;
    type Error = Error;

    fn encode(&mut self, item: Self::Item, buf: &mut BytesMut) -> Result<()> {
        let mut data_vec = vec![0; std::mem::size_of::<Self::Item>()];
        match ssmarshal::serialize(&mut data_vec, &item) {
            Ok(_size) => {},
            Err(e) => return Err(e.into()),
        };
        Ok(self.upstream.encode(data_vec,buf)?)
    }
}
