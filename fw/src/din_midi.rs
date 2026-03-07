use defmt::warn;
use embassy_futures::select::{Either, select};
use embassy_stm32::{
    mode::Async,
    usart::{Uart, UartRx, UartTx},
};
use expresso::midi::{DinMidiDecoder, DinMidiEncoder, MidiDecoder, MidiEncoder, PacketSink};

use crate::{MsgReceiver, MsgSender};

// Local sink that buffers encoded DIN bytes for a single async UART write.
struct ByteBuf<const N: usize> {
    buf: [u8; N],
    len: usize,
}

impl<const N: usize> ByteBuf<N> {
    fn new() -> Self {
        Self { buf: [0; N], len: 0 }
    }
}

impl<const N: usize> PacketSink for ByteBuf<N> {
    type Packet = u8;
    type Error = core::convert::Infallible;

    fn emit(&mut self, byte: u8) -> Result<(), Self::Error> {
        if self.len < N {
            self.buf[self.len] = byte;
            self.len += 1;
        }
        Ok(())
    }
}

#[embassy_executor::task]
pub async fn task(uart: Uart<'static, Async>, from_router: MsgReceiver, to_router: MsgSender) {
    let (tx, rx) = uart.split();

    match select(rx_loop(rx, to_router), tx_loop(tx, from_router)).await {
        Either::First(_) => warn!("DIN MIDI RX loop exited"),
        Either::Second(_) => warn!("DIN MIDI TX loop exited"),
    }
}

async fn rx_loop(mut rx: UartRx<'static, Async>, to_router: MsgSender) {
    let mut buf = [0u8; 64];
    let mut decoder = DinMidiDecoder::<64>::new();

    loop {
        match rx.read_until_idle(&mut buf).await {
            Ok(len) => {
                for &byte in &buf[..len] {
                    if let Some(msg) = decoder.feed(byte) {
                        if let Some(static_msg) = crate::to_static(msg) {
                            if to_router.try_send(static_msg).is_err() {
                                warn!("DIN MIDI RX: channel full, message dropped");
                            }
                        }
                    }
                }
            }
            Err(_) => {
                decoder.reset();
            }
        }
    }
}

async fn tx_loop(mut tx: UartTx<'static, Async>, from_router: MsgReceiver) {
    let mut encoder = DinMidiEncoder;

    loop {
        let message = from_router.receive().await;
        let mut buf = ByteBuf::<4>::new();
        let _ = encoder.emit(&message, &mut buf);
        if tx.write(&buf.buf[..buf.len]).await.is_err() {
            warn!("DIN MIDI TX: write error");
        }
    }
}
