#![no_std]
#![no_main]

mod config;
mod din_midi;
mod expression;
mod midi;
mod router;
mod usb_midi;

use defmt::info;
use embassy_executor::Spawner;
use embassy_stm32::gpio::{Level, Output, Speed};
use embassy_stm32::rcc::{Hse, HseMode};
use embassy_stm32::time::Hertz;
use embassy_stm32::{Config, bind_interrupts, peripherals, usart, usb};
use midi::MidiEventChannel;
use {defmt_rtt as _, panic_probe as _};

bind_interrupts!(struct Irqs {
    USB_LP  => usb::InterruptHandler<peripherals::USB>;
    USART1  => usart::InterruptHandler<peripherals::USART1>;
});

static MIDI_BUS: MidiEventChannel = MidiEventChannel::new();
static TO_USB:   MidiEventChannel = MidiEventChannel::new();
static TO_DIN:   MidiEventChannel = MidiEventChannel::new();

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    // Clock configuration:
    //   - System clock: HSI 16 MHz (default)
    //   - HSE: 16 MHz external oscillator (on PF0/PF1)
    //   - USB clock: HSI48 48 MHz via CRS (default for G4, sync_from_usb enabled by default)
    let mut config = Config::default();
    config.rcc.hse = Some(Hse {
        freq: Hertz(16_000_000),
        mode: HseMode::Oscillator,
    });

    let p = embassy_stm32::init(config);

    // PB12 (GPIO_LED1): on immediately after boot to confirm firmware is alive.
    let _led_boot = Output::new(p.PB12, Level::High, Speed::Low);

    info!("Expresso firmware starting");

    // USB MIDI
    let (usb_dev, midi_class) = usb_midi::build(p.USB, p.PA12, p.PA11);
    spawner.spawn(usb_midi::device_task(usb_dev)).unwrap();
    spawner.spawn(usb_midi::io_task(midi_class, TO_USB.receiver(), MIDI_BUS.sender())).unwrap();

    // Router: consumes MIDI_BUS, dispatches to TO_USB / TO_DIN.
    spawner
        .spawn(router::task(MIDI_BUS.receiver(), TO_USB.sender(), TO_DIN.sender()))
        .unwrap();

    // // DIN MIDI: bidirectional bridge between the 5-pin DIN jack and the MIDI bus.
    // let din_midi = din_midi::DinMidi::new(din_midi::DinMidiConfig {
    //     usart:  p.USART1,
    //     tx_pin: p.PA9,
    //     rx_pin: p.PA10,
    //     tx_dma: p.DMA1_CH4,
    //     rx_dma: p.DMA1_CH5,
    // });
    // spawner
    //     .spawn(din_midi::task(din_midi, TO_DIN.receiver(), MIDI_BUS.sender()))
    //     .unwrap();

    // // Expression pedals: producer-only, samples all four TRS jacks via ADC.
    // // Jack pin mapping (from expresso.ioc):
    // //   Jack 0 — PA0 (V_tip), PA1 (V_sleeve) → ADC1,  CC 11 (Expression)
    // //   Jack 1 — PA2 (V_tip), PA3 (V_sleeve) → ADC1,  CC  1 (Modulation)
    // //   Jack 2 — PA4 (V_tip), PA5 (V_sleeve) → ADC2,  CC  7 (Volume)
    // //   Jack 3 — PA6 (V_tip), PA7 (V_sleeve) → ADC2,  CC 74 (Brightness)
    // use embassy_stm32::adc::AdcChannel;
    // use expression::ExpressionDriver;
    // let expression = ExpressionDriver::new(expression::ExpressionConfig {
    //     adc1: p.ADC1,
    //     adc2: p.ADC2,
    //     adc1_channels: [
    //         expression::ExpressionChannelConfig { v_tip: p.PA0.degrade_adc(), v_sleeve: p.PA1.degrade_adc(), cc: 11 },
    //         expression::ExpressionChannelConfig { v_tip: p.PA2.degrade_adc(), v_sleeve: p.PA3.degrade_adc(), cc: 1  },
    //     ],
    //     adc2_channels: [
    //         expression::ExpressionChannelConfig { v_tip: p.PA4.degrade_adc(), v_sleeve: p.PA5.degrade_adc(), cc: 7  },
    //         expression::ExpressionChannelConfig { v_tip: p.PA6.degrade_adc(), v_sleeve: p.PA7.degrade_adc(), cc: 74 },
    //     ],
    //     midi_channel: 0,
    // });
    // spawner.spawn(expression::expression_task(expression, MIDI_BUS.sender())).unwrap();

    // PB13 (GPIO_LED2): on once all tasks are spawned and init is complete.
    let _led_init = Output::new(p.PB13, Level::High, Speed::Low);

    info!("All tasks spawned, initialisation complete");

    // Keep main task alive so the LED Output guards are not dropped.
    core::future::pending::<()>().await
}
