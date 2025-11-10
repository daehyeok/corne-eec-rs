use crate::eck::analog::{ADCReader, DisChargeDelay};
use embassy_stm32::{adc, peripherals::ADC1, Peri};

pub struct Adc<'a, ADCPIN: adc::AdcChannel<ADC1>> {
    stm32_adc: adc::Adc<'a, ADC1>,
    pin: ADCPIN,
}

impl<'a, ADCPIN: adc::AdcChannel<ADC1>> Adc<'a, ADCPIN> {
    pub fn new(adc1: Peri<'a, ADC1>, pin: ADCPIN) -> Self {
        let stm32_adc = adc::Adc::new(adc1);
        Self { stm32_adc, pin }
    }
}

impl<'a, ADCPIN: adc::AdcChannel<ADC1>> ADCReader for Adc<'a, ADCPIN>
where
    ADCPIN: adc::AdcChannel<ADC1>,
{
    #[inline(always)]
    fn read(&mut self) -> u16 {
        self.stm32_adc.blocking_read(&mut self.pin)
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
