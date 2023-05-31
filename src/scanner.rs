use core::{self, char};

use heapless::String;
use keyberon::layout::Event;
use log;

use crate::analog::{RxModule, TxModule};
use crate::debounce::Debouncer;
use crate::error::KeyboardError;

#[allow(dead_code)]
fn debug_print_2d_arr<const ROWS: usize, const COLS: usize>(arr: &[[u16; COLS]; ROWS]) {
    let mut s: String<1024> = String::new();

    for r in arr.iter() {
        for c in r.iter() {
            let d = u32::from(*c);
            // ugly implement of string converting.
            s.push(char::from_digit((d / 1000) % 10, 10).unwrap())
                .unwrap();
            s.push(char::from_digit((d / 100) % 10, 10).unwrap())
                .unwrap();
            s.push(char::from_digit((d / 10) % 10, 10).unwrap())
                .unwrap();
            s.push(char::from_digit(d % 10, 10).unwrap()).unwrap();
            s.push_str(", ").unwrap();
        }
        s.push_str("\n").unwrap();
    }

    log::debug!("{}", s);
}

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

    debouncer: Debouncer<TXSIZE, RXSIZE>,

    limits: [[u16; RXSIZE]; TXSIZE],
    values: [[u16; RXSIZE]; TXSIZE],

    coord_iter: CoordIterator<TXSIZE, RXSIZE>,
}

impl<TX, RX, const TXSIZE: usize, const RXSIZE: usize> ECScanner<TX, RX, TXSIZE, RXSIZE>
where
    TX: TxModule,
    RX: RxModule,
{
    pub fn new(tx: TX, rx_mux: RX, nb_bounce: u8, limits: [[u16; RXSIZE]; TXSIZE]) -> Self {
        Self {
            tx,
            rx: rx_mux,
            debouncer: Debouncer::new(nb_bounce),
            limits,
            values: [[0; RXSIZE]; TXSIZE],
            coord_iter: CoordIterator::<TXSIZE, RXSIZE>::new(),
        }
    }

    fn scan_raw(&mut self, coord: &MatrixCoord) -> Result<Option<Event>, KeyboardError> {
        self.rx.select(coord.rx)?;
        self.tx.charge_capacitor(coord.tx)?;
        let value = self.rx.read()?;
        self.tx.discharge_capacitor(coord.tx)?;

        self.values[coord.tx][coord.rx] = value;
        let is_pressed = value > self.limits[coord.tx][coord.rx];

        if self.debouncer.update(coord.tx, coord.rx, is_pressed)? {
            let e = match is_pressed {
                true => Event::Press(coord.rx as u8, coord.tx as u8),
                false => Event::Release(coord.rx as u8, coord.tx as u8),
            };

            return Ok(Some(e));
        };

        return Ok(None);
    }
}

#[derive(Debug, Clone)]
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

    fn next(&mut self) -> Option<MatrixCoord> {
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

#[allow(unused_must_use)]
impl<TX, RX, const TXSIZE: usize, const RXSIZE: usize> Scanner for ECScanner<TX, RX, TXSIZE, RXSIZE>
where
    TX: TxModule,
    RX: RxModule,
{
    fn scan(&mut self) -> Result<Option<Event>, KeyboardError> {
        while let Some(coord) = self.coord_iter.next() {
            if let Some(e) = self.scan_raw(&coord)? {
                return Ok(Some(e));
            }
        }

        Ok(None)
    }
}
