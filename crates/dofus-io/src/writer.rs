use byteorder::{BigEndian, WriteBytesExt};

const CHUNK_BIT_SIZE: u32 = 7;
const MASK_10000000: u8 = 0x80;
const MASK_01111111: u8 = 0x7f;

pub struct BigEndianWriter {
    buffer: Vec<u8>,
}

impl BigEndianWriter {
    pub fn new() -> Self {
        Self { buffer: Vec::new() }
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            buffer: Vec::with_capacity(capacity),
        }
    }

    pub fn write_var_int(&mut self, data: i32) {
        // Use unsigned representation for encoding
        let mut value = data as u32;

        loop {
            let mut byte = (value & MASK_01111111 as u32) as u8;
            value >>= CHUNK_BIT_SIZE;

            if value != 0 {
                byte |= MASK_10000000;
            }

            self.write_byte(byte);

            if value == 0 {
                break;
            }
        }
    }

    pub fn write_var_uint(&mut self, data: u32) {
        self.write_var_int(data as i32);
    }

    pub fn write_var_short(&mut self, data: i16) {
        // Use unsigned representation for encoding
        let mut value = data as u16;

        loop {
            let mut byte = (value & MASK_01111111 as u16) as u8;
            value >>= CHUNK_BIT_SIZE;

            if value != 0 {
                byte |= MASK_10000000;
            }

            self.write_byte(byte);

            if value == 0 {
                break;
            }
        }
    }

    pub fn write_var_ushort(&mut self, data: u16) {
        self.write_var_short(data as i16);
    }

    pub fn write_var_long(&mut self, data: i64) {
        let value = data as u64;
        let mut low = value & 0xffffffff;
        let mut high = value >> 32;

        if high == 0 {
            self.write_var_int(data as i32);
            return;
        }

        for _ in 0..4 {
            self.write_byte(((low & MASK_01111111 as u64) | MASK_10000000 as u64) as u8);
            low >>= 7;
        }

        if (high & 0xfffffff8) == 0 {
            self.write_byte(((high << 4) | low) as u8);
        } else {
            self.write_byte(
                ((((high << 4) | low) & MASK_01111111 as u64) | MASK_10000000 as u64) as u8,
            );
            high >>= 3;

            while high >= 0x80 {
                self.write_byte(
                    ((high & MASK_01111111 as u64) | MASK_10000000 as u64) as u8,
                );
                high >>= 7;
            }

            self.write_byte(high as u8);
        }
    }

    pub fn write_var_ulong(&mut self, data: u64) {
        self.write_var_long(data as i64);
    }

    pub fn write_byte(&mut self, data: u8) {
        self.buffer.push(data);
    }

    pub fn write_signed_byte(&mut self, data: i8) {
        self.buffer.push(data as u8);
    }

    pub fn write_boolean(&mut self, data: bool) {
        self.write_byte(if data { 1 } else { 0 });
    }

    pub fn write_short(&mut self, data: i16) {
        self.buffer.write_i16::<BigEndian>(data).unwrap();
    }

    pub fn write_ushort(&mut self, data: u16) {
        self.buffer.write_u16::<BigEndian>(data).unwrap();
    }

    pub fn write_int(&mut self, data: i32) {
        self.buffer.write_i32::<BigEndian>(data).unwrap();
    }

    pub fn write_uint(&mut self, data: u32) {
        self.buffer.write_u32::<BigEndian>(data).unwrap();
    }

    pub fn write_long(&mut self, data: i64) {
        self.buffer.write_i64::<BigEndian>(data).unwrap();
    }

    pub fn write_ulong(&mut self, data: u64) {
        self.buffer.write_u64::<BigEndian>(data).unwrap();
    }

    pub fn write_float(&mut self, data: f32) {
        self.buffer.write_f32::<BigEndian>(data).unwrap();
    }

    pub fn write_double(&mut self, data: f64) {
        self.buffer.write_f64::<BigEndian>(data).unwrap();
    }

    pub fn write_utf(&mut self, data: &str) {
        let bytes = data.as_bytes();
        self.write_ushort(bytes.len() as u16);
        self.buffer.extend_from_slice(bytes);
    }

    pub fn write_utf_bytes(&mut self, data: &str) {
        self.buffer.extend_from_slice(data.as_bytes());
    }

    pub fn write_bytes(&mut self, data: &[u8]) {
        self.buffer.extend_from_slice(data);
    }

    pub fn data(&self) -> &[u8] {
        &self.buffer
    }

    pub fn into_data(self) -> Vec<u8> {
        self.buffer
    }

    pub fn len(&self) -> usize {
        self.buffer.len()
    }

    pub fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }

    pub fn clear(&mut self) {
        self.buffer.clear();
    }
}

impl Default for BigEndianWriter {
    fn default() -> Self {
        Self::new()
    }
}
