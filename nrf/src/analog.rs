use eck_rs::analog::{ADCReader, DisChargeDelay};
use embassy_futures::block_on;
use embassy_nrf::{
    bind_interrupts,
    peripherals::SAADC,
    saadc::{self, ChannelConfig, Config, Input, Oversample, Reference, Resolution, Saadc, Time},
    Peripheral,
};

bind_interrupts!(struct Irqs {
    SAADC => saadc::InterruptHandler;
});

pub struct Adc<'a>(Saadc<'a, 1>);

impl<'a> Adc<'a> {
    pub fn new(saadc: SAADC, adc_pin: impl Peripheral<P = impl Input>) -> Self {
        let mut adc_config = Config::default();
        adc_config.resolution = Resolution::_14BIT;
        adc_config.oversample = Oversample::BYPASS;

        let mut channel_config = ChannelConfig::single_ended(adc_pin);
        channel_config.time = Time::_3US;
        //channel_config.gain = Gain::GAIN2;
        channel_config.reference = Reference::INTERNAL;

        let saadc = Saadc::new(saadc, Irqs, adc_config, [channel_config]);
        Self(saadc)
    }
}

impl<'a> ADCReader for Adc<'a> {
    type AdcUnit = i16;

    #[inline(always)]
    fn read(&mut self) -> i16 {
        let mut buf = [0; 1];
        block_on(self.0.sample(&mut buf));
        buf[0]
    }
}

pub struct NrfDisChargeDelay;

impl NrfDisChargeDelay {
    pub fn new() -> Self {
        Self {}
    }
}

impl DisChargeDelay for NrfDisChargeDelay {
    #[inline(always)]
    fn delay(&mut self) {
        cortex_m::asm::delay(crate::config::DISCHARGE_DELAY_CLOCKS);
    }
}
