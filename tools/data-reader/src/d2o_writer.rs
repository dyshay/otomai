//! D2O binary writer — serializes JSON objects back to D2O format.

use anyhow::{bail, Context, Result};
use byteorder::{BigEndian, WriteBytesExt};
use serde_json::Value;
use std::io::{Cursor, Seek, SeekFrom, Write};

use crate::d2o::{D2OClassDef, D2OFieldType, NULL_OBJECT_MARKER};

/// Write a complete D2O file from class definitions and objects.
pub fn write_d2o(
    classes: &std::collections::HashMap<i32, D2OClassDef>,
    objects: &[(i32, Value)],
) -> Result<Vec<u8>> {
    let mut buf = Vec::new();
    let mut cursor = Cursor::new(&mut buf);

    // Header: "D2O" + placeholder for indexes_pointer
    cursor.write_all(b"D2O")?;
    cursor.write_i32::<BigEndian>(0)?; // placeholder

    // Write all objects, tracking (object_id, offset)
    let mut index = Vec::with_capacity(objects.len());
    for (id, value) in objects {
        let offset = cursor.position() as i32;
        let class_name = value
            .get("_class")
            .and_then(|v| v.as_str())
            .context("Object missing _class field")?;

        let (&class_id, class_def) = classes
            .iter()
            .find(|(_, c)| c.name == class_name)
            .context(format!("Unknown class: {}", class_name))?;

        cursor.write_i32::<BigEndian>(class_id)?;
        write_object_fields(&mut cursor, class_def, value, classes)?;
        index.push((*id, offset));
    }

    // Write index
    let indexes_pointer = cursor.position() as i32;
    let indexes_length = (index.len() as i32) * 8;
    cursor.write_i32::<BigEndian>(indexes_length)?;
    for (key, offset) in &index {
        cursor.write_i32::<BigEndian>(*key)?;
        cursor.write_i32::<BigEndian>(*offset)?;
    }

    // Write class definitions
    cursor.write_i32::<BigEndian>(classes.len() as i32)?;
    for (&class_id, class_def) in classes {
        cursor.write_i32::<BigEndian>(class_id)?;
        write_utf(&mut cursor, &class_def.name)?;
        write_utf(&mut cursor, &class_def.package)?;
        cursor.write_i32::<BigEndian>(class_def.fields.len() as i32)?;
        for field in &class_def.fields {
            write_utf(&mut cursor, &field.name)?;
            write_field_type(&mut cursor, &field.field_type)?;
        }
    }

    // Go back and write the real indexes_pointer
    cursor.seek(SeekFrom::Start(3))?;
    cursor.write_i32::<BigEndian>(indexes_pointer)?;

    Ok(buf)
}

fn write_object_fields(
    w: &mut impl Write,
    class_def: &D2OClassDef,
    value: &Value,
    classes: &std::collections::HashMap<i32, D2OClassDef>,
) -> Result<()> {
    for field in &class_def.fields {
        let field_value = value.get(&field.name).unwrap_or(&Value::Null);
        write_field_value(w, &field.field_type, field_value, classes)?;
    }
    Ok(())
}

fn write_field_value(
    w: &mut impl Write,
    field_type: &D2OFieldType,
    value: &Value,
    classes: &std::collections::HashMap<i32, D2OClassDef>,
) -> Result<()> {
    match field_type {
        D2OFieldType::Int => {
            w.write_i32::<BigEndian>(value.as_i64().unwrap_or(0) as i32)?;
        }
        D2OFieldType::Bool => {
            w.write_u8(if value.as_bool().unwrap_or(false) { 1 } else { 0 })?;
        }
        D2OFieldType::String => {
            if value.is_null() {
                write_utf(w, "null")?;
            } else {
                let s = value.as_str().unwrap_or("");
                write_utf(w, s)?;
            }
        }
        D2OFieldType::Number => {
            w.write_f64::<BigEndian>(value.as_f64().unwrap_or(0.0))?;
        }
        D2OFieldType::I18n => {
            w.write_i32::<BigEndian>(value.as_i64().unwrap_or(0) as i32)?;
        }
        D2OFieldType::UInt => {
            w.write_u32::<BigEndian>(value.as_u64().unwrap_or(0) as u32)?;
        }
        D2OFieldType::Vector(_, inner) => {
            let arr = value.as_array();
            let count = arr.map(|a| a.len()).unwrap_or(0) as i32;
            w.write_i32::<BigEndian>(count)?;
            if let Some(arr) = arr {
                for item in arr {
                    write_field_value(w, inner, item, classes)?;
                }
            }
        }
        D2OFieldType::Object(_) => {
            if value.is_null() {
                w.write_i32::<BigEndian>(NULL_OBJECT_MARKER)?;
            } else {
                let class_name = value
                    .get("_class")
                    .and_then(|v| v.as_str())
                    .context("Embedded object missing _class")?;

                let (&class_id, class_def) = classes
                    .iter()
                    .find(|(_, c)| c.name == class_name)
                    .context(format!("Unknown embedded class: {}", class_name))?;

                w.write_i32::<BigEndian>(class_id)?;
                write_object_fields(w, class_def, value, classes)?;
            }
        }
    }
    Ok(())
}

fn write_field_type(w: &mut impl Write, ft: &D2OFieldType) -> Result<()> {
    match ft {
        D2OFieldType::Int => w.write_i32::<BigEndian>(-1)?,
        D2OFieldType::Bool => w.write_i32::<BigEndian>(-2)?,
        D2OFieldType::String => w.write_i32::<BigEndian>(-3)?,
        D2OFieldType::Number => w.write_i32::<BigEndian>(-4)?,
        D2OFieldType::I18n => w.write_i32::<BigEndian>(-5)?,
        D2OFieldType::UInt => w.write_i32::<BigEndian>(-6)?,
        D2OFieldType::Vector(type_name, inner) => {
            w.write_i32::<BigEndian>(-99)?;
            write_utf(w, type_name)?;
            write_field_type(w, inner)?;
        }
        D2OFieldType::Object(id) => w.write_i32::<BigEndian>(*id)?,
    }
    Ok(())
}

fn write_utf(w: &mut impl Write, s: &str) -> Result<()> {
    let bytes = s.as_bytes();
    if bytes.len() > u16::MAX as usize {
        bail!("String too long for D2O UTF: {} bytes", bytes.len());
    }
    w.write_u16::<BigEndian>(bytes.len() as u16)?;
    w.write_all(bytes)?;
    Ok(())
}
