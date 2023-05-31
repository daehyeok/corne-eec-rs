use crate::error::KeyboardError;

pub struct Debouncer<const ROWS: usize, const COLS: usize> {
    state: [[bool; COLS]; ROWS], //current key state.
    hit_cnt: [[u8; COLS]; ROWS],
    nb_bounce: u8,
}

impl<const ROWS: usize, const COLS: usize> Debouncer<ROWS, COLS> {
    pub const fn new(nb_bounce: u8) -> Self {
        Self {
            state: [[false; COLS]; ROWS],
            hit_cnt: [[0; COLS]; ROWS],
            nb_bounce,
        }
    }

    /// Updates the key history
    pub fn update(
        &mut self,
        row: usize,
        col: usize,
        is_pressed: bool,
    ) -> Result<bool, KeyboardError> {
        if row >= ROWS {
            return Err(KeyboardError::RowOutOfRange(row));
        }

        if col >= COLS {
            return Err(KeyboardError::ColOutOfRange(col));
        }

        let mut is_changed = false;
        if self.state[row][col] == is_pressed {
            // if state not chagned. Assume that previous signal is noise.
            // Reset the count
            self.hit_cnt[row][col] = 0;
        } else if self.hit_cnt[row][col] == self.nb_bounce - 1 {
            // Reach the limit. Toggle key state
            self.hit_cnt[row][col] = 0;
            self.state[row][col] = !self.state[row][col];
            is_changed = true
        } else {
            self.hit_cnt[row][col] += 1;
        }

        Ok(is_changed)
    }
}
