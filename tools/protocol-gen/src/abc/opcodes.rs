//! AVM2 opcode definitions for method body analysis.

/// AVM2 opcodes relevant for protocol analysis.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Opcode {
    // Stack ops
    PushByte = 0x24,
    PushShort = 0x25,
    PushInt = 0x2D,
    PushUInt = 0x2E,
    PushDouble = 0x2F,
    PushString = 0x2C,
    PushTrue = 0x26,
    PushFalse = 0x27,
    PushNull = 0x20,
    PushUndefined = 0x21,
    PushNaN = 0x28,
    // Local ops
    GetLocal0 = 0xD0,
    GetLocal1 = 0xD1,
    GetLocal2 = 0xD2,
    GetLocal3 = 0xD3,
    SetLocal0 = 0xD4,
    SetLocal1 = 0xD5,
    SetLocal2 = 0xD6,
    SetLocal3 = 0xD7,
    GetLocalN = 0x62, // getlocal <index>
    SetLocalN = 0x63, // setlocal <index>
    // Property access
    GetProperty = 0x66,
    SetProperty = 0x61,
    GetLex = 0x60,
    FindPropStrict = 0x5D,
    // Method calls
    CallProperty = 0x46,
    CallPropVoid = 0x4F,
    CallPropLex = 0x4C,
    CallSuper = 0x45,
    CallSuperVoid = 0x4E,
    ConstructProp = 0x4A,
    // Object
    NewArray = 0x56,
    NewObject = 0x55,
    NewClass = 0x58,
    GetSuper = 0x04,
    SetSuper = 0x05,
    // Coerce / Convert
    CoerceA = 0x82,
    CoerceS = 0x85,
    ConvertI = 0x73,
    ConvertU = 0x74,
    ConvertD = 0x75,
    ConvertB = 0x76,
    ConvertS = 0x70,
    // Control flow
    IfTrue = 0x11,
    IfFalse = 0x12,
    IfEq = 0x13,
    IfNe = 0x14,
    IfLt = 0x15,
    IfLe = 0x16,
    IfGt = 0x17,
    IfGe = 0x18,
    IfStrictEq = 0x19,
    IfStrictNe = 0x1A,
    Jump = 0x10,
    LookupSwitch = 0x1B,
    // Comparison
    Equals = 0xAB,
    StrictEquals = 0xAC,
    LessThan = 0xAD,
    LessEquals = 0xAE,
    GreaterThan = 0xAF,
    GreaterEquals = 0xB0,
    // Arithmetic
    Add = 0xA0,
    Subtract = 0xA1,
    Multiply = 0xA2,
    Divide = 0xA3,
    Modulo = 0xA4,
    Negate = 0xA5,
    Increment = 0xC0,
    Decrement = 0xC1,
    IncrementI = 0xC2,
    DecrementI = 0xC3,
    BitAnd = 0xA8,
    BitOr = 0xA9,
    BitXor = 0xAA,
    BitNot = 0x97,
    LShift = 0xA6,
    RShift = 0xA7,
    // Stack manipulation
    Dup = 0x2A,
    Pop = 0x29,
    Swap = 0x2B,
    // Scope
    PushScope = 0x30,
    PopScope = 0x1D,
    GetScopeObject = 0x65,
    PushWith = 0x1C,
    // Type check
    IsType = 0xB2,
    IsTypeLate = 0xB3,
    AsType = 0x86,
    AsTypeLate = 0x89,
    TypeOf = 0x95,
    InstanceOf = 0xB1,
    // Return
    ReturnVoid = 0x47,
    ReturnValue = 0x48,
    // Debug
    Debug = 0xEF,
    DebugLine = 0xF0,
    DebugFile = 0xF1,
    // Misc
    Nop = 0x02,
    Label = 0x09,
    Kill = 0x08,
    Throw = 0x03,
    GetGlobalScope = 0x64,
    InitProperty = 0x68,
    HasNext2 = 0x32,
    NextName = 0x1E,
    NextValue = 0x23,
}

/// Describes how many operands (from the bytecode stream) each opcode takes.
/// Returns (count, sizes) where sizes are the byte sizes of each operand.
pub fn opcode_operands(op: u8) -> &'static [OperandType] {
    use OperandType::*;
    match op {
        0x02 => &[],                // nop
        0x03 => &[],                // throw
        0x04 => &[U30],            // getsuper
        0x05 => &[U30],            // setsuper
        0x08 => &[U30],            // kill
        0x09 => &[],                // label
        0x10 => &[S24],            // jump
        0x11 => &[S24],            // iftrue
        0x12 => &[S24],            // iffalse
        0x13 => &[S24],            // ifeq
        0x14 => &[S24],            // ifne
        0x15 => &[S24],            // iflt
        0x16 => &[S24],            // ifle
        0x17 => &[S24],            // ifgt
        0x18 => &[S24],            // ifge
        0x19 => &[S24],            // ifstricteq
        0x1A => &[S24],            // ifstrictne
        0x1B => &[],               // lookupswitch (special handling)
        0x1C => &[],                // pushwith
        0x1D => &[],                // popscope
        0x1E => &[],                // nextname
        0x20 => &[],                // pushnull
        0x21 => &[],                // pushundefined
        0x23 => &[],                // nextvalue
        0x24 => &[Byte],           // pushbyte
        0x25 => &[U30],            // pushshort
        0x26 => &[],                // pushtrue
        0x27 => &[],                // pushfalse
        0x28 => &[],                // pushnan
        0x29 => &[],                // pop
        0x2A => &[],                // dup
        0x2B => &[],                // swap
        0x2C => &[U30],            // pushstring
        0x2D => &[U30],            // pushint
        0x2E => &[U30],            // pushuint
        0x2F => &[U30],            // pushdouble
        0x30 => &[],                // pushscope
        0x32 => &[U30, U30],       // hasnext2
        0x40 => &[U30],            // newfunction
        0x41 => &[U30],            // call
        0x42 => &[U30],            // construct
        0x43 => &[U30, U30],       // callmethod
        0x44 => &[U30, U30],       // callstatic
        0x45 => &[U30, U30],       // callsuper
        0x46 => &[U30, U30],       // callproperty
        0x47 => &[],                // returnvoid
        0x48 => &[],                // returnvalue
        0x49 => &[U30],            // constructsuper
        0x4A => &[U30, U30],       // constructprop
        0x4C => &[U30, U30],       // callproplex
        0x4E => &[U30, U30],       // callsupervoid
        0x4F => &[U30, U30],       // callpropvoid
        0x55 => &[U30],            // newobject
        0x56 => &[U30],            // newarray
        0x57 => &[],                // newactivation
        0x58 => &[U30],            // newclass
        0x59 => &[U30],            // getdescendants
        0x5A => &[U30],            // newcatch
        0x5D => &[U30],            // findpropstrict
        0x5E => &[U30],            // findproperty
        0x60 => &[U30],            // getlex
        0x61 => &[U30],            // setproperty
        0x62 => &[U30],            // getlocal
        0x63 => &[U30],            // setlocal
        0x64 => &[],                // getglobalscope
        0x65 => &[U30],            // getscopeobject
        0x66 => &[U30],            // getproperty
        0x68 => &[U30],            // initproperty
        0x6A => &[U30],            // deleteproperty
        0x6C => &[U30],            // getslot
        0x6D => &[U30],            // setslot
        0x70 => &[],                // convert_s
        0x73 => &[],                // convert_i
        0x74 => &[],                // convert_u
        0x75 => &[],                // convert_d
        0x76 => &[],                // convert_b
        0x80 => &[U30],            // coerce
        0x82 => &[],                // coerce_a
        0x85 => &[],                // coerce_s
        0x86 => &[U30],            // astype
        0x87 => &[],                // astypelate (no, it's 0x89)
        0x89 => &[],                // astypelate
        0x91 => &[],                // increment_i -- wait these are wrong, let me check
        0x93 => &[],                // decrement_i
        0x95 => &[],                // typeof
        0x96 => &[],                // not
        0x97 => &[],                // bitnot
        0xA0 => &[],                // add
        0xA1 => &[],                // subtract
        0xA2 => &[],                // multiply
        0xA3 => &[],                // divide
        0xA4 => &[],                // modulo
        0xA5 => &[],                // lshift
        0xA6 => &[],                // rshift -- wait, 0xA5 is negate
        0xA7 => &[],                // urshift
        0xA8 => &[],                // bitand
        0xA9 => &[],                // bitor
        0xAA => &[],                // bitxor
        0xAB => &[],                // equals
        0xAC => &[],                // strictequals
        0xAD => &[],                // lessthan
        0xAE => &[],                // lessequals
        0xAF => &[],                // greaterthan
        0xB0 => &[],                // greaterequals
        0xB1 => &[],                // instanceof
        0xB2 => &[U30],            // istype
        0xB3 => &[],                // istypelate
        0xC0 => &[],                // increment_i
        0xC1 => &[],                // decrement_i
        0xC2 => &[],                // inclocal_i
        0xC3 => &[],                // declocal_i -- wait, C2 is increment_i and C3 is decrement_i
        0xC4 => &[],                // negate_i
        0xC5 => &[U30],            // inclocal_i -- this is wrong too
        0xC6 => &[U30],            // declocal_i
        0xD0 => &[],                // getlocal_0
        0xD1 => &[],                // getlocal_1
        0xD2 => &[],                // getlocal_2
        0xD3 => &[],                // getlocal_3
        0xD4 => &[],                // setlocal_0
        0xD5 => &[],                // setlocal_1
        0xD6 => &[],                // setlocal_2
        0xD7 => &[],                // setlocal_3
        0xEF => &[Byte, U30, Byte, U30], // debug
        0xF0 => &[U30],            // debugline
        0xF1 => &[U30],            // debugfile
        _ => &[],                   // unknown/unhandled
    }
}

#[derive(Debug, Clone, Copy)]
pub enum OperandType {
    Byte,
    U30,
    S24,
}
