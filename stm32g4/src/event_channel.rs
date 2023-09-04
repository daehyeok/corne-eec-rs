use eck_rs::event::Event;
use embassy_sync::blocking_mutex::raw::NoopRawMutex;
use embassy_sync::channel::{Channel, Receiver, Sender};
use static_cell::StaticCell;

pub type EventChannel = Channel<NoopRawMutex, Event, EVENT_CHANNEL_SIZE>;
pub type EventReceiver<'a> = Receiver<'a, NoopRawMutex, Event, EVENT_CHANNEL_SIZE>;
pub type EventSender<'a> = Sender<'a, NoopRawMutex, Event, EVENT_CHANNEL_SIZE>;

const EVENT_CHANNEL_SIZE: usize = 20;
static EVENT_CHANNEL: StaticCell<EventChannel> = StaticCell::new();

pub fn init() -> &'static mut EventChannel {
    EVENT_CHANNEL.init(EventChannel::new())
}
