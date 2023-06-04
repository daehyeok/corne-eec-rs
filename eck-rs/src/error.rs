#[derive(defmt::Format, Debug)]
pub enum KeyboardError {
    RowOutOfRange(usize),
    ColOutOfRange(usize),
    MuxOutOfRange(usize),
    Gpio,

    InvaildHeader,
    InvailedCRC,
}
