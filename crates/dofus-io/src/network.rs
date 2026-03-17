use crate::{BigEndianReader, BigEndianWriter};
use anyhow::{bail, Result};

const BIT_RIGHT_SHIFT_LEN_PACKET_ID: u16 = 2;
const BIT_MASK: u16 = 3;

#[derive(Debug, Clone)]
pub struct NetworkMessageHeader {
    pub message_id: u16,
    pub instance_id: u32,
    pub payload_size: u32,
}

pub fn read_header(reader: &mut BigEndianReader) -> Result<NetworkMessageHeader> {
    let header = reader.read_ushort()?;

    let message_id = header >> BIT_RIGHT_SHIFT_LEN_PACKET_ID;
    let type_length = (header & BIT_MASK) as usize;

    let instance_id = reader.read_uint()?;

    let payload_size = match type_length {
        0 => 0,
        1 | 2 => reader.read_uint_n(type_length)?,
        3 => {
            let high = (reader.read_byte()? as u32) << 16;
            let low = reader.read_ushort()? as u32;
            high | low
        }
        _ => bail!("Invalid type length: '{type_length}'"),
    };

    Ok(NetworkMessageHeader {
        message_id,
        instance_id,
        payload_size,
    })
}

pub fn write_header(
    writer: &mut BigEndianWriter,
    message_id: u16,
    instance_id: u32,
    payload_length: usize,
) {
    let type_len = compute_type_length(payload_length);

    writer.write_ushort((message_id << BIT_RIGHT_SHIFT_LEN_PACKET_ID) | type_len as u16);
    writer.write_uint(instance_id);

    match type_len {
        0 => {}
        1 => writer.write_byte(payload_length as u8),
        2 => writer.write_ushort(payload_length as u16),
        3 => {
            writer.write_byte((payload_length >> 16) as u8);
            writer.write_ushort(payload_length as u16);
        }
        _ => unreachable!(),
    }
}

pub fn compute_type_length(length: usize) -> usize {
    if length > 65535 {
        3
    } else if length > 255 {
        2
    } else if length > 0 {
        1
    } else {
        0
    }
}

/// Minimum header size: 2 (header short) + 4 (instance_id)
pub const MIN_HEADER_SIZE: usize = 6;
