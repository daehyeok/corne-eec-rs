use eck_rs::analog::{ADCReader, DisChargeDelay};
use embassy_stm32::{adc, peripherals};

pub struct Adc<'a, ADCPIN: adc::AdcPin<peripherals::ADC2>> {
    stm32_adc: adc::Adc<'a, peripherals::ADC2>,
    pin: ADCPIN,
}

impl<'a, ADCPIN: adc::AdcPin<peripherals::ADC2>> Adc<'a, ADCPIN> {
    pub fn new(adc1: peripherals::ADC2, pin: ADCPIN) -> Self {
        let stm32_adc = adc::Adc::new(adc1, &mut embassy_time::Delay);
        Self { stm32_adc, pin }
    }
}

impl<'a, ADCPIN> ADCReader for Adc<'a, ADCPIN>
where
    ADCPIN: adc::AdcPin<peripherals::ADC2> + embassy_stm32::gpio::low_level::Pin,
{
    type AdcUnit = u16;

    #[inline(always)]
    fn read(&mut self) -> u16 {
        self.stm32_adc.read(&mut self.pin)
    }
}

pub struct CortexDisChargeDelay;

impl CortexDisChargeDelay {
    pub fn new() -> Self {
        Self {}
    }
}

impl DisChargeDelay for CortexDisChargeDelay {
    #[inline(always)]
    fn delay(&mut self) {
        cortex_m::asm::delay(crate::config::DISCHARGE_DELAY_CLOCKS);
    }
}
