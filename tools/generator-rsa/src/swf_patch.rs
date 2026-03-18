//! SWF binary patching — replaces embedded binary assets.
//!
//! Targets two DefineBinaryData assets in DofusInvoker.swf:
//!   1. SIGNATURE_KEY_DATA — contains "DofusPublicKey" + N + E
//!   2. _verifyKey — contains a PEM public key

use anyhow::{bail, Context, Result};
use flate2::read::ZlibDecoder;
use flate2::write::ZlibEncoder;
use flate2::Compression;
use std::io::{Read, Write};

const TAG_DEFINE_BINARY_DATA: u16 = 87;

/// Patch a SWF file, replacing the two embedded RSA key assets.
pub fn patch_swf(
    swf_data: &[u8],
    signature_bin: &[u8],  // New SIGNATURE_KEY_DATA content
    verify_key: &[u8],     // New _verifyKey content (PEM bytes)
) -> Result<Vec<u8>> {
    if swf_data.len() < 8 {
        bail!("SWF file too small");
    }

    let compression = &swf_data[0..3];
    let version = swf_data[3];
    let file_length = u32::from_le_bytes([swf_data[4], swf_data[5], swf_data[6], swf_data[7]]);

    // Decompress if needed
    let raw = match compression {
        b"FWS" => swf_data[8..].to_vec(),
        b"CWS" => {
            let mut decoder = ZlibDecoder::new(&swf_data[8..]);
            let mut decompressed = Vec::with_capacity(file_length as usize);
            decoder.read_to_end(&mut decompressed)?;
            decompressed
        }
        _ => bail!("Unsupported SWF compression: {:?}", std::str::from_utf8(compression)),
    };

    // Parse and patch tags
    let (patched_tags, sig_count, pem_count) = patch_tags(&raw, signature_bin, verify_key)?;
    let sig_found = sig_count > 0;
    let verify_found = pem_count > 0;

    if !sig_found {
        eprintln!("  Warning: no DofusPublicKey assets found in SWF");
    }
    if !verify_found {
        eprintln!("  Warning: no PEM public key assets found in SWF");
    }

    // Rebuild SWF
    let new_file_length = (8 + patched_tags.len()) as u32;

    let mut output = Vec::with_capacity(new_file_length as usize);

    // Re-compress with same method as original
    match compression {
        b"FWS" => {
            output.extend_from_slice(b"FWS");
            output.push(version);
            output.extend_from_slice(&new_file_length.to_le_bytes());
            output.extend_from_slice(&patched_tags);
        }
        b"CWS" => {
            output.extend_from_slice(b"CWS");
            output.push(version);
            // file_length in header is the UNCOMPRESSED size
            let uncompressed_length = (8 + patched_tags.len()) as u32;
            output.extend_from_slice(&uncompressed_length.to_le_bytes());
            let mut encoder = ZlibEncoder::new(Vec::new(), Compression::best());
            encoder.write_all(&patched_tags)?;
            output.extend_from_slice(&encoder.finish()?);
        }
        _ => unreachable!(),
    }

    println!("  Patched SWF: {} -> {} bytes (uncompressed tags)",
        swf_data.len(), patched_tags.len() + 8);
    if sig_found { println!("  Replaced {} DofusPublicKey asset(s) ({} bytes each)", sig_count, signature_bin.len()); }
    if verify_found { println!("  Replaced {} PEM public key asset(s) ({} bytes each)", pem_count, verify_key.len()); }

    Ok(output)
}

fn patch_tags(
    raw: &[u8],
    signature_bin: &[u8],
    verify_key: &[u8],
) -> Result<(Vec<u8>, u32, u32)> {
    // The raw data starts with a RECT (variable length) + frame info.
    // We need to skip the SWF header (after the 8-byte file header).
    // RECT is encoded as: Nbits (5 bits) then 4 * Nbits bits, rounded up to bytes.
    let nbits = (raw[0] >> 3) as usize;
    let total_rect_bits = 5 + nbits * 4;
    let rect_bytes = (total_rect_bits + 7) / 8;
    // After RECT: frame_rate (u16) + frame_count (u16) = 4 bytes
    let header_len = rect_bytes + 4;

    if raw.len() < header_len {
        bail!("SWF body too small for header");
    }

    let mut output = Vec::with_capacity(raw.len());
    // Copy the header (RECT + frame info) as-is
    output.extend_from_slice(&raw[..header_len]);

    let mut pos = header_len;
    let mut sig_replaced = false;
    let mut pem_replace_count = 0u32;

    while pos < raw.len() {
        if pos + 2 > raw.len() { break; }

        let tag_code_and_length = u16::from_le_bytes([raw[pos], raw[pos + 1]]);
        let tag_type = tag_code_and_length >> 6;
        let mut tag_length = (tag_code_and_length & 0x3F) as usize;
        let mut header_size = 2;

        if tag_length == 0x3F {
            // Long tag
            if pos + 6 > raw.len() { break; }
            tag_length = u32::from_le_bytes([
                raw[pos + 2], raw[pos + 3], raw[pos + 4], raw[pos + 5],
            ]) as usize;
            header_size = 6;
        }

        let tag_data_start = pos + header_size;
        let tag_data_end = tag_data_start + tag_length;

        if tag_data_end > raw.len() {
            output.extend_from_slice(&raw[pos..]);
            break;
        }

        let tag_data = &raw[tag_data_start..tag_data_end];

        if tag_type == TAG_DEFINE_BINARY_DATA && tag_length > 6 {
            let binary_data = &tag_data[6..];

            // Replace only the FIRST DofusPublicKey asset (SIGNATURE_KEY_DATA)
            if !sig_replaced && contains_pattern(binary_data, b"DofusPublicKey") {
                write_binary_data_tag(&mut output, &tag_data[..2], signature_bin);
                sig_replaced = true;
                pos = tag_data_end;
                continue;
            }

            // Replace only the FIRST PEM public key (_verifyKey, char_id=115)
            // Comes before PUBLIC_KEY_V2 (char_id=117) in the SWF tag stream
            if pem_replace_count == 0 && contains_pattern(binary_data, b"BEGIN PUBLIC KEY") {
                write_binary_data_tag(&mut output, &tag_data[..2], verify_key);
                pem_replace_count += 1;
                pos = tag_data_end;
                continue;
            }
        }

        // Copy tag as-is
        output.extend_from_slice(&raw[pos..tag_data_end]);
        pos = tag_data_end;
    }

    Ok((output, if sig_replaced { 1 } else { 0 }, pem_replace_count))
}

fn write_binary_data_tag(output: &mut Vec<u8>, character_id_bytes: &[u8], new_data: &[u8]) {
    // Tag data = character_id (2) + reserved (4) + data
    let tag_data_len = 2 + 4 + new_data.len();

    // Always use long tag format for safety
    let tag_code_and_length = (TAG_DEFINE_BINARY_DATA << 6) | 0x3F;
    output.extend_from_slice(&tag_code_and_length.to_le_bytes());
    output.extend_from_slice(&(tag_data_len as u32).to_le_bytes());

    // character_id (keep original)
    output.extend_from_slice(character_id_bytes);
    // reserved
    output.extend_from_slice(&[0u8; 4]);
    // new data
    output.extend_from_slice(new_data);
}

fn contains_pattern(data: &[u8], pattern: &[u8]) -> bool {
    data.windows(pattern.len()).any(|w| w == pattern)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contains_pattern_found() {
        assert!(contains_pattern(b"hello DofusPublicKey world", b"DofusPublicKey"));
    }

    #[test]
    fn contains_pattern_not_found() {
        assert!(!contains_pattern(b"hello world", b"DofusPublicKey"));
    }

    #[test]
    fn contains_pattern_at_start() {
        assert!(contains_pattern(b"DofusPublicKey_rest", b"DofusPublicKey"));
    }

    #[test]
    fn contains_pattern_exact() {
        assert!(contains_pattern(b"abc", b"abc"));
    }

    #[test]
    fn write_binary_data_tag_format() {
        let mut output = Vec::new();
        let char_id = [0x01, 0x00]; // character_id = 1 (LE)
        let data = b"test data";

        write_binary_data_tag(&mut output, &char_id, data);

        // Tag header: (87 << 6) | 0x3F = 0x15FF (LE)
        assert_eq!(output[0], 0xFF);
        assert_eq!(output[1], 0x15);

        // Tag data length (LE u32): 2 (char_id) + 4 (reserved) + 9 (data) = 15
        let tag_len = u32::from_le_bytes([output[2], output[3], output[4], output[5]]);
        assert_eq!(tag_len, 15);

        // Character ID preserved
        assert_eq!(&output[6..8], &char_id);

        // Reserved = 0
        assert_eq!(&output[8..12], &[0, 0, 0, 0]);

        // Data
        assert_eq!(&output[12..], b"test data");
    }

    #[test]
    fn patch_swf_uncompressed() {
        // Build a minimal FWS (uncompressed) SWF with two DefineBinaryData tags
        let mut swf = Vec::new();

        // SWF header
        swf.extend_from_slice(b"FWS"); // signature
        swf.push(10); // version
        swf.extend_from_slice(&0u32.to_le_bytes()); // placeholder file_length

        // RECT: 5 bits for Nbits=0, so 1 byte total (5 bits + padding)
        swf.push(0x00); // Nbits=0, all zeros

        // Frame rate + frame count
        swf.extend_from_slice(&[0, 24]); // frame rate
        swf.extend_from_slice(&[1, 0]);  // frame count

        // DefineBinaryData tag #1: contains "DofusPublicKey"
        let sig_data = b"DofusPublicKey\x00old_n\x00old_e";
        let tag1_data_len = 2 + 4 + sig_data.len();
        let tag1_header = (TAG_DEFINE_BINARY_DATA << 6) | 0x3F;
        swf.extend_from_slice(&tag1_header.to_le_bytes());
        swf.extend_from_slice(&(tag1_data_len as u32).to_le_bytes());
        swf.extend_from_slice(&[0x01, 0x00]); // char_id
        swf.extend_from_slice(&[0, 0, 0, 0]); // reserved
        swf.extend_from_slice(sig_data);

        // DefineBinaryData tag #2: contains PEM key
        let pem_data = b"-----BEGIN PUBLIC KEY-----\nOLD_KEY\n-----END PUBLIC KEY-----";
        let tag2_data_len = 2 + 4 + pem_data.len();
        let tag2_header = (TAG_DEFINE_BINARY_DATA << 6) | 0x3F;
        swf.extend_from_slice(&tag2_header.to_le_bytes());
        swf.extend_from_slice(&(tag2_data_len as u32).to_le_bytes());
        swf.extend_from_slice(&[0x02, 0x00]); // char_id
        swf.extend_from_slice(&[0, 0, 0, 0]); // reserved
        swf.extend_from_slice(pem_data);

        // End tag
        swf.extend_from_slice(&[0, 0]);

        // Fix file_length
        let total = swf.len() as u32;
        swf[4..8].copy_from_slice(&total.to_le_bytes());

        // Patch
        let new_sig = b"DofusPublicKey\x00new_n_hex\x00new_e_hex";
        let new_pem = b"-----BEGIN PUBLIC KEY-----\nNEW_KEY_DATA\n-----END PUBLIC KEY-----";

        let patched = patch_swf(&swf, new_sig, new_pem).unwrap();

        // Verify it's still valid FWS
        assert_eq!(&patched[0..3], b"FWS");

        // Verify the new data is present
        assert!(contains_pattern(&patched, b"new_n_hex"));
        assert!(contains_pattern(&patched, b"NEW_KEY_DATA"));

        // Verify old data is gone
        assert!(!contains_pattern(&patched, b"old_n"));
        assert!(!contains_pattern(&patched, b"OLD_KEY"));
    }

    #[test]
    fn patch_swf_compressed_roundtrip() {
        // Build a minimal CWS (zlib compressed) SWF
        let mut raw_body = Vec::new();

        // RECT + frame info
        raw_body.push(0x00); // Nbits=0
        raw_body.extend_from_slice(&[0, 24, 1, 0]); // frame rate + count

        // A DefineBinaryData with "DofusPublicKey"
        let sig_data = b"header DofusPublicKey footer";
        let tag_data_len = 2 + 4 + sig_data.len();
        let tag_header = (TAG_DEFINE_BINARY_DATA << 6) | 0x3F;
        raw_body.extend_from_slice(&tag_header.to_le_bytes());
        raw_body.extend_from_slice(&(tag_data_len as u32).to_le_bytes());
        raw_body.extend_from_slice(&[0x01, 0x00]);
        raw_body.extend_from_slice(&[0, 0, 0, 0]);
        raw_body.extend_from_slice(sig_data);

        // End tag
        raw_body.extend_from_slice(&[0, 0]);

        // Compress
        let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(&raw_body).unwrap();
        let compressed = encoder.finish().unwrap();

        let uncompressed_len = (8 + raw_body.len()) as u32;
        let mut swf = Vec::new();
        swf.extend_from_slice(b"CWS");
        swf.push(10);
        swf.extend_from_slice(&uncompressed_len.to_le_bytes());
        swf.extend_from_slice(&compressed);

        let new_sig = b"replaced DofusPublicKey content";
        let patched = patch_swf(&swf, new_sig, b"no pem here").unwrap();

        // Should still be CWS
        assert_eq!(&patched[0..3], b"CWS");

        // Decompress and verify
        let mut decoder = ZlibDecoder::new(&patched[8..]);
        let mut decompressed = Vec::new();
        decoder.read_to_end(&mut decompressed).unwrap();

        assert!(contains_pattern(&decompressed, b"replaced DofusPublicKey content"));
    }
}
