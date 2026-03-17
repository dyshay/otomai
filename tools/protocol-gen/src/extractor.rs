//! Extracts Dofus protocol messages and types from parsed ABC bytecode.
//!
//! Strategy:
//! 1. Build class hierarchy (which class extends which)
//! 2. Find all classes descending from NetworkMessage / NetworkType
//! 3. For each, extract protocolId from static traits
//! 4. Analyze serialize method body to determine field order and types
//! 5. Output structured ProtocolClass descriptions for codegen

use crate::abc::opcodes::{self, OperandType};
use crate::abc::reader::AbcReader;
use crate::abc::*;
use std::collections::{HashMap, HashSet};

/// A fully extracted protocol class ready for code generation.
#[derive(Debug, Clone)]
pub struct ProtocolClass {
    pub name: String,
    pub full_name: String,
    pub protocol_id: u32,
    pub parent: Option<String>,
    pub is_message: bool, // true = message, false = type
    pub fields: Vec<ProtocolField>,
    pub boolean_byte_wrappers: Vec<BooleanWrapper>,
}

#[derive(Debug, Clone)]
pub struct ProtocolField {
    pub name: String,
    pub field_type: FieldType,
    pub write_method: String,
}

#[derive(Debug, Clone)]
pub enum FieldType {
    Bool,
    Byte,
    UByte,
    Short,
    UShort,
    Int,
    UInt,
    Long,
    ULong,
    Float,
    Double,
    String,
    VarInt,
    VarUInt,
    VarShort,
    VarUShort,
    VarLong,
    VarULong,
    ByteArray,
    /// A nested protocol type
    Type(String),
    /// Vector/Array of another type with a length prefix
    Vector {
        inner: Box<FieldType>,
        length_type: String,
    },
    /// A polymorphic type (type ID is written first, then data)
    TypeManager(String),
}

#[derive(Debug, Clone)]
pub struct BooleanWrapper {
    pub box_var: u32,
    pub fields: Vec<(u8, String)>, // (bit offset, field name)
}

/// A parsed AS3 enum (static const class).
#[derive(Debug, Clone)]
pub struct ProtocolEnum {
    pub name: String,
    pub values: Vec<(String, i64)>,
    pub value_type: String, // "uint", "int"
}

/// Polymorphic type hierarchy entry.
#[derive(Debug, Clone)]
pub struct TypeHierarchy {
    pub base_name: String,
    /// All concrete types in this hierarchy (name, protocol_id), including the base itself
    pub variants: Vec<(String, u32)>,
}

/// Extract all protocol classes from a parsed ABC file.
pub fn extract_protocol(abc: &AbcFile) -> Vec<ProtocolClass> {
    let cp = &abc.constant_pool;

    // Step 1: Build class name → index map and parent map
    let mut name_to_idx: HashMap<String, usize> = HashMap::new();
    let mut idx_to_name: HashMap<usize, String> = HashMap::new();
    let mut parent_map: HashMap<usize, String> = HashMap::new();

    for (i, inst) in abc.instances.iter().enumerate() {
        let name = cp.multiname_name(inst.name).to_string();
        let full_name = cp.multiname_full(inst.name);
        let parent_name = cp.multiname_name(inst.super_name).to_string();

        name_to_idx.insert(name.clone(), i);
        idx_to_name.insert(i, name);
        if !parent_name.is_empty() {
            parent_map.insert(i, parent_name);
        }
    }

    // Step 2: Find base classes
    let message_bases = find_descendants("NetworkMessage", &name_to_idx, &parent_map);
    let type_bases = find_descendants("NetworkType", &name_to_idx, &parent_map);

    // Also look for INetworkMessage / INetworkType interface patterns
    let message_bases2 = find_descendants("INetworkMessage", &name_to_idx, &parent_map);

    let all_messages: HashSet<usize> = message_bases.union(&message_bases2).copied().collect();

    tracing::info!(
        messages = all_messages.len(),
        types = type_bases.len(),
        "Found protocol classes"
    );

    // Step 3: Build method → body map
    let mut method_body_map: HashMap<u32, usize> = HashMap::new();
    for (i, body) in abc.method_bodies.iter().enumerate() {
        method_body_map.insert(body.method, i);
    }

    // Step 4: Extract each class
    let mut results = Vec::new();

    let process_class = |idx: usize, is_message: bool| -> Option<ProtocolClass> {
        let inst = &abc.instances[idx];
        let cls = &abc.classes[idx];
        let name = cp.multiname_name(inst.name).to_string();
        let full_name = cp.multiname_full(inst.name);
        let parent = parent_map.get(&idx).cloned();

        // Get protocolId from class static traits
        let protocol_id = find_protocol_id(cls, cp);
        if protocol_id.is_none() {
            // Some abstract base classes don't have protocol IDs
            return None;
        }
        let protocol_id = protocol_id.unwrap();

        // Find the serialize method
        let serialize_method = find_method_by_name(inst, "serialize", cp);
        let fields = if let Some(method_idx) = serialize_method {
            if let Some(&body_idx) = method_body_map.get(&method_idx) {
                analyze_serialize(&abc.method_bodies[body_idx], abc)
            } else {
                Vec::new()
            }
        } else {
            Vec::new()
        };

        // Find boolean wrappers from serialize
        let boolean_wrappers = if let Some(method_idx) = serialize_method {
            if let Some(&body_idx) = method_body_map.get(&method_idx) {
                find_boolean_wrappers(&abc.method_bodies[body_idx], abc)
            } else {
                Vec::new()
            }
        } else {
            Vec::new()
        };

        Some(ProtocolClass {
            name,
            full_name,
            protocol_id,
            parent,
            is_message,
            fields,
            boolean_byte_wrappers: boolean_wrappers,
        })
    };

    for &idx in &all_messages {
        if let Some(cls) = process_class(idx, true) {
            results.push(cls);
        }
    }

    for &idx in &type_bases {
        if !all_messages.contains(&idx) {
            if let Some(cls) = process_class(idx, false) {
                results.push(cls);
            }
        }
    }

    results.sort_by_key(|c| c.protocol_id);
    results
}

/// Find all class indices that descend from `base_name`.
fn find_descendants(
    base_name: &str,
    name_to_idx: &HashMap<String, usize>,
    parent_map: &HashMap<usize, String>,
) -> HashSet<usize> {
    let mut result = HashSet::new();
    let mut queue: Vec<String> = vec![base_name.to_string()];
    let mut visited = HashSet::new();

    while let Some(current) = queue.pop() {
        if !visited.insert(current.clone()) {
            continue;
        }

        // Find all classes whose parent is `current`
        for (&idx, parent) in parent_map {
            if parent == &current {
                result.insert(idx);
                if let Some(name) = name_to_idx.iter().find(|(_, &v)| v == idx).map(|(k, _)| k) {
                    queue.push(name.clone());
                }
            }
        }
    }

    result
}

/// Find protocolId constant in class static traits.
fn find_protocol_id(cls: &ClassInfo, cp: &ConstantPool) -> Option<u32> {
    for t in &cls.traits {
        let name = cp.multiname_name(t.name);
        if name == "protocolId" || name == "id" {
            match &t.data {
                TraitData::Const { vindex, vkind, .. } | TraitData::Slot { vindex, vkind, .. } => {
                    if *vindex > 0 {
                        return match vkind {
                            // 0x03 = int constant, 0x04 = uint constant
                            0x03 => cp.integers.get(*vindex as usize).map(|&v| v as u32),
                            0x04 => cp.uintegers.get(*vindex as usize).copied(),
                            _ => None,
                        };
                    }
                }
                _ => {}
            }
        }
    }
    None
}

/// Find a method index by name in instance traits.
fn find_method_by_name(inst: &InstanceInfo, method_name: &str, cp: &ConstantPool) -> Option<u32> {
    for t in &inst.traits {
        let name = cp.multiname_name(t.name);
        if name == method_name {
            match &t.data {
                TraitData::Method { method, .. }
                | TraitData::Getter { method, .. }
                | TraitData::Setter { method, .. } => return Some(*method),
                _ => {}
            }
        }
    }
    None
}

/// Analyze a serialize method body to extract field write operations in order.
fn analyze_serialize(body: &MethodBody, abc: &AbcFile) -> Vec<ProtocolField> {
    let cp = &abc.constant_pool;
    let mut fields = Vec::new();
    let code = &body.code;
    let mut r = AbcReader::new(code);

    // Walk through bytecode looking for patterns:
    // getproperty <field_name> ... callpropvoid/callproperty <writer_method> <arg_count>
    // This gives us field name + writer method in order

    let mut last_property: Option<String> = None;

    while r.remaining() > 0 {
        let _pos = r.position();
        let op = match r.read_u8() {
            Ok(v) => v,
            Err(_) => break,
        };

        match op {
            0x66 => {
                // getproperty
                if let Ok(idx) = r.read_u30() {
                    let name = cp.multiname_name(idx).to_string();
                    if !name.is_empty() && name != "length" && !name.starts_with("_") {
                        last_property = Some(name);
                    }
                }
            }
            0x4F => {
                // callpropvoid
                if let Ok(idx) = r.read_u30() {
                    let method_name = cp.multiname_name(idx).to_string();
                    let _arg_count = r.read_u30().unwrap_or(0);

                    if let Some(field_name) = last_property.take() {
                        if let Some(ft) = writer_method_to_field_type(&method_name) {
                            fields.push(ProtocolField {
                                name: field_name,
                                field_type: ft,
                                write_method: method_name,
                            });
                        }
                    }
                }
            }
            0x46 => {
                // callproperty
                if let Ok(idx) = r.read_u30() {
                    let method_name = cp.multiname_name(idx).to_string();
                    let _arg_count = r.read_u30().unwrap_or(0);

                    if method_name == "serialize" {
                        // Nested type serialization
                        if let Some(field_name) = last_property.take() {
                            fields.push(ProtocolField {
                                name: field_name.clone(),
                                field_type: FieldType::Type(String::new()), // resolved later
                                write_method: "serialize".to_string(),
                            });
                        }
                    } else if let Some(field_name) = last_property.take() {
                        if let Some(ft) = writer_method_to_field_type(&method_name) {
                            fields.push(ProtocolField {
                                name: field_name,
                                field_type: ft,
                                write_method: method_name,
                            });
                        }
                    }
                }
            }
            // Skip operands for other opcodes
            _ => {
                let operands = opcodes::opcode_operands(op);
                if op == 0x1B {
                    // lookupswitch: special format
                    let _default = r.read_i24().ok();
                    let case_count = r.read_u30().unwrap_or(0);
                    for _ in 0..=case_count {
                        let _ = r.read_i24();
                    }
                } else {
                    for operand in operands {
                        match operand {
                            OperandType::Byte => { let _ = r.read_u8(); }
                            OperandType::U30 => { let _ = r.read_u30(); }
                            OperandType::S24 => { let _ = r.read_bytes(3); }
                        }
                    }
                }
            }
        }
    }

    fields
}

/// Map Dofus BinaryWriter method names to field types.
fn writer_method_to_field_type(method: &str) -> Option<FieldType> {
    match method {
        "writeBoolean" => Some(FieldType::Bool),
        "writeByte" => Some(FieldType::Byte),
        "writeUnsignedByte" => Some(FieldType::UByte),
        "writeShort" => Some(FieldType::Short),
        "writeUShort" => Some(FieldType::UShort),
        "writeInt" => Some(FieldType::Int),
        "writeUInt" | "writeUnsignedInt" => Some(FieldType::UInt),
        "writeFloat" => Some(FieldType::Float),
        "writeDouble" => Some(FieldType::Double),
        "writeUTF" => Some(FieldType::String),
        "writeVarInt" => Some(FieldType::VarInt),
        "writeVarUInt" => Some(FieldType::VarUInt),
        "writeVarShort" => Some(FieldType::VarShort),
        "writeVarUhShort" | "writeVarUShort" => Some(FieldType::VarUShort),
        "writeVarLong" => Some(FieldType::VarLong),
        "writeVarUhLong" | "writeVarULong" => Some(FieldType::VarULong),
        "writeBytes" => Some(FieldType::ByteArray),
        _ => None,
    }
}

/// Find boolean byte wrapper patterns in serialize method.
fn find_boolean_wrappers(body: &MethodBody, abc: &AbcFile) -> Vec<BooleanWrapper> {
    // Boolean wrappers show up as: setFlag(box, offset, this.field)
    // We look for callpropvoid/callproperty "setFlag" sequences
    // This is a simplified extraction — full analysis would require stack tracking
    let _ = (body, abc);
    Vec::new()
}
