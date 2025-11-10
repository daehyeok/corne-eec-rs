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
use defmt::*;
use defmt_rtt as _;
use embassy_executor::{self, Spawner};
use embassy_stm32::{
    adc::{self},
    bind_interrupts, gpio, i2c,
    peripherals::{self, ADC1},
    rcc::{self},
    usart::{self, Uart, UartRx, UartTx},
    usb, Peri,
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
    USB_UCPD1_2 => usb::InterruptHandler<peripherals::USB>;
});

bind_interrupts!(struct Irqs {
    I2C1 => i2c::EventInterruptHandler<peripherals::I2C1>, i2c::ErrorInterruptHandler<peripherals::I2C1>;
});

macro_rules! matrix_output {
    ($pin:expr) => {
        gpio::Output::new($pin, gpio::Level::Low, gpio::Speed::VeryHigh)
    };
}

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    let mut config = embassy_stm32::Config::default();
    {
        use embassy_stm32::rcc::*;
        // config.rcc.hsi48 = Some(Hsi48Config {
        //     sync_from_usb: true,
        // });
        config.rcc.mux.usbsel = mux::Usbsel::HSI48;
        config.rcc.mux.adcsel = rcc::mux::Adcsel::SYS;
    }
    let p = embassy_stm32::init(config);

    debug!("init embassy");

    let mut split_side_out = matrix_output!(p.PC15);
    split_side_out.set_high();
    let split_side_in = gpio::Input::new(p.PC14, gpio::Pull::Up);

    let split_side = match split_side_in.is_high() {
        true => SplitSide::Right,
        false => SplitSide::Left,
    };
    split_side_out.set_low();
    info!("Keyboard side: {:?}", split_side);

    match split_side {
        SplitSide::Left => {
            bind_interrupts!(struct Irqs {
                USART1 => usart::InterruptHandler<peripherals::USART1>;
            });
            let keyboard_cfg = config::KeyboardConfig {
                usb_connected: gpio::Input::new(p.PC6, gpio::Pull::Down).is_high(),
                usb_driver: embassy_stm32::usb::Driver::new(p.USB, UsbIrqs, p.PA12, p.PA11),
                uart: Uart::new(
                    p.USART1,
                    p.PA10,
                    p.PA9,
                    Irqs,
                    p.DMA1_CH1,
                    p.DMA1_CH2,
                    config::usart_config(),
                )
                .unwrap(),
                col_mux_enable: matrix_output!(p.PA7),
                col_mux_sels: [
                    matrix_output!(p.PA4),
                    matrix_output!(p.PA5),
                    matrix_output!(p.PA6),
                ],
                col_mux_channel: [6, 7, 2, 1, 0, 3, 4],
                drain: gpio::OutputOpenDrain::new(p.PB0, gpio::Level::High, gpio::Speed::VeryHigh),
                row_pins: [
                    matrix_output!(p.PA0),
                    matrix_output!(p.PA1),
                    matrix_output!(p.PA2),
                    matrix_output!(p.PA3),
                ],
                transform: config::left_matrix_transform,
                thresholds: [[4000u16; 7]; 4],
                nbounce: 2,
            };

            main_task(spawner, keyboard_cfg, p.ADC1, p.PB1).await;
        }
        SplitSide::Right => {
            bind_interrupts!(struct Irqs {
                USART3_4_5_6_LPUART1 =>     usart::InterruptHandler<peripherals::USART3>;
            });
            let keyboard_cfg = config::KeyboardConfig {
                usb_connected: gpio::Input::new(p.PA0, gpio::Pull::Down).is_high(),
                usb_driver: embassy_stm32::usb::Driver::new(p.USB, UsbIrqs, p.PA12, p.PA11),
                uart: Uart::new(
                    p.USART3,
                    p.PB9,
                    p.PB8,
                    Irqs,
                    p.DMA1_CH1,
                    p.DMA1_CH2,
                    config::usart_config(),
                )
                .unwrap(),
                col_mux_enable: matrix_output!(p.PB0),
                col_mux_sels: [
                    matrix_output!(p.PA1),
                    matrix_output!(p.PA2),
                    matrix_output!(p.PA3),
                ],
                col_mux_channel: [1, 0, 4, 6, 7, 5, 2],
                drain: gpio::OutputOpenDrain::new(p.PA7, gpio::Level::High, gpio::Speed::VeryHigh),
                row_pins: [
                    matrix_output!(p.PA9),
                    matrix_output!(p.PA8),
                    matrix_output!(p.PB2),
                    matrix_output!(p.PB1),
                ],
                transform: config::right_matrix_transform,
                thresholds: [[4000u16; 7]; 4],
                nbounce: 2,
            };

            main_task(spawner, keyboard_cfg, p.ADC1, p.PA5).await;
        }
    }
}

async fn main_task<'a, ADCPIN: adc::AdcChannel<ADC1>>(
    spawner: Spawner,
    keyboard_cfg: config::KeyboardConfig,
    adc1: Peri<'a, ADC1>,
    adc_pin: ADCPIN,
) {
    let channel = event_channel::init();

    let (uart_tx, uart_rx) = keyboard_cfg.uart.split();
    info!("USB connected: {:?}", keyboard_cfg.usb_connected);
    if keyboard_cfg.usb_connected {
        hid::init(keyboard_cfg.usb_driver, &spawner, channel.receiver()).await;
        spawner.must_spawn(uart_read_task(channel.sender(), uart_rx));
    } else {
        spawner.must_spawn(slave_event_task(channel.receiver(), uart_tx))
    }

    info!("Start main scan task.");
    let discharge_delay = analog::CortexDisChargeDelay::new();
    let mux8 = unwrap!(Mux8::new(
        keyboard_cfg.col_mux_enable,
        keyboard_cfg.col_mux_sels,
        keyboard_cfg.col_mux_channel,
    ));
    let adc = analog::Adc::new(adc1, adc_pin);
    let rx_mux = RxMux::new(mux8, adc);
    let tx_charger = TxCharger::new(keyboard_cfg.drain, keyboard_cfg.row_pins, discharge_delay);
    let mut scanner = ECScanner::new(
        tx_charger,
        rx_mux,
        keyboard_cfg.transform,
        keyboard_cfg.nbounce,
        keyboard_cfg.thresholds,
    );

    scanner.dischage_all();
    loop {
        while let Some(e) = scanner.scan() {
            channel.sender().send(e).await;
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
