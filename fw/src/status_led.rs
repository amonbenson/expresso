use embassy_futures::select::{Either, select};
use embassy_stm32::gpio::OutputType;
use embassy_stm32::peripherals::{PB0, PB4, PB5, TIM3};
use embassy_stm32::time::Hertz;
use embassy_stm32::timer::low_level::CountingMode;
use embassy_stm32::timer::simple_pwm::{PwmPin, SimplePwm};
use embassy_stm32::Peri;
use embassy_time::{Duration, Ticker};

use crate::types::{StatusEvent, StatusReceiver};

// --------------------------------------------------------------------------
// Timing
// --------------------------------------------------------------------------

const UPDATE_HZ: u64 = 50;

// Number of update ticks a trigger flash stays lit (100 ms at 50 Hz).
const FLASH_TICKS: u32 = 5;

// --------------------------------------------------------------------------
// Color
// --------------------------------------------------------------------------

#[derive(Clone, Copy, Default)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Color {
    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }

    pub const BLACK: Self = Self::new(0, 0, 0);

    /// Blend two colors by taking the per-channel maximum.
    pub fn max_mix(self, other: Self) -> Self {
        Self {
            r: self.r.max(other.r),
            g: self.g.max(other.g),
            b: self.b.max(other.b),
        }
    }
}

// --------------------------------------------------------------------------
// Color palette
// --------------------------------------------------------------------------

const COLOR_POWER: Color = Color::new(10, 8, 3); // dim warm white
const COLOR_USB_CONNECTED: Color = Color::new(0, 20, 80); // blue
const COLOR_MIDI_USB: Color = Color::new(0, 120, 0); // green
const COLOR_MIDI_DIN: Color = Color::new(120, 80, 0); // amber
const COLOR_MIDI_EXP: Color = Color::new(0, 80, 100); // cyan
const COLOR_SETTINGS: Color = Color::new(100, 100, 100); // white

// --------------------------------------------------------------------------
// Animation
//
// Currently only Flash is implemented.  Future variants (DoubleFlash, Pulse,
// …) can be added here without changing LedState — each trigger slot simply
// stores an Animation and the task drives it via `tick()`.
// --------------------------------------------------------------------------

#[derive(Clone, Copy)]
pub enum Animation {
    /// Stay lit for `total` ticks, then go dark.
    Flash { color: Color, remaining: u32 },
}

impl Animation {
    fn flash(color: Color) -> Self {
        Self::Flash {
            color,
            remaining: FLASH_TICKS,
        }
    }

    /// Advance by one tick.  Returns `true` while the animation is still active.
    fn tick(&mut self) -> bool {
        match self {
            Self::Flash { remaining, .. } => {
                if *remaining > 0 {
                    *remaining -= 1;
                }
                *remaining > 0
            }
        }
    }

    fn color(&self) -> Color {
        match self {
            Self::Flash { color, remaining } if *remaining > 0 => *color,
            _ => Color::BLACK,
        }
    }
}

// --------------------------------------------------------------------------
// Trigger slot indices
// --------------------------------------------------------------------------

const TRIG_MIDI_USB_IN: usize = 0;
const TRIG_MIDI_USB_OUT: usize = 1;
const TRIG_MIDI_DIN_IN: usize = 2;
const TRIG_MIDI_DIN_OUT: usize = 3;
const TRIG_MIDI_EXP: usize = 4;
const TRIG_SETTINGS: usize = 5;
const NUM_TRIGGERS: usize = 6;

const TRIGGER_COLORS: [Color; NUM_TRIGGERS] = [
    COLOR_MIDI_USB, // in
    COLOR_MIDI_USB, // out
    COLOR_MIDI_DIN, // in
    COLOR_MIDI_DIN, // out
    COLOR_MIDI_EXP,
    COLOR_SETTINGS,
];

// --------------------------------------------------------------------------
// LED state
// --------------------------------------------------------------------------

struct LedState {
    power: bool,
    usb_connected: bool,
    triggers: [Option<Animation>; NUM_TRIGGERS],
}

impl LedState {
    fn new() -> Self {
        Self {
            power: false,
            usb_connected: false,
            triggers: [None; NUM_TRIGGERS],
        }
    }

    fn apply(&mut self, event: StatusEvent) {
        match event {
            StatusEvent::Power(on) => self.power = on,
            StatusEvent::UsbConnected(on) => self.usb_connected = on,
            StatusEvent::MidiUsbIn => self.arm(TRIG_MIDI_USB_IN),
            StatusEvent::MidiUsbOut => self.arm(TRIG_MIDI_USB_OUT),
            StatusEvent::MidiDinIn => self.arm(TRIG_MIDI_DIN_IN),
            StatusEvent::MidiDinOut => self.arm(TRIG_MIDI_DIN_OUT),
            StatusEvent::MidiExpression => self.arm(TRIG_MIDI_EXP),
            StatusEvent::SettingsUpdate => self.arm(TRIG_SETTINGS),
        }
    }

    fn arm(&mut self, slot: usize) {
        self.triggers[slot] = Some(Animation::flash(TRIGGER_COLORS[slot]));
    }

    /// Advance all active animations by one tick.
    fn tick(&mut self) {
        for slot in &mut self.triggers {
            if let Some(anim) = slot {
                if !anim.tick() {
                    *slot = None;
                }
            }
        }
    }

    /// Compute the final blended color from all active sources.
    fn color(&self) -> Color {
        let mut color = Color::BLACK;
        if self.power {
            color = color.max_mix(COLOR_POWER);
        }
        if self.usb_connected {
            color = color.max_mix(COLOR_USB_CONNECTED);
        }
        for slot in &self.triggers {
            if let Some(anim) = slot {
                color = color.max_mix(anim.color());
            }
        }
        color
    }
}

// --------------------------------------------------------------------------
// PWM helpers
// --------------------------------------------------------------------------

fn duty_from_brightness(brightness: u8, max: u32) -> u32 {
    brightness as u32 * max / 255
}

fn apply_color(pwm: &mut SimplePwm<'static, TIM3>, max: u32, color: Color) {
    pwm.ch1().set_duty_cycle(duty_from_brightness(color.r, max));
    pwm.ch2().set_duty_cycle(duty_from_brightness(color.g, max));
    pwm.ch3().set_duty_cycle(duty_from_brightness(color.b, max));
}

// --------------------------------------------------------------------------
// Task
//
// Pin assignment (reassignable — must not conflict with other peripherals):
//   PB4  →  TIM3_CH1  →  R
//   PB5  →  TIM3_CH2  →  G
//   PB0  →  TIM3_CH3  →  B
// --------------------------------------------------------------------------

#[embassy_executor::task]
pub async fn task(
    tim: Peri<'static, TIM3>,
    pin_r: Peri<'static, PB4>,
    pin_g: Peri<'static, PB5>,
    pin_b: Peri<'static, PB0>,
    events: StatusReceiver,
) {
    let ch_r = PwmPin::new(pin_r, OutputType::PushPull);
    let ch_g = PwmPin::new(pin_g, OutputType::PushPull);
    let ch_b = PwmPin::new(pin_b, OutputType::PushPull);

    let mut pwm = SimplePwm::new(
        tim,
        Some(ch_r),
        Some(ch_g),
        Some(ch_b),
        None,
        Hertz(1_000),
        CountingMode::EdgeAlignedUp,
    );

    let max = pwm.ch1().max_duty_cycle();
    pwm.ch1().enable();
    pwm.ch2().enable();
    pwm.ch3().enable();

    let mut state = LedState::new();
    let mut ticker = Ticker::every(Duration::from_hz(UPDATE_HZ));

    loop {
        // Either a new event arrives or a timer tick fires.
        match select(events.receive(), ticker.next()).await {
            Either::First(event) => state.apply(event),
            Either::Second(_) => state.tick(),
        }

        apply_color(&mut pwm, max, state.color());
    }
}
