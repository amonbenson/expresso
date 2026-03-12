use crate::midi::PacketSink;

pub struct CollectSink<T, const N: usize> {
    pub buf: [Option<T>; N],
    len: usize,
}

impl<T: Copy, const N: usize> CollectSink<T, N> {
    pub fn new() -> Self {
        Self {
            buf: [None; N],
            len: 0,
        }
    }

    pub fn get(&self, i: usize) -> T {
        self.buf[i].unwrap()
    }

    pub fn len(&self) -> usize {
        self.len
    }
}

#[derive(Debug)]
pub struct SinkFullError;

impl<T: Copy, const N: usize> PacketSink for CollectSink<T, N> {
    type Packet = T;
    type Error = SinkFullError;

    fn emit(&mut self, packet: T) -> Result<(), SinkFullError> {
        if self.len >= N {
            return Err(SinkFullError);
        }
        self.buf[self.len] = Some(packet);
        self.len += 1;
        Ok(())
    }
}
