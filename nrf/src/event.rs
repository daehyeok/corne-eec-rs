use embassy_sync::blocking_mutex::raw::NoopRawMutex;
use embassy_sync::channel::{Channel, Receiver, Sender};
use serde::{Deserialize, Serialize};

const EVENT_CHANNEL_SIZE: usize = 20;

pub type EventChannel = Channel<NoopRawMutex, Event, EVENT_CHANNEL_SIZE>;
pub type EventReceiver<'a> = Receiver<'a, NoopRawMutex, Event, EVENT_CHANNEL_SIZE>;
pub type EventSender<'a> = Sender<'a, NoopRawMutex, Event, EVENT_CHANNEL_SIZE>;

pub type KeyberonEvent = keyberon::layout::Event;

#[derive(Serialize, Deserialize, defmt::Format, Debug, Copy, Clone, PartialEq, Eq, Default)]
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

    pub fn serialize(self) -> Result<[u8; 3], ()> {
        match self {
            Event::KeyRelease(i, j) => Ok([0, i, j]),
            Event::KeyPress(i, j) => Ok([1, i, j]),
            _ => Err(()),
        }
    }

    pub fn deserialize(buf: &[u8; 3]) -> Result<Self, ()> {
        match buf[0] {
            0 => Ok(Event::KeyRelease(buf[1], buf[2])),
            1 => Ok(Event::KeyPress(buf[1], buf[2])),
            _ => Err(()),
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
