mod reader;
mod writer;
pub mod boolean_byte_wrapper;
pub mod network;

pub use reader::BigEndianReader;
pub use writer::BigEndianWriter;

/// Trait for types that can be serialized to the Dofus binary protocol.
pub trait DofusSerialize {
    fn serialize(&self, writer: &mut BigEndianWriter);
}

/// Trait for types that can be deserialized from the Dofus binary protocol.
pub trait DofusDeserialize: Sized {
    fn deserialize(reader: &mut BigEndianReader) -> anyhow::Result<Self>;
}

/// Trait for protocol messages (have a message ID).
pub trait DofusMessage: DofusSerialize + DofusDeserialize {
    const MESSAGE_ID: u16;
}

/// Trait for protocol types (have a type ID, used as sub-structures).
pub trait DofusType: DofusSerialize + DofusDeserialize {
    const TYPE_ID: u16;
}

#[cfg(test)]
mod tests {
    use super::*;

    // ─── VarInt ──────────────────────────────────────────────────

    #[test]
    fn test_var_int_roundtrip() {
        let values = [0, 1, 127, 128, 255, 256, 16383, 16384, 0x7fffffff, -1, -128, -32768, i32::MIN, i32::MAX];
        for &v in &values {
            let mut writer = BigEndianWriter::new();
            writer.write_var_int(v);
            let mut reader = BigEndianReader::new(writer.into_data());
            assert_eq!(reader.read_var_int().unwrap(), v, "var_int roundtrip for {v}");
        }
    }

    #[test]
    fn test_var_int_single_byte() {
        // Values 0-127 should encode as a single byte (no continuation bit)
        let mut writer = BigEndianWriter::new();
        writer.write_var_int(0);
        assert_eq!(writer.data(), &[0]);

        let mut writer = BigEndianWriter::new();
        writer.write_var_int(127);
        assert_eq!(writer.data(), &[127]);
    }

    #[test]
    fn test_var_int_two_bytes() {
        // 128 should need 2 bytes: 0x80|0x00, 0x01
        let mut writer = BigEndianWriter::new();
        writer.write_var_int(128);
        assert_eq!(writer.len(), 2);
        assert_eq!(writer.data()[0] & 0x80, 0x80, "first byte has continuation bit");
    }

    #[test]
    fn test_var_uint_roundtrip() {
        let values: &[u32] = &[0, 1, 127, 128, 255, 0xffff, 0xffffff, u32::MAX];
        for &v in values {
            let mut writer = BigEndianWriter::new();
            writer.write_var_uint(v);
            let mut reader = BigEndianReader::new(writer.into_data());
            assert_eq!(reader.read_var_uint().unwrap(), v, "var_uint roundtrip for {v}");
        }
    }

    // ─── VarShort ────────────────────────────────────────────────

    #[test]
    fn test_var_short_roundtrip() {
        let values: &[i16] = &[0, 1, 127, 128, 255, 256, 0x7fff, -1, -128, -32768, i16::MIN, i16::MAX];
        for &v in values {
            let mut writer = BigEndianWriter::new();
            writer.write_var_short(v);
            let mut reader = BigEndianReader::new(writer.into_data());
            assert_eq!(reader.read_var_short().unwrap(), v, "var_short roundtrip for {v}");
        }
    }

    #[test]
    fn test_var_ushort_roundtrip() {
        let values: &[u16] = &[0, 1, 127, 128, 255, 256, 0x7fff, 0xffff];
        for &v in values {
            let mut writer = BigEndianWriter::new();
            writer.write_var_ushort(v);
            let mut reader = BigEndianReader::new(writer.into_data());
            assert_eq!(reader.read_var_ushort().unwrap(), v, "var_ushort roundtrip for {v}");
        }
    }

    // ─── VarLong ─────────────────────────────────────────────────

    #[test]
    fn test_var_long_roundtrip() {
        let values: &[i64] = &[
            0, 1, 127, 128, 0x7fffffff,
            0x100000000, 0x7fffffffffffffff,
            -1, -128, -0x100000000,
        ];
        for &v in values {
            let mut writer = BigEndianWriter::new();
            writer.write_var_long(v);
            let mut reader = BigEndianReader::new(writer.into_data());
            assert_eq!(reader.read_var_long().unwrap(), v, "var_long roundtrip for {v}");
        }
    }

    #[test]
    fn test_var_long_small_values_use_var_int() {
        // Values that fit in 32 bits should use the same encoding as var_int
        let mut w1 = BigEndianWriter::new();
        w1.write_var_long(42);
        let mut w2 = BigEndianWriter::new();
        w2.write_var_int(42);
        assert_eq!(w1.data(), w2.data(), "small var_long should match var_int encoding");
    }

    #[test]
    fn test_var_ulong_roundtrip() {
        let values: &[u64] = &[0, 1, 127, 128, 0xffffffff, 0x100000000, u64::MAX >> 1];
        for &v in values {
            let mut writer = BigEndianWriter::new();
            writer.write_var_ulong(v);
            let mut reader = BigEndianReader::new(writer.into_data());
            assert_eq!(reader.read_var_ulong().unwrap(), v, "var_ulong roundtrip for {v}");
        }
    }

    // ─── Primitives ──────────────────────────────────────────────

    #[test]
    fn test_byte_roundtrip() {
        for v in [0u8, 1, 127, 128, 255] {
            let mut writer = BigEndianWriter::new();
            writer.write_byte(v);
            let mut reader = BigEndianReader::new(writer.into_data());
            assert_eq!(reader.read_byte().unwrap(), v);
        }
    }

    #[test]
    fn test_signed_byte_roundtrip() {
        for v in [0i8, 1, -1, 127, -128] {
            let mut writer = BigEndianWriter::new();
            writer.write_signed_byte(v);
            let mut reader = BigEndianReader::new(writer.into_data());
            assert_eq!(reader.read_signed_byte().unwrap(), v);
        }
    }

    #[test]
    fn test_boolean_roundtrip() {
        let mut writer = BigEndianWriter::new();
        writer.write_boolean(true);
        writer.write_boolean(false);
        let mut reader = BigEndianReader::new(writer.into_data());
        assert_eq!(reader.read_boolean().unwrap(), true);
        assert_eq!(reader.read_boolean().unwrap(), false);
    }

    #[test]
    fn test_boolean_nonzero_is_true() {
        // AS3 readBoolean returns true for any non-zero byte
        let mut reader = BigEndianReader::new(vec![2]);
        assert_eq!(reader.read_boolean().unwrap(), true);
        let mut reader = BigEndianReader::new(vec![255]);
        assert_eq!(reader.read_boolean().unwrap(), true);
    }

    #[test]
    fn test_short_roundtrip() {
        for v in [0i16, 1, -1, 32767, -32768] {
            let mut writer = BigEndianWriter::new();
            writer.write_short(v);
            let mut reader = BigEndianReader::new(writer.into_data());
            assert_eq!(reader.read_short().unwrap(), v);
        }
    }

    #[test]
    fn test_ushort_roundtrip() {
        for v in [0u16, 1, 255, 256, 65535] {
            let mut writer = BigEndianWriter::new();
            writer.write_ushort(v);
            let mut reader = BigEndianReader::new(writer.into_data());
            assert_eq!(reader.read_ushort().unwrap(), v);
        }
    }

    #[test]
    fn test_int_roundtrip() {
        for v in [0i32, 1, -1, i32::MAX, i32::MIN] {
            let mut writer = BigEndianWriter::new();
            writer.write_int(v);
            let mut reader = BigEndianReader::new(writer.into_data());
            assert_eq!(reader.read_int().unwrap(), v);
        }
    }

    #[test]
    fn test_uint_roundtrip() {
        for v in [0u32, 1, u32::MAX] {
            let mut writer = BigEndianWriter::new();
            writer.write_uint(v);
            let mut reader = BigEndianReader::new(writer.into_data());
            assert_eq!(reader.read_uint().unwrap(), v);
        }
    }

    #[test]
    fn test_long_roundtrip() {
        for v in [0i64, 1, -1, i64::MAX, i64::MIN] {
            let mut writer = BigEndianWriter::new();
            writer.write_long(v);
            let mut reader = BigEndianReader::new(writer.into_data());
            assert_eq!(reader.read_long().unwrap(), v);
        }
    }

    #[test]
    fn test_double_roundtrip() {
        for v in [0.0f64, 1.0, -1.0, 3.14159265358979, f64::MAX, f64::MIN] {
            let mut writer = BigEndianWriter::new();
            writer.write_double(v);
            let mut reader = BigEndianReader::new(writer.into_data());
            assert_eq!(reader.read_double().unwrap(), v);
        }
    }

    #[test]
    fn test_float_roundtrip() {
        for v in [0.0f32, 1.0, -1.0, 2.5] {
            let mut writer = BigEndianWriter::new();
            writer.write_float(v);
            let mut reader = BigEndianReader::new(writer.into_data());
            assert_eq!(reader.read_float().unwrap(), v);
        }
    }

    // ─── UTF ─────────────────────────────────────────────────────

    #[test]
    fn test_utf_roundtrip() {
        let mut writer = BigEndianWriter::new();
        writer.write_utf("Hello, Dofus!");
        let mut reader = BigEndianReader::new(writer.into_data());
        assert_eq!(reader.read_utf().unwrap(), "Hello, Dofus!");
    }

    #[test]
    fn test_utf_empty_string() {
        let mut writer = BigEndianWriter::new();
        writer.write_utf("");
        assert_eq!(writer.data(), &[0, 0]); // 2-byte length prefix = 0
        let mut reader = BigEndianReader::new(writer.into_data());
        assert_eq!(reader.read_utf().unwrap(), "");
    }

    #[test]
    fn test_utf_unicode() {
        let mut writer = BigEndianWriter::new();
        writer.write_utf("Crâ ébène");
        let mut reader = BigEndianReader::new(writer.into_data());
        assert_eq!(reader.read_utf().unwrap(), "Crâ ébène");
    }

    #[test]
    fn test_utf_encoding_format() {
        // writeUTF writes: u16 length (big endian) + UTF-8 bytes
        let mut writer = BigEndianWriter::new();
        writer.write_utf("AB");
        assert_eq!(writer.data(), &[0, 2, b'A', b'B']);
    }

    // ─── Bytes ───────────────────────────────────────────────────

    #[test]
    fn test_bytes_roundtrip() {
        let data = vec![1, 2, 3, 4, 5];
        let mut writer = BigEndianWriter::new();
        writer.write_bytes(&data);
        let mut reader = BigEndianReader::new(writer.into_data());
        assert_eq!(reader.read_bytes(5).unwrap(), data);
    }

    #[test]
    fn test_bytes_available() {
        let reader = BigEndianReader::new(vec![1, 2, 3, 4, 5]);
        assert_eq!(reader.bytes_available(), 5);
    }

    // ─── BooleanByteWrapper ──────────────────────────────────────

    #[test]
    fn test_boolean_byte_wrapper() {
        let mut flag = 0u8;
        flag = boolean_byte_wrapper::set_flag(flag, 0, true).unwrap();
        flag = boolean_byte_wrapper::set_flag(flag, 2, true).unwrap();
        assert!(boolean_byte_wrapper::get_flag(flag, 0).unwrap());
        assert!(!boolean_byte_wrapper::get_flag(flag, 1).unwrap());
        assert!(boolean_byte_wrapper::get_flag(flag, 2).unwrap());
    }

    #[test]
    fn test_boolean_byte_wrapper_all_bits() {
        let mut flag = 0u8;
        for i in 0..8 {
            flag = boolean_byte_wrapper::set_flag(flag, i, true).unwrap();
        }
        assert_eq!(flag, 0xFF);
        for i in 0..8 {
            assert!(boolean_byte_wrapper::get_flag(flag, i).unwrap());
        }
    }

    #[test]
    fn test_boolean_byte_wrapper_unset() {
        let mut flag = 0xFF;
        flag = boolean_byte_wrapper::set_flag(flag, 3, false).unwrap();
        assert!(!boolean_byte_wrapper::get_flag(flag, 3).unwrap());
        assert!(boolean_byte_wrapper::get_flag(flag, 2).unwrap());
        assert!(boolean_byte_wrapper::get_flag(flag, 4).unwrap());
    }

    #[test]
    fn test_boolean_byte_wrapper_overflow() {
        assert!(boolean_byte_wrapper::set_flag(0, 8, true).is_err());
        assert!(boolean_byte_wrapper::get_flag(0, 8).is_err());
    }

    // ─── Network header ──────────────────────────────────────────

    #[test]
    fn test_network_header_roundtrip() {
        let mut writer = BigEndianWriter::new();
        network::write_header(&mut writer, 4849, 1, 13);
        let mut reader = BigEndianReader::new(writer.into_data());
        let header = network::read_header(&mut reader).unwrap();
        assert_eq!(header.message_id, 4849);
        assert_eq!(header.instance_id, 1);
        assert_eq!(header.payload_size, 13);
    }

    #[test]
    fn test_network_header_zero_payload() {
        let mut writer = BigEndianWriter::new();
        network::write_header(&mut writer, 1, 0, 0);
        let mut reader = BigEndianReader::new(writer.into_data());
        let header = network::read_header(&mut reader).unwrap();
        assert_eq!(header.message_id, 1);
        assert_eq!(header.payload_size, 0);
    }

    #[test]
    fn test_network_header_large_payload() {
        // 3-byte length field for payloads > 65535
        let mut writer = BigEndianWriter::new();
        network::write_header(&mut writer, 100, 1, 100000);
        let mut reader = BigEndianReader::new(writer.into_data());
        let header = network::read_header(&mut reader).unwrap();
        assert_eq!(header.message_id, 100);
        assert_eq!(header.payload_size, 100000);
    }

    #[test]
    fn test_network_header_max_message_id() {
        // Message ID uses 14 bits (max 16383)
        let mut writer = BigEndianWriter::new();
        network::write_header(&mut writer, 16383, 1, 5);
        let mut reader = BigEndianReader::new(writer.into_data());
        let header = network::read_header(&mut reader).unwrap();
        assert_eq!(header.message_id, 16383);
    }

    // ─── Combined / integration ──────────────────────────────────

    #[test]
    fn test_multiple_fields_sequential() {
        // Simulate a typical message: bool flags + string + varint + vector
        let mut writer = BigEndianWriter::new();
        // BooleanByteWrapper flags
        let mut flag = 0u8;
        flag = boolean_byte_wrapper::set_flag(flag, 0, true).unwrap();
        flag = boolean_byte_wrapper::set_flag(flag, 1, false).unwrap();
        flag = boolean_byte_wrapper::set_flag(flag, 2, true).unwrap();
        writer.write_byte(flag);
        // String field
        writer.write_utf("TestUser");
        // VarInt field
        writer.write_var_int(42);
        // Vector<u8> with short length
        let items: Vec<u8> = vec![10, 20, 30];
        writer.write_short(items.len() as i16);
        for &item in &items {
            writer.write_byte(item);
        }

        let mut reader = BigEndianReader::new(writer.into_data());
        let read_flag = reader.read_byte().unwrap();
        assert!(boolean_byte_wrapper::get_flag(read_flag, 0).unwrap());
        assert!(!boolean_byte_wrapper::get_flag(read_flag, 1).unwrap());
        assert!(boolean_byte_wrapper::get_flag(read_flag, 2).unwrap());
        assert_eq!(reader.read_utf().unwrap(), "TestUser");
        assert_eq!(reader.read_var_int().unwrap(), 42);
        let count = reader.read_short().unwrap() as usize;
        assert_eq!(count, 3);
        let mut read_items = Vec::new();
        for _ in 0..count {
            read_items.push(reader.read_byte().unwrap());
        }
        assert_eq!(read_items, vec![10, 20, 30]);
        assert_eq!(reader.bytes_available(), 0);
    }

    #[test]
    fn test_big_endian_byte_order() {
        // Verify big-endian encoding: 0x1234 should be [0x12, 0x34]
        let mut writer = BigEndianWriter::new();
        writer.write_short(0x1234);
        assert_eq!(writer.data(), &[0x12, 0x34]);

        let mut writer = BigEndianWriter::new();
        writer.write_int(0x12345678);
        assert_eq!(writer.data(), &[0x12, 0x34, 0x56, 0x78]);
    }
}
