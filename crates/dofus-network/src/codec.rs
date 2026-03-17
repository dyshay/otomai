use bytes::BytesMut;
use dofus_io::network::{self, MIN_HEADER_SIZE};
use dofus_io::{BigEndianReader, BigEndianWriter, DofusMessage};
use tokio_util::codec::{Decoder, Encoder};

/// Raw frame: (message_id, instance_id, payload).
#[derive(Debug, Clone)]
pub struct RawMessage {
    pub message_id: u16,
    pub instance_id: u32,
    pub payload: Vec<u8>,
}

/// Codec for Dofus protocol framing.
pub struct DofusCodec;

impl DofusCodec {
    pub fn new() -> Self {
        Self
    }
}

impl Default for DofusCodec {
    fn default() -> Self {
        Self::new()
    }
}

impl Decoder for DofusCodec {
    type Item = RawMessage;
    type Error = anyhow::Error;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        if src.len() < MIN_HEADER_SIZE {
            return Ok(None);
        }

        // Peek at header without consuming
        let mut peek = BigEndianReader::new(src.to_vec());
        let header = network::read_header(&mut peek)?;
        let header_size = peek.position() as usize;
        let total_size = header_size + header.payload_size as usize;

        if src.len() < total_size {
            src.reserve(total_size - src.len());
            return Ok(None);
        }

        // Now consume
        let frame_bytes = src.split_to(total_size);
        let mut reader = BigEndianReader::new(frame_bytes.to_vec());
        let header = network::read_header(&mut reader)?;

        let payload = if header.payload_size > 0 {
            reader.read_bytes(header.payload_size as usize)?
        } else {
            Vec::new()
        };

        Ok(Some(RawMessage {
            message_id: header.message_id,
            instance_id: header.instance_id,
            payload,
        }))
    }
}

impl Encoder<RawMessage> for DofusCodec {
    type Error = anyhow::Error;

    fn encode(&mut self, item: RawMessage, dst: &mut BytesMut) -> Result<(), Self::Error> {
        let mut writer = BigEndianWriter::new();
        network::write_header(&mut writer, item.message_id, item.instance_id, item.payload.len());
        writer.write_bytes(&item.payload);
        dst.extend_from_slice(writer.data());
        Ok(())
    }
}

/// Helper: serialize a typed message into a RawMessage.
pub fn encode_message<M: DofusMessage>(msg: &M, instance_id: u32) -> RawMessage {
    let mut payload_writer = BigEndianWriter::new();
    msg.serialize(&mut payload_writer);
    RawMessage {
        message_id: M::MESSAGE_ID,
        instance_id,
        payload: payload_writer.into_data(),
    }
}
