#![no_std]
#![no_main]

mod config;
mod din_midi;
mod expression;
mod midi;
mod router;
mod usb_midi;

use embassy_executor::Spawner;
use embassy_stm32::gpio::{Level, Output, Speed};
use embassy_stm32::rcc::{Hse, HseMode};
use embassy_stm32::time::Hertz;
use embassy_stm32::usart::Uart;
use embassy_stm32::{Config, bind_interrupts, peripherals, usart, usb};
use midi::MidiMessageChannel;
use {defmt_rtt as _, panic_probe as _};

bind_interrupts!(struct Irqs {
    USB_LP => usb::InterruptHandler<peripherals::USB>;
    USART1 => usart::InterruptHandler<peripherals::USART1>;
});

// Architecture:
// ┌────────────┐     ┌──────────┐     ┌────────────┐
// │  USB-MIDI  ├────►│          │────►│  USB-MIDI  │
// └────────────┘     │          │     └────────────┘
// ┌────────────┐     │          │     ┌────────────┐
// │  DIN-MIDI  ├────►│  Router  │────►│  DIN-MIDI  │
// └────────────┘     │          │     └────────────┘
// ┌────────────┐     │          │
// │ Expression ├────►│          │
// └────────────┘     └──────────┘

static USB_TO_ROUTER: MidiMessageChannel = MidiMessageChannel::new();
static DIN_TO_ROUTER: MidiMessageChannel = MidiMessageChannel::new();
static EXP_TO_ROUTER: MidiMessageChannel = MidiMessageChannel::new();

static ROUTER_TO_USB: MidiMessageChannel = MidiMessageChannel::new();
static ROUTER_TO_DIN: MidiMessageChannel = MidiMessageChannel::new();

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    let mut config = Config::default();
    config.rcc.hse = Some(Hse {
        freq: Hertz(16_000_000),
        mode: HseMode::Oscillator,
    });

    let p = embassy_stm32::init(config);

    let mut led_boot = Output::new(p.PB12, Level::Low, Speed::Low);
    let mut led_init = Output::new(p.PB13, Level::Low, Speed::Low);

    led_boot.set_high();

    // Usb Midi
    let (usb_dev, midi_class) = usb_midi::build(p.USB, p.PA12, p.PA11);
    spawner.spawn(usb_midi::device_task(usb_dev)).unwrap();
    spawner
        .spawn(usb_midi::io_task(
            midi_class,
            ROUTER_TO_USB.receiver(),
            USB_TO_ROUTER.sender(),
        ))
        .unwrap();

    // Din Midi
    let uart_config = {
        let mut config = usart::Config::default();
        config.baudrate = 31250;
        config
    };
    let uart = Uart::new(
        p.USART1,
        p.PA10,
        p.PA9,
        Irqs,
        p.DMA1_CH4,
        p.DMA1_CH5,
        uart_config,
    )
    .unwrap();
    spawner
        .spawn(din_midi::task(
            uart,
            ROUTER_TO_DIN.receiver(),
            DIN_TO_ROUTER.sender(),
        ))
        .unwrap();

    // Midi Router
    spawner
        .spawn(router::task(
            USB_TO_ROUTER.receiver(),
            DIN_TO_ROUTER.receiver(),
            EXP_TO_ROUTER.receiver(),
            ROUTER_TO_USB.sender(),
            ROUTER_TO_DIN.sender(),
        ))
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

    led_init.set_high();

    // Keep main task alive so the LED Output guards are not dropped.
    core::future::pending::<()>().await
}
