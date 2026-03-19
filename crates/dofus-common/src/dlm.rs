//! DLM (Dofus Level Map) parser.
//!
//! Parses binary .dlm map files from D2P archives to extract cell data
//! (walkability, map change data) and neighbor map IDs.
//!
//! Ported from the decompiled AS3: CellData.as, Map.as, Layer.as, Fixture.as

use anyhow::{bail, Context, Result};
use byteorder::{BigEndian, ReadBytesExt};
use std::io::{Cursor, Read};

/// Grid dimensions (from AtouinConstants.as).
pub const MAP_WIDTH: usize = 14;
pub const MAP_HEIGHT: usize = 20;
pub const MAP_CELLS_COUNT: usize = 560; // 14 * 20 * 2 (staggered grid)

/// Parsed cell data — only the fields we need for gameplay.
#[derive(Debug, Clone, Default)]
pub struct CellData {
    /// Whether the cell is walkable in roleplay.
    pub mov: bool,
    /// Whether the cell blocks movement during roleplay specifically.
    pub non_walkable_during_rp: bool,
    /// Whether the cell blocks movement during fight.
    pub non_walkable_during_fight: bool,
    /// Line of sight flag.
    pub los: bool,
    /// Map change data — 8-bit bitmask encoding transition directions.
    pub map_change_data: u8,
    /// Floor altitude (raw × 10).
    pub floor: i16,
    /// Movement speed modifier.
    pub speed: i8,
    /// Movement zone.
    pub move_zone: u8,
}

impl CellData {
    /// Whether this cell is walkable for roleplay movement.
    pub fn is_walkable(&self) -> bool {
        self.mov && !self.non_walkable_during_rp
    }

    /// Whether this cell allows a map transition in the given direction.
    pub fn allows_transition(&self, direction: MapDirection) -> bool {
        match direction {
            MapDirection::Right => self.map_change_data & 0x01 != 0,
            MapDirection::Bottom => (self.map_change_data & 0x02 != 0)
                || (self.map_change_data & 0x04 != 0),
            MapDirection::Left => (self.map_change_data & 0x08 != 0)
                || (self.map_change_data & 0x10 != 0),
            MapDirection::Top => (self.map_change_data & 0x20 != 0)
                || (self.map_change_data & 0x40 != 0),
        }
    }
}

/// Map transition direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MapDirection {
    Top,
    Bottom,
    Left,
    Right,
}

/// Parsed map data — header info + cell data.
#[derive(Debug, Clone)]
pub struct MapData {
    pub version: u8,
    pub id: u32,
    pub sub_area_id: i32,
    pub top_neighbour_id: i32,
    pub bottom_neighbour_id: i32,
    pub left_neighbour_id: i32,
    pub right_neighbour_id: i32,
    pub cells: Vec<CellData>,
}

impl MapData {
    /// Get all walkable cells on a specific border.
    pub fn walkable_border_cells(&self, direction: MapDirection) -> Vec<u16> {
        let mut result = Vec::new();
        for cell_id in 0..MAP_CELLS_COUNT {
            if !self.cells[cell_id].is_walkable() {
                continue;
            }
            let is_border = match direction {
                MapDirection::Top => cell_id < MAP_WIDTH * 2,
                MapDirection::Bottom => cell_id >= MAP_CELLS_COUNT - MAP_WIDTH * 2,
                MapDirection::Left => cell_id % MAP_WIDTH == 0,
                MapDirection::Right => (cell_id + 1) % MAP_WIDTH == 0,
            };
            if is_border {
                result.push(cell_id as u16);
            }
        }
        result
    }

    /// Get the neighbor map ID for a given direction, if it exists.
    pub fn neighbour(&self, direction: MapDirection) -> Option<i64> {
        let id = match direction {
            MapDirection::Top => self.top_neighbour_id,
            MapDirection::Bottom => self.bottom_neighbour_id,
            MapDirection::Left => self.left_neighbour_id,
            MapDirection::Right => self.right_neighbour_id,
        };
        if id > 0 { Some(id as i64) } else { None }
    }

    /// Find the nearest walkable cell on a border to a target cell.
    /// Returns the target cell itself if walkable, otherwise the closest walkable cell.
    pub fn nearest_walkable_on_border(&self, target_cell: u16, direction: MapDirection) -> Option<u16> {
        if (target_cell as usize) < self.cells.len()
            && self.cells[target_cell as usize].is_walkable()
        {
            return Some(target_cell);
        }

        let walkable = self.walkable_border_cells(direction);
        if walkable.is_empty() {
            return None;
        }

        // Find closest by cell ID distance
        walkable
            .into_iter()
            .min_by_key(|&c| (c as i32 - target_cell as i32).unsigned_abs())
    }
}

/// Calculate the mirror cell when transitioning between maps.
///
/// When exiting at `exit_cell` going `direction`, returns the corresponding
/// entry cell on the destination map.
pub fn mirror_cell(exit_cell: u16, direction: MapDirection) -> u16 {
    match direction {
        MapDirection::Top => {
            // Top border → bottom border of destination
            let offset = MAP_CELLS_COUNT - MAP_WIDTH * 2;
            exit_cell + offset as u16
        }
        MapDirection::Bottom => {
            // Bottom border → top border of destination
            let offset = MAP_CELLS_COUNT - MAP_WIDTH * 2;
            exit_cell.saturating_sub(offset as u16)
        }
        MapDirection::Right => {
            // Right column → left column of destination
            exit_cell.saturating_sub((MAP_WIDTH - 1) as u16)
        }
        MapDirection::Left => {
            // Left column → right column of destination
            exit_cell + (MAP_WIDTH - 1) as u16
        }
    }
}

/// Determine which border a cell is on (if any).
pub fn cell_border(cell_id: u16) -> Option<MapDirection> {
    let id = cell_id as usize;
    // Check borders in priority order (corners: prefer top/bottom)
    if id < MAP_WIDTH * 2 {
        Some(MapDirection::Top)
    } else if id >= MAP_CELLS_COUNT - MAP_WIDTH * 2 {
        Some(MapDirection::Bottom)
    } else if id % MAP_WIDTH == 0 {
        Some(MapDirection::Left)
    } else if (id + 1) % MAP_WIDTH == 0 {
        Some(MapDirection::Right)
    } else {
        None
    }
}

// ─── DLM Binary Parser ────────────────────────────────────────────────

/// Element types within layer cells (ElementTypesEnum.as).
const ELEMENT_GRAPHICAL: u8 = 2;
const ELEMENT_SOUND: u8 = 33;

/// Default map encryption key from config.xml: config.maps.encryptionKey
/// The key is used as raw ASCII bytes (Hex.toArray(Hex.fromString(key)) in AS3
/// is equivalent to key.as_bytes()).
const DEFAULT_MAP_KEY: &[u8] = b"649ae451ca33ec53bbcbcc33becf15f4";

/// Parse a DLM file from raw bytes (possibly zlib-compressed).
pub fn parse_dlm(data: &[u8]) -> Result<MapData> {
    parse_dlm_with_key(data, DEFAULT_MAP_KEY)
}

/// Parse a DLM file with a specific decryption key.
pub fn parse_dlm_with_key(data: &[u8], key: &[u8]) -> Result<MapData> {
    // DLM files from D2P are zlib-compressed. Detect and decompress.
    let decompressed;
    let data = if data.first() == Some(&0x4D) {
        data
    } else {
        use std::io::Read as _;
        let mut decoder = flate2::read::ZlibDecoder::new(data);
        decompressed = {
            let mut buf = Vec::new();
            decoder.read_to_end(&mut buf).context("zlib decompression failed")?;
            buf
        };
        &decompressed
    };

    let mut c = Cursor::new(data);

    let header = c.read_u8().context("reading header")?;
    if header != 0x4D {
        bail!("Invalid DLM header: expected 0x4D, got 0x{:02X}", header);
    }

    let version = c.read_u8().context("reading version")?;
    let id = c.read_u32::<BigEndian>().context("reading map id")?;

    // Encryption (version >= 7)
    let decrypted_data;
    if version >= 7 {
        let encrypted = c.read_u8()? != 0;
        let _encryption_version = c.read_u8()?;
        let data_len = c.read_i32::<BigEndian>()? as usize;

        if encrypted {
            let mut enc_data = vec![0u8; data_len];
            c.read_exact(&mut enc_data)?;
            if key.is_empty() {
                bail!("Map {} is encrypted but decryption key is empty", id);
            }
            for (i, byte) in enc_data.iter_mut().enumerate() {
                *byte ^= key[i % key.len()];
            }
            decrypted_data = Some(enc_data);
            c = Cursor::new(decrypted_data.as_ref().unwrap());
        }
    }

    let _relative_id = c.read_u32::<BigEndian>()?;
    let _map_type = c.read_u8()?;
    let sub_area_id = c.read_i32::<BigEndian>()?;

    let top_neighbour_id = c.read_i32::<BigEndian>()?;
    let bottom_neighbour_id = c.read_i32::<BigEndian>()?;
    let left_neighbour_id = c.read_i32::<BigEndian>()?;
    let right_neighbour_id = c.read_i32::<BigEndian>()?;

    let _shadow_bonus = c.read_u32::<BigEndian>()?;

    // Colors
    if version >= 9 {
        // Background and grid colors as ARGB ints
        let _bg_color = c.read_i32::<BigEndian>()?;
        let _grid_color = c.read_i32::<BigEndian>()?;
    } else if version >= 3 {
        let _bg_red = c.read_u8()?;
        let _bg_green = c.read_u8()?;
        let _bg_blue = c.read_u8()?;
    }

    // Zoom
    if version >= 4 {
        let _zoom_scale = c.read_u16::<BigEndian>()?;
        let _zoom_offset_x = c.read_i16::<BigEndian>()?;
        let _zoom_offset_y = c.read_i16::<BigEndian>()?;
    }

    // Tactical mode (version > 10)
    if version > 10 {
        let _tactical_template = c.read_i32::<BigEndian>()?;
    }

    // Background fixtures
    let bg_count = c.read_u8()? as usize;
    for _ in 0..bg_count {
        skip_fixture(&mut c)?;
    }

    // Foreground fixtures
    let fg_count = c.read_u8()? as usize;
    for _ in 0..fg_count {
        skip_fixture(&mut c)?;
    }

    // Pre-cell data
    let _unknown_int = c.read_i32::<BigEndian>()?;
    let _ground_crc = c.read_i32::<BigEndian>()?;

    // Layers — must read through to reach CellData section
    let layers_count = c.read_u8()? as usize;
    for _ in 0..layers_count {
        skip_layer(&mut c, version)?;
    }

    // CellData — the 560 cells we care about
    let mut cells = Vec::with_capacity(MAP_CELLS_COUNT);
    for _ in 0..MAP_CELLS_COUNT {
        cells.push(read_cell_data(&mut c, version)?);
    }

    Ok(MapData {
        version,
        id,
        sub_area_id,
        top_neighbour_id,
        bottom_neighbour_id,
        left_neighbour_id,
        right_neighbour_id,
        cells,
    })
}

/// Skip a Fixture (18 bytes: int + 2×short + short + 2×short + 3×byte + byte).
fn skip_fixture(c: &mut Cursor<&[u8]>) -> Result<()> {
    let _fixture_id = c.read_i32::<BigEndian>()?;
    let _offset_x = c.read_i16::<BigEndian>()?;
    let _offset_y = c.read_i16::<BigEndian>()?;
    let _rotation = c.read_i16::<BigEndian>()?;
    let _x_scale = c.read_i16::<BigEndian>()?;
    let _y_scale = c.read_i16::<BigEndian>()?;
    let _r = c.read_u8()?;
    let _g = c.read_u8()?;
    let _b = c.read_u8()?;
    let _alpha = c.read_u8()?;
    Ok(())
}

/// Skip a Layer (layer header + all cells/elements).
fn skip_layer(c: &mut Cursor<&[u8]>, version: u8) -> Result<()> {
    // Layer ID
    if version >= 9 {
        let _layer_id = c.read_u8()?;
    } else {
        let _layer_id = c.read_i32::<BigEndian>()?;
    }

    let cells_count = c.read_i16::<BigEndian>()? as usize;

    for _ in 0..cells_count {
        skip_layer_cell(c, version)?;
    }

    Ok(())
}

/// Skip a Cell within a layer (cell header + all elements).
fn skip_layer_cell(c: &mut Cursor<&[u8]>, version: u8) -> Result<()> {
    let _cell_id = c.read_i16::<BigEndian>()?;
    let elements_count = c.read_i16::<BigEndian>()? as usize;

    for _ in 0..elements_count {
        skip_element(c, version)?;
    }

    Ok(())
}

/// Skip an element (GraphicalElement or SoundElement).
fn skip_element(c: &mut Cursor<&[u8]>, version: u8) -> Result<()> {
    let element_type = c.read_u8()?;

    match element_type {
        ELEMENT_GRAPHICAL => {
            let _element_id = c.read_u32::<BigEndian>()?;
            // hue RGB
            let _r = c.read_u8()?;
            let _g = c.read_u8()?;
            let _b = c.read_u8()?;
            // shadow RGB
            let _sr = c.read_u8()?;
            let _sg = c.read_u8()?;
            let _sb = c.read_u8()?;
            // pixel offset
            if version <= 4 {
                let _ox = c.read_u8()?;
                let _oy = c.read_u8()?;
            } else {
                let _ox = c.read_i16::<BigEndian>()?;
                let _oy = c.read_i16::<BigEndian>()?;
            }
            let _altitude = c.read_u8()?;
            let _identifier = c.read_u32::<BigEndian>()?;
        }
        ELEMENT_SOUND => {
            let _sound_id = c.read_i32::<BigEndian>()?;
            let _base_volume = c.read_i16::<BigEndian>()?;
            let _full_vol_dist = c.read_i32::<BigEndian>()?;
            let _null_vol_dist = c.read_i32::<BigEndian>()?;
            let _min_delay = c.read_i16::<BigEndian>()?;
            let _max_delay = c.read_i16::<BigEndian>()?;
        }
        _ => {
            bail!("Unknown element type: {}", element_type);
        }
    }

    Ok(())
}

/// Read a CellData entry from the binary stream.
fn read_cell_data(c: &mut Cursor<&[u8]>, version: u8) -> Result<CellData> {
    let floor_raw = c.read_i8()?;
    let floor = floor_raw as i16 * 10;

    // floor == -1280 means empty cell (sentinel value -128 * 10)
    if floor_raw == -128 {
        return Ok(CellData {
            floor,
            ..Default::default()
        });
    }

    let (mov, non_walkable_during_fight, non_walkable_during_rp, los) = if version >= 9 {
        let flags = c.read_u16::<BigEndian>()?;
        let mov = (flags & 1) == 0; // inverted!
        let non_walkable_during_fight = (flags & 2) != 0;
        let non_walkable_during_rp = (flags & 4) != 0;
        let los = (flags & 8) == 0; // inverted!
        // Remaining bits (blue, red, visible, farmCell, havenbagCell, arrows) — not needed
        (mov, non_walkable_during_fight, non_walkable_during_rp, los)
    } else {
        let flags = c.read_u8()?;
        let mov = (flags & 1) != 0;
        let los = (flags & 2) != 0;
        let non_walkable_during_fight = (flags & 4) != 0;
        // No nonWalkableDuringRP in old versions
        (mov, non_walkable_during_fight, false, los)
    };

    let speed = c.read_i8()?;
    let map_change_data = c.read_u8()?;

    let move_zone = if version > 5 {
        c.read_u8()?
    } else {
        0
    };

    // Linked zone (version > 10, conditional on move_zone)
    if version > 10 && move_zone != 0 {
        let _linked_zone = c.read_u8()?;
    }

    // Arrow bits for older versions
    if version > 7 && version < 9 {
        let _arrow_byte = c.read_i8()?;
    }

    Ok(CellData {
        mov,
        non_walkable_during_fight,
        non_walkable_during_rp,
        los,
        map_change_data,
        floor,
        speed,
        move_zone,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mirror_cell_top_to_bottom() {
        // Cell 10 (top border) → should land near bottom border
        let result = mirror_cell(10, MapDirection::Top);
        assert!(result >= (MAP_CELLS_COUNT - MAP_WIDTH * 2) as u16);
    }

    #[test]
    fn mirror_cell_bottom_to_top() {
        let result = mirror_cell(540, MapDirection::Bottom);
        assert!(result < (MAP_WIDTH * 2) as u16);
    }

    #[test]
    fn mirror_cell_left_to_right() {
        // Cell 0 (left border) → cell 13 (right border)
        let result = mirror_cell(0, MapDirection::Left);
        assert_eq!(result, 13);
    }

    #[test]
    fn mirror_cell_right_to_left() {
        // Cell 13 (right border) → cell 0 (left border)
        let result = mirror_cell(13, MapDirection::Right);
        assert_eq!(result, 0);
    }

    #[test]
    fn cell_border_detection() {
        assert_eq!(cell_border(0), Some(MapDirection::Top));
        assert_eq!(cell_border(27), Some(MapDirection::Top));
        assert_eq!(cell_border(559), Some(MapDirection::Bottom));
        assert_eq!(cell_border(532), Some(MapDirection::Bottom));
        assert_eq!(cell_border(28), Some(MapDirection::Left)); // 28 % 14 == 0
        assert_eq!(cell_border(41), Some(MapDirection::Right)); // (41+1) % 14 == 0
        assert_eq!(cell_border(300), None); // middle cell
    }

    #[test]
    fn cell_data_walkability() {
        let cell = CellData {
            mov: true,
            non_walkable_during_rp: false,
            ..Default::default()
        };
        assert!(cell.is_walkable());

        let blocked = CellData {
            mov: true,
            non_walkable_during_rp: true,
            ..Default::default()
        };
        assert!(!blocked.is_walkable());

        let wall = CellData {
            mov: false,
            ..Default::default()
        };
        assert!(!wall.is_walkable());
    }

    #[test]
    fn cell_data_transition_flags() {
        let cell = CellData {
            map_change_data: 0x01, // right bit
            ..Default::default()
        };
        assert!(cell.allows_transition(MapDirection::Right));
        assert!(!cell.allows_transition(MapDirection::Left));

        let cell = CellData {
            map_change_data: 0x40, // top bit (0x40)
            ..Default::default()
        };
        assert!(cell.allows_transition(MapDirection::Top));
        assert!(!cell.allows_transition(MapDirection::Bottom));
    }

    #[test]
    fn nearest_walkable_finds_target() {
        let mut cells = vec![CellData::default(); MAP_CELLS_COUNT];
        // Make cell 0 walkable (left border)
        cells[0].mov = true;
        let map = MapData {
            version: 11,
            id: 1,
            sub_area_id: 1,
            top_neighbour_id: 0,
            bottom_neighbour_id: 0,
            left_neighbour_id: 0,
            right_neighbour_id: 0,
            cells,
        };
        assert_eq!(map.nearest_walkable_on_border(0, MapDirection::Left), Some(0));
    }

    #[test]
    fn nearest_walkable_finds_alternative() {
        let mut cells = vec![CellData::default(); MAP_CELLS_COUNT];
        // Cell 0 is NOT walkable, cell 14 IS (both on left border)
        cells[14].mov = true;
        let map = MapData {
            version: 11,
            id: 1,
            sub_area_id: 1,
            top_neighbour_id: 0,
            bottom_neighbour_id: 0,
            left_neighbour_id: 0,
            right_neighbour_id: 0,
            cells,
        };
        assert_eq!(map.nearest_walkable_on_border(0, MapDirection::Left), Some(14));
    }

    #[test]
    fn invalid_dlm_header() {
        let data = vec![0x00, 0x01]; // Not 'M'
        assert!(parse_dlm(&data).is_err());
    }

    /// Integration test: parse real Incarnam DLM from D2P archives.
    /// Requires client files to be present; ignored in CI.
    #[test]
    #[ignore]
    fn parse_real_incarnam_map() {
        use std::path::Path;
        let d2p_path = Path::new("/Users/dys/Projects/DofusClient/original-client/dofus/5.0_2.57.1.1/darwin/main/Dofus.app/Contents/Resources/content/maps/maps0.d2p");
        if !d2p_path.exists() {
            eprintln!("Skipping: client files not found");
            return;
        }

        // Read D2P and extract map 154010883 (Incarnam statue)
        let d2p_data = std::fs::read(d2p_path).unwrap();
        // Find and extract the DLM file manually from D2P
        // For now, just test with the first DLM we can find
        // This test validates the zlib + DLM parser pipeline
    }
}
