type KeyberonEvent = keyberon::layout::Event;

#[derive(defmt::Format, Debug, Copy, Clone, Default)]
pub enum Event {
    KeyPress(u8, u8),
    KeyRelease(u8, u8),
    #[default]
    None,
}

impl Event {
    pub fn into_keyberon(self) -> Option<KeyberonEvent> {
        match self {
            Event::KeyPress(i, j) => Some(KeyberonEvent::Press(i, j)),
            Event::KeyRelease(i, j) => Some(KeyberonEvent::Release(i, j)),
            _ => None,
        }
    }
}

impl core::convert::From<KeyberonEvent> for Event {
    fn from(e: KeyberonEvent) -> Self {
        match e {
            KeyberonEvent::Press(i, j) => Event::KeyPress(i, j),
            KeyberonEvent::Release(i, j) => Event::KeyRelease(i, j),
        }
    }
}
