#![no_std]
#![no_main]
#![feature(type_alias_impl_trait)]
use crate::config::SplitSide;
use config::MatrixConfig;
use defmt::*;
use eck_rs::analog::{RxMux, TxCharger};
use eck_rs::mux::Mux8;
use eck_rs::scanner::ECScanner;
use eck_rs::{self};
use embassy_executor::Spawner;
use embassy_stm32::peripherals::{self, DMA1_CH1, DMA2_CH1};
use embassy_stm32::rcc::{
    AdcClockSource, Clock48MhzSrc, ClockSrc, CrsConfig, CrsSyncSource, Pll, PllM, PllN, PllR,
    PllSrc,
};
use embassy_stm32::time::Hertz;
use embassy_stm32::usart::{self, Uart, UartTx};
use embassy_stm32::{self, bind_interrupts, gpio};
use embassy_time::Timer;
use paste::paste;
use {defmt_rtt as _, panic_probe as _};

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
    pub fn new(// pc6: &mut peripherals::PC6,
        // pa0: &mut peripherals::PA0,
        // pa8: &mut peripherals::PA8,
    ) -> Self {
        // let mut left_vbus_pin = gpio::Flex::new(pc6);
        // let mut right_vbus_pin = gpio::Flex::new(pa0);
        // left_vbus_pin.set_as_input(gpio::Pull::Down);
        // right_vbus_pin.set_as_input(gpio::Pull::Down);
        // let handness_pin = gpio::Input::new(pa8, gpio::Pull::Down);

        // let left_vbus_detect = left_vbus_pin.is_high();
        // if !left_vbus_detect {
        //     debug!("left vbus is low");
        //     left_vbus_pin.set_as_output(gpio::Speed::Medium);
        //     left_vbus_pin.set_high();
        // }

        // let split_side = match handness_pin.is_high() {
        //     true => SplitSide::Left,
        //     false => SplitSide::Right,
        // };

        // let vbus_dectect = match split_side {
        //     SplitSide::Left => left_vbus_detect,
        //     SplitSide::Right => right_vbus_pin.is_high(),
        // };

        // if !left_vbus_detect {
        //     left_vbus_pin.set_low();
        // }

        Self {
            usb_connected: true,
            split_side: SplitSide::Right,
        }
    }
}

// embassy task can't handling generic func.
macro_rules! run_maintask {
    ($split_side:ident, $p: ident, $status: ident, $channel: ident, $spawner: ident ) => {
        let adc = define_adc!($p);
        let uart = define_usart!(SplitSide::$split_side, $p);
        let matrix_cfg = define_matrix_config!(SplitSide::$split_side, $p);

        let (uart_tx, uart_rx) = uart.split();
        let comm_rx = comm::CommRx::new(uart_rx, $channel.sender());
        $spawner.must_spawn(paste! {[< $split_side:lower _uart_read_task >](comm_rx)});
        if !$status.usb_connected {
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
        source: PllSrc::HSE(Hertz(12_000_000)),
        prediv_m: PllM::Div3,
        mul_n: PllN::Mul85,
        div_p: None,
        div_q: Some(embassy_stm32::rcc::PllQ::Div2),
        div_r: Some(PllR::Div2),
    });

    config.rcc.mux = ClockSrc::PLL;
    config.rcc.adc12_clock_source = AdcClockSource::SysClk;
    config.rcc.clock_48mhz_src = Some(Clock48MhzSrc::Hsi48(Some(CrsConfig {
        sync_src: CrsSyncSource::Usb,
    })));

    let p = embassy_stm32::init(config);
    let channel = event_channel::init();

    let status = KeyboardStatus::new();
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

        Timer::after(config::SCAN_DELAY).await;
    }
}

async fn slave_event_handler<T: usart::BasicInstance, DMA: usart::TxDma<T>>(
    receiver: event_channel::EventReceiver<'static>,
    uart_tx: &mut UartTx<'static, T, DMA>,
) {
    loop {
        let event = receiver.recv().await;
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
async fn left_uart_read_task(mut comm_rx: comm::CommRx<'static, peripherals::USART1, DMA1_CH1>) {
    info!("Start left_uart_read_task");
    comm_rx.run().await;
}

#[embassy_executor::task]
async fn right_uart_read_task(mut comm_rx: comm::CommRx<'static, peripherals::USART2, DMA1_CH1>) {
    info!("Start right_uart_read_task");
    comm_rx.run().await;
}
