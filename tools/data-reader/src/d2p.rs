//! D2P (Data Pack) file reader.
//!
//! D2P files are archives containing multiple files (typically maps, audio, etc.)
//! with an index at the end of the file.
//!
//! Format:
//!   [file data...]
//!   [properties table at end - 2 bytes before EOF points to start]
//!   [index table]

use anyhow::{bail, Context, Result};
use byteorder::{BigEndian, ReadBytesExt};
use std::collections::HashMap;
use std::io::{Cursor, Read, Seek, SeekFrom};

#[derive(Debug, Clone)]
pub struct D2PEntry {
    pub filename: String,
    pub offset: u32,
    pub length: u32,
}

pub struct D2PReader {
    data: Vec<u8>,
    entries: HashMap<String, D2PEntry>,
    properties: HashMap<String, String>,
}

impl D2PReader {
    pub fn open(path: &std::path::Path) -> Result<Self> {
        let data = std::fs::read(path).context("Reading D2P file")?;
        Self::from_bytes(data)
    }

    pub fn from_bytes(data: Vec<u8>) -> Result<Self> {
        let len = data.len();
        if len < 26 {
            bail!("D2P file too small");
        }

        let mut cursor = Cursor::new(&data);

        // Header: 2 bytes (vMax, vMin)
        let _v_max = cursor.read_u8()?;
        let _v_min = cursor.read_u8()?;

        // Footer: last 24 bytes
        // Order per AS3 PakProtocol2: dataOffset, dataCount, indexOffset, indexCount,
        //   propertiesOffset, propertiesCount
        cursor.seek(SeekFrom::End(-24))?;

        let data_offset = cursor.read_u32::<BigEndian>()?;
        let _data_count = cursor.read_u32::<BigEndian>()?;
        let index_offset = cursor.read_u32::<BigEndian>()?;
        let index_count = cursor.read_u32::<BigEndian>()?;
        let properties_offset = cursor.read_u32::<BigEndian>()?;
        let properties_count = cursor.read_u32::<BigEndian>()?;

        // Read properties
        cursor.seek(SeekFrom::Start(properties_offset as u64))?;
        let mut properties = HashMap::new();
        for _ in 0..properties_count {
            let key = read_utf(&mut cursor)?;
            let value = read_utf(&mut cursor)?;
            properties.insert(key, value);
        }

        // Read file index
        cursor.seek(SeekFrom::Start(index_offset as u64))?;
        let mut entries = HashMap::new();
        for _ in 0..index_count {
            let filename = read_utf(&mut cursor)?;
            let offset = cursor.read_u32::<BigEndian>()?;
            let length = cursor.read_u32::<BigEndian>()?;

            entries.insert(
                filename.clone(),
                D2PEntry {
                    filename,
                    offset: offset + data_offset,
                    length,
                },
            );
        }

        Ok(Self {
            data,
            entries,
            properties,
        })
    }

    /// List all files in the archive.
    pub fn filenames(&self) -> Vec<&str> {
        let mut names: Vec<&str> = self.entries.keys().map(|s| s.as_str()).collect();
        names.sort();
        names
    }

    /// Get archive properties.
    pub fn properties(&self) -> &HashMap<String, String> {
        &self.properties
    }

    /// Number of files in the archive.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Read a file's raw bytes from the archive.
    pub fn read_file(&self, filename: &str) -> Result<Vec<u8>> {
        let entry = self
            .entries
            .get(filename)
            .context(format!("File '{}' not found in D2P", filename))?;

        let start = entry.offset as usize;
        let end = start + entry.length as usize;

        if end > self.data.len() {
            bail!(
                "D2P entry '{}' out of bounds: {}..{} (file size {})",
                filename,
                start,
                end,
                self.data.len()
            );
        }

        Ok(self.data[start..end].to_vec())
    }

    /// Extract all files to a directory.
    pub fn extract_all(&self, output_dir: &std::path::Path) -> Result<usize> {
        std::fs::create_dir_all(output_dir)?;
        let mut count = 0;

        for (filename, _entry) in &self.entries {
            let data = self.read_file(filename)?;
            let out_path = output_dir.join(filename);

            if let Some(parent) = out_path.parent() {
                std::fs::create_dir_all(parent)?;
            }

            std::fs::write(&out_path, &data)?;
            count += 1;
        }

        Ok(count)
    }
}

fn read_utf(cursor: &mut (impl Read + ReadBytesExt)) -> Result<String> {
    let len = cursor.read_u16::<BigEndian>()? as usize;
    let mut buf = vec![0u8; len];
    cursor.read_exact(&mut buf)?;
    Ok(String::from_utf8(buf)?)
}
