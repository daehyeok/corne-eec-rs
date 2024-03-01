use digital_hal::OutputPin;
use embedded_hal::digital::v2 as digital_hal;

use crate::mux::Multiplxer;

pub trait ADCReader {
    type AdcUnit: core::fmt::Debug
        + core::cmp::PartialOrd
        + core::marker::Copy
        + core::default::Default;
    fn read(&mut self) -> Self::AdcUnit;
}

pub trait RxModule {
    type AdcUnit: core::fmt::Debug
        + core::cmp::PartialOrd
        + core::marker::Copy
        + core::default::Default;
    fn select(&mut self, idx: usize);
    fn read(&mut self) -> Self::AdcUnit;
}

pub trait DisChargeDelay {
    fn delay(&mut self);
}

pub trait TxModule {
    fn charge_capacitor(&mut self, idx: usize);
    fn discharge_capacitor(&mut self, idx: usize);
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
    type AdcUnit = ADC::AdcUnit;
    #[inline(always)]
    fn read(&mut self) -> ADC::AdcUnit {
        self.adc.read()
    }

    #[inline(always)]
    fn select(&mut self, idx: usize) {
        self.mux.select(idx).unwrap();
    }
}

pub struct TxCharger<OPIN, ODPIN, DELAY, const CS: usize> {
    drain_pin: ODPIN,
    channel_pins: [OPIN; CS],
    discharge_delay: DELAY,
}

impl<OPIN, ODPIN, DELAY, const TX_SIZE: usize> TxCharger<OPIN, ODPIN, DELAY, TX_SIZE>
where
    ODPIN: OutputPin,
    OPIN: OutputPin,
    DELAY: DisChargeDelay,
{
    pub fn new(drain_pin: ODPIN, channel_pins: [OPIN; TX_SIZE], discharge_delay: DELAY) -> Self {
        let mut charger = Self {
            drain_pin,
            channel_pins,
            discharge_delay,
        };

        for i in 0..TX_SIZE {
            charger.set_low(i);
        }

        charger
    }

    #[inline(always)]
    fn set_high(&mut self, idx: usize) {
        if self.channel_pins[idx].set_high().is_err() {
            panic!("Failed to set TxPin: {} high", idx)
        };
    }

    #[inline(always)]
    fn set_low(&mut self, idx: usize) {
        if self.channel_pins[idx].set_low().is_err() {
            panic!("Failed to set TxPin: {} low", idx)
        }
    }
}

impl<OPIN, ODPIN, DELAY, const TX_SIZE: usize> TxModule for TxCharger<OPIN, ODPIN, DELAY, TX_SIZE>
where
    OPIN: OutputPin,
    ODPIN: OutputPin,
    DELAY: DisChargeDelay,
{
    #[inline(always)]
    fn charge_capacitor(&mut self, idx: usize) {
        if self.drain_pin.set_high().is_err() {
            panic!("Failed to open drain pin.")
        }

        self.set_high(idx);
    }

    #[inline(always)]
    fn discharge_capacitor(&mut self, idx: usize) {
        self.set_low(idx);
        if self.drain_pin.set_low().is_err() {
            panic!("Failed to ground drain pin.")
        }
        self.discharge_delay.delay();
    }
}
