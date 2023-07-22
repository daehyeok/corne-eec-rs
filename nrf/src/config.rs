use embassy_nrf::{
    gpio::{AnyPin, Flex, Input, Output},
    uarte::{self, Baudrate, Parity},
};
use embassy_time::Duration;

#[macro_export]
macro_rules! pushpull_output {
    ($pin:expr) => {
        Output::new(
                AnyPin::from($pin),
                gpio::Level::Low,
                OutputDrive::Standard,
        )
    };
    ($pin:expr, $($pin2:expr),+) => {
        [pushpull_output!($pin), $(pushpull_output!($pin2)),+ ]
    };
}

#[macro_export]
macro_rules! pulldown_input {
    ($pin:expr) => {
        Input::new(AnyPin::from($pin), gpio::Pull::Down)
    };
}

pub const DISCHARGE_DELAY_CLOCKS: u32 = 2500;
pub const SCAN_DELAY: Duration = Duration::from_millis(100);
pub const TICK_PERIOD: Duration = Duration::from_millis(1);

pub const RX_SIZE: usize = 7;
pub const TX_SIZE: usize = 4;

pub struct MatrixConfig {
    pub vbus_detect: Input<'static, AnyPin>,
    pub col_mux_enable: Output<'static, AnyPin>,
    pub col_mux_sels: [Output<'static, AnyPin>; 3],
    pub col_mux_channel: [u8; RX_SIZE],
    pub drain: Flex<'static, AnyPin>,
    pub row_pins: [Output<'static, AnyPin>; TX_SIZE],
    pub uart_tx: AnyPin,
    pub uart_rx: AnyPin,
    pub transform: fn(u8, u8) -> (u8, u8),
    pub thresholds: [[i16; RX_SIZE]; TX_SIZE],
    pub nbounce: u8,
}

pub fn uarte_config() -> uarte::Config {
    let mut uart_config = uarte::Config::default();
    uart_config.parity = Parity::EXCLUDED;
    uart_config.baudrate = Baudrate::BAUD115200;
    uart_config
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
        (4, (TX_SIZE + RX_SIZE - 1) as u8 - tx)
    } else {
        (tx, rx + 5)
    }
}
