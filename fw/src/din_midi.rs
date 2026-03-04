use defmt::info;
use embassy_stm32::{peripherals, Peri};

use crate::midi::{MidiBridge, MidiReceiver, MidiSender};

// ---------------------------------------------------------------------------
// Config
// ---------------------------------------------------------------------------

/// Construction parameters for [`DinMidi`].
///
/// Pins (from expresso.ioc): PA9 = USART1_TX, PA10 = USART1_RX
pub struct DinMidiConfig {
    pub usart:  Peri<'static, peripherals::USART1>,
    pub tx_pin: Peri<'static, peripherals::PA9>,
    pub rx_pin: Peri<'static, peripherals::PA10>,
    pub tx_dma: Peri<'static, peripherals::DMA1_CH4>,
    pub rx_dma: Peri<'static, peripherals::DMA1_CH5>,
}

// ---------------------------------------------------------------------------
// Driver struct
// ---------------------------------------------------------------------------

/// DIN MIDI (5-pin DIN / TRS) peripheral driver.
///
/// Runs a full-duplex 31 250 bps MIDI link over USART1.
///
/// - **Inbound** (DIN → bus): bytes received on RX are parsed into
///   [`MidiEvent`]s and placed on the shared event bus via `to_bus`.
/// - **Outbound** (bus → DIN): [`MidiEvent`]s delivered by the router via
///   `from_router` are serialised and sent over TX.
pub struct DinMidi {
    usart:  Peri<'static, peripherals::USART1>,
    tx_pin: Peri<'static, peripherals::PA9>,
    rx_pin: Peri<'static, peripherals::PA10>,
    tx_dma: Peri<'static, peripherals::DMA1_CH4>,
    rx_dma: Peri<'static, peripherals::DMA1_CH5>,
}

impl DinMidi {
    pub fn new(config: DinMidiConfig) -> Self {
        Self {
            usart:  config.usart,
            tx_pin: config.tx_pin,
            rx_pin: config.rx_pin,
            tx_dma: config.tx_dma,
            rx_dma: config.rx_dma,
        }
    }
}

// ---------------------------------------------------------------------------
// MidiBridge impl
// ---------------------------------------------------------------------------

impl MidiBridge for DinMidi {
    async fn run(self, from_router: MidiReceiver<'static>, to_bus: MidiSender<'static>) {
        let _ = (self.usart, self.tx_pin, self.rx_pin, self.tx_dma, self.rx_dma, from_router, to_bus);

        info!("DIN MIDI task started");

        // TODO: Initialise USART1 at 31 250 bps and enter the TX/RX loop.
        //       Rough structure:
        //
        //   let mut config = UartConfig::default();
        //   config.baudrate = crate::config::DIN_MIDI_BAUD;
        //   let uart = Uart::new(self.usart, self.rx_pin, self.tx_pin, crate::Irqs,
        //                        self.tx_dma, self.rx_dma, config).unwrap();
        //   let (mut tx, mut rx) = uart.split();
        //   join(din_tx_loop(&mut tx, from_router),
        //        din_rx_loop(&mut rx, to_bus)).await;

        core::future::pending::<()>().await;
    }
}

// ---------------------------------------------------------------------------
// Task
// ---------------------------------------------------------------------------

#[embassy_executor::task]
pub async fn task(
    driver: DinMidi,
    from_router: MidiReceiver<'static>,
    to_bus: MidiSender<'static>,
) {
    driver.run(from_router, to_bus).await;
}
