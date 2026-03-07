#![no_std]
#![no_main]

mod config;
mod din_midi;
mod expression;
mod router;
mod usb_midi;

use embassy_executor::Spawner;
use embassy_stm32::adc::{Adc, AdcChannel, AdcConfig};
use embassy_stm32::gpio::{Level, Output, Speed};
use embassy_stm32::rcc::{Hse, HseMode};
use embassy_stm32::time::Hertz;
use embassy_stm32::usart::Uart;
use embassy_stm32::{Config, bind_interrupts, peripherals, usart, usb};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::{Channel, Receiver, Sender};
use expresso::midi::MidiMessage;

use {defmt_rtt as _, panic_probe as _};

bind_interrupts!(struct Irqs {
    USB_LP => usb::InterruptHandler<peripherals::USB>;
    USART1 => usart::InterruptHandler<peripherals::USART1>;
});

// ---- Channel types ----

const MSG_CAP: usize = 16;
pub type MsgChannel = Channel<CriticalSectionRawMutex, MidiMessage<'static>, MSG_CAP>;
pub type MsgSender = Sender<'static, CriticalSectionRawMutex, MidiMessage<'static>, MSG_CAP>;
pub type MsgReceiver = Receiver<'static, CriticalSectionRawMutex, MidiMessage<'static>, MSG_CAP>;

// ---- Static channels ----

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

static USB_TO_ROUTER: MsgChannel = Channel::new();
static DIN_TO_ROUTER: MsgChannel = Channel::new();
static EXP_TO_ROUTER: MsgChannel = Channel::new();

static ROUTER_TO_USB: MsgChannel = Channel::new();
static ROUTER_TO_DIN: MsgChannel = Channel::new();

// ---- Helper: lift a borrowed MidiMessage into a 'static one ----
// Sysex borrows its payload; all other variants own their data.
pub fn to_static(msg: MidiMessage<'_>) -> Option<MidiMessage<'static>> {
    match msg {
        MidiMessage::NoteOn { channel, note, velocity } => {
            Some(MidiMessage::NoteOn { channel, note, velocity })
        }
        MidiMessage::NoteOff { channel, note, velocity } => {
            Some(MidiMessage::NoteOff { channel, note, velocity })
        }
        MidiMessage::ControlChange { channel, control, value } => {
            Some(MidiMessage::ControlChange { channel, control, value })
        }
        MidiMessage::ProgramChange { channel, program } => {
            Some(MidiMessage::ProgramChange { channel, program })
        }
        MidiMessage::PitchBend { channel, value } => {
            Some(MidiMessage::PitchBend { channel, value })
        }
        MidiMessage::Sysex(_) => None,
    }
}

// ---- Entry point ----

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

    // USB MIDI
    let (usb_dev, midi_class) = usb_midi::build(p.USB, p.PA12, p.PA11);
    spawner.spawn(usb_midi::device_task(usb_dev)).unwrap();
    spawner
        .spawn(usb_midi::io_task(
            midi_class,
            ROUTER_TO_USB.receiver(),
            USB_TO_ROUTER.sender(),
        ))
        .unwrap();

    // DIN MIDI
    let uart_config = {
        let mut c = usart::Config::default();
        c.baudrate = 31250;
        c
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

    // Expression Pedals
    // Jack pin mapping:
    //   Jack 0 — PA0 (V_ring), PA1 (V_sleeve) → ADC1
    //   Jack 1 — PA2 (V_ring), PA3 (V_sleeve) → ADC1
    //   Jack 2 — PA4 (V_ring), PA5 (V_sleeve) → ADC2
    //   Jack 3 — PA6 (V_ring), PA7 (V_sleeve) → ADC2
    let adc1 = Adc::new(p.ADC1, AdcConfig::default());
    let adc2 = Adc::new(p.ADC2, AdcConfig::default());
    spawner
        .spawn(expression::task(
            adc1,
            adc2,
            [
                (p.PA0.degrade_adc(), p.PA1.degrade_adc()),
                (p.PA2.degrade_adc(), p.PA3.degrade_adc()),
            ],
            [
                (p.PA4.degrade_adc(), p.PA5.degrade_adc()),
                (p.PA6.degrade_adc(), p.PA7.degrade_adc()),
            ],
            EXP_TO_ROUTER.sender(),
        ))
        .unwrap();

    // Router
    spawner
        .spawn(router::task(
            USB_TO_ROUTER.receiver(),
            DIN_TO_ROUTER.receiver(),
            EXP_TO_ROUTER.receiver(),
            ROUTER_TO_USB.sender(),
            ROUTER_TO_DIN.sender(),
        ))
        .unwrap();

    led_init.set_high();

    core::future::pending::<()>().await
}
