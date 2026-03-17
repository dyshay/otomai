//! Low-level reader for ABC binary format (little-endian, LEB128 variable ints).

use anyhow::{bail, Result};

pub struct AbcReader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> AbcReader<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    pub fn position(&self) -> usize {
        self.pos
    }

    pub fn remaining(&self) -> usize {
        self.data.len().saturating_sub(self.pos)
    }

    pub fn read_u8(&mut self) -> Result<u8> {
        if self.pos >= self.data.len() {
            bail!("EOF reading u8 at position {}", self.pos);
        }
        let v = self.data[self.pos];
        self.pos += 1;
        Ok(v)
    }

    pub fn read_u16(&mut self) -> Result<u16> {
        if self.pos + 2 > self.data.len() {
            bail!("EOF reading u16 at position {}", self.pos);
        }
        let v = u16::from_le_bytes([self.data[self.pos], self.data[self.pos + 1]]);
        self.pos += 2;
        Ok(v)
    }

    pub fn read_i24(&mut self) -> Result<i32> {
        if self.pos + 3 > self.data.len() {
            bail!("EOF reading i24 at position {}", self.pos);
        }
        let v = self.data[self.pos] as i32
            | (self.data[self.pos + 1] as i32) << 8
            | (self.data[self.pos + 2] as i32) << 16;
        self.pos += 3;
        // Sign extend from 24 bits
        Ok(if v & 0x800000 != 0 {
            v | !0xffffff
        } else {
            v
        })
    }

    pub fn read_d64(&mut self) -> Result<f64> {
        if self.pos + 8 > self.data.len() {
            bail!("EOF reading d64 at position {}", self.pos);
        }
        let bytes: [u8; 8] = self.data[self.pos..self.pos + 8].try_into().unwrap();
        self.pos += 8;
        Ok(f64::from_le_bytes(bytes))
    }

    /// Read an unsigned 30-bit variable-length integer (ABC u30/u32 encoding).
    /// Uses LEB128 encoding: 7 bits per byte, MSB = continuation.
    pub fn read_u30(&mut self) -> Result<u32> {
        let mut result: u32 = 0;
        let mut shift: u32 = 0;

        for _ in 0..5 {
            let byte = self.read_u8()?;
            result |= ((byte & 0x7f) as u32) << shift;

            if byte & 0x80 == 0 {
                return Ok(result);
            }

            shift += 7;
        }

        bail!("u30 overflow at position {}", self.pos);
    }

    /// Read a signed 32-bit variable-length integer (ABC s32 encoding).
    pub fn read_s32(&mut self) -> Result<i32> {
        Ok(self.read_u30()? as i32)
    }

    pub fn read_bytes(&mut self, len: usize) -> Result<&'a [u8]> {
        if self.pos + len > self.data.len() {
            bail!(
                "EOF reading {} bytes at position {}",
                len,
                self.pos
            );
        }
        let slice = &self.data[self.pos..self.pos + len];
        self.pos += len;
        Ok(slice)
    }

    pub fn skip(&mut self, n: usize) -> Result<()> {
        if self.pos + n > self.data.len() {
            bail!("EOF skipping {} bytes at position {}", n, self.pos);
        }
        self.pos += n;
        Ok(())
    }
}
