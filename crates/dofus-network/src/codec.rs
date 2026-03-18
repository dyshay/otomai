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

#[cfg(test)]
mod tests {
    use super::*;
    use tokio_util::codec::{Decoder, Encoder};

    #[test]
    fn codec_encode_decode_roundtrip() {
        let msg = RawMessage {
            message_id: 42,
            instance_id: 1,
            payload: vec![0xDE, 0xAD, 0xBE, 0xEF],
        };

        let mut codec = DofusCodec::new();
        let mut buf = BytesMut::new();

        codec.encode(msg.clone(), &mut buf).unwrap();
        assert!(!buf.is_empty());

        let decoded = codec.decode(&mut buf).unwrap().unwrap();
        assert_eq!(decoded.message_id, 42);
        assert_eq!(decoded.instance_id, 1);
        assert_eq!(decoded.payload, vec![0xDE, 0xAD, 0xBE, 0xEF]);
    }

    #[test]
    fn codec_decode_empty_payload() {
        let msg = RawMessage {
            message_id: 1,
            instance_id: 0,
            payload: vec![],
        };

        let mut codec = DofusCodec::new();
        let mut buf = BytesMut::new();
        codec.encode(msg, &mut buf).unwrap();

        let decoded = codec.decode(&mut buf).unwrap().unwrap();
        assert_eq!(decoded.message_id, 1);
        assert_eq!(decoded.payload, Vec::<u8>::new());
    }

    #[test]
    fn codec_decode_incomplete_returns_none() {
        let mut codec = DofusCodec::new();
        let mut buf = BytesMut::from(&[0u8; 2][..]); // too small for a full frame
        assert!(codec.decode(&mut buf).unwrap().is_none());
    }

    #[test]
    fn codec_decode_partial_payload_returns_none() {
        let msg = RawMessage {
            message_id: 100,
            instance_id: 1,
            payload: vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10],
        };

        let mut codec = DofusCodec::new();
        let mut full_buf = BytesMut::new();
        codec.encode(msg, &mut full_buf).unwrap();

        // Give only half the frame
        let half = full_buf.len() / 2;
        let mut partial = BytesMut::from(&full_buf[..half]);
        assert!(codec.decode(&mut partial).unwrap().is_none());

        // Now give the full frame
        let mut complete = full_buf;
        let decoded = codec.decode(&mut complete).unwrap().unwrap();
        assert_eq!(decoded.payload.len(), 10);
    }

    #[test]
    fn codec_multiple_messages_in_stream() {
        let mut codec = DofusCodec::new();
        let mut buf = BytesMut::new();

        for i in 0..5 {
            let msg = RawMessage {
                message_id: i,
                instance_id: i as u32,
                payload: vec![i as u8; (i + 1) as usize],
            };
            codec.encode(msg, &mut buf).unwrap();
        }

        for i in 0..5 {
            let decoded = codec.decode(&mut buf).unwrap().unwrap();
            assert_eq!(decoded.message_id, i);
            assert_eq!(decoded.payload.len(), (i + 1) as usize);
        }

        assert!(codec.decode(&mut buf).unwrap().is_none());
    }

    #[test]
    fn codec_large_payload() {
        let msg = RawMessage {
            message_id: 999,
            instance_id: 42,
            payload: vec![0xAB; 100_000],
        };

        let mut codec = DofusCodec::new();
        let mut buf = BytesMut::new();
        codec.encode(msg, &mut buf).unwrap();

        let decoded = codec.decode(&mut buf).unwrap().unwrap();
        assert_eq!(decoded.message_id, 999);
        assert_eq!(decoded.payload.len(), 100_000);
    }
}
