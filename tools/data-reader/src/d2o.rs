//! D2O (Game Data Object) file reader.
//!
//! Format:
//!   "D2O" magic (3 bytes)
//!   indexes_pointer: i32  — offset to index table
//!   [object data...]
//!   [index table: count = size/8, each entry = (key: i32, offset: i32)]
//!   [class definitions: count: i32, then class_id, name, package, fields...]

use anyhow::{bail, Context, Result};
use byteorder::{BigEndian, ReadBytesExt};
use serde_json::{json, Map, Value};
use std::collections::HashMap;
use std::io::{Cursor, Read, Seek, SeekFrom};

const D2O_MAGIC: &[u8; 3] = b"D2O";

// GameDataTypeEnum
const TYPE_INT: i32 = -1;
const TYPE_BOOL: i32 = -2;
const TYPE_STRING: i32 = -3;
const TYPE_NUMBER: i32 = -4;
const TYPE_I18N: i32 = -5;
const TYPE_UINT: i32 = -6;
const TYPE_VECTOR: i32 = -99;

pub const NULL_OBJECT_MARKER: i32 = -1431655766; // 0xAAAAAAAA as i32

#[derive(Debug, Clone)]
pub struct D2OClassDef {
    pub class_id: i32,
    pub name: String,
    pub package: String,
    pub fields: Vec<D2OFieldDef>,
}

#[derive(Debug, Clone)]
pub struct D2OFieldDef {
    pub name: String,
    pub field_type: D2OFieldType,
}

#[derive(Debug, Clone)]
pub enum D2OFieldType {
    Int,
    Bool,
    String,
    Number,
    I18n,
    UInt,
    Vector(String, Box<D2OFieldType>), // (inner_type_name, inner_type)
    Object(i32), // class_id
}

pub struct D2OReader {
    classes: HashMap<i32, D2OClassDef>,
    indexes: Vec<(i32, i32)>, // (object_id, offset)
    content_offset: u64,
    data: Vec<u8>,
}

impl D2OReader {
    pub fn open(path: &std::path::Path) -> Result<Self> {
        let data = std::fs::read(path).context("Reading D2O file")?;
        Self::from_bytes(data)
    }

    pub fn from_bytes(data: Vec<u8>) -> Result<Self> {
        let mut cursor = Cursor::new(&data);

        // Check for AKSF signed container
        let mut magic = [0u8; 3];
        cursor.read_exact(&mut magic)?;

        let content_offset: u64;
        if &magic == b"AKS" {
            // Signed file — skip signature
            let mut f = [0u8; 1];
            cursor.read_exact(&mut f)?; // 'F'
            let _format_version = cursor.read_i16::<BigEndian>()?;
            let sig_len = cursor.read_i32::<BigEndian>()?;
            cursor.seek(SeekFrom::Current(sig_len as i64))?;
            content_offset = cursor.position();

            // Read D2O magic
            cursor.read_exact(&mut magic)?;
            if &magic != D2O_MAGIC {
                bail!("Expected D2O magic after AKSF header");
            }
        } else if &magic == D2O_MAGIC {
            content_offset = 0;
        } else {
            bail!("Invalid D2O file: bad magic {:?}", magic);
        }

        let indexes_pointer = cursor.read_i32::<BigEndian>()? as u64;

        // AS3: stream.position = contentOffset + indexesPointer
        cursor.seek(SeekFrom::Start(content_offset + indexes_pointer))?;

        // Read index entries
        let indexes_length = cursor.read_i32::<BigEndian>()?;
        let entry_count = indexes_length / 8;
        let mut indexes = Vec::with_capacity(entry_count as usize);
        for _ in 0..entry_count {
            let key = cursor.read_i32::<BigEndian>()?;
            let pointer = cursor.read_i32::<BigEndian>()?;
            indexes.push((key, pointer));
        }

        // Read class definitions
        let class_count = cursor.read_i32::<BigEndian>()?;
        let mut classes = HashMap::new();

        for _ in 0..class_count {
            let class_id = cursor.read_i32::<BigEndian>()?;
            let name = read_utf(&mut cursor)?;
            let package = read_utf(&mut cursor)?;
            let field_count = cursor.read_i32::<BigEndian>()?;

            let mut fields = Vec::with_capacity(field_count as usize);
            for _ in 0..field_count {
                let field_name = read_utf(&mut cursor)?;
                let field_type = read_field_type(&mut cursor)?;
                fields.push(D2OFieldDef {
                    name: field_name,
                    field_type,
                });
            }

            classes.insert(
                class_id,
                D2OClassDef {
                    class_id,
                    name,
                    package,
                    fields,
                },
            );
        }

        Ok(Self {
            classes,
            indexes,
            content_offset,
            data,
        })
    }

    /// Get all class definitions.
    pub fn classes(&self) -> &HashMap<i32, D2OClassDef> {
        &self.classes
    }

    /// Get all object IDs.
    pub fn object_ids(&self) -> Vec<i32> {
        self.indexes.iter().map(|(id, _)| *id).collect()
    }

    /// Read a single object by ID.
    pub fn read_object(&self, object_id: i32) -> Result<Value> {
        let offset = self
            .indexes
            .iter()
            .find(|(id, _)| *id == object_id)
            .map(|(_, off)| *off)
            .context("Object ID not found")?;

        let mut cursor = Cursor::new(&self.data);
        cursor.seek(SeekFrom::Start(self.content_offset + offset as u64))?;

        let class_id = cursor.read_i32::<BigEndian>()?;
        self.read_object_fields(&mut cursor, class_id)
    }

    /// Read all objects as JSON values.
    pub fn read_all_objects(&self) -> Result<Vec<Value>> {
        let mut objects = Vec::with_capacity(self.indexes.len());
        for &(id, _) in &self.indexes {
            objects.push(self.read_object(id)?);
        }
        Ok(objects)
    }

    fn read_object_fields(&self, cursor: &mut Cursor<&Vec<u8>>, class_id: i32) -> Result<Value> {
        let class_def = self
            .classes
            .get(&class_id)
            .context(format!("Unknown class ID: {}", class_id))?;

        let mut map = Map::new();
        map.insert("_class".to_string(), json!(class_def.name));

        for field in &class_def.fields {
            let value = self.read_field_value(cursor, &field.field_type)?;
            map.insert(field.name.clone(), value);
        }

        Ok(Value::Object(map))
    }

    fn read_field_value(
        &self,
        cursor: &mut Cursor<&Vec<u8>>,
        field_type: &D2OFieldType,
    ) -> Result<Value> {
        match field_type {
            D2OFieldType::Int => Ok(json!(cursor.read_i32::<BigEndian>()?)),
            D2OFieldType::Bool => Ok(json!(cursor.read_u8()? != 0)),
            D2OFieldType::String => {
                let len = cursor.read_u16::<BigEndian>()? as usize;
                if len == 0 {
                    return Ok(json!(""));
                }
                let mut buf = vec![0u8; len];
                cursor.read_exact(&mut buf)?;
                match String::from_utf8(buf) {
                    Ok(s) if s == "null" => Ok(Value::Null),
                    Ok(s) => Ok(json!(s)),
                    Err(e) => Ok(json!(format!("<invalid utf8: {} bytes>", e.into_bytes().len()))),
                }
            }
            D2OFieldType::Number => Ok(json!(cursor.read_f64::<BigEndian>()?)),
            D2OFieldType::I18n => Ok(json!(cursor.read_i32::<BigEndian>()?)),
            D2OFieldType::UInt => Ok(json!(cursor.read_u32::<BigEndian>()?)),
            D2OFieldType::Vector(_, inner) => {
                let count = cursor.read_i32::<BigEndian>()?;
                let mut vec = Vec::with_capacity(count.max(0) as usize);
                for _ in 0..count {
                    vec.push(self.read_field_value(cursor, inner)?);
                }
                Ok(Value::Array(vec))
            }
            D2OFieldType::Object(class_id) => {
                let actual_class_id = cursor.read_i32::<BigEndian>()?;
                if actual_class_id == NULL_OBJECT_MARKER {
                    Ok(Value::Null)
                } else {
                    self.read_object_fields(cursor, actual_class_id)
                }
            }
        }
    }
}

fn read_field_type<R: Read + ReadBytesExt>(cursor: &mut R) -> Result<D2OFieldType> {
    let type_id = cursor.read_i32::<BigEndian>()?;
    match type_id {
        TYPE_INT => Ok(D2OFieldType::Int),
        TYPE_BOOL => Ok(D2OFieldType::Bool),
        TYPE_STRING => Ok(D2OFieldType::String),
        TYPE_NUMBER => Ok(D2OFieldType::Number),
        TYPE_I18N => Ok(D2OFieldType::I18n),
        TYPE_UINT => Ok(D2OFieldType::UInt),
        TYPE_VECTOR => {
            // AS3: reads inner type name (readUTF) then recursively reads inner type
            let inner_type_name = read_utf(cursor)?;
            let inner = read_field_type(cursor)?;
            Ok(D2OFieldType::Vector(inner_type_name, Box::new(inner)))
        }
        id if id > 0 => Ok(D2OFieldType::Object(id)),
        _ => bail!("Unknown D2O field type: {}", type_id),
    }
}

fn read_utf<R: Read + ReadBytesExt>(cursor: &mut R) -> Result<String> {
    let len = cursor.read_u16::<BigEndian>()? as usize;
    let mut buf = vec![0u8; len];
    cursor.read_exact(&mut buf)?;
    Ok(String::from_utf8(buf)?)
}
