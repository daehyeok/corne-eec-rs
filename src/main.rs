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

/// Detects which side of the split keyboard this is (left or right).
///
/// Uses PC15 as output and PC14 as input to determine the keyboard side.
/// The detection works by setting PC15 high and reading PC14's state.
fn detect_split_side(p: &mut embassy_stm32::Peripherals) -> SplitSide {
    let mut split_side_out = matrix_output!(p.PC15.reborrow());
    split_side_out.set_high();
    let split_side_in = gpio::Input::new(p.PC14.reborrow(), gpio::Pull::Up);

    let split_side = match split_side_in.is_high() {
        true => SplitSide::Right,
        false => SplitSide::Left,
    };
    split_side_out.set_low();

    info!("Keyboard side: {:?}", split_side);
    split_side
}

/// Builds the keyboard configuration for the left side.
///
/// # Arguments
/// * `p` - STM32 peripherals
///
/// # Returns
/// A configured `KeyboardConfig` for the left side with all GPIO pins and peripherals set up.
fn build_left_keyboard_config(
    p: embassy_stm32::Peripherals,
) -> (
    config::KeyboardConfig,
    Peri<'static, ADC1>,
    impl adc::AdcChannel<ADC1>,
) {
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

    (keyboard_cfg, p.ADC1, p.PB1)
}

/// Builds the keyboard configuration for the right side.
///
/// # Arguments
/// * `p` - STM32 peripherals
///
/// # Returns
/// A configured `KeyboardConfig` for the right side with all GPIO pins and peripherals set up.
fn build_right_keyboard_config(
    p: embassy_stm32::Peripherals,
) -> (
    config::KeyboardConfig,
    Peri<'static, ADC1>,
    impl adc::AdcChannel<ADC1>,
) {
    bind_interrupts!(struct Irqs {
        USART3_4_5_6_LPUART1 => usart::InterruptHandler<peripherals::USART3>;
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

    (keyboard_cfg, p.ADC1, p.PA5)
}

/// Main entry point for the keyboard firmware.
///
/// Initializes the STM32 peripherals, detects the keyboard side (left or right),
/// and starts the main task with the appropriate configuration.
#[embassy_executor::main]
async fn main(spawner: Spawner) {
    // Configure STM32 clocks and peripherals
    let mut config = embassy_stm32::Config::default();
    {
        use embassy_stm32::rcc::*;
        config.rcc.mux.usbsel = mux::Usbsel::HSI48;
        config.rcc.mux.adcsel = rcc::mux::Adcsel::SYS;
    }
    let mut p = embassy_stm32::init(config);
    debug!("init embassy");

    // Detect which side of the split keyboard this is
    let split_side = detect_split_side(&mut p);

    // Build configuration and start main task based on keyboard side
    match split_side {
        SplitSide::Left => {
            let (keyboard_cfg, adc1, adc_pin) = build_left_keyboard_config(p);
            main_task(spawner, keyboard_cfg, adc1, adc_pin).await;
        }
        SplitSide::Right => {
            let (keyboard_cfg, adc1, adc_pin) = build_right_keyboard_config(p);
            main_task(spawner, keyboard_cfg, adc1, adc_pin).await;
        }
    }
}

/// Main keyboard scanning task.
///
/// This task initializes the event channel, sets up USB/UART communication,
/// creates the scanner, and runs the main scanning loop.
///
/// # Arguments
/// * `spawner` - Embassy task spawner for creating background tasks
/// * `keyboard_cfg` - Keyboard configuration
/// * `adc1` - ADC peripheral
/// * `adc_pin` - ADC input pin
async fn main_task<'a, ADCPIN: adc::AdcChannel<ADC1>>(
    spawner: Spawner,
    keyboard_cfg: config::KeyboardConfig,
    adc1: Peri<'a, ADC1>,
    adc_pin: ADCPIN,
) {
    let channel = event_channel::init();

    // Split UART for bidirectional communication
    let (uart_tx, uart_rx) = keyboard_cfg.uart.split();
    info!("USB connected: {:?}", keyboard_cfg.usb_connected);

    // Set up communication based on USB connection status
    if keyboard_cfg.usb_connected {
        // This side is connected to USB - initialize HID and receive from other side
        hid::init(keyboard_cfg.usb_driver, &spawner, channel.receiver()).await;
        spawner.must_spawn(uart_read_task(channel.sender(), uart_rx));
    } else {
        // This side is not connected to USB - send events to the other side
        spawner.must_spawn(slave_event_task(channel.receiver(), uart_tx))
    }

    // Create and initialize the scanner
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

    // Main scanning loop
    loop {
        while let Some(e) = scanner.scan() {
            channel.sender().send(e).await;
        }
        //debug!("{:?}", scanner.raw_values());
        Timer::after(config::SCAN_DELAY).await;
    }
}

/// Task for the slave side (non-USB connected) to forward events to the master side.
///
/// This task receives keyboard events from the local scanner and sends them
/// via UART to the other half of the split keyboard.
///
/// # Arguments
/// * `receiver` - Event channel receiver for local keyboard events
/// * `tx` - UART transmitter for sending events to the other side
#[embassy_executor::task]
async fn slave_event_task(
    receiver: event_channel::EventReceiver<'static>,
    mut tx: UartTx<'static, embassy_stm32::mode::Async>,
) {
    info!("Start slave_event_task");
    loop {
        let event = receiver.receive().await;
        debug!("Received Event: {:?}", defmt::Debug2Format(&event));

        // Send event to the other half
        debug!("Send event to other side");
        if let Err(err) = comm::send(&event, &mut tx).await {
            error!("Usart Send Error: {:?}", defmt::Debug2Format(&err));
        };
    }
}

/// Task for the master side (USB connected) to receive events from the slave side.
///
/// This task receives keyboard events from the other half of the split keyboard
/// via UART and forwards them to the event channel for HID processing.
///
/// # Arguments
/// * `event_sender` - Event channel sender for forwarding received events
/// * `rx` - UART receiver for receiving events from the other side
#[embassy_executor::task]
async fn uart_read_task(
    event_sender: event_channel::EventSender<'static>,
    rx: UartRx<'static, embassy_stm32::mode::Async>,
) {
    info!("Start uart_read_task");
    comm::receive(event_sender, rx).await;
}
