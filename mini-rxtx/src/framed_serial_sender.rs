use byteorder;
use byteorder::ByteOrder;

use heapless::spsc::Queue;
use heapless::consts::U128;

pub(crate) struct FramedSender {
    inner: Queue<u8, U128>,
}

impl FramedSender {
    pub(crate) const fn new(inner: Queue<u8, U128>) -> FramedSender {
        FramedSender { inner }
    }

    /// called when we know we can send a byte (txe set)
    #[inline]
    pub(crate) fn pump(&mut self) -> Option<u8> {
        self.inner.dequeue()
    }

    #[inline]
    pub(crate) fn send_frame(&mut self, frame: &[u8]) -> Result<(), u8> {
        let mut lenbuf = [0, 0];
        byteorder::LittleEndian::write_u16(&mut lenbuf, frame.len() as u16);

        for byte in lenbuf.iter() {
            self.inner.enqueue(*byte)?;
        }
        for byte in frame.iter() {
            self.inner.enqueue(*byte)?;
        }
        Ok(())
    }

}
