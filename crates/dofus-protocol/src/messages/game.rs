use dofus_io::{BigEndianReader, BigEndianWriter, DofusDeserialize, DofusMessage, DofusSerialize};

/// ID: 1864 — Client sends ticket to world server.
#[derive(Debug, Clone)]
pub struct AuthenticationTicketMessage {
    pub lang: String,
    pub ticket: String,
}

impl DofusSerialize for AuthenticationTicketMessage {
    fn serialize(&self, writer: &mut BigEndianWriter) {
        writer.write_utf(&self.lang);
        writer.write_utf(&self.ticket);
    }
}

impl DofusDeserialize for AuthenticationTicketMessage {
    fn deserialize(reader: &mut BigEndianReader) -> anyhow::Result<Self> {
        Ok(Self {
            lang: reader.read_utf()?,
            ticket: reader.read_utf()?,
        })
    }
}

impl DofusMessage for AuthenticationTicketMessage {
    const MESSAGE_ID: u16 = 1864;
}

/// ID: 9070 — Ping message.
#[derive(Debug, Clone)]
pub struct BasicPingMessage {
    pub quiet: bool,
}

impl DofusSerialize for BasicPingMessage {
    fn serialize(&self, writer: &mut BigEndianWriter) {
        writer.write_boolean(self.quiet);
    }
}

impl DofusDeserialize for BasicPingMessage {
    fn deserialize(reader: &mut BigEndianReader) -> anyhow::Result<Self> {
        Ok(Self {
            quiet: reader.read_boolean()?,
        })
    }
}

impl DofusMessage for BasicPingMessage {
    const MESSAGE_ID: u16 = 9070;
}

/// ID: 6816 — Pong response.
#[derive(Debug, Clone)]
pub struct BasicPongMessage {
    pub quiet: bool,
}

impl DofusSerialize for BasicPongMessage {
    fn serialize(&self, writer: &mut BigEndianWriter) {
        writer.write_boolean(self.quiet);
    }
}

impl DofusDeserialize for BasicPongMessage {
    fn deserialize(reader: &mut BigEndianReader) -> anyhow::Result<Self> {
        Ok(Self {
            quiet: reader.read_boolean()?,
        })
    }
}

impl DofusMessage for BasicPongMessage {
    const MESSAGE_ID: u16 = 6816;
}
