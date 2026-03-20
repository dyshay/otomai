//! Movement timing validation (anti-speedhack).

use std::time::Instant;

/// Time per cell in milliseconds for running (fastest legitimate speed).
const RUN_MS_PER_CELL: [u64; 8] = [
    255, // 0 E  (horizontal diagonal)
    170, // 1 SE (linear)
    150, // 2 S  (vertical diagonal)
    170, // 3 SW (linear)
    255, // 4 W  (horizontal diagonal)
    170, // 5 NW (linear)
    150, // 6 N  (vertical diagonal)
    170, // 7 NE (linear)
];

/// Tolerance factor — allow 20% faster than theoretical minimum.
pub const TIMING_TOLERANCE: f64 = 0.8;

/// Extract cell ID from a key_movement entry (lower 12 bits).
pub fn cell_from_key(key: i16) -> u16 {
    (key as u16) & 0x0FFF
}

/// Extract direction from a key_movement entry (bits 12-14).
pub fn direction_from_key(key: i16) -> u8 {
    ((key as u16 >> 12) & 0x07) as u8
}

/// Decode client key_movements into (cell_id, direction) pairs.
pub fn decode_path(key_movements: &[i16]) -> Vec<(u16, u8)> {
    key_movements.iter().map(|&k| (cell_from_key(k), direction_from_key(k))).collect()
}

/// Calculate the minimum expected duration of a path in milliseconds.
pub fn expected_path_duration_ms(key_movements: &[i16]) -> u64 {
    if key_movements.len() <= 1 { return 0; }
    key_movements[1..].iter()
        .map(|&key| {
            let dir = direction_from_key(key) as usize;
            RUN_MS_PER_CELL[dir.min(7)]
        })
        .sum()
}

/// State tracked during an active movement.
pub struct MovementState {
    pub start_time: Instant,
    pub path_cells: Vec<u16>,
    pub expected_duration_ms: u64,
    pub dest_cell: u16,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_key_movement() {
        let key = ((1u16 << 12) | 300u16) as i16;
        assert_eq!(cell_from_key(key), 300);
        assert_eq!(direction_from_key(key), 1);
    }

    #[test]
    fn decode_all_directions() {
        for dir in 0u8..8 {
            let cell = 200u16;
            let key = (((dir as u16) << 12) | cell) as i16;
            assert_eq!(direction_from_key(key), dir);
            assert_eq!(cell_from_key(key), cell);
        }
    }

    #[test]
    fn path_duration_empty() {
        assert_eq!(expected_path_duration_ms(&[]), 0);
        assert_eq!(expected_path_duration_ms(&[300]), 0);
    }

    #[test]
    fn path_duration_linear_run() {
        let keys: Vec<i16> = (0..5).map(|i| ((1u16 << 12) | (100 + i)) as i16).collect();
        assert_eq!(expected_path_duration_ms(&keys), 4 * 170);
    }

    #[test]
    fn path_duration_mixed_directions() {
        let keys = vec![
            ((1u16 << 12) | 100) as i16,
            ((1u16 << 12) | 101) as i16,
            ((0u16 << 12) | 102) as i16,
            ((6u16 << 12) | 103) as i16,
        ];
        assert_eq!(expected_path_duration_ms(&keys), 170 + 255 + 150);
    }

    #[test]
    fn timing_tolerance_allows_slight_fast() {
        let expected = 9 * 170u64;
        let min_expected = (expected as f64 * TIMING_TOLERANCE) as u64;
        assert_eq!(min_expected, 1224);
        assert!(1300 >= min_expected);
        assert!(1000 < min_expected);
    }
}
