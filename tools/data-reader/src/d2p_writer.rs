//! D2P binary writer — packs files into a D2P archive.

use anyhow::Result;
use byteorder::{BigEndian, WriteBytesExt};
use std::collections::HashMap;
use std::io::{Cursor, Write};

/// Write a complete D2P archive.
///
/// - `files`: filename → file data
/// - `properties`: archive properties (key → value)
pub fn write_d2p(
    files: &HashMap<String, Vec<u8>>,
    properties: &HashMap<String, String>,
) -> Result<Vec<u8>> {
    let mut buf = Vec::new();
    let mut cursor = Cursor::new(&mut buf);

    // Version header
    cursor.write_u8(2)?; // vMax
    cursor.write_u8(1)?; // vMin

    let data_offset = 2u32; // right after the version bytes

    // Write file data, tracking offsets relative to data_offset
    let mut file_entries: Vec<(String, u32, u32)> = Vec::new(); // (name, offset_from_data_start, length)
    let mut sorted_names: Vec<String> = files.keys().cloned().collect();
    sorted_names.sort();

    for name in &sorted_names {
        let data = &files[name];
        let offset = cursor.position() as u32 - data_offset;
        cursor.write_all(data)?;
        file_entries.push((name.clone(), offset, data.len() as u32));
    }

    // Properties section
    let properties_offset = cursor.position() as u32;
    let mut sorted_props: Vec<(&String, &String)> = properties.iter().collect();
    sorted_props.sort_by_key(|(k, _)| k.as_str());
    let properties_count = sorted_props.len() as u32;

    for (key, value) in &sorted_props {
        write_utf(&mut cursor, key)?;
        write_utf(&mut cursor, value)?;
    }

    // Index section
    let index_offset = cursor.position() as u32;
    let index_count = file_entries.len() as u32;

    for (name, offset, length) in &file_entries {
        write_utf(&mut cursor, name)?;
        cursor.write_u32::<BigEndian>(*offset)?;
        cursor.write_u32::<BigEndian>(*length)?;
    }

    let data_count = index_offset - data_offset; // total bytes of file data + properties

    // Footer (24 bytes)
    cursor.write_u32::<BigEndian>(data_offset)?;
    cursor.write_u32::<BigEndian>(data_count)?;
    cursor.write_u32::<BigEndian>(index_offset)?;
    cursor.write_u32::<BigEndian>(index_count)?;
    cursor.write_u32::<BigEndian>(properties_offset)?;
    cursor.write_u32::<BigEndian>(properties_count)?;

    Ok(buf)
}

fn write_utf(w: &mut (impl Write + WriteBytesExt), s: &str) -> Result<()> {
    let bytes = s.as_bytes();
    w.write_u16::<BigEndian>(bytes.len() as u16)?;
    w.write_all(bytes)?;
    Ok(())
}
