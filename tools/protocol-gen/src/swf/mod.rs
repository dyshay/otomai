//! SWF container parser — extracts DoABC tags from SWF files.

use anyhow::{bail, Context, Result};
use std::io::Read;

const TAG_DO_ABC: u16 = 82;
const TAG_DO_ABC_DEFINE: u16 = 72;

#[derive(Debug)]
pub struct SwfFile {
    pub version: u8,
    pub abc_blocks: Vec<AbcBlock>,
}

#[derive(Debug)]
pub struct AbcBlock {
    pub name: String,
    pub data: Vec<u8>,
}

pub fn parse_swf(raw: &[u8]) -> Result<SwfFile> {
    if raw.len() < 8 {
        bail!("SWF file too small");
    }

    let signature = &raw[0..3];
    let version = raw[3];
    let file_length = u32::from_le_bytes([raw[4], raw[5], raw[6], raw[7]]) as usize;

    let decompressed = match signature {
        b"FWS" => raw.to_vec(),
        b"CWS" => {
            // zlib compressed
            let mut out = raw[0..8].to_vec();
            let mut decoder = flate2::read::ZlibDecoder::new(&raw[8..]);
            decoder
                .read_to_end(&mut out)
                .context("Failed to decompress CWS (zlib)")?;
            out
        }
        b"ZWS" => {
            // LZMA compressed
            let mut out = raw[0..8].to_vec();
            // ZWS format: 8 byte header + 4 bytes compressed length + 5 bytes LZMA props + LZMA data
            if raw.len() < 17 {
                bail!("ZWS file too small for LZMA header");
            }
            let _compressed_len =
                u32::from_le_bytes([raw[8], raw[9], raw[10], raw[11]]) as usize;
            let lzma_data = &raw[12..];
            // Build a proper LZMA stream header for lzma-rs
            // LZMA props (5 bytes) + uncompressed size (8 bytes LE) + compressed data
            let uncompressed_size = (file_length - 8) as u64;
            let mut lzma_stream = Vec::new();
            lzma_stream.extend_from_slice(&lzma_data[..5]); // LZMA properties
            lzma_stream.extend_from_slice(&uncompressed_size.to_le_bytes()); // uncompressed size
            if lzma_data.len() > 5 {
                lzma_stream.extend_from_slice(&lzma_data[5..]); // compressed data
            }
            let mut decoder = std::io::Cursor::new(&lzma_stream);
            lzma_rs::lzma_decompress(&mut decoder, &mut out)
                .context("Failed to decompress ZWS (LZMA)")?;
            out
        }
        _ => bail!(
            "Unknown SWF signature: {:?}",
            String::from_utf8_lossy(signature)
        ),
    };

    tracing::info!(
        version,
        file_length,
        decompressed_len = decompressed.len(),
        "Parsed SWF header"
    );

    let abc_blocks = extract_abc_tags(&decompressed)?;
    tracing::info!(count = abc_blocks.len(), "Found DoABC blocks");

    Ok(SwfFile {
        version,
        abc_blocks,
    })
}

fn extract_abc_tags(data: &[u8]) -> Result<Vec<AbcBlock>> {
    let mut blocks = Vec::new();

    // Skip SWF header: 8 bytes fixed + RECT (variable) + frame rate (2) + frame count (2)
    // RECT is bit-packed: Nbits(5 bits) then 4 × Nbits-bit fields
    if data.len() < 9 {
        bail!("SWF too small for header");
    }

    let rect_start = 8;
    let nbits = (data[rect_start] >> 3) as usize;
    let rect_total_bits = 5 + nbits * 4;
    let rect_bytes = (rect_total_bits + 7) / 8;
    let after_rect = rect_start + rect_bytes;

    // frame_rate (2 bytes) + frame_count (2 bytes)
    let mut pos = after_rect + 4;

    // Parse tags
    while pos + 2 <= data.len() {
        let tag_code_and_length = u16::from_le_bytes([data[pos], data[pos + 1]]);
        pos += 2;

        let tag_type = tag_code_and_length >> 6;
        let mut tag_length = (tag_code_and_length & 0x3f) as usize;

        if tag_length == 0x3f {
            // Extended length
            if pos + 4 > data.len() {
                break;
            }
            tag_length = u32::from_le_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]])
                as usize;
            pos += 4;
        }

        if pos + tag_length > data.len() {
            tracing::warn!(
                tag_type,
                tag_length,
                pos,
                data_len = data.len(),
                "Tag extends beyond file"
            );
            break;
        }

        let tag_data = &data[pos..pos + tag_length];

        match tag_type {
            TAG_DO_ABC => {
                // DoABC2: flags(u32) + name(null-terminated string) + ABC data
                if tag_data.len() < 5 {
                    pos += tag_length;
                    continue;
                }
                let _flags = u32::from_le_bytes([tag_data[0], tag_data[1], tag_data[2], tag_data[3]]);
                let name_start = 4;
                let name_end = tag_data[name_start..]
                    .iter()
                    .position(|&b| b == 0)
                    .map(|p| name_start + p)
                    .unwrap_or(tag_data.len());
                let name =
                    String::from_utf8_lossy(&tag_data[name_start..name_end]).to_string();
                let abc_start = name_end + 1;
                if abc_start < tag_data.len() {
                    blocks.push(AbcBlock {
                        name,
                        data: tag_data[abc_start..].to_vec(),
                    });
                }
            }
            TAG_DO_ABC_DEFINE => {
                // DoABC (tag 72): raw ABC data, no flags/name
                blocks.push(AbcBlock {
                    name: String::new(),
                    data: tag_data.to_vec(),
                });
            }
            0 => break, // End tag
            _ => {}
        }

        pos += tag_length;
    }

    Ok(blocks)
}
