use expresso::midi::PacketSink;

pub struct Collector<const N: usize, T>
where
    T: Copy + Default,
{
    data: [T; N],
    length: usize,
}

impl<const N: usize, T> Collector<N, T>
where
    T: Copy + Default,
{
    pub fn new() -> Self {
        Self {
            data: [T::default(); N],
            length: 0,
        }
    }

    pub fn push(&mut self, item: T) -> Result<(), T> {
        if self.length == N {
            return Err(item);
        }

        self.data[self.length] = item;
        self.length += 1;

        Ok(())
    }

    pub fn clear(&mut self) {
        self.length = 0;
    }

    pub fn get(&self, index: usize) -> &T {
        &self.data[index]
    }

    pub fn items(&self) -> &[T] {
        &self.data[..self.length]
    }

    pub fn len(&self) -> usize {
        self.length
    }
}

impl<const N: usize, T> PacketSink for Collector<N, T>
where
    T: Copy + Default,
{
    type Packet = T;
    type Error = core::convert::Infallible;

    fn emit(&mut self, item: T) -> Result<(), Self::Error> {
        self.push(item).ok();
        Ok(())
    }
}
