#![no_std]
#![no_main]
#![feature(type_alias_impl_trait)]
use crate::config::SplitSide;
use config::MatrixConfig;
use defmt::*;
use defmt_rtt as _;
use eck_rs::{
    analog::{RxMux, TxCharger},
    mux::Mux8,
    scanner::ECScanner,
};
use embassy_executor::Spawner;
use embassy_stm32::{
    bind_interrupts, gpio,
    peripherals::{self, DMA1_CH1, DMA2_CH1},
    rcc::{
        AdcClockSource, Clk48Src, Hse, HseMode, Pll, PllMul, PllPreDiv, PllQDiv, PllRDiv, Pllsrc,
    },
    time::Hertz,
    usart::{self, Uart, UartRx, UartTx},
};
use embassy_time::Timer;
use panic_probe as _;
use paste::paste;

mod analog;
mod comm;
mod config;
mod event_channel;
mod hid;
mod layers;

struct KeyboardStatus {
    pub usb_connected: bool,
    pub split_side: SplitSide,
}

impl KeyboardStatus {
    pub fn new(pa4: &mut peripherals::PA4, pb4: &mut peripherals::PB4) -> Self {
        let handness_pin = gpio::Input::new(pa4, gpio::Pull::Down);
        let split_side = match handness_pin.is_high() {
            true => SplitSide::Right,
            false => SplitSide::Left,
        };

        let vbus_pin = gpio::Input::new(pb4, gpio::Pull::Down);
        let usb_connected = vbus_pin.is_high();

        if usb_connected {
            debug!("VBUS detected");
        }

        Self {
            usb_connected: true,
            split_side, //: SplitSide::Right,
        }
    }
}

// embassy task can't handling generic func.
macro_rules! run_maintask {
    ($split_side:ident, $p: ident, $status: ident, $channel: ident, $spawner: ident ) => {
        let adc = define_adc!($p);
        let uart = define_usart!(SplitSide::$split_side, $p);
        let matrix_cfg = define_matrix_config!(SplitSide::$split_side, $p);

        let (uart_tx, uart_rx) = uart.expect("USART SPLIT").split();
        if $status.usb_connected {
            $spawner.must_spawn(
                paste! {[< $split_side:lower _uart_read_task >]($channel.sender(), uart_rx)},
            );
        } else {
            $spawner.must_spawn(
                paste! {[< $split_side:lower _slave_event_task >]($channel.receiver(), uart_tx)},
            )
        }

        main_task(matrix_cfg, adc, $channel.sender()).await;
    };
}

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    let mut config = embassy_stm32::Config::default();

    config.rcc.pll = Some(Pll {
        source: Pllsrc::HSE,
        prediv: PllPreDiv::DIV3,
        mul: PllMul::MUL85,
        divp: None,
        divq: Some(PllQDiv::DIV2),
        divr: Some(PllRDiv::DIV2),
    });

    config.rcc.hse = Some(Hse {
        freq: Hertz(12_000_000),
        mode: HseMode::Oscillator,
    });
    config.rcc.adc12_clock_source = AdcClockSource::SYS;
    config.rcc.clk48_src = Clk48Src::HSI48;

    let mut p = embassy_stm32::init(config);
    let channel = event_channel::init();

    let status = KeyboardStatus::new(&mut p.PA4, &mut p.PB4);
    info!("Keyboard side: {:?}", status.split_side);
    info!("USB connected: {:?}", status.usb_connected);

    if status.usb_connected {
        hid::init(p.USB, p.PA12, p.PA11, &spawner, channel.receiver()).await;
    }

    match status.split_side {
        SplitSide::Left => {
            run_maintask!(Left, p, status, channel, spawner);
        }
        SplitSide::Right => {
            run_maintask!(Right, p, status, channel, spawner);
        }
    };
}

async fn main_task<
    ADCPIN: embassy_stm32::adc::AdcPin<peripherals::ADC2> + embassy_stm32::gpio::low_level::Pin,
>(
    matrix_cfg: MatrixConfig,
    adc: analog::Adc<'static, ADCPIN>,
    event_sender: event_channel::EventSender<'static>,
) {
    info!("Start main scan task.");
    let discharge_delay = analog::CortexDisChargeDelay::new();
    let mux8 = unwrap!(Mux8::new(
        matrix_cfg.col_mux_enable,
        matrix_cfg.col_mux_sels,
        matrix_cfg.col_mux_channel,
    ));
    let rx_mux = RxMux::new(mux8, adc);
    let tx_charger = TxCharger::new(matrix_cfg.drain, matrix_cfg.row_pins, discharge_delay);
    let mut scanner = ECScanner::new(
        tx_charger,
        rx_mux,
        matrix_cfg.transform,
        matrix_cfg.nbounce,
        matrix_cfg.thresholds,
    );

    scanner.dischage_all();

    loop {
        while let Some(e) = scanner.scan() {
            event_sender.send(e).await;
        }
        //debug!("{:?}", scanner.raw_values());
        Timer::after(config::SCAN_DELAY).await;
    }
}

async fn slave_event_handler<T: usart::BasicInstance, DMA: usart::TxDma<T>>(
    receiver: event_channel::EventReceiver<'static>,
    uart_tx: &mut UartTx<'static, T, DMA>,
) {
    loop {
        let event = receiver.receive().await;
        debug!("Received Event: {:?}", defmt::Debug2Format(&event));

        // send event to other halve.
        debug!("Send event to other side");
        if let Err(err) = comm::send(&event, uart_tx).await {
            error!("Usart Send Error: {:?}", defmt::Debug2Format(&err));
        };
    }
}

//embassy not allowd generic task. Wrapping generic funtions.
#[embassy_executor::task]
async fn left_slave_event_task(
    receiver: event_channel::EventReceiver<'static>,
    mut tx: UartTx<'static, peripherals::USART1, DMA2_CH1>,
) {
    info!("Start left_slave_event_task");
    slave_event_handler(receiver, &mut tx).await;
}

#[embassy_executor::task]
async fn right_slave_event_task(
    receiver: event_channel::EventReceiver<'static>,
    mut tx: UartTx<'static, peripherals::USART2, DMA2_CH1>,
) {
    info!("Start right_slave_event_task");
    slave_event_handler(receiver, &mut tx).await;
}

#[embassy_executor::task]
async fn left_uart_read_task(
    event_sender: event_channel::EventSender<'static>,
    rx: UartRx<'static, peripherals::USART1, DMA1_CH1>,
) {
    info!("Start left_uart_read_task");
    comm::receive(event_sender, rx).await;
}

#[embassy_executor::task]
async fn right_uart_read_task(
    event_sender: event_channel::EventSender<'static>,
    rx: UartRx<'static, peripherals::USART2, DMA1_CH1>,
) {
    info!("Start right_uart_read_task");
    comm::receive(event_sender, rx).await;
}
