use core::{default::Default, num::TryFromIntError};
use defmt::*;
use eck_rs::event::Event;
use embassy_stm32::usart::{self, RxDma, TxDma, UartRx, UartTx};
use embedded_io::blocking::ReadExactError;
use static_cell::StaticCell;

use crate::event_channel::EventSender;

const RING_BUFFER_SIZE: usize = 255;
const MSG_BUFFER_SIZE: usize = 255;
static RX_RING_BUFFER: StaticCell<[u8; RING_BUFFER_SIZE]> = StaticCell::new();

#[derive(Debug, Clone)]
pub enum CommError {
    Usart(usart::Error),
    UsartRead(ReadExactError<usart::Error>),
    Serialize(postcard::Error),
    TryFromIntError(TryFromIntError),
    NotImplemented,
    Invailed,
}

impl From<usart::Error> for CommError {
    fn from(err: usart::Error) -> Self {
        Self::Usart(err)
    }
}

impl From<ReadExactError<usart::Error>> for CommError {
    fn from(err: ReadExactError<usart::Error>) -> Self {
        Self::UsartRead(err)
    }
}

impl From<postcard::Error> for CommError {
    fn from(err: postcard::Error) -> Self {
        Self::Serialize(err)
    }
}

impl From<TryFromIntError> for CommError {
    fn from(err: TryFromIntError) -> Self {
        Self::TryFromIntError(err)
    }
}

#[derive(Default, Debug, Clone, PartialEq, Eq)]
enum ReadState {
    #[default]
    Header,
    Type,
    TxIdx,
    RxIdx,
}

// index will not excess 0xff, Use it as a header.
const HEADER_BYTE: u8 = 0xff;

#[derive(Default)]
struct ReadStateMachine {
    state: ReadState,
    buf: [u8; 3],
}

impl ReadStateMachine {
    fn push(&mut self, byte: u8) -> Result<Option<Event>, CommError> {
        if self.state != ReadState::Header && byte == HEADER_BYTE {
            self.state = ReadState::Header;
            return Err(CommError::Invailed);
        }

        match self.state {
            ReadState::Header => {
                self.state = ReadState::Type;
            }
            ReadState::Type => {
                self.state = ReadState::TxIdx;
                self.buf[0] = byte
            }
            ReadState::TxIdx => {
                self.state = ReadState::RxIdx;
                self.buf[1] = byte
            }
            ReadState::RxIdx => {
                self.state = ReadState::Header;
                return match self.buf[0] {
                    0 => Ok(Some(Event::KeyPress(self.buf[1], byte))),
                    1 => Ok(Some(Event::KeyRelease(self.buf[1], byte))),
                    _ => Err(CommError::NotImplemented),
                };
            }
        }

        Ok(None)
    }
}

pub async fn receive<'a, T: usart::BasicInstance, DMA: RxDma<T>>(
    event_sender: EventSender<'a>,
    rx: UartRx<'a, T, DMA>,
) {
    info!("Start UART read task.");
    let uart_buf = RX_RING_BUFFER.init([0u8; RING_BUFFER_SIZE]);
    let mut uart_rx = rx.into_ring_buffered(uart_buf);
    if let Err(e) = uart_rx.start() {
        error!("UART start error: {:?}", e);
    };
    let mut state_machine = ReadStateMachine::default();
    loop {
        let mut buf = [0u8; MSG_BUFFER_SIZE];
        let res = uart_rx.read(&mut buf).await;
        if let Err(e) = res {
            error!("UART read error: {:?}", e);
            continue;
        }
        let len = res.unwrap();
        for byte in buf.iter().take(len) {
            match state_machine.push(*byte) {
                Ok(Some(e)) => event_sender.send(e).await,
                Ok(None) => {}
                Err(e) => error!("Failed to deserialzed. - {:?}", defmt::Debug2Format(&e)),
            }
        }
    }
}

// send event to other halve.
pub async fn send<'a, T: usart::BasicInstance, DMA: TxDma<T>>(
    e: &Event,
    uart_tx: &mut UartTx<'a, T, DMA>,
) -> Result<(), CommError> {
    match e {
        Event::KeyPress(i, j) => uart_tx.write(&[HEADER_BYTE, 0, *i, *j]).await?,
        Event::KeyRelease(i, j) => uart_tx.write(&[HEADER_BYTE, 1, *i, *j]).await?,
        _ => return Err(CommError::NotImplemented),
    }

    Ok(())
}
