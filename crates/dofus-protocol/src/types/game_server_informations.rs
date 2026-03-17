use dofus_io::{
    boolean_byte_wrapper, BigEndianReader, BigEndianWriter, DofusDeserialize, DofusSerialize,
    DofusType,
};

/// Protocol type ID: 1852
#[derive(Debug, Clone, Default)]
pub struct GameServerInformations {
    pub is_mono_account: bool,
    pub is_selectable: bool,
    pub id: i16,
    pub server_type: u8,
    pub status: u8,
    pub completion: u8,
    pub characters_count: u8,
    pub characters_slots: u8,
    pub date: f64,
}

impl DofusSerialize for GameServerInformations {
    fn serialize(&self, writer: &mut BigEndianWriter) {
        let mut box0 = 0u8;
        box0 = boolean_byte_wrapper::set_flag(box0, 0, self.is_mono_account).unwrap();
        box0 = boolean_byte_wrapper::set_flag(box0, 1, self.is_selectable).unwrap();
        writer.write_byte(box0);
        writer.write_var_short(self.id);
        writer.write_byte(self.server_type);
        writer.write_byte(self.status);
        writer.write_byte(self.completion);
        writer.write_byte(self.characters_count);
        writer.write_byte(self.characters_slots);
        writer.write_double(self.date);
    }
}

impl DofusDeserialize for GameServerInformations {
    fn deserialize(reader: &mut BigEndianReader) -> anyhow::Result<Self> {
        let box0 = reader.read_byte()?;
        Ok(Self {
            is_mono_account: boolean_byte_wrapper::get_flag(box0, 0)?,
            is_selectable: boolean_byte_wrapper::get_flag(box0, 1)?,
            id: reader.read_var_short()?,
            server_type: reader.read_byte()?,
            status: reader.read_byte()?,
            completion: reader.read_byte()?,
            characters_count: reader.read_byte()?,
            characters_slots: reader.read_byte()?,
            date: reader.read_double()?,
        })
    }
}

impl DofusType for GameServerInformations {
    const TYPE_ID: u16 = 1852;
}
