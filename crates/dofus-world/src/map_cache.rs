//! Map data cache — lazily loads and parses DLM files from D2P archives.
//!
//! Provides fast lookup of cell walkability and neighbor map IDs for
//! movement validation and map transitions.

use anyhow::{Context, Result};
use dofus_common::dlm::{self, MapData};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::RwLock;

/// In-memory cache of parsed map data, loaded lazily from D2P archives.
pub struct MapCache {
    /// All raw DLM files keyed by map_id, loaded from D2P on startup.
    raw_maps: HashMap<i64, Vec<u8>>,
    /// Parsed map data, populated lazily on first access.
    parsed: RwLock<HashMap<i64, MapData>>,
}

impl MapCache {
    /// Load all DLM files from D2P archives in the given directory.
    /// Only reads the D2P index and raw bytes — does NOT parse DLM yet (lazy).
    pub fn load_from_dir(maps_dir: &Path) -> Result<Self> {
        let mut raw_maps = HashMap::new();

        // Find all maps*.d2p files
        let mut d2p_paths: Vec<PathBuf> = std::fs::read_dir(maps_dir)
            .context("reading maps directory")?
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| {
                p.extension().map(|e| e == "d2p").unwrap_or(false)
                    && p.file_name()
                        .and_then(|n| n.to_str())
                        .map(|n| n.starts_with("maps"))
                        .unwrap_or(false)
            })
            .collect();
        d2p_paths.sort();

        for d2p_path in &d2p_paths {
            let reader = d2p::D2PReader::open(d2p_path)
                .with_context(|| format!("opening {}", d2p_path.display()))?;

            for fname in reader.filenames() {
                if !fname.ends_with(".dlm") {
                    continue;
                }
                // Extract map_id from filename like "3/154010883.dlm"
                let map_id = fname
                    .rsplit('/')
                    .next()
                    .and_then(|s| s.strip_suffix(".dlm"))
                    .and_then(|s| s.parse::<i64>().ok());

                if let Some(map_id) = map_id {
                    if let Ok(data) = reader.read_file(&fname) {
                        raw_maps.insert(map_id, data);
                    }
                }
            }
        }

        tracing::info!("Loaded {} raw map files from D2P archives", raw_maps.len());

        Ok(Self {
            raw_maps,
            parsed: RwLock::new(HashMap::new()),
        })
    }

    /// Create an empty cache (when no maps_dir is configured).
    pub fn empty() -> Self {
        Self {
            raw_maps: HashMap::new(),
            parsed: RwLock::new(HashMap::new()),
        }
    }

    /// Get parsed map data for a map_id. Parses lazily on first access.
    pub fn get(&self, map_id: i64) -> Option<MapData> {
        // Check parsed cache first
        {
            let cache = self.parsed.read().unwrap();
            if let Some(data) = cache.get(&map_id) {
                return Some(data.clone());
            }
        }

        // Parse from raw data
        let raw = self.raw_maps.get(&map_id)?;
        match dlm::parse_dlm(raw) {
            Ok(data) => {
                let mut cache = self.parsed.write().unwrap();
                cache.insert(map_id, data.clone());
                Some(data)
            }
            Err(e) => {
                tracing::warn!(map_id, error = %e, "Failed to parse DLM");
                None
            }
        }
    }

    /// Get neighbor map ID for a given direction.
    pub fn get_neighbour(&self, map_id: i64, direction: dlm::MapDirection) -> Option<i64> {
        self.get(map_id)?.neighbour(direction)
    }

    /// Check if a specific cell is walkable on a map.
    pub fn is_cell_walkable(&self, map_id: i64, cell_id: u16) -> bool {
        self.get(map_id)
            .map(|m| {
                (cell_id as usize) < m.cells.len() && m.cells[cell_id as usize].is_walkable()
            })
            .unwrap_or(false)
    }

    /// Number of raw map files loaded.
    pub fn raw_count(&self) -> usize {
        self.raw_maps.len()
    }

    /// Number of parsed (cached) maps.
    pub fn parsed_count(&self) -> usize {
        self.parsed.read().unwrap().len()
    }
}

/// Minimal D2P reader — just what we need to extract files.
/// Re-uses the same format as data-reader/src/d2p.rs.
mod d2p {
    use anyhow::{bail, Context, Result};
    use byteorder::{BigEndian, ReadBytesExt};
    use std::collections::HashMap;
    use std::io::{Cursor, Read, Seek, SeekFrom};
    use std::path::Path;

    pub struct D2PEntry {
        pub offset: u32,
        pub length: u32,
    }

    pub struct D2PReader {
        data: Vec<u8>,
        entries: HashMap<String, D2PEntry>,
    }

    impl D2PReader {
        pub fn open(path: &Path) -> Result<Self> {
            let data = std::fs::read(path).context("Reading D2P file")?;
            Self::from_bytes(data)
        }

        pub fn from_bytes(data: Vec<u8>) -> Result<Self> {
            let len = data.len();
            if len < 26 {
                bail!("D2P file too small");
            }

            let mut cursor = Cursor::new(&data);
            let _v_max = cursor.read_u8()?;
            let _v_min = cursor.read_u8()?;

            // Footer: last 24 bytes
            cursor.seek(SeekFrom::End(-24))?;
            let data_offset = cursor.read_u32::<BigEndian>()?;
            let _data_count = cursor.read_u32::<BigEndian>()?;
            let index_offset = cursor.read_u32::<BigEndian>()?;
            let index_count = cursor.read_u32::<BigEndian>()?;
            let properties_offset = cursor.read_u32::<BigEndian>()?;
            let _properties_count = cursor.read_u32::<BigEndian>()?;

            // Skip properties
            let _ = properties_offset;

            // Read index
            cursor.seek(SeekFrom::Start(index_offset as u64))?;
            let mut entries = HashMap::new();
            let mut bytes_read = 0u32;

            while bytes_read < index_count {
                let name_len = cursor.read_u16::<BigEndian>()? as usize;
                let mut name_buf = vec![0u8; name_len];
                cursor.read_exact(&mut name_buf)?;
                let name = String::from_utf8_lossy(&name_buf).to_string();
                let offset = cursor.read_u32::<BigEndian>()? + data_offset;
                let length = cursor.read_u32::<BigEndian>()?;

                bytes_read += 2 + name_len as u32 + 8;
                entries.insert(name, D2PEntry { offset, length });
            }

            Ok(Self { data, entries })
        }

        pub fn filenames(&self) -> Vec<String> {
            self.entries.keys().cloned().collect()
        }

        pub fn read_file(&self, name: &str) -> Result<Vec<u8>> {
            let entry = self.entries.get(name).context("File not found in D2P")?;
            let start = entry.offset as usize;
            let end = start + entry.length as usize;
            if end > self.data.len() {
                bail!("D2P entry out of bounds");
            }
            Ok(self.data[start..end].to_vec())
        }
    }
}
