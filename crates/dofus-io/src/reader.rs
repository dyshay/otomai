use anyhow::{bail, Result};
use byteorder::{BigEndian, ReadBytesExt};
use std::io::{Cursor, Read};

const INT_SIZE: u32 = 32;
const SHORT_SIZE: u32 = 16;
const SHORT_MAX_VALUE: i32 = 0x7fff;
const USHORT_MAX_VALUE: i32 = 0x10000;
const CHUNK_BIT_SIZE: u32 = 7;
const MASK_10000000: u8 = 128;
const MASK_01111111: u8 = 127;

pub struct BigEndianReader {
    cursor: Cursor<Vec<u8>>,
}

impl BigEndianReader {
    pub fn new(data: Vec<u8>) -> Self {
        Self {
            cursor: Cursor::new(data),
        }
    }

    pub fn read_var_int(&mut self) -> Result<i32> {
        let mut value: i32 = 0;
        let mut size: u32 = 0;

        while size < INT_SIZE {
            let byte = self.read_byte()?;
            let bit = (byte & MASK_10000000) == MASK_10000000;

            if size > 0 {
                value |= ((byte & MASK_01111111) as i32) << size;
            } else {
                value |= (byte & MASK_01111111) as i32;
            }

            size += CHUNK_BIT_SIZE;

            if !bit {
                return Ok(value);
            }
        }

        bail!("Overflow varint: too much data");
    }

    pub fn read_var_uint(&mut self) -> Result<u32> {
        Ok(self.read_var_int()? as u32)
    }

    pub fn read_var_short(&mut self) -> Result<i16> {
        let mut value: i32 = 0;
        let mut offset: u32 = 0;

        while offset < SHORT_SIZE {
            let byte = self.read_byte()?;
            let bit = (byte & MASK_10000000) == MASK_10000000;

            if offset > 0 {
                value |= ((byte & MASK_01111111) as i32) << offset;
            } else {
                value |= (byte & MASK_01111111) as i32;
            }

            offset += CHUNK_BIT_SIZE;

            if !bit {
                if value > SHORT_MAX_VALUE {
                    value -= USHORT_MAX_VALUE;
                }
                return Ok(value as i16);
            }
        }

        bail!("Overflow var short: too much data");
    }

    pub fn read_var_ushort(&mut self) -> Result<u16> {
        Ok(self.read_var_short()? as u16)
    }

    pub fn read_var_long(&mut self) -> Result<i64> {
        let mut low: u64 = 0;
        let mut high: u64;
        let mut size: u32 = 0;
        let mut last_byte: u8;

        while size < 28 {
            last_byte = self.read_byte()?;

            if (last_byte & MASK_10000000) == MASK_10000000 {
                low |= ((last_byte & MASK_01111111) as u64) << size;
                size += 7;
            } else {
                low |= (last_byte as u64) << size;
                return Ok(low as i64);
            }
        }

        last_byte = self.read_byte()?;

        if (last_byte & MASK_10000000) == MASK_10000000 {
            low |= ((last_byte & MASK_01111111) as u64) << size;
            high = ((last_byte & MASK_01111111) as u64) >> 4;

            size = 3;

            while size < 32 {
                last_byte = self.read_byte()?;

                if (last_byte & MASK_10000000) == MASK_10000000 {
                    high |= ((last_byte & MASK_01111111) as u64) << size;
                } else {
                    break;
                }

                size += 7;
            }

            high |= (last_byte as u64) << size;

            return Ok(((low & 0xffffffff) | (high << 32)) as i64);
        }

        low |= (last_byte as u64) << size;
        high = (last_byte as u64) >> 4;

        Ok(((low & 0xffffffff) | (high << 32)) as i64)
    }

    pub fn read_var_ulong(&mut self) -> Result<u64> {
        Ok(self.read_var_long()? as u64)
    }

    pub fn read_byte(&mut self) -> Result<u8> {
        Ok(self.cursor.read_u8()?)
    }

    pub fn read_signed_byte(&mut self) -> Result<i8> {
        Ok(self.cursor.read_i8()?)
    }

    pub fn read_boolean(&mut self) -> Result<bool> {
        Ok(self.read_byte()? != 0)
    }

    pub fn read_short(&mut self) -> Result<i16> {
        Ok(self.cursor.read_i16::<BigEndian>()?)
    }

    pub fn read_ushort(&mut self) -> Result<u16> {
        Ok(self.cursor.read_u16::<BigEndian>()?)
    }

    pub fn read_int(&mut self) -> Result<i32> {
        Ok(self.cursor.read_i32::<BigEndian>()?)
    }

    pub fn read_uint(&mut self) -> Result<u32> {
        Ok(self.cursor.read_u32::<BigEndian>()?)
    }

    pub fn read_uint_n(&mut self, byte_length: usize) -> Result<u32> {
        let mut value: u32 = 0;
        for i in 0..byte_length {
            value |= (self.read_byte()? as u32) << (8 * (byte_length - 1 - i));
        }
        Ok(value)
    }

    pub fn read_long(&mut self) -> Result<i64> {
        Ok(self.cursor.read_i64::<BigEndian>()?)
    }

    pub fn read_ulong(&mut self) -> Result<u64> {
        Ok(self.cursor.read_u64::<BigEndian>()?)
    }

    pub fn read_float(&mut self) -> Result<f32> {
        Ok(self.cursor.read_f32::<BigEndian>()?)
    }

    pub fn read_double(&mut self) -> Result<f64> {
        Ok(self.cursor.read_f64::<BigEndian>()?)
    }

    pub fn read_utf(&mut self) -> Result<String> {
        let length = self.read_ushort()? as usize;
        self.read_utf_bytes(length)
    }

    pub fn read_utf_bytes(&mut self, size: usize) -> Result<String> {
        let mut buf = vec![0u8; size];
        Read::read_exact(&mut self.cursor, &mut buf)?;
        Ok(String::from_utf8(buf)?)
    }

    pub fn read_bytes(&mut self, size: usize) -> Result<Vec<u8>> {
        let mut buf = vec![0u8; size];
        Read::read_exact(&mut self.cursor, &mut buf)?;
        Ok(buf)
    }

    pub fn position(&self) -> u64 {
        self.cursor.position()
    }

    pub fn set_position(&mut self, pos: u64) {
        self.cursor.set_position(pos);
    }

    pub fn bytes_available(&self) -> usize {
        let pos = self.cursor.position() as usize;
        let len = self.cursor.get_ref().len();
        len.saturating_sub(pos)
    }
}
