#![no_std]
#![no_main]
#![feature(type_alias_impl_trait)]
use core::mem;
use defmt::{debug, error, info, unwrap};
use eck_rs::{
    self,
    analog::{RxMux, TxCharger},
    mux::Mux8,
    scanner::ECScanner,
};
use embassy_executor::Spawner;
use embassy_nrf::{
    bind_interrupts,
    gpio::{self, AnyPin, Flex, Input, Output, OutputDrive},
    nvmc::Nvmc,
    pac, peripherals,
    uarte::{self, Uarte, UarteRx, UarteTx},
};
use embassy_time::Timer;
use static_cell::StaticCell;
use {defmt_rtt as _, panic_probe as _};

mod analog;
mod config;
mod event;
mod hid;
mod layers;

static EVENT_CHANNEL: StaticCell<event::EventChannel> = StaticCell::new();
static SHARED_LAYOUT: StaticCell<layers::SharedLayout> = StaticCell::new();

bind_interrupts!(struct Irqs {
    UARTE0_UART0 => uarte::InterruptHandler<peripherals::UARTE0>;
});

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    let p = embassy_nrf::init(Default::default());
    let clock: pac::CLOCK = unsafe { mem::transmute(()) };

    clock.tasks_hfclkstart.write(|w| unsafe { w.bits(1) });
    while clock.events_hfclkstarted.read().bits() != 1 {}

    let mut handness_pin = Flex::new(AnyPin::from(p.P1_04));
    handness_pin.set_as_input(gpio::Pull::Down);

    let (mut matrix_cfg, adc) = match handness_pin.is_low() {
        true => {
            info!("Init Left Keyboard");
            (
                config::MatrixConfig {
                    vbus_detect: pulldown_input!(p.P1_13),
                    col_mux_enable: pushpull_output!(p.P1_02),
                    col_mux_sels: pushpull_output! {p.P1_03, p.P1_00, p.P0_22},
                    col_mux_channel: [7, 5, 2, 1, 0, 3, 6],
                    drain: handness_pin,
                    row_pins: pushpull_output!(p.P0_12, p.P0_23, p.P0_21, p.P0_19),
                    uart_tx: AnyPin::from(p.P1_11),
                    uart_rx: AnyPin::from(p.P1_10),
                    transform: config::left_matrix_transform,
                    thresholds: [[3000i16; 7]; 4],
                    nbounce: 2,
                },
                analog::Adc::new(p.SAADC, p.P0_03),
            )
        }
        false => {
            info!("Init Right Keyboard");
            (
                config::MatrixConfig {
                    vbus_detect: pulldown_input!(p.P1_06),
                    col_mux_enable: pushpull_output!(p.P0_29),
                    col_mux_sels: pushpull_output! {p.P0_04, p.P0_31, p.P0_30},
                    col_mux_channel: [5, 2, 1, 0, 3, 6, 7],
                    drain: Flex::new(AnyPin::from(p.P0_28)),
                    row_pins: pushpull_output!(p.P0_19, p.P0_21, p.P0_23, p.P0_12),
                    uart_tx: AnyPin::from(p.P0_09),
                    uart_rx: AnyPin::from(p.P0_10),
                    transform: config::right_matrix_transform,
                    thresholds: [[4024i16; 7]; 4],
                    nbounce: 2,
                },
                analog::Adc::new(p.SAADC, p.P0_03),
            )
        }
    };

    let _f = Nvmc::new(p.NVMC);

    let is_master = matrix_cfg.vbus_detect.is_high();
    info!("USB connected: {}", is_master);

    // Declare a bounded chappppnnnel of 3 u32s.
    let channel = EVENT_CHANNEL.init(event::EventChannel::new());

    // Config UART
    let uart = Uarte::new(
        p.UARTE0,
        Irqs,
        matrix_cfg.uart_rx,
        matrix_cfg.uart_tx,
        config::uarte_config(),
    );

    let (uart_tx, uart_rx) = uart.split();

    let layout = SHARED_LAYOUT.init(layers::new_shared_layout());
    unwrap!(spawner.spawn(uart_reader(uart_rx, channel.sender())));

    if is_master {
        // Create the driver, from the HAL.
        let usb_hid = hid::init(p.USBD, matrix_cfg.vbus_detect);
        unwrap!(spawner.spawn(hid::usb_device_handler(&mut usb_hid.device)));
        unwrap!(spawner.spawn(hid::keyberon_tick(&mut usb_hid.writer, layout)));
        unwrap!(spawner.spawn(master_event_handler(channel.receiver(), layout)));
    } else {
        unwrap!(spawner.spawn(slave_event_handler(channel.receiver(), uart_tx)));
    }

    let mux8 = unwrap!(Mux8::new(
        matrix_cfg.col_mux_enable,
        matrix_cfg.col_mux_sels,
        matrix_cfg.col_mux_channel,
    ));
    matrix_cfg.drain.set_low();
    matrix_cfg
        .drain
        .set_as_input_output(gpio::Pull::Down, OutputDrive::Standard0Disconnect1);
    let rx_mux = RxMux::new(mux8, adc);
    let discharge_delay = analog::NrfDisChargeDelay::new();
    let tx_charger = TxCharger::new(matrix_cfg.drain, matrix_cfg.row_pins, discharge_delay);
    let mut scanner = ECScanner::new(
        tx_charger,
        rx_mux,
        matrix_cfg.nbounce,
        matrix_cfg.thresholds,
    );

    //start matrix scan.
    let event_sender = channel.sender();
    loop {
        while let Some(e) = scanner.scan() {
            debug!("Scanned Event: {:?}", defmt::Debug2Format(&e));
            event_sender
                .send(e.transform(matrix_cfg.transform).into())
                .await;
        }

        //debug_print(scanner.raw_values());
        Timer::after(config::SCAN_DELAY).await;
    }
}

pub fn debug_print(matrix: &[[i16; 7]; 4]) {
    debug! {"START------------"}
    matrix.iter().for_each(|r| debug!("{:?}", r));
    debug! {"END--------------"}
}

#[embassy_executor::task]
async fn master_event_handler(
    receiver: event::EventReceiver<'static>,
    layout: &'static layers::SharedLayout,
) {
    loop {
        let event = receiver.recv().await;
        debug!(
            "Received transformed Event: {:?}",
            defmt::Debug2Format(&event)
        );

        let key_event = match event.into_keyberon() {
            Some(e) => e,
            None => continue,
        };
        layout.lock(|l| {
            l.borrow_mut().event(key_event);
        });
    }
}

#[embassy_executor::task]
async fn slave_event_handler(
    receiver: event::EventReceiver<'static>,
    mut uart_tx: UarteTx<'static, peripherals::UARTE0>,
) {
    loop {
        let event = receiver.recv().await;
        debug!(
            "Received transformed Event: {:?}",
            defmt::Debug2Format(&event)
        );

        debug!("Send event to other side");
        // send event to other halve.
        let buf = match event.serialize() {
            Ok(d) => d,
            Err(_) => {
                error!("Serialize error: {}", event);
                continue;
            }
        };
        if let Err(e) = uart_tx.write(&buf).await {
            error!("UART send error: {}", defmt::Debug2Format(&e))
        }
    }
}

#[embassy_executor::task]
async fn uart_reader(
    mut rx: UarteRx<'static, peripherals::UARTE0>,
    event_sender: event::EventSender<'static>,
) {
    loop {
        debug!("Trying read");
        let mut buf = [0; 3];
        if let Err(e) = rx.read(&mut buf).await {
            error!("UART read error: {}", e);
            continue;
        }

        info!("Received uart data: {:?}", buf);
        match event::Event::deserialize(&buf) {
            Ok(e) => event_sender.send(e).await,
            Err(_) => error!("Deserailize error: {:?}", buf),
        }
    }
}
