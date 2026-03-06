use crate::midi::MidiMessageSink;

#[derive(Debug)]
pub enum ComponentError<E, S: MidiMessageSink> {
    Component(E),
    Sink(S::Error),
}

impl<E, S: MidiMessageSink> ComponentError<E, S> {
    pub fn map_component<F, E2>(self, f: F) -> ComponentError<E2, S>
    where
        F: FnOnce(E) -> E2,
    {
        match self {
            ComponentError::Component(e) => ComponentError::Component(f(e)),
            ComponentError::Sink(e) => ComponentError::Sink(e),
        }
    }
}

pub type ComponentResult<T, E, S> = Result<T, ComponentError<E, S>>;
