use defmt::*;
use eck_rs::event::Event;
use embassy_stm32::usart::{self, RxDma, TxDma, UartRx, UartTx};
use embedded_io::blocking::ReadExactError;
use static_cell::StaticCell;

use crate::{
    event_channel::EventSender,
    layers::{COLS, ROWS},
};

const RING_BUFFER_SIZE: usize = 255;
const MSG_BUFFER_SIZE: usize = 255;
static RX_RING_BUFFER: StaticCell<[u8; RING_BUFFER_SIZE]> = StaticCell::new();

#[derive(Debug, Clone)]
pub enum CommError {
    Usart(usart::Error),
    UsartRead(ReadExactError<usart::Error>),
    DecodeEvent,
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

// index will not excess 0xff, Use it as a header.
const HEADER_BYTE: u8 = 0xff;

fn encode_event(e: &Event) -> Option<u8> {
    match e {
        Event::KeyPress(i, j) => Some(*i * COLS as u8 + *j),
        Event::KeyRelease(i, j) => Some(0x80 | (*i * COLS as u8 + *j)),
        _ => None,
    }
}

fn decode_event(k: u8) -> Result<Event, CommError> {
    let index = k & 0x7F;

    if index >= (COLS * ROWS) as u8 {
        error!("Received index to large: {:?}", k);
        return Err(CommError::DecodeEvent);
    }
    let i = index / COLS as u8;
    let j = index % COLS as u8;

    if (k & 0x80) == 0 {
        Ok(Event::KeyPress(i, j))
    } else {
        Ok(Event::KeyRelease(i, j))
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

    let mut wait_header = true;
    loop {
        let mut buf = [0u8; 1];
        let res = uart_rx.read(&mut buf).await;
        if let Err(e) = res {
            error!("UART read error: {:?}", e);
            continue;
        }
        let len = res.unwrap();
        for b in buf.iter().take(len) {
            debug!("USART received byte {:?}", b);
            if !wait_header && *b != HEADER_BYTE {
                match decode_event(*b) {
                    Ok(e) => {
                        debug!("USART received Event {:?}", e);
                        event_sender.send(e).await
                    }
                    Err(_) => error!("Decode error"),
                }
                wait_header = true;
            }
            if *b == HEADER_BYTE {
                wait_header = false;
                continue;
            }
        }
    }
}

// send event to other halve.
pub async fn send<'a, T: usart::BasicInstance, DMA: TxDma<T>>(
    e: &Event,
    uart_tx: &mut UartTx<'a, T, DMA>,
) -> Result<(), CommError> {
    if let Some(k) = encode_event(e) {
        debug!("USART Send event {:?}", e);
        uart_tx.write(&[HEADER_BYTE, k]).await?
    }

    Ok(())
}
