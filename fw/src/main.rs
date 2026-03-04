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
use embassy_stm32::{bind_interrupts, peripherals, usart, usb, Config};
use midi::MidiChan;
use {defmt_rtt as _, panic_probe as _};

bind_interrupts!(struct Irqs {
    USB_LP => usb::InterruptHandler<peripherals::USB>;
    USART1 => usart::InterruptHandler<peripherals::USART1>;
});

static MIDI_BUS: MidiChan = MidiChan::new();
static TO_USB: MidiChan = MidiChan::new();
static TO_DIN: MidiChan = MidiChan::new();

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    let p = embassy_stm32::init(Config::default());

    info!("Expresso firmware starting");

    // Router: consumes MIDI_BUS, dispatches to TO_USB / TO_DIN.
    spawner
        .spawn(router::task(
            MIDI_BUS.receiver(),
            TO_USB.sender(),
            TO_DIN.sender(),
        ))
        .unwrap();

    // USB MIDI: bidirectional: handles outgoing USB_MIDI messages and posts incoming messages to the main MIDI_BUS
    spawner
        .spawn(usb_midi::task(
            p.USB,
            p.PA12,
            p.PA11,
            TO_USB.receiver(),
            MIDI_BUS.sender(),
        ))
        .unwrap();

    // DIN MIDI: bidirectional: handles outgoing DIN_MIDI messages and posts incoming messages to the main MIDI_BUS
    spawner
        .spawn(din_midi::task(
            p.USART1,
            p.PA9,
            p.PA10,
            p.DMA1_CH4,
            p.DMA1_CH5,
            TO_DIN.receiver(),
            MIDI_BUS.sender(),
        ))
        .unwrap();

    // Expression pedals: producer only: posts incoming messages to the main MIDI_BUS
    spawner
        .spawn(expression::task(
            p.ADC1,
            p.PA0, p.PA1,
            p.PA2, p.PA3,
            p.ADC2,
            p.PA4, p.PA5,
            p.PA6, p.PA7,
            MIDI_BUS.sender(),
        ))
        .unwrap();
}
