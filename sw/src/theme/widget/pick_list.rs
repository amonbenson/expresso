use iced::border::rounded;
use iced::theme::Theme;
use iced::widget::pick_list::{PickList, Style};
use std::borrow::Borrow;

use crate::theme::config::RADIUS;

pub fn pick_list<'a, T, L, V, Message, Renderer>(
    options: L,
    selected: Option<V>,
    on_selected: impl Fn(T) -> Message + 'a,
) -> PickList<'a, T, L, V, Message, Theme, Renderer>
where
    T: ToString + PartialEq + Clone + 'a,
    L: Borrow<[T]> + 'a,
    V: Borrow<T> + 'a,
    Message: Clone,
    Renderer: iced::advanced::text::Renderer,
{
    PickList::new(options, selected, on_selected).style(|theme: &Theme, _status| Style {
        text_color: theme.extended_palette().primary.base.text,
        background: theme.extended_palette().primary.base.color.into(),
        placeholder_color: theme.extended_palette().primary.base.text.scale_alpha(0.5),
        handle_color: theme.extended_palette().primary.base.text,
        border: rounded(RADIUS),
    })
}
