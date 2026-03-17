//! ABC (ActionScript Byte Code) parser.
//! Reference: AVM2 Overview (Adobe), avm2overview.pdf

pub mod opcodes;
pub mod reader;

use anyhow::{bail, Result};
use reader::AbcReader;

// ── Top-level ABC file ──────────────────────────────────────────

#[derive(Debug)]
pub struct AbcFile {
    pub minor_version: u16,
    pub major_version: u16,
    pub constant_pool: ConstantPool,
    pub methods: Vec<MethodInfo>,
    pub metadata: Vec<MetadataInfo>,
    pub instances: Vec<InstanceInfo>,
    pub classes: Vec<ClassInfo>,
    pub scripts: Vec<ScriptInfo>,
    pub method_bodies: Vec<MethodBody>,
}

// ── Constant pool ───────────────────────────────────────────────

#[derive(Debug, Default)]
pub struct ConstantPool {
    pub integers: Vec<i32>,
    pub uintegers: Vec<u32>,
    pub doubles: Vec<f64>,
    pub strings: Vec<String>,
    pub namespaces: Vec<Namespace>,
    pub ns_sets: Vec<Vec<u32>>,
    pub multinames: Vec<Multiname>,
}

#[derive(Debug, Clone)]
pub struct Namespace {
    pub kind: u8,
    pub name: u32, // string index
}

#[derive(Debug, Clone)]
pub enum Multiname {
    QName { ns: u32, name: u32 },
    QNameA { ns: u32, name: u32 },
    RTQName { name: u32 },
    RTQNameA { name: u32 },
    RTQNameL,
    RTQNameLA,
    Multiname { name: u32, ns_set: u32 },
    MultinameA { name: u32, ns_set: u32 },
    MultinameL { ns_set: u32 },
    MultinameLA { ns_set: u32 },
    TypeName { qname: u32, params: Vec<u32> },
    Void,
}

// ── Method / Instance / Class / etc ─────────────────────────────

#[derive(Debug)]
pub struct MethodInfo {
    pub param_count: u32,
    pub return_type: u32,
    pub param_types: Vec<u32>,
    pub name: u32,
    pub flags: u8,
    pub options: Vec<OptionValue>,
    pub param_names: Vec<u32>,
}

#[derive(Debug)]
pub struct OptionValue {
    pub val: u32,
    pub kind: u8,
}

#[derive(Debug)]
pub struct MetadataInfo {
    pub name: u32,
    pub items: Vec<(u32, u32)>,
}

#[derive(Debug)]
pub struct InstanceInfo {
    pub name: u32,         // multiname index
    pub super_name: u32,   // multiname index
    pub flags: u8,
    pub protected_ns: u32,
    pub interfaces: Vec<u32>,
    pub iinit: u32,        // method index
    pub traits: Vec<Trait>,
}

#[derive(Debug)]
pub struct ClassInfo {
    pub cinit: u32,
    pub traits: Vec<Trait>,
}

#[derive(Debug)]
pub struct ScriptInfo {
    pub init: u32,
    pub traits: Vec<Trait>,
}

#[derive(Debug)]
pub struct MethodBody {
    pub method: u32,
    pub max_stack: u32,
    pub local_count: u32,
    pub init_scope_depth: u32,
    pub max_scope_depth: u32,
    pub code: Vec<u8>,
    pub exceptions: Vec<ExceptionInfo>,
    pub traits: Vec<Trait>,
}

#[derive(Debug)]
pub struct ExceptionInfo {
    pub from: u32,
    pub to: u32,
    pub target: u32,
    pub exc_type: u32,
    pub var_name: u32,
}

#[derive(Debug, Clone)]
pub struct Trait {
    pub name: u32,     // multiname index
    pub kind: u8,
    pub data: TraitData,
    pub metadata: Vec<u32>,
}

#[derive(Debug, Clone)]
pub enum TraitData {
    Slot {
        slot_id: u32,
        type_name: u32,
        vindex: u32,
        vkind: u8,
    },
    Method { disp_id: u32, method: u32 },
    Getter { disp_id: u32, method: u32 },
    Setter { disp_id: u32, method: u32 },
    Class { slot_id: u32, classi: u32 },
    Function { slot_id: u32, function: u32 },
    Const {
        slot_id: u32,
        type_name: u32,
        vindex: u32,
        vkind: u8,
    },
}

// ── Instance flags ──────────────────────────────────────────────

pub const CLASS_SEALED: u8 = 0x01;
pub const CLASS_FINAL: u8 = 0x02;
pub const CLASS_INTERFACE: u8 = 0x04;
pub const CLASS_PROTECTED_NS: u8 = 0x08;

// ── Method flags ────────────────────────────────────────────────

pub const METHOD_NEED_ARGUMENTS: u8 = 0x01;
pub const METHOD_NEED_ACTIVATION: u8 = 0x02;
pub const METHOD_NEED_REST: u8 = 0x04;
pub const METHOD_HAS_OPTIONAL: u8 = 0x08;
pub const METHOD_SET_DXNS: u8 = 0x40;
pub const METHOD_HAS_PARAM_NAMES: u8 = 0x80;

// ── Trait kinds ─────────────────────────────────────────────────

pub const TRAIT_SLOT: u8 = 0;
pub const TRAIT_METHOD: u8 = 1;
pub const TRAIT_GETTER: u8 = 2;
pub const TRAIT_SETTER: u8 = 3;
pub const TRAIT_CLASS: u8 = 4;
pub const TRAIT_FUNCTION: u8 = 5;
pub const TRAIT_CONST: u8 = 6;
pub const TRAIT_ATTR_METADATA: u8 = 0x40;

// ── Parser ──────────────────────────────────────────────────────

pub fn parse_abc(data: &[u8]) -> Result<AbcFile> {
    let mut r = AbcReader::new(data);

    let minor_version = r.read_u16()?;
    let major_version = r.read_u16()?;

    tracing::debug!(minor_version, major_version, "ABC version");

    let constant_pool = parse_constant_pool(&mut r)?;
    tracing::debug!(
        ints = constant_pool.integers.len(),
        uints = constant_pool.uintegers.len(),
        doubles = constant_pool.doubles.len(),
        strings = constant_pool.strings.len(),
        namespaces = constant_pool.namespaces.len(),
        multinames = constant_pool.multinames.len(),
        "Constant pool parsed"
    );

    let method_count = r.read_u30()?;
    let mut methods = Vec::with_capacity(method_count as usize);
    for _ in 0..method_count {
        methods.push(parse_method_info(&mut r)?);
    }

    let metadata_count = r.read_u30()?;
    let mut metadata = Vec::with_capacity(metadata_count as usize);
    for _ in 0..metadata_count {
        metadata.push(parse_metadata(&mut r)?);
    }

    let class_count = r.read_u30()?;
    let mut instances = Vec::with_capacity(class_count as usize);
    for _ in 0..class_count {
        instances.push(parse_instance_info(&mut r)?);
    }

    let mut classes = Vec::with_capacity(class_count as usize);
    for _ in 0..class_count {
        classes.push(parse_class_info(&mut r)?);
    }

    let script_count = r.read_u30()?;
    let mut scripts = Vec::with_capacity(script_count as usize);
    for _ in 0..script_count {
        scripts.push(parse_script_info(&mut r)?);
    }

    let method_body_count = r.read_u30()?;
    let mut method_bodies = Vec::with_capacity(method_body_count as usize);
    for _ in 0..method_body_count {
        method_bodies.push(parse_method_body(&mut r)?);
    }

    tracing::debug!(
        methods = methods.len(),
        classes = instances.len(),
        method_bodies = method_bodies.len(),
        "ABC parsed"
    );

    Ok(AbcFile {
        minor_version,
        major_version,
        constant_pool,
        methods,
        metadata,
        instances,
        classes,
        scripts,
        method_bodies,
    })
}

fn parse_constant_pool(r: &mut AbcReader) -> Result<ConstantPool> {
    let mut pool = ConstantPool::default();

    // Integers
    let int_count = r.read_u30()? as usize;
    pool.integers.push(0); // index 0 is implicit
    for _ in 1..int_count {
        pool.integers.push(r.read_s32()?);
    }

    // Unsigned integers
    let uint_count = r.read_u30()? as usize;
    pool.uintegers.push(0);
    for _ in 1..uint_count {
        pool.uintegers.push(r.read_u30()?);
    }

    // Doubles
    let double_count = r.read_u30()? as usize;
    pool.doubles.push(f64::NAN);
    for _ in 1..double_count {
        pool.doubles.push(r.read_d64()?);
    }

    // Strings
    let string_count = r.read_u30()? as usize;
    pool.strings.push(String::new()); // index 0 is ""
    for _ in 1..string_count {
        let len = r.read_u30()? as usize;
        let bytes = r.read_bytes(len)?;
        pool.strings.push(String::from_utf8_lossy(bytes).to_string());
    }

    // Namespaces
    let ns_count = r.read_u30()? as usize;
    pool.namespaces.push(Namespace { kind: 0, name: 0 });
    for _ in 1..ns_count {
        let kind = r.read_u8()?;
        let name = r.read_u30()?;
        pool.namespaces.push(Namespace { kind, name });
    }

    // Namespace sets
    let ns_set_count = r.read_u30()? as usize;
    pool.ns_sets.push(Vec::new());
    for _ in 1..ns_set_count {
        let count = r.read_u30()?;
        let mut ns_set = Vec::with_capacity(count as usize);
        for _ in 0..count {
            ns_set.push(r.read_u30()?);
        }
        pool.ns_sets.push(ns_set);
    }

    // Multinames
    let multiname_count = r.read_u30()? as usize;
    pool.multinames.push(Multiname::Void); // index 0
    for _ in 1..multiname_count {
        pool.multinames.push(parse_multiname(r)?);
    }

    Ok(pool)
}

fn parse_multiname(r: &mut AbcReader) -> Result<Multiname> {
    let kind = r.read_u8()?;
    let mn = match kind {
        0x07 => Multiname::QName {
            ns: r.read_u30()?,
            name: r.read_u30()?,
        },
        0x0D => Multiname::QNameA {
            ns: r.read_u30()?,
            name: r.read_u30()?,
        },
        0x0F => Multiname::RTQName {
            name: r.read_u30()?,
        },
        0x10 => Multiname::RTQNameA {
            name: r.read_u30()?,
        },
        0x11 => Multiname::RTQNameL,
        0x12 => Multiname::RTQNameLA,
        0x09 => Multiname::Multiname {
            name: r.read_u30()?,
            ns_set: r.read_u30()?,
        },
        0x0E => Multiname::MultinameA {
            name: r.read_u30()?,
            ns_set: r.read_u30()?,
        },
        0x1B => Multiname::MultinameL {
            ns_set: r.read_u30()?,
        },
        0x1C => Multiname::MultinameLA {
            ns_set: r.read_u30()?,
        },
        0x1D => {
            // TypeName (generics like Vector.<int>)
            let qname = r.read_u30()?;
            let param_count = r.read_u30()?;
            let mut params = Vec::with_capacity(param_count as usize);
            for _ in 0..param_count {
                params.push(r.read_u30()?);
            }
            Multiname::TypeName { qname, params }
        }
        _ => bail!("Unknown multiname kind: 0x{:02X}", kind),
    };
    Ok(mn)
}

fn parse_method_info(r: &mut AbcReader) -> Result<MethodInfo> {
    let param_count = r.read_u30()?;
    let return_type = r.read_u30()?;
    let mut param_types = Vec::with_capacity(param_count as usize);
    for _ in 0..param_count {
        param_types.push(r.read_u30()?);
    }
    let name = r.read_u30()?;
    let flags = r.read_u8()?;

    let mut options = Vec::new();
    if flags & METHOD_HAS_OPTIONAL != 0 {
        let option_count = r.read_u30()?;
        for _ in 0..option_count {
            let val = r.read_u30()?;
            let kind = r.read_u8()?;
            options.push(OptionValue { val, kind });
        }
    }

    let mut param_names = Vec::new();
    if flags & METHOD_HAS_PARAM_NAMES != 0 {
        for _ in 0..param_count {
            param_names.push(r.read_u30()?);
        }
    }

    Ok(MethodInfo {
        param_count,
        return_type,
        param_types,
        name,
        flags,
        options,
        param_names,
    })
}

fn parse_metadata(r: &mut AbcReader) -> Result<MetadataInfo> {
    let name = r.read_u30()?;
    let item_count = r.read_u30()?;
    let mut items = Vec::with_capacity(item_count as usize);
    for _ in 0..item_count {
        let key = r.read_u30()?;
        let val = r.read_u30()?;
        items.push((key, val));
    }
    Ok(MetadataInfo { name, items })
}

fn parse_instance_info(r: &mut AbcReader) -> Result<InstanceInfo> {
    let name = r.read_u30()?;
    let super_name = r.read_u30()?;
    let flags = r.read_u8()?;

    let protected_ns = if flags & CLASS_PROTECTED_NS != 0 {
        r.read_u30()?
    } else {
        0
    };

    let interface_count = r.read_u30()?;
    let mut interfaces = Vec::with_capacity(interface_count as usize);
    for _ in 0..interface_count {
        interfaces.push(r.read_u30()?);
    }

    let iinit = r.read_u30()?;

    let trait_count = r.read_u30()?;
    let mut traits = Vec::with_capacity(trait_count as usize);
    for _ in 0..trait_count {
        traits.push(parse_trait(r)?);
    }

    Ok(InstanceInfo {
        name,
        super_name,
        flags,
        protected_ns,
        interfaces,
        iinit,
        traits,
    })
}

fn parse_class_info(r: &mut AbcReader) -> Result<ClassInfo> {
    let cinit = r.read_u30()?;
    let trait_count = r.read_u30()?;
    let mut traits = Vec::with_capacity(trait_count as usize);
    for _ in 0..trait_count {
        traits.push(parse_trait(r)?);
    }
    Ok(ClassInfo { cinit, traits })
}

fn parse_script_info(r: &mut AbcReader) -> Result<ScriptInfo> {
    let init = r.read_u30()?;
    let trait_count = r.read_u30()?;
    let mut traits = Vec::with_capacity(trait_count as usize);
    for _ in 0..trait_count {
        traits.push(parse_trait(r)?);
    }
    Ok(ScriptInfo { init, traits })
}

fn parse_trait(r: &mut AbcReader) -> Result<Trait> {
    let name = r.read_u30()?;
    let kind_byte = r.read_u8()?;
    let kind = kind_byte & 0x0F;

    let data = match kind {
        TRAIT_SLOT | TRAIT_CONST => {
            let slot_id = r.read_u30()?;
            let type_name = r.read_u30()?;
            let vindex = r.read_u30()?;
            let vkind = if vindex != 0 { r.read_u8()? } else { 0 };
            if kind == TRAIT_CONST {
                TraitData::Const { slot_id, type_name, vindex, vkind }
            } else {
                TraitData::Slot { slot_id, type_name, vindex, vkind }
            }
        }
        TRAIT_METHOD => TraitData::Method {
            disp_id: r.read_u30()?,
            method: r.read_u30()?,
        },
        TRAIT_GETTER => TraitData::Getter {
            disp_id: r.read_u30()?,
            method: r.read_u30()?,
        },
        TRAIT_SETTER => TraitData::Setter {
            disp_id: r.read_u30()?,
            method: r.read_u30()?,
        },
        TRAIT_CLASS => TraitData::Class {
            slot_id: r.read_u30()?,
            classi: r.read_u30()?,
        },
        TRAIT_FUNCTION => TraitData::Function {
            slot_id: r.read_u30()?,
            function: r.read_u30()?,
        },
        _ => bail!("Unknown trait kind: {}", kind),
    };

    let mut metadata_indices = Vec::new();
    if kind_byte & TRAIT_ATTR_METADATA != 0 {
        let metadata_count = r.read_u30()?;
        for _ in 0..metadata_count {
            metadata_indices.push(r.read_u30()?);
        }
    }

    Ok(Trait {
        name,
        kind,
        data,
        metadata: metadata_indices,
    })
}

fn parse_method_body(r: &mut AbcReader) -> Result<MethodBody> {
    let method = r.read_u30()?;
    let max_stack = r.read_u30()?;
    let local_count = r.read_u30()?;
    let init_scope_depth = r.read_u30()?;
    let max_scope_depth = r.read_u30()?;

    let code_length = r.read_u30()? as usize;
    let code = r.read_bytes(code_length)?.to_vec();

    let exception_count = r.read_u30()?;
    let mut exceptions = Vec::with_capacity(exception_count as usize);
    for _ in 0..exception_count {
        exceptions.push(ExceptionInfo {
            from: r.read_u30()?,
            to: r.read_u30()?,
            target: r.read_u30()?,
            exc_type: r.read_u30()?,
            var_name: r.read_u30()?,
        });
    }

    let trait_count = r.read_u30()?;
    let mut traits = Vec::with_capacity(trait_count as usize);
    for _ in 0..trait_count {
        traits.push(parse_trait(r)?);
    }

    Ok(MethodBody {
        method,
        max_stack,
        local_count,
        init_scope_depth,
        max_scope_depth,
        code,
        exceptions,
        traits,
    })
}

// ── Helpers ─────────────────────────────────────────────────────

impl ConstantPool {
    pub fn get_string(&self, index: u32) -> &str {
        self.strings
            .get(index as usize)
            .map(|s| s.as_str())
            .unwrap_or("")
    }

    pub fn multiname_name(&self, index: u32) -> &str {
        match self.multinames.get(index as usize) {
            Some(Multiname::QName { name, .. }) | Some(Multiname::QNameA { name, .. }) => {
                self.get_string(*name)
            }
            Some(Multiname::RTQName { name }) | Some(Multiname::RTQNameA { name }) => {
                self.get_string(*name)
            }
            Some(Multiname::Multiname { name, .. }) | Some(Multiname::MultinameA { name, .. }) => {
                self.get_string(*name)
            }
            Some(Multiname::TypeName { qname, .. }) => self.multiname_name(*qname),
            _ => "",
        }
    }

    pub fn multiname_ns(&self, index: u32) -> &str {
        match self.multinames.get(index as usize) {
            Some(Multiname::QName { ns, .. }) | Some(Multiname::QNameA { ns, .. }) => {
                if let Some(namespace) = self.namespaces.get(*ns as usize) {
                    self.get_string(namespace.name)
                } else {
                    ""
                }
            }
            _ => "",
        }
    }

    /// Full qualified name like "com.ankamagames.dofus.network.messages.game::SomeMessage"
    pub fn multiname_full(&self, index: u32) -> String {
        let ns = self.multiname_ns(index);
        let name = self.multiname_name(index);
        if ns.is_empty() {
            name.to_string()
        } else {
            format!("{}.{}", ns, name)
        }
    }
}
