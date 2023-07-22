use defmt::{debug, error};
use embassy_nrf::gpio::{AnyPin, Input};
use embassy_nrf::usb::vbus_detect::SoftwareVbusDetect;
use embassy_nrf::usb::Driver;
use embassy_nrf::{bind_interrupts, peripherals, usb};
use embassy_time::Timer;
use embassy_usb::class::hid::{HidReader, HidReaderWriter, HidWriter, State};
use embassy_usb::{Builder, Config};
use static_cell::StaticCell;
use usbd_hid::descriptor::{KeyboardReport, SerializedDescriptor};

use {defmt_rtt as _, panic_probe as _};

/// USB VID, PID for a generic keyboard from
/// https://github.com/obdev/v-usb/blob/master/usbdrv/USB-IDs-for-free.txt
const VID: u16 = 0x16c0;
const PID: u16 = 0x27db;
const USB_MANUFACTURER: &str = "Daehyeok Mun";
const USB_PRODUCT: &str = "Corne EEC Nordic";
const USB_SERIAL_NUMBER: &str = "87654321";
const READ_N: usize = 1;
const WRITE_N: usize = 8;

//Type alias for generic USB types.
pub type NrfUsbDriver<'a> = Driver<'a, peripherals::USBD, &'a SoftwareVbusDetect>;
pub type NrfHidReaderWriter<'a> = HidReaderWriter<'a, NrfUsbDriver<'a>, READ_N, WRITE_N>;
pub type NrfHidWriter<'a> = HidWriter<'a, NrfUsbDriver<'a>, WRITE_N>;
pub type NrfHidReader<'a> = HidReader<'a, NrfUsbDriver<'a>, READ_N>;
pub type NrfUsbDevice<'a> = embassy_usb::UsbDevice<'a, NrfUsbDriver<'a>>;

pub struct UsbHid<'a> {
    pub reader: NrfHidReader<'a>,
    pub writer: NrfHidWriter<'a>,
    pub device: NrfUsbDevice<'a>,
}

// Store everything on static.
static VBUS_PIN: StaticCell<Input<AnyPin>> = StaticCell::new();
static VBUS_DETECT: StaticCell<VbusDetect> = StaticCell::new();
static USB_CONFIG: StaticCell<Config> = StaticCell::new();
static USB_BUFFER: StaticCell<UsbBuffer> = StaticCell::new();
static USB_STATE: StaticCell<State> = StaticCell::new();
static USB_HID: StaticCell<UsbHid> = StaticCell::new();

bind_interrupts!(struct Irqs {
    USBD => usb::InterruptHandler<peripherals::USBD>;
    POWER_CLOCK => usb::vbus_detect::InterruptHandler;
});

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

struct VbusDetect<'a> {
    software_detect: SoftwareVbusDetect,
    vbus_input: &'a Input<'a, AnyPin>,
}

impl<'a> VbusDetect<'a> {
    pub fn new(vbus_input: &'a Input<'a, AnyPin>) -> Self {
        let connected = vbus_input.is_high();
        let software_detect = SoftwareVbusDetect::new(connected, connected);
        Self {
            software_detect,
            vbus_input,
        }
    }

    #[allow(dead_code)]
    pub fn detect(&self) {
        debug!("Detecting vbus");
        self.software_detect.detected(self.vbus_input.is_high());
    }

    pub fn sofware_vbus_detect(&self) -> &SoftwareVbusDetect {
        &self.software_detect
    }
}

pub fn init(
    usbd: embassy_nrf::peripherals::USBD,
    input: Input<'static, AnyPin>,
) -> &mut UsbHid<'static> {
    let vbus_input = VBUS_PIN.init(input);
    let vbus_detect = VBUS_DETECT.init(VbusDetect::new(vbus_input));

    // Create embassy-usb Config
    let config = USB_CONFIG.init(Config::new(VID, PID));
    config.manufacturer = Some(USB_MANUFACTURER);
    config.product = Some(USB_PRODUCT);
    config.serial_number = Some(USB_SERIAL_NUMBER);
    config.max_power = 100;
    config.max_packet_size_0 = 64;

    let buffer = USB_BUFFER.init(UsbBuffer::new());
    let driver = Driver::new(usbd, Irqs, vbus_detect.sofware_vbus_detect());

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

    // Create classes on the builder.
    let config = embassy_usb::class::hid::Config {
        report_descriptor: KeyboardReport::desc(),
        request_handler: None,
        poll_ms: 1,
        max_packet_size: 64,
    };

    let rw = NrfHidReaderWriter::new(&mut builder, state, config);
    let (reader, writer) = rw.split();
    let device = builder.build();

    // Build the builder.
    USB_HID.init(UsbHid {
        reader,
        writer,
        device,
    })
}

#[embassy_executor::task]
pub async fn usb_device_handler(device: &'static mut NrfUsbDevice<'static>) {
    // Run the USB device.
    device.run().await;
}

#[embassy_executor::task]
pub async fn keyberon_tick(
    hid_writer: &'static mut NrfHidWriter<'static>,
    layout: &'static crate::layers::SharedLayout,
) {
    let mut cur_report: keyberon::key_code::KbHidReport =
        layout.lock(|l| l.borrow().keycodes().collect());

    loop {
        // send key report to USB HID
        let keyberon_report: keyberon::key_code::KbHidReport = layout.lock(|l| {
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
            if let Err(e) = hid_writer.write_serialize(&report).await {
                error!("USB hid report error: {}", e);
            };
        }

        cur_report = keyberon_report;
        Timer::after(crate::config::TICK_PERIOD).await;
    }
}
