use iced::advanced::{Widget, layout, renderer};
use iced::{Color, Element, Length, Size, border};

pub struct Knob {
    size: f32,
}

impl Knob {
    pub fn new(size: f32) -> Self {
        Self { size }
    }
}

pub fn knob() -> Knob {
    Knob::new(48.)
}

impl<Message, Theme, Renderer> Widget<Message, Theme, Renderer> for Knob
where
    Renderer: renderer::Renderer,
{
    fn size(&self) -> Size<Length> {
        Size {
            width: Length::Shrink,
            height: Length::Shrink,
        }
    }

    fn layout(
        &mut self,
        _tree: &mut iced::advanced::widget::Tree,
        _renderer: &Renderer,
        _limits: &iced::advanced::layout::Limits,
    ) -> iced::advanced::layout::Node {
        layout::Node::new(Size::new(self.size, self.size))
    }

    fn draw(
        &self,
        _tree: &iced::advanced::widget::Tree,
        renderer: &mut Renderer,
        _theme: &Theme,
        _style: &renderer::Style,
        layout: layout::Layout<'_>,
        _cursor: iced::advanced::mouse::Cursor,
        _viewport: &iced::Rectangle,
    ) {
        renderer.fill_quad(
            renderer::Quad {
                bounds: layout.bounds(),
                border: border::rounded(self.size / 2.),
                ..renderer::Quad::default()
            },
            Color::BLACK,
        );
    }
}

impl<Message, Theme, Renderer> From<Knob> for Element<'_, Message, Theme, Renderer>
where
    Renderer: renderer::Renderer,
{
    fn from(knob: Knob) -> Self {
        Self::new(knob)
    }
}

// Alternative Canvas Implementation (not working though)
// struct KnobProgram;

// impl<Message> canvas::Program<Message> for KnobProgram {
//     type State = ();

//     fn draw(
//         &self,
//         _state: &Self::State,
//         renderer: &Renderer,
//         _theme: &Theme,
//         bounds: Rectangle,
//         _cursor: iced::advanced::mouse::Cursor,
//     ) -> Vec<canvas::Geometry> {
//         let mut frame = canvas::Frame::new(renderer, bounds.size());
//         frame.stroke(
//             &Path::circle(bounds.center(), 24.),
//             canvas::Stroke::default(),
//         );
//         vec![frame.into_geometry()]
//     }
// }
