#![no_std]
#![no_main]
#![feature(type_alias_impl_trait)]
use config::MatrixConfig;
use defmt::*;
use eck_rs::{
    self,
    analog::{RxMux, TxCharger},
    mux::Mux8,
    scanner::ECScanner,
};
use embassy_executor::Spawner;
use embassy_stm32::{
    self, bind_interrupts, gpio, pac,
    peripherals::{self, DMA1_CH1, DMA2_CH1},
    usart::{self, Uart, UartTx},
    usb,
};
use embassy_time::Timer;
use static_cell::StaticCell;
use {defmt_rtt as _, panic_probe as _};

mod analog;
mod comm;
mod config;
mod event_channel;
mod hid;
mod layers;

static KEYBERON_TICK_RES: StaticCell<hid::KeyberonTickRes> = StaticCell::new();
static SHARED_LAYOUT: StaticCell<layers::SharedLayout> = StaticCell::new();

bind_interrupts!(struct UsbIrqs {
    USB_UCPD1_2 => usb::InterruptHandler<peripherals::USB>;
});

#[derive(defmt::Format, Debug)]
enum SplitSide {
    Left,
    Right,
}

struct KeyboardStatus {
    pub usb_connected: bool,
    pub split_side: SplitSide,
}

impl KeyboardStatus {
    pub fn new(
        pc6: &mut peripherals::PC6,
        pa0: &mut peripherals::PA0,
        pa8: &mut peripherals::PA8,
    ) -> Self {
        let mut left_vbus_pin = gpio::Flex::new(pc6);
        let mut right_vbus_pin = gpio::Flex::new(pa0);
        left_vbus_pin.set_as_input(gpio::Pull::Down);
        right_vbus_pin.set_as_input(gpio::Pull::Down);
        let handness_pin = gpio::Input::new(pa8, gpio::Pull::Down);

        let left_vbus_detect = left_vbus_pin.is_high();
        if !left_vbus_detect {
            debug!("left vbus is low");
            left_vbus_pin.set_as_output(gpio::Speed::Medium);
            left_vbus_pin.set_high();
        }

        let split_side = match handness_pin.is_high() {
            true => SplitSide::Left,
            false => SplitSide::Right,
        };

        let vbus_dectect = match split_side {
            SplitSide::Left => left_vbus_detect,
            SplitSide::Right => right_vbus_pin.is_high(),
        };

        if !left_vbus_detect {
            left_vbus_pin.set_low();
        }

        Self {
            usb_connected: vbus_dectect,
            split_side,
        }
    }
}

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    let mut p = embassy_stm32::init(embassy_stm32::Config::default());

    pac::RCC.cr().modify(|w| w.set_hsi48on(true));
    pac::RCC
        .ccipr2()
        .modify(|w| w.set_usbsel(pac::rcc::vals::Usbsel::HSI48));

    // Declare a bounded channel  of 3 u32s.
    let channel = event_channel::init();
    let layout = SHARED_LAYOUT.init(layers::new_shared_layout());

    let status = KeyboardStatus::new(&mut p.PC6, &mut p.PA0, &mut p.PA8);
    info!("Keyboard side: {:?}", status.split_side);
    info!("USB connected: {:?}", status.usb_connected);

    if status.usb_connected {
        let usb_driver = usb::Driver::new(p.USB, UsbIrqs, p.PA12, p.PA11);
        let usb_hid = hid::init(usb_driver);
        spawner.must_spawn(hid::usb_device_task(&mut usb_hid.device));

        hid::wait_until_configured().await;

        let tick_res =
            KEYBERON_TICK_RES.init(hid::KeyberonTickRes::new(&mut usb_hid.writer, layout));
        spawner.must_spawn(hid::keyberon_tick(tick_res));
        spawner.must_spawn(master_event_handler(channel.receiver(), layout));
    }

    //Run tasks
    match status.split_side {
        SplitSide::Left => {
            bind_interrupts!(struct Irqs {
                USART1 => usart::InterruptHandler<peripherals::USART1>;
            });
            let uart = Uart::new(
                p.USART1,
                p.PA10,
                p.PA9,
                Irqs,
                p.DMA2_CH1,
                p.DMA1_CH1,
                config::usart_config(),
            );
            let adc = analog::Adc::new(p.ADC1, p.PB1);
            let matrix_cfg = config::MatrixConfig {
                col_mux_enable: pushpull_output!(p.PA7),
                col_mux_sels: pushpull_output! {p.PA4, p.PA5, p.PA6},
                col_mux_channel: [6, 7, 2, 1, 0, 3, 4],
                drain: opendrain_output! {p.PB2},
                row_pins: pushpull_output!(p.PA0, p.PA1, p.PA2, p.PA3),
                transform: config::left_matrix_transform,
                thresholds: [[2000u16; 7]; 4],
                nbounce: 2,
            };

            let (uart_tx, uart_rx) = uart.split();

            let comm_rx = comm::CommRx::new(uart_rx, channel.sender());
            spawner.must_spawn(left_uart_read_task(comm_rx));
            if !status.usb_connected {
                spawner.must_spawn(left_slave_event_task(channel.receiver(), uart_tx))
            }
            main_task(matrix_cfg, adc, channel.sender()).await;
        }
        SplitSide::Right => {
            bind_interrupts!(struct Irqs {
                USART3_4_5_6_LPUART1 =>     usart::InterruptHandler<peripherals::USART3>;
            });
            let uart = Uart::new(
                p.USART3,
                p.PB9,
                p.PB8,
                Irqs,
                p.DMA2_CH1,
                p.DMA1_CH1,
                config::usart_config(),
            );
            let adc = analog::Adc::new(p.ADC1, p.PA5);
            let matrix_cfg = config::MatrixConfig {
                col_mux_enable: pushpull_output!(p.PB0),
                col_mux_sels: pushpull_output! {p.PA1, p.PA2, p.PA3},
                col_mux_channel: [2, 5, 7, 6, 4, 0, 1],
                drain: opendrain_output! {p.PA7},
                row_pins: pushpull_output!(p.PA9, p.PA8, p.PB2, p.PB1),
                transform: config::right_matrix_transform,
                thresholds: [[2000u16; 7]; 4],
                nbounce: 2,
            };

            let (uart_tx, uart_rx) = uart.split();

            let comm_rx = comm::CommRx::new(uart_rx, channel.sender());
            spawner.must_spawn(right_uart_read_task(comm_rx));
            if !status.usb_connected {
                spawner.must_spawn(right_slave_event_task(channel.receiver(), uart_tx))
            }

            main_task(matrix_cfg, adc, channel.sender()).await;
        }
    }
}

async fn main_task<ADCPIN: embassy_stm32::adc::AdcPin<peripherals::ADC1>>(
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

#[embassy_executor::task]
async fn master_event_handler(
    receiver: event_channel::EventReceiver<'static>,
    layout: &'static layers::SharedLayout,
) {
    info!("Start master_event_handler");
    loop {
        let event = receiver.recv().await;
        debug!("Received Event: {:?}", defmt::Debug2Format(&event));

        let key_event = match event.into_keyberon() {
            Some(e) => e,
            None => continue,
        };
        layout.lock(|l| {
            l.borrow_mut().event(key_event);
        });
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
    mut tx: UartTx<'static, peripherals::USART3, DMA2_CH1>,
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
async fn right_uart_read_task(mut comm_rx: comm::CommRx<'static, peripherals::USART3, DMA1_CH1>) {
    info!("Start right_uart_read_task");
    comm_rx.run().await;
}
