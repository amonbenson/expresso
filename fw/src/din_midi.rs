use defmt::warn;
use embassy_futures::select::{Either, select};
use embassy_stm32::{
    mode::Async,
    usart::{Uart, UartRx, UartTx},
};
use expresso::midi::{
    DecodeResult, DinMidiDecoder, DinMidiEncoder, MidiDecoder, MidiEncoder, MidiEndpoint,
};

use crate::{InMsgSender, MsgReceiver, collector::Collector};

type ByteCollector<const N: usize> = Collector<N, u8>;

#[embassy_executor::task]
pub async fn task(uart: Uart<'static, Async>, from_router: MsgReceiver, to_router: InMsgSender) {
    let (tx, rx) = uart.split();

    match select(rx_loop(rx, to_router), tx_loop(tx, from_router)).await {
        Either::First(_) => warn!("DIN MIDI RX loop exited"),
        Either::Second(_) => warn!("DIN MIDI TX loop exited"),
    }
}

async fn rx_loop(mut rx: UartRx<'static, Async>, to_router: InMsgSender) {
    let mut buffer = [0u8; 64];
    let mut decoder = DinMidiDecoder::<64>::new();

    loop {
        match rx.read_until_idle(&mut buffer).await {
            Ok(len) => {
                for &byte in &buffer[..len] {
                    if let Some(DecodeResult::Message(msg)) = decoder.feed(byte) {
                        if to_router.try_send((msg, MidiEndpoint::Din)).is_err() {
                            warn!("DIN MIDI RX: channel full, message dropped");
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
        let mut buffer = ByteCollector::<4>::new();
        let _ = encoder.emit(&message, &mut buffer);
        if tx.write(&buffer.items()).await.is_err() {
            warn!("DIN MIDI TX: write error");
        }
    }
}
