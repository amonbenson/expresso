use defmt::info;
use embassy_stm32::{peripherals, Peri};

use crate::midi::{MidiReceiver, MidiSender};

#[embassy_executor::task]
pub async fn task(
    usart: Peri<'static, peripherals::USART1>,
    tx_pin: Peri<'static, peripherals::PA9>,
    rx_pin: Peri<'static, peripherals::PA10>,
    tx_dma: Peri<'static, peripherals::DMA1_CH4>,
    rx_dma: Peri<'static, peripherals::DMA1_CH5>,
    from_router: MidiReceiver<'static>,
    to_bus: MidiSender<'static>,
) {
    let _ = (usart, tx_pin, rx_pin, tx_dma, rx_dma, from_router, to_bus);

    info!("DIN MIDI task started");

    // TODO: Initialise USART1 at 31 250 bps and enter the TX/RX loop.
    //       Rough structure:
    //
    //   let mut config = UartConfig::default();
    //   config.baudrate = crate::config::DIN_MIDI_BAUD;
    //   let uart = Uart::new(usart, rx_pin, tx_pin, crate::Irqs,
    //                        tx_dma, rx_dma, config).unwrap();
    //   let (mut tx, mut rx) = uart.split();
    //   join(din_tx_loop(&mut tx, from_router),
    //        din_rx_loop(&mut rx, to_bus)).await;

    core::future::pending::<()>().await;
}
