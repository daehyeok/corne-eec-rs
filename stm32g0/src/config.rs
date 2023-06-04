use embassy_stm32::gpio::{AnyPin, Output};
use embassy_stm32::usart::{self, Parity};
use embassy_time::Duration;

#[macro_export]
macro_rules! pushpull_output {
    ($pin:expr) => {
        gpio::Output::new(
                gpio::AnyPin::from($pin),
                gpio::Level::Low,
                gpio::Speed::VeryHigh,
        )
    };
    ($pin:expr, $($pin2:expr),+) => {
        [pushpull_output!($pin), $(pushpull_output!($pin2)),+ ]
    };
}

#[macro_export]
macro_rules! opendrain_output {
    ($pin:expr) => {
        gpio::OutputOpenDrain::new(
            $pin,
            gpio::Level::High,
            gpio::Speed::VeryHigh,
            gpio::Pull::None,
        )
        .degrade()
    };
}

#[macro_export]
macro_rules! pulldown_input {
    ($pin:expr) => {
        gpio::Input::new(gpio::AnyPin::from($pin), gpio::Pull::Down)
    };
}

/// USB VID, PID for a generic keyboard from
/// https://github.com/obdev/v-usb/blob/master/usbdrv/USB-IDs-for-free.txt
pub const USB_VID: u16 = 0x16c0;
pub const USB_PID: u16 = 0x27db;
pub const USB_MANUFACTURER: &str = "Daehyeok Mun";
pub const USB_PRODUCT: &str = "Corne EEC - STM32";
pub const USB_SERIAL_NUMBER: &str = env!("CARGO_PKG_VERSION");

pub const DISCHARGE_DELAY_CLOCKS: u32 = 2500;
pub const SCAN_DELAY: Duration = Duration::from_millis(1);
pub const TICK_PERIOD: Duration = Duration::from_millis(1);

pub const RX_SIZE: usize = 7;
pub const TX_SIZE: usize = 4;

pub type AdcUnit = u16;

pub struct MatrixConfig {
    pub col_mux_enable: Output<'static, AnyPin>,
    pub col_mux_sels: [Output<'static, AnyPin>; 3],
    pub col_mux_channel: [u8; RX_SIZE],
    pub drain: Output<'static, AnyPin>,
    pub row_pins: [Output<'static, AnyPin>; TX_SIZE],
    pub transform: fn(u8, u8) -> (u8, u8),
    pub thresholds: [[AdcUnit; RX_SIZE]; TX_SIZE],
    pub nbounce: u8,
}

pub fn usart_config() -> usart::Config {
    let mut cfg = usart::Config::default();
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
