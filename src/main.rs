#![no_std]
#![no_main]
#![feature(type_alias_impl_trait)]
#![feature(impl_trait_in_assoc_type)]
use crate::{
    config::SplitSide,
    eck::{
        analog::{RxMux, TxCharger},
        mux::Mux8,
        scanner::ECScanner,
    },
};
use config::MatrixConfig;
use defmt::*;
use defmt_rtt as _;
use embassy_executor::{self, Spawner};
use embassy_stm32::{
    bind_interrupts, gpio, i2c, peripherals,
    rcc::{self, Hse, HseMode, Pll, PllMul, PllPreDiv, PllQDiv, PllRDiv, PllSource},
    time::Hertz,
    usart::{self, Uart, UartRx, UartTx},
};
use embassy_time::Timer;
use panic_probe as _;

mod analog;
mod comm;
mod config;
mod eck;
mod event_channel;
mod hid;
mod layers;

bind_interrupts!(struct UsbIrqs {
    USB_LP => embassy_stm32::usb::InterruptHandler<peripherals::USB>;
});

bind_interrupts!(struct I2cIrqs {
    I2C1_ER => i2c::ErrorInterruptHandler<peripherals::I2C1>;
    I2C1_EV => i2c::EventInterruptHandler<peripherals::I2C1>;
    I2C2_ER => i2c::ErrorInterruptHandler<peripherals::I2C2>;
    I2C2_EV => i2c::EventInterruptHandler<peripherals::I2C2>;
});

macro_rules! matrix_output {
    ($pin:expr) => {
        gpio::Output::new($pin, gpio::Level::Low, gpio::Speed::VeryHigh)
    };
}

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    let mut config = embassy_stm32::Config::default();

    config.rcc.pll = Some(Pll {
        source: PllSource::HSI,
        prediv: PllPreDiv::DIV1,
        mul: PllMul::MUL12,
        divp: None,
        divq: Some(PllQDiv::DIV4),
        divr: Some(PllRDiv::DIV2),
    });

    config.rcc.hse = Some(Hse {
        freq: Hertz(8_000_000),
        mode: HseMode::Oscillator,
    });

    config.rcc.mux.adc12sel = rcc::mux::Adcsel::SYS;
    config.rcc.mux.clk48sel = rcc::mux::Clk48sel::HSI48;

    debug!("init embassy");
    let p = embassy_stm32::init(config);
    let channel = event_channel::init();

    let split_side_output = gpio::Input::new(p.PB3, gpio::Pull::Down);
    let mut pa4 = matrix_output!(p.PA4);
    let split_side = match split_side_output.is_high() {
        true => SplitSide::Left,
        false => SplitSide::Right,
    };
    pa4.set_low();
    info!("Keyboard side: {:?}", split_side);

    let (uart, matrix_cfg) = match split_side {
        SplitSide::Left => {
            bind_interrupts!(struct Irqs {
                USART1 => usart::InterruptHandler<peripherals::USART1>;
            });
            (
                Uart::new(
                    p.USART1,
                    p.PA10,
                    p.PA9,
                    Irqs,
                    p.DMA1_CH1,
                    p.DMA1_CH2,
                    config::usart_config(),
                ),
                config::MatrixConfig {
                    col_mux_enable: matrix_output!(p.PA8),
                    col_mux_sels: [pa4, matrix_output!(p.PA5), matrix_output!(p.PA6)],
                    col_mux_channel: [6, 7, 2, 1, 0, 3, 4],
                    drain: gpio::OutputOpenDrain::new(
                        p.PB0,
                        gpio::Level::High,
                        gpio::Speed::VeryHigh,
                    ),
                    row_pins: [
                        matrix_output!(p.PA0),
                        matrix_output!(p.PA1),
                        matrix_output!(p.PA2),
                        matrix_output!(p.PA3),
                    ],
                    transform: config::left_matrix_transform,
                    thresholds: [[4000u16; 7]; 4],
                    nbounce: 2,
                },
            )
        }
        SplitSide::Right => {
            bind_interrupts!(struct Irqs {
                USART2 => usart::InterruptHandler<peripherals::USART2>;
            });
            (
                Uart::new(
                    p.USART2,
                    p.PA3,
                    p.PA2,
                    Irqs,
                    p.DMA1_CH1,
                    p.DMA1_CH2,
                    config::usart_config(),
                ),
                config::MatrixConfig {
                    col_mux_enable: matrix_output!(p.PA1),
                    col_mux_sels: [
                        matrix_output!(p.PA0),
                        matrix_output!(p.PA5),
                        matrix_output!(p.PA6),
                    ],
                    col_mux_channel: [2, 5, 7, 6, 4, 0, 1],
                    drain: gpio::OutputOpenDrain::new(
                        p.PB0,
                        gpio::Level::High,
                        gpio::Speed::VeryHigh,
                    ),
                    row_pins: [
                        matrix_output!(p.PA15),
                        matrix_output!(p.PA10),
                        matrix_output!(p.PA9),
                        matrix_output!(p.PA8),
                    ],
                    transform: config::right_matrix_transform,
                    thresholds: [[4000u16; 7]; 4],
                    nbounce: 2,
                },
            )
        }
    };

    let (uart_tx, uart_rx) = uart.expect("USART SPLIT").split();

    let usb_connected = gpio::Input::new(p.PB6, gpio::Pull::Down).is_high();
    info!("USB connected: {:?}", usb_connected);
    if usb_connected {
        let driver = embassy_stm32::usb::Driver::new(p.USB, UsbIrqs, p.PA12, p.PA11);
        hid::init(driver, &spawner, channel.receiver()).await;
        spawner.must_spawn(uart_read_task(channel.sender(), uart_rx));
    } else {
        spawner.must_spawn(slave_event_task(channel.receiver(), uart_tx))
    }

    let adc = analog::Adc::new(p.ADC2, p.PA7);

    main_task(matrix_cfg, adc, channel.sender()).await;
}

async fn main_task<ADCPIN: embassy_stm32::adc::AdcChannel<peripherals::ADC2>>(
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

#[embassy_executor::task]
async fn slave_event_task(
    receiver: event_channel::EventReceiver<'static>,
    mut tx: UartTx<'static, embassy_stm32::mode::Async>,
) {
    info!("Start slave_event_task");
    loop {
        let event = receiver.receive().await;
        debug!("Received Event: {:?}", defmt::Debug2Format(&event));

        // send event to other halve.
        debug!("Send event to other side");
        if let Err(err) = comm::send(&event, &mut tx).await {
            error!("Usart Send Error: {:?}", defmt::Debug2Format(&err));
        };
    }
}

#[embassy_executor::task]
async fn uart_read_task(
    event_sender: event_channel::EventSender<'static>,
    rx: UartRx<'static, embassy_stm32::mode::Async>,
) {
    info!("Start uart_read_task");
    comm::receive(event_sender, rx).await;
}
