use iced::border::rounded;
use iced::theme::Theme;
use iced::widget::text_input::{Style, TextInput};

use crate::theme::config::RADIUS;

pub fn text_input<'a, Message, Renderer>(
    placeholder: &str,
    value: &str,
) -> TextInput<'a, Message, Theme, Renderer>
where
    Message: Clone,
    Renderer: iced::advanced::text::Renderer,
{
    TextInput::new(placeholder, value).style(|theme: &Theme, _status| Style {
        background: theme.palette().primary.scale_alpha(0.25).into(),
        value: theme.palette().text,
        placeholder: theme.palette().text.scale_alpha(0.5),
        icon: theme.palette().text,
        selection: theme.palette().primary,
        border: rounded(RADIUS),
    })
}
