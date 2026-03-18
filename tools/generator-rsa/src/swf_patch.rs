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
    let (patched_tags, sig_found, verify_found) = patch_tags(&raw, signature_bin, verify_key)?;

    if !sig_found {
        eprintln!("  Warning: SIGNATURE_KEY_DATA asset not found in SWF");
    }
    if !verify_found {
        eprintln!("  Warning: _verifyKey asset not found in SWF");
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
    if sig_found { println!("  Replaced SIGNATURE_KEY_DATA ({} bytes)", signature_bin.len()); }
    if verify_found { println!("  Replaced _verifyKey ({} bytes)", verify_key.len()); }

    Ok(output)
}

fn patch_tags(
    raw: &[u8],
    signature_bin: &[u8],
    verify_key: &[u8],
) -> Result<(Vec<u8>, bool, bool)> {
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
    let mut sig_found = false;
    let mut verify_found = false;

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
            // Truncated tag — copy remaining bytes as-is
            output.extend_from_slice(&raw[pos..]);
            break;
        }

        let tag_data = &raw[tag_data_start..tag_data_end];

        if tag_type == TAG_DEFINE_BINARY_DATA && tag_length > 6 {
            // DefineBinaryData: character_id (u16 LE) + reserved (u32 LE) + data
            let binary_data = &tag_data[6..];

            if !sig_found && contains_pattern(binary_data, b"DofusPublicKey") {
                // Replace with our signature.bin
                write_binary_data_tag(&mut output, &tag_data[..2], signature_bin);
                sig_found = true;
                pos = tag_data_end;
                continue;
            }

            if !verify_found && contains_pattern(binary_data, b"BEGIN PUBLIC KEY") {
                // Replace with our verify key
                write_binary_data_tag(&mut output, &tag_data[..2], verify_key);
                verify_found = true;
                pos = tag_data_end;
                continue;
            }
        }

        // Copy tag as-is
        output.extend_from_slice(&raw[pos..tag_data_end]);
        pos = tag_data_end;
    }

    Ok((output, sig_found, verify_found))
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
