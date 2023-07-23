use crate::analog::{RxModule, TxModule};
use crate::debounce::Debouncer;
use crate::error::KeyboardError;
use crate::event::Event;
use defmt::*;

pub trait Scanner {
    // return changed key's coordnation.
    // if reach the end of matrix return None,
    // Then next call will re-start from front of matrix.
    fn scan(&mut self) -> Result<Option<Event>, KeyboardError>;
}

// use keyberon::layout::Event;
pub struct ECScanner<TX, RX, const TXSIZE: usize, const RXSIZE: usize>
where
    TX: TxModule,
    RX: RxModule,
{
    rx: RX,
    tx: TX,
    transform: fn(u8, u8) -> (u8, u8),

    debouncer: Debouncer<TXSIZE, RXSIZE>,

    thresholds: [[RX::AdcUnit; RXSIZE]; TXSIZE],
    values: [[RX::AdcUnit; RXSIZE]; TXSIZE],

    coord_iter: CoordIterator<TXSIZE, RXSIZE>,
}

impl<TX, RX, const TXSIZE: usize, const RXSIZE: usize> ECScanner<TX, RX, TXSIZE, RXSIZE>
where
    TX: TxModule,
    RX: RxModule,
{
    pub fn new(
        tx: TX,
        rx_mux: RX,
        transform: fn(u8, u8) -> (u8, u8),
        nb_bounce: u8,
        thresholds: [[RX::AdcUnit; RXSIZE]; TXSIZE],
    ) -> Self {
        Self {
            tx,
            rx: rx_mux,
            transform,

            debouncer: Debouncer::new(nb_bounce),
            thresholds,
            values: [[RX::AdcUnit::default(); RXSIZE]; TXSIZE],
            coord_iter: CoordIterator::<TXSIZE, RXSIZE>::new(),
        }
    }

    #[inline(always)]
    fn read_raw(&mut self, coord: &MatrixCoord) -> RX::AdcUnit {
        self.tx.charge_capacitor(coord.tx);
        self.rx.read()
    }

    fn scan_raw(&mut self, coord: &MatrixCoord) -> Result<Option<Event>, KeyboardError> {
        #![allow(unused_assignments)]
        let mut value: RX::AdcUnit = RX::AdcUnit::default();

        self.rx.select(coord.rx);
        #[cfg(feature = "cortex-m")]
        {
            cortex_m::interrupt::free(|_| value = self.read_raw(coord));
        }

        #[cfg(not(feature = "cortex-m"))]
        {
            value = self.read_raw(coord);
        }
        self.tx.discharge_capacitor(coord.tx);

        self.values[coord.tx][coord.rx] = value;
        let is_pressed = value > self.thresholds[coord.tx][coord.rx];

        if self.debouncer.update(coord.tx, coord.rx, is_pressed)? {
            let (tx, rx) = (self.transform)(coord.tx as u8, coord.rx as u8);
            let e = match is_pressed {
                true => Event::KeyPress(tx, rx),
                false => Event::KeyRelease(tx, rx),
            };

            if let Event::KeyPress(_, _) = e {
                debug!(
                    "Key press event: ({:?}, {:?}) -> ({:?}, {:?}) - {:?}",
                    coord.tx,
                    coord.rx,
                    tx,
                    rx,
                    Debug2Format(&value)
                );
            }

            return Ok(Some(e));
        };

        Ok(None)
    }

    pub fn raw_values(&self) -> &[[RX::AdcUnit; RXSIZE]; TXSIZE] {
        &self.values
    }

    //discharge all lines for inital bounding.
    pub fn dischage_all(&mut self) {
        for rx_idx in 0..RXSIZE {
            self.rx.select(rx_idx);
            (0..TXSIZE).for_each(|tx| self.tx.discharge_capacitor(tx));
        }
    }

    pub fn scan(&mut self) -> Option<Event> {
        while let Some(coord) = self.coord_iter.next() {
            match self.scan_raw(&coord) {
                Ok(Some(e)) => return Some(e),
                _ => continue,
            }
        }
        None
    }
}

#[derive(defmt::Format, Debug, Clone)]
struct MatrixCoord {
    tx: usize,
    rx: usize,
}

struct CoordIterator<const TXSIZE: usize, const RXSIZE: usize> {
    coord: MatrixCoord,
}

impl<const TXSIZE: usize, const RXSIZE: usize> CoordIterator<TXSIZE, RXSIZE> {
    fn new() -> Self {
        Self {
            coord: MatrixCoord { tx: 0, rx: 0 },
        }
    }

    pub fn reset(&mut self) {
        self.coord.tx = 0;
        self.coord.rx = 0;
    }
}

impl<const TXSIZE: usize, const RXSIZE: usize> Iterator for CoordIterator<TXSIZE, RXSIZE> {
    type Item = MatrixCoord;

    fn next(&mut self) -> Option<Self::Item> {
        // reach the end of matrix
        if self.coord.rx == RXSIZE {
            self.reset();
            return None;
        }

        let cur = self.coord.clone();
        // calculate next coord
        self.coord.tx += 1;
        self.coord.rx += self.coord.tx / TXSIZE;
        self.coord.tx %= TXSIZE;

        Some(cur)
    }
}
