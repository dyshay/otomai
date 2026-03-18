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
        // Server→Client: no instance_id in header (client's lowReceive doesn't read it)
        network::write_server_header(&mut writer, item.message_id, item.payload.len());
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

#[cfg(test)]
mod tests {
    use super::*;
    use dofus_io::BigEndianWriter;
    use tokio_util::codec::{Decoder, Encoder};

    /// Build a client→server frame (WITH instance_id) for decode testing.
    fn build_client_frame(message_id: u16, instance_id: u32, payload: &[u8]) -> Vec<u8> {
        let mut writer = BigEndianWriter::new();
        network::write_header(&mut writer, message_id, instance_id, payload.len());
        writer.write_bytes(payload);
        writer.into_data()
    }

    #[test]
    fn encode_server_message_no_instance_id() {
        let msg = RawMessage {
            message_id: 42,
            instance_id: 0,
            payload: vec![0xDE, 0xAD, 0xBE, 0xEF],
        };

        let mut codec = DofusCodec::new();
        let mut buf = BytesMut::new();
        codec.encode(msg, &mut buf).unwrap();

        // Server→Client: header_short(2) + payload_len(1) + payload(4) = 7 bytes
        // NO instance_id
        assert_eq!(buf.len(), 7);
    }

    #[test]
    fn decode_client_message_with_instance_id() {
        let frame = build_client_frame(42, 1, &[0xDE, 0xAD, 0xBE, 0xEF]);
        let mut codec = DofusCodec::new();
        let mut buf = BytesMut::from(&frame[..]);

        let decoded = codec.decode(&mut buf).unwrap().unwrap();
        assert_eq!(decoded.message_id, 42);
        assert_eq!(decoded.instance_id, 1);
        assert_eq!(decoded.payload, vec![0xDE, 0xAD, 0xBE, 0xEF]);
    }

    #[test]
    fn decode_client_empty_payload() {
        let frame = build_client_frame(1, 0, &[]);
        let mut codec = DofusCodec::new();
        let mut buf = BytesMut::from(&frame[..]);

        let decoded = codec.decode(&mut buf).unwrap().unwrap();
        assert_eq!(decoded.message_id, 1);
        assert_eq!(decoded.payload, Vec::<u8>::new());
    }

    #[test]
    fn decode_incomplete_returns_none() {
        let mut codec = DofusCodec::new();
        let mut buf = BytesMut::from(&[0u8; 2][..]); // too small
        assert!(codec.decode(&mut buf).unwrap().is_none());
    }

    #[test]
    fn decode_partial_payload_returns_none() {
        let frame = build_client_frame(100, 1, &[1, 2, 3, 4, 5, 6, 7, 8, 9, 10]);
        let half = frame.len() / 2;

        let mut codec = DofusCodec::new();
        let mut partial = BytesMut::from(&frame[..half]);
        assert!(codec.decode(&mut partial).unwrap().is_none());

        let mut complete = BytesMut::from(&frame[..]);
        let decoded = codec.decode(&mut complete).unwrap().unwrap();
        assert_eq!(decoded.payload.len(), 10);
    }

    #[test]
    fn decode_multiple_client_messages() {
        let mut buf = BytesMut::new();
        for i in 0u16..5 {
            buf.extend_from_slice(&build_client_frame(i, i as u32, &vec![i as u8; (i + 1) as usize]));
        }

        let mut codec = DofusCodec::new();
        for i in 0u16..5 {
            let decoded = codec.decode(&mut buf).unwrap().unwrap();
            assert_eq!(decoded.message_id, i);
            assert_eq!(decoded.payload.len(), (i + 1) as usize);
        }
        assert!(codec.decode(&mut buf).unwrap().is_none());
    }

    #[test]
    fn decode_large_payload() {
        let payload = vec![0xAB; 100_000];
        let frame = build_client_frame(999, 42, &payload);
        let mut codec = DofusCodec::new();
        let mut buf = BytesMut::from(&frame[..]);

        let decoded = codec.decode(&mut buf).unwrap().unwrap();
        assert_eq!(decoded.message_id, 999);
        assert_eq!(decoded.payload.len(), 100_000);
    }
}
