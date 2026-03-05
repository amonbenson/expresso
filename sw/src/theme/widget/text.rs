use iced::theme::Theme;
use iced::widget::text::{IntoFragment, Style, Text};

pub fn text<'a, Renderer>(text: impl IntoFragment<'a>) -> Text<'a, Theme, Renderer>
where
    Renderer: iced::advanced::text::Renderer,
{
    Text::new(text).style(|theme: &Theme| Style {
        color: theme.extended_palette().background.base.text.into(),
    })
}

pub fn primary_text<'a, Renderer>(text: impl IntoFragment<'a>) -> Text<'a, Theme, Renderer>
where
    Renderer: iced::advanced::text::Renderer,
{
    Text::new(text).size(16).style(|theme: &Theme| Style {
        color: theme.extended_palette().primary.base.color.into(),
    })
}
