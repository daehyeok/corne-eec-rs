use digital_hal::OutputPin;
use embedded_hal::digital::v2 as digital_hal;

use crate::error::KeyboardError;

pub trait Multiplxer {
    fn enable(&mut self) -> Result<(), KeyboardError>;
    fn disable(&mut self) -> Result<(), KeyboardError>;
    fn select(&mut self, idx: usize) -> Result<(), KeyboardError>;
}

// for 74HC4051. Active low.
pub struct Mux8<O, const CS: usize>
where
    O: OutputPin,
{
    enable_pin: O,
    select_pins: [O; 3],
    channels: [u8; CS],
}

impl<O, E, const CS: usize> Mux8<O, CS>
where
    O: OutputPin<Error = E>,
{
    pub fn new(
        enable_pin: O,
        select_pins: [O; 3],
        channels: [u8; CS],
    ) -> Result<Self, KeyboardError> {
        let mut mux = Self {
            enable_pin,
            select_pins,
            channels,
        };

        mux.disable()?;
        Ok(mux)
    }
}

impl<O, const CS: usize> Multiplxer for Mux8<O, CS>
where
    O: OutputPin,
{
    fn enable(&mut self) -> Result<(), KeyboardError> {
        match self.enable_pin.set_low() {
            Ok(_) => Ok(()),
            Err(_) => Err(KeyboardError::Gpio),
        }
    }

    fn disable(&mut self) -> Result<(), KeyboardError> {
        match self.enable_pin.set_high() {
            Ok(_) => Ok(()),
            Err(_) => Err(KeyboardError::Gpio),
        }
    }

    fn select(&mut self, idx: usize) -> Result<(), KeyboardError> {
        self.disable()?;

        if CS < idx {
            return Err(KeyboardError::ColOutOfRange(idx));
        }

        let ch = self.channels[idx];
        if 7 < ch {
            return Err(KeyboardError::MuxOutOfRange(idx));
        }

        let mut mask: u8 = 1;
        for pin in self.select_pins.iter_mut() {
            let _ = match ch & mask != 0 {
                true => pin.set_high(),
                false => pin.set_low(),
            };
            mask <<= 1;
        }

        self.enable()
    }
}
