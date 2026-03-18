//! D2I binary writer — serializes translation data back to D2I format.

use anyhow::Result;
use byteorder::{BigEndian, WriteBytesExt};
use std::collections::HashMap;
use std::io::{Cursor, Seek, SeekFrom, Write};

/// Write a complete D2I file from text entries.
///
/// - `texts`: text_id → text string
/// - `undiacritical`: text_id → un-diacritical variant (without accents)
/// - `named_texts`: text_key (string) → text string
pub fn write_d2i(
    texts: &HashMap<i32, String>,
    undiacritical: &HashMap<i32, String>,
    named_texts: &HashMap<String, String>,
) -> Result<Vec<u8>> {
    let mut buf = Vec::new();
    let mut cursor = Cursor::new(&mut buf);

    // Placeholder for indexes_pointer
    cursor.write_i32::<BigEndian>(0)?;

    // Write string data and track offsets
    let mut text_offsets: HashMap<i32, u32> = HashMap::new();
    let mut undiacritical_offsets: HashMap<i32, u32> = HashMap::new();
    let mut named_offsets: HashMap<String, u32> = HashMap::new();

    // Sort keys for deterministic output
    let mut text_keys: Vec<i32> = texts.keys().copied().collect();
    text_keys.sort();

    for &key in &text_keys {
        let offset = cursor.position() as u32;
        write_utf(&mut cursor, &texts[&key])?;
        text_offsets.insert(key, offset);
    }

    // Write undiacritical strings
    for &key in &text_keys {
        if let Some(text) = undiacritical.get(&key) {
            let offset = cursor.position() as u32;
            write_utf(&mut cursor, text)?;
            undiacritical_offsets.insert(key, offset);
        }
    }

    // Write named text strings
    let mut named_keys: Vec<String> = named_texts.keys().cloned().collect();
    named_keys.sort();

    for key in &named_keys {
        let offset = cursor.position() as u32;
        write_utf(&mut cursor, &named_texts[key])?;
        named_offsets.insert(key.clone(), offset);
    }

    // Now write index section
    let indexes_pointer = cursor.position() as i32;

    // Text index section
    // Calculate section size: each entry = 4 (key) + 1 (has_diacritical) + 4 (pointer) [+ 4 if diacritical]
    let mut index_section = Vec::new();
    {
        let mut ic = Cursor::new(&mut index_section);
        for &key in &text_keys {
            ic.write_i32::<BigEndian>(key)?;
            let has_diacritical = undiacritical_offsets.contains_key(&key);
            ic.write_u8(if has_diacritical { 1 } else { 0 })?;
            ic.write_u32::<BigEndian>(text_offsets[&key])?;
            if has_diacritical {
                ic.write_u32::<BigEndian>(undiacritical_offsets[&key])?;
            }
        }
    }
    cursor.write_i32::<BigEndian>(index_section.len() as i32)?;
    cursor.write_all(&index_section)?;

    // Named text section
    let mut named_section = Vec::new();
    {
        let mut nc = Cursor::new(&mut named_section);
        for key in &named_keys {
            write_utf(&mut nc, key)?;
            nc.write_u32::<BigEndian>(named_offsets[key])?;
        }
    }
    cursor.write_i32::<BigEndian>(named_section.len() as i32)?;
    cursor.write_all(&named_section)?;

    // Sort index section (text_id order = sort order)
    let sort_section_len = (text_keys.len() as i32) * 4;
    cursor.write_i32::<BigEndian>(sort_section_len)?;
    for &key in &text_keys {
        cursor.write_i32::<BigEndian>(key)?;
    }

    // Write indexes_pointer at offset 0
    cursor.seek(SeekFrom::Start(0))?;
    cursor.write_i32::<BigEndian>(indexes_pointer)?;

    Ok(buf)
}

fn write_utf(w: &mut (impl Write + WriteBytesExt), s: &str) -> Result<()> {
    let bytes = s.as_bytes();
    w.write_u16::<BigEndian>(bytes.len() as u16)?;
    w.write_all(bytes)?;
    Ok(())
}
