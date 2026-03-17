//! Parses decompiled ActionScript 3 (.as) files to extract protocol classes.

use crate::extractor::{BooleanWrapper, FieldType, ProtocolClass, ProtocolEnum, ProtocolField, TypeHierarchy};
use anyhow::{Context, Result};
use std::collections::{HashMap, HashSet};
use std::path::Path;

/// Parse all protocol classes from a directory of decompiled .as files.
pub fn parse_protocol_dir(scripts_dir: &Path) -> Result<Vec<ProtocolClass>> {
    let messages_dir = scripts_dir.join("com/ankamagames/dofus/network/messages");
    let types_dir = scripts_dir.join("com/ankamagames/dofus/network/types");

    let mut classes = Vec::new();

    if messages_dir.exists() {
        let files = collect_as_files(&messages_dir)?;
        tracing::info!(count = files.len(), "Found message .as files");
        for path in &files {
            match parse_as_file(path, true) {
                Ok(Some(cls)) => classes.push(cls),
                Ok(None) => {}
                Err(e) => tracing::warn!(file = %path.display(), error = %e, "Failed to parse message"),
            }
        }
    }

    if types_dir.exists() {
        let files = collect_as_files(&types_dir)?;
        tracing::info!(count = files.len(), "Found type .as files");
        for path in &files {
            match parse_as_file(path, false) {
                Ok(Some(cls)) => classes.push(cls),
                Ok(None) => {}
                Err(e) => tracing::warn!(file = %path.display(), error = %e, "Failed to parse type"),
            }
        }
    }

    resolve_inheritance(&mut classes);
    classes.sort_by_key(|c| c.protocol_id);
    Ok(classes)
}

fn collect_as_files(dir: &Path) -> Result<Vec<std::path::PathBuf>> {
    let mut files = Vec::new();
    collect_recursive(dir, &mut files)?;
    Ok(files)
}

fn collect_recursive(dir: &Path, files: &mut Vec<std::path::PathBuf>) -> Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            collect_recursive(&path, files)?;
        } else if path.extension().map(|e| e == "as").unwrap_or(false) {
            files.push(path);
        }
    }
    Ok(())
}

fn parse_as_file(path: &Path, is_message: bool) -> Result<Option<ProtocolClass>> {
    let source = std::fs::read_to_string(path)
        .with_context(|| format!("Reading {}", path.display()))?;

    let (class_name, parent) = match extract_class_declaration(&source) {
        Some(v) => v,
        None => return Ok(None),
    };

    if class_name == "NetworkMessage" || class_name == "NetworkType" {
        return Ok(None);
    }

    let protocol_id = match extract_protocol_id(&source) {
        Some(id) => id,
        None => return Ok(None),
    };

    let full_name = path_to_full_name(path);
    let method_name = format!("serializeAs_{}", class_name);
    let (fields, boolean_wrappers) = extract_fields(&source, &method_name);

    Ok(Some(ProtocolClass {
        name: class_name,
        full_name,
        protocol_id,
        parent,
        is_message,
        fields,
        boolean_byte_wrappers: boolean_wrappers,
    }))
}

fn extract_class_declaration(source: &str) -> Option<(String, Option<String>)> {
    for line in source.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("public class ") {
            let rest = &trimmed["public class ".len()..];
            let parts: Vec<&str> = rest.split_whitespace().collect();
            let class_name = parts.first()?.to_string();
            let parent = parts.iter()
                .position(|&p| p == "extends")
                .and_then(|i| parts.get(i + 1))
                .map(|s| s.to_string());
            return Some((class_name, parent));
        }
    }
    None
}

fn extract_protocol_id(source: &str) -> Option<u32> {
    for line in source.lines() {
        let trimmed = line.trim();
        if trimmed.contains("protocolId") && trimmed.contains("=") {
            if let Some(eq_pos) = trimmed.find('=') {
                let num_str = trimmed[eq_pos + 1..].trim().trim_end_matches(';').trim();
                return num_str.parse::<u32>().ok();
            }
        }
    }
    None
}

fn path_to_full_name(path: &Path) -> String {
    let path_str = path.to_string_lossy();
    if let Some(idx) = path_str.find("com/ankamagames") {
        return path_str[idx..].trim_end_matches(".as").replace('/', ".");
    }
    path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default()
}

// ─── Field extraction ────────────────────────────────────────────

/// Extract fields from the serializeAs_ method. This is the core logic.
///
/// Patterns handled:
/// 1. `output.writeXxx(this.field)` → primitive field
/// 2. `BooleanByteWrapper.setFlag(...)` + `output.writeByte(_box0)` → packed booleans
/// 3. `output.writeXxx(this.arr.length)` + `for(...) { output.writeYyy(this.arr[...]) }` → Vec<primitive>
/// 4. `output.writeShort(this.arr.length)` + `for(...) { this.arr[i].serializeAs_T(output) }` → Vec<Type>
/// 5. `this.field.serializeAs_Type(output)` → nested type
/// 6. `output.writeShort(this.field.getTypeId())` + `this.field.serialize(output)` → polymorphic
/// 7. `super.serializeAs_Parent(output)` → parent fields (resolved later)
fn extract_fields(source: &str, method_name: &str) -> (Vec<ProtocolField>, Vec<BooleanWrapper>) {
    let mut fields = Vec::new();
    let mut boolean_wrappers = Vec::new();

    let body = match extract_method_body(source, method_name) {
        Some(b) => b,
        None => return (fields, boolean_wrappers),
    };

    let lines: Vec<&str> = body.lines().map(|l| l.trim()).collect();
    let mut i = 0;
    let mut box_count: u32 = 0;

    while i < lines.len() {
        let line = lines[i];

        // Skip empty lines, braces, var declarations, if/throw blocks
        if line.is_empty() || line == "{" || line == "}" || line.starts_with("var ")
            || line.starts_with("if(") || line.starts_with("throw ")
            || line.starts_with("//") || line.starts_with("else")
        {
            i += 1;
            continue;
        }

        // Pattern: BooleanByteWrapper.setFlag block
        if line.contains("BooleanByteWrapper.setFlag") {
            let (bool_fields, wrapper, consumed) = parse_boolean_block(&lines, i);
            if let Some(w) = wrapper {
                boolean_wrappers.push(BooleanWrapper {
                    box_var: box_count,
                    fields: w,
                });
            }
            for bf in bool_fields {
                fields.push(bf);
            }
            box_count += 1;
            i += consumed;
            continue;
        }

        // Pattern: super.serializeAs_xxx(output) — skip, handled by inheritance
        if line.contains("super.serializeAs_") || line.contains("super.serialize(") {
            i += 1;
            continue;
        }

        // Pattern: output.writeXxx(this.field.length) → start of array
        if line.starts_with("output.write") && line.contains(".length)") {
            if let Some((arr_name, len_method)) = parse_length_write(line) {
                // Look ahead for the for loop to determine element type
                let (elem_type, consumed) = parse_for_loop(&lines, i + 1, &arr_name);
                fields.push(ProtocolField {
                    name: arr_name,
                    field_type: FieldType::Vector {
                        inner: Box::new(elem_type),
                        length_type: len_method,
                    },
                    write_method: "vector".to_string(),
                });
                i += 1 + consumed;
                continue;
            }
        }

        // Pattern: output.writeShort(this.field.getTypeId()) → polymorphic type
        if line.starts_with("output.write") && line.contains(".getTypeId()") {
            if let Some(field_name) = extract_type_id_field(line) {
                // Next relevant line should be this.field.serialize(output) or .serializeAs_
                let type_name = find_serialize_after(
                    &lines, i + 1, &field_name,
                );
                fields.push(ProtocolField {
                    name: field_name,
                    field_type: FieldType::TypeManager(type_name.unwrap_or_default()),
                    write_method: "TypeManager".to_string(),
                });
                // Skip the serialize line
                i += 2;
                continue;
            }
        }

        // Pattern: this.field.serializeAs_Type(output)
        if line.starts_with("this.") && line.contains(".serializeAs_") {
            if let Some((field_name, type_name)) = parse_direct_serialize(line) {
                fields.push(ProtocolField {
                    name: field_name,
                    field_type: FieldType::Type(type_name.clone()),
                    write_method: format!("serializeAs_{}", type_name),
                });
                i += 1;
                continue;
            }
        }

        // Pattern: this.field.serialize(output)
        if line.starts_with("this.") && line.ends_with(".serialize(output);") {
            let field_name = line["this.".len()..line.len() - ".serialize(output);".len()].to_string();
            if !field_name.contains('[') {
                fields.push(ProtocolField {
                    name: field_name,
                    field_type: FieldType::Type(String::new()),
                    write_method: "serialize".to_string(),
                });
                i += 1;
                continue;
            }
        }

        // Pattern: output.writeXxx(this.field) → simple primitive
        if line.starts_with("output.write") {
            if let Some((field_name, write_method)) = parse_simple_write(line) {
                fields.push(ProtocolField {
                    name: field_name,
                    field_type: writer_to_field_type(&write_method),
                    write_method,
                });
                i += 1;
                continue;
            }
        }

        i += 1;
    }

    // Resolve types from "public var" declarations for Type("") fields
    resolve_field_types_from_declarations(source, &mut fields);

    (fields, boolean_wrappers)
}

/// Parse a BooleanByteWrapper.setFlag block. Returns (fields, wrapper_bits, lines_consumed).
fn parse_boolean_block(lines: &[&str], start: usize) -> (Vec<ProtocolField>, Option<Vec<(u8, String)>>, usize) {
    let mut bool_fields = Vec::new();
    let mut wrapper_bits = Vec::new();
    let mut i = start;

    // Collect all setFlag lines
    while i < lines.len() && lines[i].contains("BooleanByteWrapper.setFlag") {
        if let Some((bit, field_name)) = parse_set_flag(lines[i]) {
            wrapper_bits.push((bit, field_name.clone()));
            bool_fields.push(ProtocolField {
                name: field_name,
                field_type: FieldType::Bool,
                write_method: format!("BooleanByteWrapper"),
            });
        }
        i += 1;
    }

    // Skip the output.writeByte(_boxN) line
    if i < lines.len() && lines[i].contains("writeByte") && lines[i].contains("_box") {
        i += 1;
    }

    (bool_fields, Some(wrapper_bits), i - start)
}

/// Parse `BooleanByteWrapper.setFlag(_box0,N,this.field)` → (bit, field_name)
fn parse_set_flag(line: &str) -> Option<(u8, String)> {
    let start = line.find("setFlag(")? + "setFlag(".len();
    let end = line[start..].find(')')? + start;
    let args: Vec<&str> = line[start..end].split(',').map(|s| s.trim()).collect();
    if args.len() != 3 { return None; }
    let bit: u8 = args[1].parse().ok()?;
    let field = args[2].strip_prefix("this.")?.to_string();
    Some((bit, field))
}

/// Parse `output.writeXxx(this.arr.length)` → (array_name, write_method)
fn parse_length_write(line: &str) -> Option<(String, String)> {
    let paren = line.find('(')?;
    let method = line["output.".len()..paren].to_string();
    let args = &line[paren + 1..line.rfind(')')?];
    let field_expr = args.strip_prefix("this.")?;
    let dot_len = field_expr.find(".length")?;
    let arr_name = field_expr[..dot_len].to_string();
    Some((arr_name, method))
}

/// Parse a for loop after an array length write. Returns (element_type, lines_consumed).
fn parse_for_loop(lines: &[&str], start: usize, _arr_name: &str) -> (FieldType, usize) {
    let mut i = start;

    // Find the for statement
    while i < lines.len() {
        if lines[i].starts_with("for(") || lines[i].starts_with("for (") {
            break;
        }
        // Skip if/throw validation lines
        if lines[i].starts_with("if(") || lines[i].starts_with("throw ") || lines[i] == "{" || lines[i] == "}" {
            i += 1;
            continue;
        }
        break;
    }

    if i >= lines.len() || (!lines[i].starts_with("for(") && !lines[i].starts_with("for (")) {
        return (FieldType::Byte, 0);
    }

    // Scan the for loop body
    let loop_start = i;
    i += 1; // skip "for(...)"
    if i < lines.len() && lines[i] == "{" { i += 1; }

    let mut elem_type = FieldType::Byte;
    let mut depth = 1;

    while i < lines.len() && depth > 0 {
        let line = lines[i];
        if line == "{" { depth += 1; }
        if line == "}" { depth -= 1; if depth == 0 { i += 1; break; } }

        // output.writeXxx(this.arr[...]) → primitive element
        if line.starts_with("output.write") && line.contains("[") {
            let paren = line.find('(').unwrap_or(0);
            let method = &line["output.".len()..paren];
            elem_type = writer_to_field_type(method);
        }

        // (this.arr[i] as Type).serializeAs_Type(output) → Type element
        if line.starts_with("(this.") && line.contains(".serializeAs_") {
            if let Some(type_name) = extract_serialize_as_type(line) {
                elem_type = FieldType::Type(type_name);
            }
        }

        // this.arr[i].serializeAs_Type(output)
        if line.starts_with("this.") && line.contains("[") && line.contains(".serializeAs_") {
            if let Some(type_name) = extract_serialize_as_type(line) {
                elem_type = FieldType::Type(type_name);
            }
        }

        // Polymorphic in array: output.writeShort(this.arr[i].getTypeId())
        if line.starts_with("output.write") && line.contains(".getTypeId()") {
            // Next line should have serialize call with type info
            if i + 1 < lines.len() {
                if let Some(type_name) = extract_serialize_as_type(lines[i + 1]) {
                    elem_type = FieldType::TypeManager(type_name);
                } else {
                    elem_type = FieldType::TypeManager(String::new());
                }
            }
        }

        i += 1;
    }

    (elem_type, i - loop_start)
}

/// Extract type name from serializeAs_TypeName pattern
fn extract_serialize_as_type(line: &str) -> Option<String> {
    let marker = ".serializeAs_";
    let pos = line.find(marker)? + marker.len();
    let end = line[pos..].find('(')?;
    Some(line[pos..pos + end].to_string())
}

/// Parse `output.writeShort(this.field.getTypeId())` → field_name
fn extract_type_id_field(line: &str) -> Option<String> {
    let start = line.find("(this.")? + "(this.".len();
    let end = line[start..].find(".getTypeId()")?;
    Some(line[start..start + end].to_string())
}

/// Find the serialize/serializeAs call for a field after the typeId write
fn find_serialize_after(lines: &[&str], start: usize, field_name: &str) -> Option<String> {
    for i in start..std::cmp::min(start + 5, lines.len()) {
        let line = lines[i];
        let prefix = format!("this.{}.", field_name);
        if line.starts_with(&prefix) {
            if let Some(type_name) = extract_serialize_as_type(line) {
                return Some(type_name);
            }
            if line.contains(".serialize(") {
                return None; // generic serialize, type unknown from here
            }
        }
    }
    None
}

/// Parse `this.field.serializeAs_Type(output)` → (field_name, type_name)
fn parse_direct_serialize(line: &str) -> Option<(String, String)> {
    if line.contains("[") { return None; }
    let field_start = "this.".len();
    let serialize_pos = line.find(".serializeAs_")?;
    let field_name = line[field_start..serialize_pos].to_string();
    let type_start = serialize_pos + ".serializeAs_".len();
    let paren = line[type_start..].find('(')?;
    let type_name = line[type_start..type_start + paren].to_string();
    Some((field_name, type_name))
}

/// Parse `output.writeXxx(this.field)` → (field_name, write_method)
fn parse_simple_write(line: &str) -> Option<(String, String)> {
    let paren = line.find('(')?;
    let method = line["output.".len()..paren].to_string();
    let args = &line[paren + 1..line.rfind(')')?];

    // Must be this.field (not this.field.length, not this.field[i], not this.field.getTypeId())
    let field = args.strip_prefix("this.")?;
    if field.contains('.') || field.contains('[') {
        return None;
    }

    Some((field.to_string(), method))
}

/// Resolve FieldType::Type("") by looking at `public var field:TypeName` declarations
fn resolve_field_types_from_declarations(source: &str, fields: &mut [ProtocolField]) {
    let mut var_types: HashMap<String, String> = HashMap::new();

    for line in source.lines() {
        let trimmed = line.trim();
        // public var fieldName:TypeName = ...;
        // public var fieldName:Vector.<TypeName> = ...;
        if trimmed.starts_with("public var ") {
            let rest = &trimmed["public var ".len()..];
            if let Some(colon) = rest.find(':') {
                let var_name = rest[..colon].to_string();
                let after_colon = &rest[colon + 1..];
                // Get type up to = or ;
                let type_end = after_colon.find(|c: char| c == '=' || c == ';').unwrap_or(after_colon.len());
                let type_str = after_colon[..type_end].trim().to_string();
                var_types.insert(var_name, type_str);
            }
        }
    }

    for field in fields.iter_mut() {
        match &field.field_type {
            FieldType::Type(name) if name.is_empty() => {
                if let Some(type_str) = var_types.get(&field.name) {
                    field.field_type = FieldType::Type(type_str.clone());
                }
            }
            FieldType::TypeManager(name) if name.is_empty() => {
                if let Some(type_str) = var_types.get(&field.name) {
                    field.field_type = FieldType::TypeManager(type_str.clone());
                }
            }
            FieldType::Vector { inner, length_type } => {
                if matches!(inner.as_ref(), FieldType::Type(n) if n.is_empty()) {
                    if let Some(type_str) = var_types.get(&field.name) {
                        if let Some(inner_type) = extract_vector_inner_type(type_str) {
                            field.field_type = FieldType::Vector {
                                inner: Box::new(FieldType::Type(inner_type)),
                                length_type: length_type.clone(),
                            };
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

/// Extract inner type from `Vector.<TypeName>` → `TypeName`
fn extract_vector_inner_type(type_str: &str) -> Option<String> {
    let start = type_str.find("Vector.<")? + "Vector.<".len();
    let end = type_str[start..].find('>')? + start;
    Some(type_str[start..end].to_string())
}

fn writer_to_field_type(method: &str) -> FieldType {
    match method {
        "writeBoolean" => FieldType::Bool,
        "writeByte" => FieldType::Byte,
        "writeUnsignedByte" => FieldType::UByte,
        "writeShort" => FieldType::Short,
        "writeUnsignedShort" => FieldType::UShort,
        "writeInt" => FieldType::Int,
        "writeUnsignedInt" | "writeUInt" => FieldType::UInt,
        "writeFloat" => FieldType::Float,
        "writeDouble" => FieldType::Double,
        "writeUTF" => FieldType::String,
        "writeVarInt" => FieldType::VarInt,
        "writeVarUhInt" | "writeVarUInt" => FieldType::VarUInt,
        "writeVarShort" => FieldType::VarShort,
        "writeVarUhShort" | "writeVarUShort" => FieldType::VarUShort,
        "writeVarLong" => FieldType::VarLong,
        "writeVarUhLong" | "writeVarULong" => FieldType::VarULong,
        "writeBytes" => FieldType::ByteArray,
        _ => FieldType::Byte,
    }
}

fn extract_method_body<'a>(source: &'a str, method_name: &str) -> Option<&'a str> {
    let pattern = format!("function {}(", method_name);
    let start_idx = source.find(&pattern)?;
    let after_sig = &source[start_idx..];
    let brace_start = after_sig.find('{')?;
    let body_start = start_idx + brace_start + 1;

    let mut depth = 1;
    let mut pos = body_start;
    let bytes = source.as_bytes();
    while pos < bytes.len() && depth > 0 {
        match bytes[pos] {
            b'{' => depth += 1,
            b'}' => depth -= 1,
            _ => {}
        }
        pos += 1;
    }

    if depth == 0 { Some(&source[body_start..pos - 1]) } else { None }
}

fn resolve_inheritance(classes: &mut Vec<ProtocolClass>) {
    let field_map: HashMap<String, Vec<ProtocolField>> = classes
        .iter()
        .map(|c| (c.name.clone(), c.fields.clone()))
        .collect();

    let parent_map: HashMap<String, Option<String>> = classes
        .iter()
        .map(|c| (c.name.clone(), c.parent.clone()))
        .collect();

    for cls in classes.iter_mut() {
        let mut parent_fields = Vec::new();
        let mut current_parent = cls.parent.clone();
        let mut visited = HashSet::new();

        while let Some(parent_name) = current_parent {
            if parent_name == "NetworkMessage" || parent_name == "NetworkType" {
                break;
            }
            if !visited.insert(parent_name.clone()) {
                break; // avoid infinite loops
            }
            if let Some(pfields) = field_map.get(&parent_name) {
                let mut pf = pfields.clone();
                pf.append(&mut parent_fields);
                parent_fields = pf;
            }
            current_parent = parent_map.get(&parent_name).cloned().flatten();
        }

        if !parent_fields.is_empty() {
            parent_fields.append(&mut cls.fields);
            cls.fields = parent_fields;
        }
    }
}

// ─── Enum parsing ────────────────────────────────────────────────

/// Parse all enum files from the enums directory.
pub fn parse_enums(scripts_dir: &Path) -> Result<Vec<ProtocolEnum>> {
    let enums_dir = scripts_dir.join("com/ankamagames/dofus/network/enums");
    let mut enums = Vec::new();

    if !enums_dir.exists() {
        return Ok(enums);
    }

    let files = collect_as_files(&enums_dir)?;
    tracing::info!(count = files.len(), "Found enum .as files");

    for path in &files {
        match parse_enum_file(path) {
            Ok(Some(e)) => enums.push(e),
            Ok(None) => {}
            Err(e) => tracing::warn!(file = %path.display(), error = %e, "Failed to parse enum"),
        }
    }

    Ok(enums)
}

fn parse_enum_file(path: &Path) -> Result<Option<ProtocolEnum>> {
    let source = std::fs::read_to_string(path)?;

    let class_name = match extract_class_declaration(&source) {
        Some((name, _)) => name,
        None => return Ok(None),
    };

    let mut values = Vec::new();
    let mut value_type = "uint".to_string();

    for line in source.lines() {
        let trimmed = line.trim();
        // public static const NAME:uint = VALUE;
        if trimmed.starts_with("public static const ") && trimmed.contains('=') {
            let rest = &trimmed["public static const ".len()..];
            if let Some(colon) = rest.find(':') {
                let const_name = rest[..colon].to_string();
                let after_colon = &rest[colon + 1..];
                if let Some(eq) = after_colon.find('=') {
                    let type_str = after_colon[..eq].trim();
                    value_type = type_str.to_string();
                    let val_str = after_colon[eq + 1..].trim().trim_end_matches(';').trim();
                    if let Ok(val) = val_str.parse::<i64>() {
                        values.push((const_name, val));
                    }
                }
            }
        }
    }

    if values.is_empty() {
        return Ok(None);
    }

    Ok(Some(ProtocolEnum {
        name: class_name,
        values,
        value_type,
    }))
}

// ─── Type hierarchy ──────────────────────────────────────────────

/// Build type hierarchies for polymorphic dispatch.
/// Returns a map: base_type_name → TypeHierarchy (with all descendants).
pub fn build_type_hierarchies(classes: &[ProtocolClass]) -> Vec<TypeHierarchy> {
    // Build parent → children map
    let mut children_map: HashMap<String, Vec<String>> = HashMap::new();
    let mut id_map: HashMap<String, u32> = HashMap::new();

    for cls in classes {
        if !cls.is_message {
            id_map.insert(cls.name.clone(), cls.protocol_id);
            if let Some(parent) = &cls.parent {
                if parent != "NetworkType" && parent != "NetworkMessage" {
                    children_map.entry(parent.clone()).or_default().push(cls.name.clone());
                }
            }
        }
    }

    // Find all base types that have at least one child
    let mut hierarchies = Vec::new();

    for (base_name, _) in &children_map {
        // Only create hierarchy for actual roots or types used polymorphically
        // A root is a type whose parent is not in the children_map values
        // (i.e., it's not itself a child of another protocol type in this context)
        // But we also need intermediate types — so create hierarchy for every type that has children.

        let mut variants = Vec::new();
        // Include self
        if let Some(&id) = id_map.get(base_name) {
            variants.push((base_name.clone(), id));
        }
        // Collect all descendants transitively
        collect_descendants(base_name, &children_map, &id_map, &mut variants, &mut HashSet::new());

        if variants.len() > 1 {
            hierarchies.push(TypeHierarchy {
                base_name: base_name.clone(),
                variants,
            });
        }
    }

    tracing::info!(count = hierarchies.len(), "Built type hierarchies");
    hierarchies
}

fn collect_descendants(
    name: &str,
    children_map: &HashMap<String, Vec<String>>,
    id_map: &HashMap<String, u32>,
    variants: &mut Vec<(String, u32)>,
    visited: &mut HashSet<String>,
) {
    if !visited.insert(name.to_string()) {
        return;
    }
    if let Some(children) = children_map.get(name) {
        for child in children {
            if let Some(&id) = id_map.get(child) {
                if !variants.iter().any(|(n, _)| n == child) {
                    variants.push((child.clone(), id));
                }
            }
            collect_descendants(child, children_map, id_map, variants, visited);
        }
    }
}
