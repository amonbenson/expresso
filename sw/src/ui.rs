use iced::widget::{Column, column, row};
use iced::{Center, Element, Fill};
use num_traits::{Bounded, Num, NumAssignOps};
use std::fmt::Display;
use std::ops::RangeInclusive;
use std::str::FromStr;
use strum::VariantArray;

use crate::device_config::{ChannelConfig, InputMode};
use crate::theme::config::SPACING;
use crate::theme::widget::{pick_list, primary_text, text, text_input};

pub fn labeled_knob<'a, Message: Clone + 'a, T, F>(
    label: &'a str,
    value: &'a T,
    range: RangeInclusive<T>,
    on_change: F,
) -> Column<'a, Message>
where
    T: Num + NumAssignOps + PartialOrd + Ord + Display + FromStr + Clone + Bounded,
    F: Fn(T) -> Message + Copy + 'static,
{
    column![
        text(label).align_x(Center),
        text_input("", &value.to_string())
            .on_input(move |s| {
                let value: T = s.parse().unwrap_or(T::zero()); // parse string value
                let value = value.max(range.start().clone()).min(range.end().clone()); // clamp to range
                on_change(value)
            })
            .width(64)
    ]
    .spacing(SPACING / 2.)
    .align_x(Center)
    .width(Fill)
}

pub fn channel_strip<'a, Message: Clone + 'a>(
    channel_index: usize,
    channel: &'a ChannelConfig,
    on_change: impl Fn(ChannelConfig) -> Message + Copy + 'static,
) -> Element<'a, Message> {
    let channel_clone = channel.clone();

    column![
        primary_text((channel_index + 1).to_string()).size(36),
        pick_list(
            InputMode::VARIANTS,
            Some(&channel.input.mode),
            move |value| on_change(channel_clone.with_input_mode(value)),
        )
        .width(Fill),
        // row![
        //     button("C")
        //         .on_press_with(move || on_change(channel_clone.with_input_mode(InputMode::Continuous)))
        //         .style(if channel.input.mode == InputMode::Continuous { button::primary } else { button::text }),
        //     button("S")
        //         .on_press_with(move || on_change(channel_clone.with_input_mode(InputMode::Switch)))
        //         .style(if channel.input.mode == InputMode::Switch { button::primary } else { button::text }),
        //     button("M→T")
        //         .on_press_with(move || on_change(channel_clone.with_input_mode(InputMode::MomentaryAsToggle)))
        //         .style(if channel.input.mode == InputMode::MomentaryAsToggle { button::primary } else { button::text }),
        //     button("T→M")
        //         .on_press_with(move || on_change(channel_clone.with_input_mode(InputMode::ToggleAsMomentary)))
        //         .style(if channel.input.mode == InputMode::ToggleAsMomentary { button::primary } else { button::text }),
        // ],
        match channel.input.mode {
            InputMode::Continuous => column![
                row![
                    labeled_knob(
                        "Minimum\nInput",
                        &channel.input.continuous.minimum_input,
                        0..=127,
                        move |value| on_change(channel_clone.with_minimum_input(value)),
                    ),
                    labeled_knob(
                        "Maximum\nInput",
                        &channel.input.continuous.maximum_input,
                        0..=127,
                        move |value| on_change(channel_clone.with_maximum_input(value)),
                    ),
                ]
                .spacing(SPACING)
                .align_y(Center)
                .width(Fill),
                row![
                    labeled_knob(
                        "Minimum\nOutput",
                        &channel.input.continuous.minimum_output,
                        0..=127,
                        move |value| on_change(channel_clone.with_minimum_output(value)),
                    ),
                    labeled_knob(
                        "Maximum\nOutput",
                        &channel.input.continuous.maximum_output,
                        0..=127,
                        move |value| on_change(channel_clone.with_maximum_output(value)),
                    ),
                ]
                .spacing(SPACING)
                .align_y(Center)
                .width(Fill),
                labeled_knob(
                    "Drive",
                    &channel.input.continuous.drive,
                    0..=127,
                    move |value| on_change(channel_clone.with_drive(value)),
                ),
            ],
            _ => column![
                row![
                    labeled_knob(
                        "Released\nValue",
                        &channel.input.switch.released_value,
                        0..=127,
                        move |value| on_change(channel_clone.with_released_value(value)),
                    ),
                    labeled_knob(
                        "Pressed\nValue",
                        &channel.input.switch.pressed_value,
                        0..=127,
                        move |value| on_change(channel_clone.with_pressed_value(value)),
                    ),
                ]
                .spacing(SPACING)
                .align_y(Center)
                .width(Fill)
            ],
        }
        .spacing(SPACING)
        .align_x(Center)
        .width(Fill)
        .height(Fill),
        labeled_knob("CC", &channel.cc, 0..=127, move |value| on_change(
            channel_clone.with_cc(value)
        ),),
        text_input("Label", channel.label_str())
            .on_input(move |label_str| on_change(channel_clone.with_label_str(&label_str)))
            .width(Fill),
    ]
    .spacing(SPACING)
    .align_x(Center)
    .width(200)
    .height(Fill)
    .into()
}
