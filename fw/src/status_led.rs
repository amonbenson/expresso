use embassy_futures::select::{Either, select};
use embassy_stm32::Peri;
use embassy_stm32::gpio::OutputType;
use embassy_stm32::peripherals::{PB0, PB4, PB5, TIM3};
use embassy_stm32::time::Hertz;
use embassy_stm32::timer::low_level::CountingMode;
use embassy_stm32::timer::simple_pwm::{PwmPin, SimplePwm};
use embassy_time::{Duration, Ticker};
use expresso::settings::Color;
use expresso::status::StatusState;

use crate::types::{SettingsMutex, StatusSubscriber};

// --------------------------------------------------------------------------
// Timing
// --------------------------------------------------------------------------

const UPDATE_HZ: u64 = 50;

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
    mut events: StatusSubscriber,
    settings: &'static SettingsMutex,
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

    let mut state = StatusState::new();
    let mut ticker = Ticker::every(Duration::from_hz(UPDATE_HZ));

    loop {
        let led_settings = settings.lock(|s| s.borrow().status);

        match select(events.next_message_pure(), ticker.next()).await {
            Either::First(event) => state.apply(event, &led_settings),
            Either::Second(_) => state.tick(),
        }

        apply_color(&mut pwm, max, state.color(&led_settings));
    }
}
