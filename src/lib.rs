#![no_std]
use error::KeyboardError;

pub mod analog;
pub mod debounce;
pub mod error;
pub mod mux;
pub mod scanner;

pub enum SplitSide {
    LEFT,
    RIGHT,
}
