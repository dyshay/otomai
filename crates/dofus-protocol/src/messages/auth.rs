use crate::types::{GameServerInformations, Version};
use dofus_io::{
    boolean_byte_wrapper, BigEndianReader, BigEndianWriter, DofusDeserialize, DofusMessage,
    DofusSerialize,
};

// ── Server → Client ──────────────────────────────────────────────

/// ID: 4849 — First message sent to client on connect.
#[derive(Debug, Clone)]
pub struct ProtocolRequired {
    pub version: String,
}

impl DofusSerialize for ProtocolRequired {
    fn serialize(&self, writer: &mut BigEndianWriter) {
        writer.write_utf(&self.version);
    }
}

impl DofusDeserialize for ProtocolRequired {
    fn deserialize(reader: &mut BigEndianReader) -> anyhow::Result<Self> {
        Ok(Self {
            version: reader.read_utf()?,
        })
    }
}

impl DofusMessage for ProtocolRequired {
    const MESSAGE_ID: u16 = 4849;
}

/// ID: 1251 — Sent after ProtocolRequired. Contains RSA public key + salt.
#[derive(Debug, Clone)]
pub struct HelloConnectMessage {
    pub salt: String,
    pub key: Vec<u8>,
}

impl DofusSerialize for HelloConnectMessage {
    fn serialize(&self, writer: &mut BigEndianWriter) {
        writer.write_utf(&self.salt);
        writer.write_var_int(self.key.len() as i32);
        for &b in &self.key {
            writer.write_byte(b);
        }
    }
}

impl DofusDeserialize for HelloConnectMessage {
    fn deserialize(reader: &mut BigEndianReader) -> anyhow::Result<Self> {
        let salt = reader.read_utf()?;
        let count = reader.read_var_int()?;
        let mut key = Vec::with_capacity(count as usize);
        for _ in 0..count {
            key.push(reader.read_byte()?);
        }
        Ok(Self { salt, key })
    }
}

impl DofusMessage for HelloConnectMessage {
    const MESSAGE_ID: u16 = 1251;
}

// ── Client → Server ──────────────────────────────────────────────

/// ID: 8050 — Client identification (credentials RSA-encrypted).
#[derive(Debug, Clone)]
pub struct IdentificationMessage {
    pub autoconnect: bool,
    pub use_certificate: bool,
    pub use_login_token: bool,
    pub version: Version,
    pub lang: String,
    pub credentials: Vec<u8>,
    pub server_id: i16,
    pub session_optional_salt: i64,
    pub failed_attempts: Vec<i16>,
}

impl DofusSerialize for IdentificationMessage {
    fn serialize(&self, writer: &mut BigEndianWriter) {
        let mut box0 = 0u8;
        box0 = boolean_byte_wrapper::set_flag(box0, 0, self.autoconnect).unwrap();
        box0 = boolean_byte_wrapper::set_flag(box0, 1, self.use_certificate).unwrap();
        box0 = boolean_byte_wrapper::set_flag(box0, 2, self.use_login_token).unwrap();
        writer.write_byte(box0);
        self.version.serialize(writer);
        writer.write_utf(&self.lang);
        writer.write_var_int(self.credentials.len() as i32);
        for &b in &self.credentials {
            writer.write_byte(b);
        }
        writer.write_short(self.server_id);
        writer.write_var_long(self.session_optional_salt);
        writer.write_short(self.failed_attempts.len() as i16);
        for &v in &self.failed_attempts {
            writer.write_var_short(v);
        }
    }
}

impl DofusDeserialize for IdentificationMessage {
    fn deserialize(reader: &mut BigEndianReader) -> anyhow::Result<Self> {
        let box0 = reader.read_byte()?;
        let autoconnect = boolean_byte_wrapper::get_flag(box0, 0)?;
        let use_certificate = boolean_byte_wrapper::get_flag(box0, 1)?;
        let use_login_token = boolean_byte_wrapper::get_flag(box0, 2)?;
        let version = Version::deserialize(reader)?;
        let lang = reader.read_utf()?;
        let cred_count = reader.read_var_int()?;
        let mut credentials = Vec::with_capacity(cred_count as usize);
        for _ in 0..cred_count {
            credentials.push(reader.read_byte()?);
        }
        let server_id = reader.read_short()?;
        let session_optional_salt = reader.read_var_long()?;
        let fa_count = reader.read_short()?;
        let mut failed_attempts = Vec::with_capacity(fa_count as usize);
        for _ in 0..fa_count {
            failed_attempts.push(reader.read_var_short()?);
        }
        Ok(Self {
            autoconnect,
            use_certificate,
            use_login_token,
            version,
            lang,
            credentials,
            server_id,
            session_optional_salt,
            failed_attempts,
        })
    }
}

impl DofusMessage for IdentificationMessage {
    const MESSAGE_ID: u16 = 8050;
}

// ── Server → Client ──────────────────────────────────────────────

/// ID: 149 — Successful identification response.
#[derive(Debug, Clone)]
pub struct IdentificationSuccessMessage {
    pub has_rights: bool,
    pub has_console_right: bool,
    pub was_already_connected: bool,
    pub login: String,
    pub nickname: String,
    pub account_id: i32,
    pub community_id: u8,
    pub secret_question: String,
    pub account_creation: f64,
    pub subscription_elapsed_duration: f64,
    pub subscription_end_date: f64,
    pub havenbag_available_room: u8,
}

impl DofusSerialize for IdentificationSuccessMessage {
    fn serialize(&self, writer: &mut BigEndianWriter) {
        let mut box0 = 0u8;
        box0 = boolean_byte_wrapper::set_flag(box0, 0, self.has_rights).unwrap();
        box0 = boolean_byte_wrapper::set_flag(box0, 1, self.has_console_right).unwrap();
        box0 = boolean_byte_wrapper::set_flag(box0, 2, self.was_already_connected).unwrap();
        writer.write_byte(box0);
        writer.write_utf(&self.login);
        writer.write_utf(&self.nickname);
        writer.write_int(self.account_id);
        writer.write_byte(self.community_id);
        writer.write_utf(&self.secret_question);
        writer.write_double(self.account_creation);
        writer.write_double(self.subscription_elapsed_duration);
        writer.write_double(self.subscription_end_date);
        writer.write_byte(self.havenbag_available_room);
    }
}

impl DofusDeserialize for IdentificationSuccessMessage {
    fn deserialize(reader: &mut BigEndianReader) -> anyhow::Result<Self> {
        let box0 = reader.read_byte()?;
        Ok(Self {
            has_rights: boolean_byte_wrapper::get_flag(box0, 0)?,
            has_console_right: boolean_byte_wrapper::get_flag(box0, 1)?,
            was_already_connected: boolean_byte_wrapper::get_flag(box0, 2)?,
            login: reader.read_utf()?,
            nickname: reader.read_utf()?,
            account_id: reader.read_int()?,
            community_id: reader.read_byte()?,
            secret_question: reader.read_utf()?,
            account_creation: reader.read_double()?,
            subscription_elapsed_duration: reader.read_double()?,
            subscription_end_date: reader.read_double()?,
            havenbag_available_room: reader.read_byte()?,
        })
    }
}

impl DofusMessage for IdentificationSuccessMessage {
    const MESSAGE_ID: u16 = 149;
}

/// ID: 1345 — Failed identification.
#[derive(Debug, Clone)]
pub struct IdentificationFailedMessage {
    pub reason: u8,
}

impl DofusSerialize for IdentificationFailedMessage {
    fn serialize(&self, writer: &mut BigEndianWriter) {
        writer.write_byte(self.reason);
    }
}

impl DofusDeserialize for IdentificationFailedMessage {
    fn deserialize(reader: &mut BigEndianReader) -> anyhow::Result<Self> {
        Ok(Self {
            reason: reader.read_byte()?,
        })
    }
}

impl DofusMessage for IdentificationFailedMessage {
    const MESSAGE_ID: u16 = 1345;
}

/// ID: 7603 — Server list sent after successful auth.
#[derive(Debug, Clone)]
pub struct ServersListMessage {
    pub servers: Vec<GameServerInformations>,
    pub already_connected_to_server_id: i16,
    pub can_create_new_character: bool,
}

impl DofusSerialize for ServersListMessage {
    fn serialize(&self, writer: &mut BigEndianWriter) {
        writer.write_short(self.servers.len() as i16);
        for server in &self.servers {
            server.serialize(writer);
        }
        writer.write_var_short(self.already_connected_to_server_id);
        writer.write_boolean(self.can_create_new_character);
    }
}

impl DofusDeserialize for ServersListMessage {
    fn deserialize(reader: &mut BigEndianReader) -> anyhow::Result<Self> {
        let count = reader.read_short()?;
        let mut servers = Vec::with_capacity(count as usize);
        for _ in 0..count {
            servers.push(GameServerInformations::deserialize(reader)?);
        }
        Ok(Self {
            servers,
            already_connected_to_server_id: reader.read_var_short()?,
            can_create_new_character: reader.read_boolean()?,
        })
    }
}

impl DofusMessage for ServersListMessage {
    const MESSAGE_ID: u16 = 7603;
}

/// ID: 7721 — Client selects a server.
#[derive(Debug, Clone)]
pub struct ServerSelectionMessage {
    pub server_id: i16,
}

impl DofusSerialize for ServerSelectionMessage {
    fn serialize(&self, writer: &mut BigEndianWriter) {
        writer.write_var_short(self.server_id);
    }
}

impl DofusDeserialize for ServerSelectionMessage {
    fn deserialize(reader: &mut BigEndianReader) -> anyhow::Result<Self> {
        Ok(Self {
            server_id: reader.read_var_short()?,
        })
    }
}

impl DofusMessage for ServerSelectionMessage {
    const MESSAGE_ID: u16 = 7721;
}

/// ID: 9610 — Server redirect data (address, port, ticket).
#[derive(Debug, Clone)]
pub struct SelectedServerDataMessage {
    pub server_id: i16,
    pub address: String,
    pub ports: Vec<i16>,
    pub can_create_new_character: bool,
    pub ticket: Vec<u8>,
}

impl DofusSerialize for SelectedServerDataMessage {
    fn serialize(&self, writer: &mut BigEndianWriter) {
        writer.write_var_short(self.server_id);
        writer.write_utf(&self.address);
        writer.write_short(self.ports.len() as i16);
        for &port in &self.ports {
            writer.write_var_short(port);
        }
        writer.write_boolean(self.can_create_new_character);
        writer.write_var_int(self.ticket.len() as i32);
        for &b in &self.ticket {
            writer.write_byte(b);
        }
    }
}

impl DofusDeserialize for SelectedServerDataMessage {
    fn deserialize(reader: &mut BigEndianReader) -> anyhow::Result<Self> {
        let server_id = reader.read_var_short()?;
        let address = reader.read_utf()?;
        let port_count = reader.read_short()?;
        let mut ports = Vec::with_capacity(port_count as usize);
        for _ in 0..port_count {
            ports.push(reader.read_var_short()?);
        }
        let can_create_new_character = reader.read_boolean()?;
        let ticket_count = reader.read_var_int()?;
        let mut ticket = Vec::with_capacity(ticket_count as usize);
        for _ in 0..ticket_count {
            ticket.push(reader.read_byte()?);
        }
        Ok(Self {
            server_id,
            address,
            ports,
            can_create_new_character,
            ticket,
        })
    }
}

impl DofusMessage for SelectedServerDataMessage {
    const MESSAGE_ID: u16 = 9610;
}
