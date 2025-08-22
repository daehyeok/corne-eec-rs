use embassy_stm32::{
    gpio::Output,
    usart::{self, Parity},
};
use embassy_time::Duration;

#[derive(defmt::Format, Debug, PartialEq)]
pub enum SplitSide {
    #[allow(dead_code)]
    Left,
    Right,
}

/// USB VID, PID for a generic keyboard from
/// https://github.com/obdev/v-usb/blob/master/usbdrv/USB-IDs-for-free.txt
pub const USB_VID: u16 = 0x16c0;
pub const USB_PID: u16 = 0x27db;
pub const USB_MANUFACTURER: &str = "Daehyeok Mun";
pub const USB_PRODUCT: &str = "Corne EEC - STM32";
pub const USB_SERIAL_NUMBER: &str = env!("CARGO_PKG_VERSION");

pub const DISCHARGE_DELAY_CLOCKS: u32 = 5000;
pub const SCAN_DELAY: Duration = Duration::from_millis(1);
pub const TICK_PERIOD: Duration = Duration::from_millis(1);

pub const RX_SIZE: usize = 7;
pub const TX_SIZE: usize = 4;

pub type AdcUnit = u16;

pub struct MatrixConfig {
    pub col_mux_enable: Output<'static>,
    pub col_mux_sels: [Output<'static>; 3],
    pub col_mux_channel: [u8; RX_SIZE],
    pub drain: embassy_stm32::gpio::OutputOpenDrain<'static>,
    pub row_pins: [Output<'static>; TX_SIZE],
    pub transform: fn(u8, u8) -> (u8, u8),
    pub thresholds: [[AdcUnit; RX_SIZE]; TX_SIZE],
    pub nbounce: u8,
}

pub fn usart_config() -> usart::Config {
    let mut cfg = usart::Config::default();
    cfg.baudrate = 4800;
    cfg.parity = Parity::ParityEven;
    cfg
}

// (tx  rx) to layout(row, col)
pub fn left_matrix_transform(tx: u8, rx: u8) -> (u8, u8) {
    if rx == (RX_SIZE - 1) as u8 {
        (4, 2 + tx)
    } else {
        (tx, rx)
    }
}

pub fn right_matrix_transform(tx: u8, rx: u8) -> (u8, u8) {
    if rx == 0 {
        (4, (TX_SIZE + RX_SIZE - 2) as u8 - tx)
    } else {
        (tx, rx + 5)
    }
}
