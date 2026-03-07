pub mod din;
pub use din::{DinMidiDecoder, DinMidiEncoder};

pub mod usb;
pub use usb::{UsbMidiDecoder, UsbMidiEncoder};

#[cfg(feature = "embassy")]
mod embassy_impl {
    use super::super::traits::PacketSink;
    use embassy_sync::blocking_mutex::raw::RawMutex;
    use embassy_sync::channel::Sender;

    impl<'ch, M, T, const N: usize> PacketSink for Sender<'ch, M, T, N>
    where
        M: RawMutex,
        T: 'ch,
    {
        type Packet = T;
        type Error = embassy_sync::channel::TrySendError<T>;

        fn emit(&mut self, packet: T) -> Result<(), Self::Error> {
            Sender::try_send(self, packet)
        }
    }
}
