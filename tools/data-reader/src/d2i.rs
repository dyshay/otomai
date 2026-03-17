//! D2I (Internationalization) file reader.
//!
//! Format:
//!   indexes_pointer: i32 (at offset 0)
//!   [string data section - raw UTF strings at various offsets]
//!   [index section at indexes_pointer]:
//!     indexes_length: i32
//!     entries: (key: i32, has_diacritical: bool, pointer: i32, [diacritical_pointer: i32])
//!   [named text section]:
//!     section_length: i32
//!     entries: (text_key: UTF, pointer: i32)
//!   [sort index section]:
//!     section_length: i32
//!     entries: (id: i32, sort_index: i32)

use anyhow::{Context, Result};
use byteorder::{BigEndian, ReadBytesExt};
use std::collections::HashMap;
use std::io::{Cursor, Read, Seek, SeekFrom};

pub struct D2IReader {
    data: Vec<u8>,
    /// text_id → offset in data
    text_indexes: HashMap<i32, u32>,
    /// text_id → offset for un-diacritical variant
    undiacritical_indexes: HashMap<i32, u32>,
    /// text_key (string) → offset in data
    named_text_indexes: HashMap<String, u32>,
    /// text_id → sort order
    sort_indexes: HashMap<i32, i32>,
}

impl D2IReader {
    pub fn open(path: &std::path::Path) -> Result<Self> {
        let data = std::fs::read(path).context("Reading D2I file")?;
        Self::from_bytes(data)
    }

    pub fn from_bytes(data: Vec<u8>) -> Result<Self> {
        let mut cursor = Cursor::new(&data);

        // Read indexes pointer
        let indexes_pointer = cursor.read_i32::<BigEndian>()? as u64;

        // Seek to index section
        cursor.seek(SeekFrom::Start(indexes_pointer))?;

        // Read text indexes
        let indexes_length = cursor.read_i32::<BigEndian>()? as u64;
        let indexes_end = cursor.position() + indexes_length;

        let mut text_indexes = HashMap::new();
        let mut undiacritical_indexes = HashMap::new();

        while cursor.position() < indexes_end {
            let key = cursor.read_i32::<BigEndian>()?;
            let has_diacritical = cursor.read_u8()? != 0;
            let pointer = cursor.read_u32::<BigEndian>()?;

            text_indexes.insert(key, pointer);

            if has_diacritical {
                let diacritical_pointer = cursor.read_u32::<BigEndian>()?;
                undiacritical_indexes.insert(key, diacritical_pointer);
            }
        }

        // Read named text indexes
        let named_length = cursor.read_i32::<BigEndian>()? as u64;
        let named_end = cursor.position() + named_length;

        let mut named_text_indexes = HashMap::new();
        while cursor.position() < named_end {
            let text_key = read_utf(&mut cursor)?;
            let pointer = cursor.read_u32::<BigEndian>()?;
            named_text_indexes.insert(text_key, pointer);
        }

        // Read sort indexes
        let sort_length = cursor.read_i32::<BigEndian>()? as u64;
        let sort_end = cursor.position() + sort_length;

        let mut sort_indexes = HashMap::new();
        let mut sort_counter = 0i32;
        while cursor.position() < sort_end {
            let id = cursor.read_i32::<BigEndian>()?;
            sort_indexes.insert(id, sort_counter);
            sort_counter += 1;
        }

        Ok(Self {
            data,
            text_indexes,
            undiacritical_indexes,
            named_text_indexes,
            sort_indexes,
        })
    }

    /// Get text by numeric ID.
    pub fn get_text(&self, key: i32) -> Result<String> {
        let &offset = self
            .text_indexes
            .get(&key)
            .context(format!("Text ID {} not found", key))?;
        self.read_string_at(offset as u64)
    }

    /// Get un-diacritical text by numeric ID (without accents).
    pub fn get_undiacritical_text(&self, key: i32) -> Result<Option<String>> {
        match self.undiacritical_indexes.get(&key) {
            Some(&offset) => Ok(Some(self.read_string_at(offset as u64)?)),
            None => Ok(None),
        }
    }

    /// Get text by string key.
    pub fn get_named_text(&self, key: &str) -> Result<String> {
        let &offset = self
            .named_text_indexes
            .get(key)
            .context(format!("Named text '{}' not found", key))?;
        self.read_string_at(offset as u64)
    }

    /// Get all text IDs.
    pub fn text_ids(&self) -> Vec<i32> {
        let mut ids: Vec<i32> = self.text_indexes.keys().copied().collect();
        ids.sort();
        ids
    }

    /// Get all named text keys.
    pub fn named_text_keys(&self) -> Vec<&str> {
        let mut keys: Vec<&str> = self.named_text_indexes.keys().map(|s| s.as_str()).collect();
        keys.sort();
        keys
    }

    /// Get all texts as a map (id → text).
    pub fn all_texts(&self) -> Result<HashMap<i32, String>> {
        let mut map = HashMap::new();
        for &key in self.text_indexes.keys() {
            map.insert(key, self.get_text(key)?);
        }
        Ok(map)
    }

    /// Number of text entries.
    pub fn len(&self) -> usize {
        self.text_indexes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.text_indexes.is_empty()
    }

    fn read_string_at(&self, offset: u64) -> Result<String> {
        let mut cursor = Cursor::new(&self.data);
        cursor.seek(SeekFrom::Start(offset))?;
        read_utf(&mut cursor)
    }
}

fn read_utf(cursor: &mut (impl Read + ReadBytesExt)) -> Result<String> {
    let len = cursor.read_u16::<BigEndian>()? as usize;
    let mut buf = vec![0u8; len];
    cursor.read_exact(&mut buf)?;
    Ok(String::from_utf8(buf)?)
}
