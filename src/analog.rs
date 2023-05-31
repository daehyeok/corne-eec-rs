use digital_hal::OutputPin;
use embedded_hal::digital::v2 as digital_hal;

use crate::error::KeyboardError;
use crate::mux::Multiplxer;

pub trait ADCReader {
    fn read(&mut self) -> Result<u16, KeyboardError>;
}

pub trait RxModule {
    fn select(&mut self, idx: usize) -> Result<(), KeyboardError>;
    fn read(&mut self) -> Result<u16, KeyboardError>;
}

pub trait ScanDelay {
    fn charge(&mut self);
    fn discharge(&mut self);
}

pub trait TxModule {
    fn charge_capacitor(&mut self, idx: usize) -> Result<(), KeyboardError>;
    fn discharge_capacitor(&mut self, idx: usize) -> Result<(), KeyboardError>;
}

pub struct RxMux<MUX, ADC> {
    mux: MUX,
    adc: ADC,
}

impl<MUX, ADC> RxMux<MUX, ADC>
where
    MUX: Multiplxer,
    ADC: ADCReader,
{
    pub fn new(mux: MUX, adc: ADC) -> Self {
        Self { mux, adc }
    }
}

impl<MUX, ADC> RxModule for RxMux<MUX, ADC>
where
    MUX: Multiplxer,
    ADC: ADCReader,
{
    fn read(&mut self) -> Result<u16, KeyboardError> {
        self.adc.read()
    }

    fn select(&mut self, idx: usize) -> Result<(), KeyboardError> {
        self.mux.select(idx)
    }
}

pub struct TxCharger<OPIN, CPIN, DELAY, const CS: usize> {
    charge_pin: CPIN,
    channel_pins: [OPIN; CS],
    delay: DELAY,
}

impl<OPIN, CPIN, DELAY, const TX_SIZE: usize> TxCharger<OPIN, CPIN, DELAY, TX_SIZE>
where
    CPIN: OutputPin,
    OPIN: OutputPin,
    DELAY: ScanDelay,
{
    pub fn new(
        charge_pin: CPIN,
        channel_pins: [OPIN; TX_SIZE],
        delay: DELAY,
    ) -> Result<Self, KeyboardError> {
        let mut reader = Self {
            charge_pin,
            channel_pins,
            delay,
        };

        for i in 0..TX_SIZE {
            reader.set_low(i)?
        }

        Ok(reader)
    }

    fn set_high(&mut self, idx: usize) -> Result<(), KeyboardError> {
        match self.channel_pins[idx].set_high() {
            Ok(_) => Ok(()),
            Err(_) => Err(KeyboardError::Gpio),
        }
    }

    fn set_low(&mut self, idx: usize) -> Result<(), KeyboardError> {
        match self.channel_pins[idx].set_low() {
            Ok(_) => Ok(()),
            Err(_) => Err(KeyboardError::Gpio),
        }
    }
}

impl<OPIN, CPIN, DELAY, const TX_SIZE: usize> TxModule for TxCharger<OPIN, CPIN, DELAY, TX_SIZE>
where
    OPIN: OutputPin,
    CPIN: OutputPin,
    DELAY: ScanDelay,
{
    fn charge_capacitor(&mut self, idx: usize) -> Result<(), KeyboardError> {
        self.charge_pin.set_high().ok();
        self.set_high(idx)?;
        self.delay.charge();
        Ok(())
    }

    fn discharge_capacitor(&mut self, idx: usize) -> Result<(), KeyboardError> {
        self.set_low(idx)?;
        self.charge_pin.set_low().ok();
        self.delay.discharge();
        Ok(())
    }
}
