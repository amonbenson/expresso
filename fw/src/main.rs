#![no_std]
#![no_main]

pub mod collector;
mod config;
mod din_midi;
mod expression;
mod router;
pub mod types;
mod usb_midi;

use core::cell::RefCell;
use embassy_executor::Spawner;
use embassy_stm32::adc::{Adc, AdcChannel, AdcConfig};
use embassy_stm32::gpio::{Level, Output, Speed};
use embassy_stm32::rcc::mux::{Adcsel, Clk48sel};
use embassy_stm32::rcc::{
    Hse, HseMode, Pll, PllMul, PllPDiv, PllPreDiv, PllQDiv, PllRDiv, PllSource,
};
use embassy_stm32::time::Hertz;
use embassy_stm32::usart::Uart;
use embassy_stm32::{Config, bind_interrupts, peripherals, usart, usb};
use embassy_sync::blocking_mutex::Mutex;
use embassy_sync::channel::Channel;
use expresso::settings::Settings;
use static_cell::StaticCell;
use types::*;

use {defmt_rtt as _, panic_probe as _};

bind_interrupts!(struct Irqs {
    USB_LP => usb::InterruptHandler<peripherals::USB>;
    USART1 => usart::InterruptHandler<peripherals::USART1>;
});

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

static TO_ROUTER: InMsgChannel = Channel::new();

static ROUTER_TO_USB: MsgChannel = Channel::new();
static ROUTER_TO_DIN: MsgChannel = Channel::new();

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    let mut config = Config::default();
    // Enable external crystal oscillator at 16 MHz
    config.rcc.hse = Some(Hse {
        freq: Hertz(16_000_000),
        mode: HseMode::Oscillator,
    });
    // Configure PLL: HSE 16MHz -> /1 -> ×12 = 192 MHz VCO -> /4 = 48 MHz for ADC12
    config.rcc.pll = Some(Pll {
        source: PllSource::HSE,
        prediv: PllPreDiv::DIV1,
        mul: PllMul::MUL12,
        divp: Some(PllPDiv::DIV4),
        divq: Some(PllQDiv::DIV4), // 192MHz / 4 = 48MHz -> USB
        divr: Some(PllRDiv::DIV2), // 192MHz / 2 = 96MHz -> PLLCLK (unused)
    });
    config.rcc.mux.adc12sel = Adcsel::PLL1_P;
    config.rcc.mux.clk48sel = Clk48sel::PLL1_Q;

    let p = embassy_stm32::init(config);

    let mut led_boot = Output::new(p.PB12, Level::Low, Speed::Low);
    let mut led_init = Output::new(p.PB13, Level::Low, Speed::Low);

    static SETTINGS: StaticCell<SettingsMutex> = StaticCell::new();
    let settings = SETTINGS.init(Mutex::new(RefCell::new(Settings::default())));

    led_boot.set_high();

    // USB MIDI
    let (usb_dev, midi_class) = usb_midi::build(p.USB, p.PA12, p.PA11);
    spawner.spawn(usb_midi::device_task(usb_dev)).unwrap();
    spawner
        .spawn(usb_midi::io_task(
            midi_class,
            ROUTER_TO_USB.receiver(),
            TO_ROUTER.sender(),
            settings,
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
        p.PB6, // TODO: Only for rev 1.0, revert to PA9 later
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
            TO_ROUTER.sender(),
        ))
        .unwrap();

    // Expression Pedals
    // Jack pin mapping:
    //   Jack 0 — PA0 (V_ring), PA1 (V_sleeve) -> ADC1
    //   Jack 1 — PA2 (V_ring), PA3 (V_sleeve) -> ADC1
    //   Jack 2 — PA4 (V_ring), PA5 (V_sleeve) -> ADC2
    //   Jack 3 — PA6 (V_ring), PA7 (V_sleeve) -> ADC2
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
            TO_ROUTER.sender(),
            settings,
        ))
        .unwrap();

    // Router
    spawner
        .spawn(router::task(
            TO_ROUTER.receiver(),
            ROUTER_TO_USB.sender(),
            ROUTER_TO_DIN.sender(),
            settings,
        ))
        .unwrap();

    led_init.set_high();

    core::future::pending::<()>().await
}
