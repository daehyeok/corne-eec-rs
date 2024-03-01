use embassy_stm32::{
    gpio::{AnyPin, Output},
    usart::{self, Parity},
};
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
    };
}

#[macro_export]
macro_rules! pulldown_input {
    ($pin:expr) => {
        gpio::Input::new(gpio::AnyPin::from($pin), gpio::Pull::Down)
    };
}

#[macro_export]
macro_rules! __define_usart_inner{
    ($p: ident, $interrupt: ident, $tx: ident, $rx: ident) =>{
{
        bind_interrupts!(struct Irqs {
                $interrupt => usart::InterruptHandler<peripherals::$interrupt>;
            });
            Uart::new(
                $p.$interrupt,
                $p.$rx,
                $p.$tx,
                Irqs,
                $p.DMA2_CH1,
                $p.DMA1_CH1,
                config::usart_config(),
            )
    }
    }
}

#[macro_export]
macro_rules! define_adc {
    ($p: ident) => {
        analog::Adc::new($p.ADC2, $p.PA7)
    };
}

#[macro_export]
macro_rules! define_usart {
    (SplitSide::Left, $p: ident) => {
        __define_usart_inner! {$p, USART1, PA9, PA10}
    };
    (SplitSide::Right, $p: ident ) => {
        __define_usart_inner! {$p, USART2, PA2, PA3}
    };
}

#[macro_export]
macro_rules! define_matrix_config {
    (SplitSide::Left, $p: ident) => {
        config::MatrixConfig {
            col_mux_enable: pushpull_output!($p.PA8),
            col_mux_sels: pushpull_output! {$p.PA4, $p.PA5, $p.PA6},
            col_mux_channel: [6, 7, 2, 1, 0, 3, 4],
            drain: opendrain_output! {$p.PB0},
            row_pins: pushpull_output!($p.PA0, $p.PA1, $p.PA2, $p.PA3),
            transform: config::right_matrix_transform,
            thresholds: [[2000u16; 7]; 4],
            nbounce: 2,
        }
    };
    (SplitSide::Right, $p: ident ) => {
        config::MatrixConfig {
            col_mux_enable: pushpull_output!($p.PA1),
            col_mux_sels: pushpull_output! {$p.PA0, $p.PA5, $p.PA6},
            col_mux_channel: [2, 5, 7, 6, 4, 0, 1],
            drain: opendrain_output! {$p.PB0},
            row_pins: pushpull_output!($p.PA15, $p.PA10, $p.PA9, $p.PA8),
            transform: config::right_matrix_transform,
            thresholds: [[2000u16; 7]; 4],
            nbounce: 2,
        }
    };
}

#[derive(defmt::Format, Debug)]
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

pub const DISCHARGE_DELAY_CLOCKS: u32 = 2500;
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
    cfg.parity = Parity::ParityEven;
    cfg
}

// (tx  rx) to layout(row, col)
#[allow(dead_code)]
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
