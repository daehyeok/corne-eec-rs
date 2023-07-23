use defmt::{debug, error, info};
use embassy_stm32::{peripherals, usb::Driver};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::signal::Signal;
use embassy_time::Timer;
use embassy_usb::class::hid::{HidReader, HidReaderWriter, HidWriter, State};
use embassy_usb::{Builder, Config, Handler};

use static_cell::StaticCell;
use usbd_hid::descriptor::{KeyboardReport, SerializedDescriptor};

use crate::config::{
    TICK_PERIOD, USB_MANUFACTURER, USB_PID, USB_PRODUCT, USB_SERIAL_NUMBER, USB_VID,
};
use {defmt_rtt as _, panic_probe as _};

const READ_N: usize = 1;
const WRITE_N: usize = 8;

//Type alias for generic USB types.
pub type Stm32UsbDriver<'a> = Driver<'a, peripherals::USB>;
pub type Stm32HidReaderWriter<'a> = HidReaderWriter<'a, Stm32UsbDriver<'a>, READ_N, WRITE_N>;
pub type Stm32HidWriter<'a> = HidWriter<'a, Stm32UsbDriver<'a>, WRITE_N>;
pub type Stm32HidReader<'a> = HidReader<'a, Stm32UsbDriver<'a>, READ_N>;
pub type Stm32UsbDevice<'a> = embassy_usb::UsbDevice<'a, Stm32UsbDriver<'a>>;

pub struct UsbHid<'a> {
    pub reader: Stm32HidReader<'a>,
    pub writer: Stm32HidWriter<'a>,
    pub device: Stm32UsbDevice<'a>,
}

static CONFIGURED: Signal<CriticalSectionRawMutex, bool> = Signal::new();
static SUSPENDED: Signal<CriticalSectionRawMutex, bool> = Signal::new();

// Store everything on static.
static USB_CONFIG: StaticCell<Config> = StaticCell::new();
static USB_BUFFER: StaticCell<UsbBuffer> = StaticCell::new();
static USB_STATE: StaticCell<State> = StaticCell::new();
static USB_HID: StaticCell<UsbHid> = StaticCell::new();
static DEVICE_HANDLER: StaticCell<DeviceStateHandler> = StaticCell::new();

// embassy-usb DeviceBuilder needs some buffers for building the descriptors.
struct UsbBuffer {
    device_descriptor: [u8; 256],
    config_descriptor: [u8; 256],
    bos_descriptor: [u8; 256],
    msos_descriptor: [u8; 256],
    control_buf: [u8; 64],
}

impl UsbBuffer {
    pub fn new() -> Self {
        Self {
            device_descriptor: [0u8; 256],
            config_descriptor: [0u8; 256],
            bos_descriptor: [0u8; 256],
            msos_descriptor: [0u8; 256],
            control_buf: [0u8; 64],
        }
    }
}

pub fn init(driver: Stm32UsbDriver<'static>) -> &'static mut UsbHid<'static> {
    // Create embassy-usb Config
    let config = USB_CONFIG.init(Config::new(USB_VID, USB_PID));
    config.manufacturer = Some(USB_MANUFACTURER);
    config.product = Some(USB_PRODUCT);
    config.serial_number = Some(USB_SERIAL_NUMBER);
    config.max_power = 100;
    config.max_packet_size_0 = 64;

    let buffer = USB_BUFFER.init(UsbBuffer::new());

    // Create embassy-usb DeviceBuilder using the driver and config.
    let state = USB_STATE.init(State::new());
    let mut builder = Builder::new(
        driver,
        *config,
        &mut buffer.device_descriptor,
        &mut buffer.config_descriptor,
        &mut buffer.bos_descriptor,
        &mut buffer.msos_descriptor,
        &mut buffer.control_buf,
    );

    let handler = DEVICE_HANDLER.init(DeviceStateHandler::new());
    builder.handler(handler);

    // Create classes on the builder.
    let config = embassy_usb::class::hid::Config {
        report_descriptor: KeyboardReport::desc(),
        request_handler: None,
        poll_ms: 1,
        max_packet_size: 64,
    };

    let rw = Stm32HidReaderWriter::new(&mut builder, state, config);
    let (reader, writer) = rw.split();
    let device = builder.build();

    // Build the builder.
    USB_HID.init(UsbHid {
        reader,
        writer,
        device,
    })
}

pub async fn wait_until_configured() {
    while let false = CONFIGURED.wait().await {}
}

struct DeviceStateHandler {}

impl DeviceStateHandler {
    fn new() -> Self {
        Self {}
    }
}

impl Handler for DeviceStateHandler {
    fn enabled(&mut self, enabled: bool) {
        debug!("USB enabled: {:?}", enabled);
        CONFIGURED.signal(false);
        SUSPENDED.signal(false);
    }

    fn reset(&mut self) {
        debug!("USB reset");
        CONFIGURED.signal(false);
    }

    fn addressed(&mut self, _addr: u8) {
        debug!("USB addressed");
        CONFIGURED.signal(false);
    }

    fn configured(&mut self, configured: bool) {
        debug!("USB configured: {:?}", configured);
        CONFIGURED.signal(configured);
    }

    fn suspended(&mut self, suspended: bool) {
        debug!("USB suspended: {:?}", suspended);
        if suspended {
            SUSPENDED.signal(true);
        } else {
            SUSPENDED.signal(false);
        }
    }
}

#[embassy_executor::task]
pub async fn usb_device_task(device: &'static mut Stm32UsbDevice<'static>) {
    // Run the USB device.
    info!("Start USB device task.");
    device.run().await;
}

pub struct KeyberonTickRes<'a> {
    hid_writer: &'a mut Stm32HidWriter<'a>,
    layout: &'a crate::layers::SharedLayout,
}

impl<'a> KeyberonTickRes<'a> {
    pub fn new(
        hid_writer: &'a mut Stm32HidWriter<'a>,
        layout: &'a crate::layers::SharedLayout,
    ) -> Self {
        Self { hid_writer, layout }
    }
}

#[embassy_executor::task]
pub async fn keyberon_tick(res: &'static mut KeyberonTickRes<'static>) {
    let mut cur_report: keyberon::key_code::KbHidReport =
        res.layout.lock(|l| l.borrow().keycodes().collect());

    loop {
        // send key report to USB HID
        let keyberon_report: keyberon::key_code::KbHidReport = res.layout.lock(|l| {
            l.borrow_mut().tick();
            l.borrow().keycodes().collect()
        });

        if cur_report != keyberon_report {
            let bytes = keyberon_report.as_bytes();
            let report = KeyboardReport {
                modifier: bytes[0],
                reserved: 0,
                leds: bytes[1],
                keycodes: [bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7]],
            };

            debug!("USB report: {:?}", bytes);
            if let Err(e) = res.hid_writer.write_serialize(&report).await {
                error!("USB hid report error: {}", e);
            };
        }

        cur_report = keyberon_report;
        Timer::after(TICK_PERIOD).await;
    }
}
