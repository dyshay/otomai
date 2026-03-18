//! Dofus RSA patcher — faithful Rust port of hetwanmod's two-key architecture.
//!
//! Two separate RSA keys:
//!   - Patcher key (1024-bit): signs AKSF files/hosts → SIGNATURE_KEY_DATA in SWF
//!   - Signature key (2048-bit): signs auth session DER → _verifyKey in SWF
//!
//! Reference:
//!   hetwanmod/tools/patcher/src/index.ts (gen + sign)
//!   hetwanmod/rustbindings/signature/rust/src/lib.rs (get_signed_key)

mod swf_patch;

use anyhow::{Context, Result};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use clap::{Parser, Subcommand};
use dofus_io::BigEndianWriter;
use pkcs1::{DecodeRsaPrivateKey, EncodeRsaPrivateKey};
use pkcs8::EncodePublicKey;
use rand::Rng;
use rsa::pkcs1v15::Pkcs1v15Sign;
use rsa::traits::PublicKeyParts;
use rsa::{BigUint, RsaPrivateKey, RsaPublicKey};
use std::fs;
use std::path::PathBuf;

const KEY_HEADER: &str = "DofusPublicKey";
const SIGN_HEADER: &str = "AKSF";
const PATCHER_KEY_BITS: usize = 1024;
const SIGNATURE_KEY_BITS: usize = 2048;
const SESSION_KEY_BITS: usize = 1024;

// -- CLI --

#[derive(Parser)]
#[command(name = "generator-rsa", about = "Dofus RSA patcher — two-key architecture")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Generate both keypairs: 1024-bit patcher + 2048-bit signature
    #[command(name = "gen")]
    Generate {
        #[arg(short, long, default_value = "./")]
        output_path: PathBuf,
    },
    /// Sign hosts string or file (AKSF format, uses 1024-bit patcher key)
    Sign {
        /// Path to 1024-bit patcher private key
        private_key_path: PathBuf,
        #[arg(long)]
        hosts: Option<String>,
        #[arg(short, long)]
        file: Option<PathBuf>,
        #[arg(short, long, default_value = "./")]
        output_path: PathBuf,
    },
    /// Generate auth session keys (uses 2048-bit signature key)
    AuthKeys {
        /// Path to 2048-bit signature private key
        #[arg(short = 'k', long)]
        private_key: PathBuf,
        #[arg(short, long, default_value = "./")]
        output_path: PathBuf,
    },
    /// Patch DofusInvoker.swf + config.xml + signature.xmls (uses both keys)
    Patch {
        /// Path to 1024-bit patcher private key (for AKSF + SIGNATURE_KEY_DATA)
        #[arg(short = 'k', long)]
        private_key: PathBuf,
        /// Path to 2048-bit signature private key (for _verifyKey)
        #[arg(long)]
        sig_key: PathBuf,
        #[arg(long)]
        swf: PathBuf,
        #[arg(long)]
        config: PathBuf,
        /// Path to original signature.xmls (will strip Ankama's AKSF and re-sign)
        #[arg(long)]
        signature_xmls: Option<PathBuf>,
        #[arg(long, default_value = "localhost")]
        host: String,
        #[arg(long, default_value = "5555")]
        port: String,
        #[arg(short, long, default_value = "./patched")]
        output: PathBuf,
    },
}

// -- Key helpers --

fn load_private_key(path: &PathBuf) -> Result<RsaPrivateKey> {
    let pem = fs::read_to_string(path).context("Failed to read private key")?;
    RsaPrivateKey::from_pkcs1_pem(&pem)
        .or_else(|_| {
            use pkcs8::DecodePrivateKey;
            RsaPrivateKey::from_pkcs8_pem(&pem)
        })
        .context("Failed to parse private key")
}

/// PKCS#1 v1.5 private encrypt — equivalent to:
///   Node.js: privateEncrypt({ key, padding: 1 }, data)
///   OpenSSL: RSA_private_encrypt(..., Padding::PKCS1)
fn pkcs1_private_encrypt(key: &RsaPrivateKey, data: &[u8]) -> Result<Vec<u8>> {
    key.sign(Pkcs1v15Sign::new_unprefixed(), data)
        .context("PKCS1v15 private encrypt failed")
}

/// DofusPublicKey binary: writeUTF("DofusPublicKey") + writeUTF(n_hex) + writeUTF(e_hex)
fn build_pub_bin(key: &RsaPublicKey) -> Vec<u8> {
    let n = BigUint::from_bytes_be(&key.n().to_bytes_be()).to_str_radix(16);
    let e = BigUint::from_bytes_be(&key.e().to_bytes_be()).to_str_radix(16);
    let mut w = BigEndianWriter::new();
    w.write_utf(KEY_HEADER);
    w.write_utf(&n);
    w.write_utf(&e);
    w.into_data()
}

/// SPKI PEM with standard 64-char line wrapping (matching Node.js generateKeyPair output)
fn build_pem(key: &RsaPublicKey) -> Result<String> {
    key.to_public_key_pem(pkcs8::LineEnding::LF)
        .context("Failed to encode public key to PEM")
}

/// Write PKCS#1 private key PEM
fn write_private_pem(key: &RsaPrivateKey, path: &PathBuf) -> Result<()> {
    let pem = key.to_pkcs1_pem(pkcs1::LineEnding::LF).context("Failed to encode private key")?;
    fs::write(path, pem.as_bytes())?;
    Ok(())
}

// -- AKSF --

/// Build AKSF signature.
/// Ref: hetwanmod patcher sign() lines 114-149
fn build_aksf(key: &RsaPrivateKey, data: &[u8]) -> Result<Vec<u8>> {
    let random: u8 = rand::thread_rng().gen_range(0..256u16) as u8;
    let md5_hex = format!("{:x}", md5::compute(data));

    let mut w = BigEndianWriter::new();
    w.write_byte(random);
    w.write_uint(data.len() as u32);
    w.write_utf_bytes(&md5_hex);
    let mut hash = w.into_data();

    for b in hash[2..].iter_mut() {
        *b ^= random;
    }

    let signed = pkcs1_private_encrypt(key, &hash)?;

    let mut out = BigEndianWriter::new();
    out.write_utf(SIGN_HEADER);
    out.write_short(1);
    out.write_int(signed.len() as i32);
    out.write_bytes(&signed);
    Ok(out.into_data())
}

// -- Commands --

/// gen — Generate both keypairs.
/// Ref: hetwanmod patcher generate() (1024-bit) + separate 2048-bit for auth
fn cmd_generate(output_path: &PathBuf) -> Result<()> {
    fs::create_dir_all(output_path)?;
    let mut rng = rand::thread_rng();

    // --- Patcher key (1024-bit) — for AKSF signing + SIGNATURE_KEY_DATA ---
    let patcher_key = RsaPrivateKey::new(&mut rng, PATCHER_KEY_BITS)
        .context("Failed to generate patcher keypair")?;
    let patcher_pub = patcher_key.to_public_key();

    fs::write(output_path.join("pub.bin"), build_pub_bin(&patcher_pub))?;
    fs::write(output_path.join("pub.pem"), build_pem(&patcher_pub)?)?;
    write_private_pem(&patcher_key, &output_path.join("priv.pem"))?;

    // --- Signature key (2048-bit) — for auth session signing + _verifyKey ---
    let sig_key = RsaPrivateKey::new(&mut rng, SIGNATURE_KEY_BITS)
        .context("Failed to generate signature keypair")?;
    let sig_pub = sig_key.to_public_key();

    fs::write(output_path.join("sig_pub.pem"), build_pem(&sig_pub)?)?;
    write_private_pem(&sig_key, &output_path.join("sig_priv.pem"))?;

    println!("Keys generated successfully");
    println!();
    println!("Patcher key (1024-bit, for AKSF + SIGNATURE_KEY_DATA):");
    println!("  {}/priv.pem", output_path.display());
    println!("  {}/pub.bin", output_path.display());
    println!("  {}/pub.pem", output_path.display());
    println!();
    println!("Signature key (2048-bit, for auth + _verifyKey):");
    println!("  {}/sig_priv.pem", output_path.display());
    println!("  {}/sig_pub.pem", output_path.display());
    Ok(())
}

/// sign — AKSF signing with 1024-bit patcher key.
/// Ref: hetwanmod patcher sign() lines 91-163
fn cmd_sign(
    private_key_path: &PathBuf,
    hosts: &Option<String>,
    file: &Option<PathBuf>,
    output_path: &PathBuf,
) -> Result<()> {
    if hosts.is_none() && file.is_none() {
        anyhow::bail!("You should pass at least one option (--hosts or --file)");
    }

    let key = load_private_key(private_key_path)?;
    let data: Vec<u8> = if let Some(h) = hosts {
        h.as_bytes().to_vec()
    } else {
        fs::read(file.as_ref().unwrap())?
    };

    let aksf = build_aksf(&key, &data)?;

    if hosts.is_some() {
        println!("{}", BASE64.encode(&aksf));
    } else {
        fs::create_dir_all(output_path)?;
        let mut output = aksf;
        output.extend_from_slice(&data);
        fs::write(output_path.join("out.bin"), &output)?;
        println!("Signed file written to: {}/out.bin", output_path.display());
    }
    Ok(())
}

/// auth-keys — Generate session keypair, sign with 2048-bit signature key.
/// Ref: hetwanmod/rustbindings/signature/rust/src/lib.rs get_signed_key()
///
///   let signature_key = Rsa::private_key_from_pem(private_key);
///   let keypair = Rsa::generate(RSA_KEY_SIZE);  // 1024
///   let public_key = keypair.public_key_to_der();
///   let private_key = keypair.private_key_to_der();
///   signature_key.private_encrypt(&public_key, ..., Padding::PKCS1);
fn cmd_auth_keys(sig_key_path: &PathBuf, output_path: &PathBuf) -> Result<()> {
    fs::create_dir_all(output_path)?;

    let signature_key = load_private_key(sig_key_path)?;
    let sig_bits = signature_key.size() * 8;
    println!("Signature key loaded ({}-bit)", sig_bits);

    let mut rng = rand::thread_rng();
    let session_key = RsaPrivateKey::new(&mut rng, SESSION_KEY_BITS)
        .context("Failed to generate session keypair")?;
    let session_pub = session_key.to_public_key();

    // SPKI DER (matches OpenSSL public_key_to_der / i2d_RSA_PUBKEY)
    let pub_der = session_pub
        .to_public_key_der()
        .context("Failed to encode session public key DER")?;

    // PKCS#1 DER (matches OpenSSL private_key_to_der / i2d_RSAPrivateKey)
    let priv_der = session_key
        .to_pkcs1_der()
        .context("Failed to encode session private key DER")?;

    // Sign session public DER with signature key (PKCS1 padding)
    let encrypted_pub = pkcs1_private_encrypt(&signature_key, pub_der.as_bytes())?;

    fs::write(output_path.join("session_pub.der"), pub_der.as_bytes())?;
    fs::write(output_path.join("session_priv.der"), priv_der.as_bytes())?;
    fs::write(output_path.join("session_signed.bin"), &encrypted_pub)?;

    println!("Auth session keys generated");
    println!("  session_pub.der:     {} bytes (SPKI DER)", pub_der.as_bytes().len());
    println!("  session_priv.der:    {} bytes (PKCS#1 DER)", priv_der.as_bytes().len());
    println!("  session_signed.bin:  {} bytes (PKCS1 encrypted)", encrypted_pub.len());
    Ok(())
}

/// Strip AKSF header from a signed file, returning the raw data after the signature.
/// Format: writeUTF("AKSF") [6] + writeShort(version) [2] + writeInt(sig_len) [4] + sig [sig_len] + raw_data
fn strip_aksf(data: &[u8]) -> Result<&[u8]> {
    if data.len() < 12 {
        anyhow::bail!("File too small for AKSF header");
    }
    let utf_len = u16::from_be_bytes([data[0], data[1]]) as usize;
    if utf_len + 2 > data.len() {
        anyhow::bail!("Invalid AKSF UTF length");
    }
    let header = &data[2..2 + utf_len];
    if header != b"AKSF" {
        // No AKSF header — return as-is (raw XML)
        return Ok(data);
    }
    // skip: writeUTF("AKSF") [6] + writeShort(version) [2]
    let sig_len = u32::from_be_bytes([data[8], data[9], data[10], data[11]]) as usize;
    let data_start = 12 + sig_len;
    if data_start > data.len() {
        anyhow::bail!("AKSF signature extends past end of file");
    }
    Ok(&data[data_start..])
}

/// patch — Patch SWF + config.xml + signature.xmls from originals/ into output/.
fn cmd_patch(
    patcher_key_path: &PathBuf,
    sig_key_path: &PathBuf,
    swf_path: &PathBuf,
    config_path: &PathBuf,
    signature_xmls_path: Option<&PathBuf>,
    host: &str,
    port: &str,
    output: &PathBuf,
) -> Result<()> {
    let patcher_key = load_private_key(patcher_key_path)?;
    let sig_key = load_private_key(sig_key_path)?;
    fs::create_dir_all(output)?;

    let patcher_pub = patcher_key.to_public_key();
    let sig_pub = sig_key.to_public_key();

    // 1. SIGNATURE_KEY_DATA ← DofusPublicKey from 1024-bit patcher key
    let pub_bin = build_pub_bin(&patcher_pub);

    // 2. _verifyKey ← PEM from 2048-bit signature key
    let verify_key = build_pem(&sig_pub)?.into_bytes();

    // 3. Patch SWF
    println!("Patching SWF: {}", swf_path.display());
    let swf_data = fs::read(swf_path)?;
    let patched_swf = swf_patch::patch_swf(&swf_data, &pub_bin, &verify_key)?;
    let swf_out = output.join("DofusInvoker.swf");
    fs::write(&swf_out, &patched_swf)?;
    println!("  -> {}", swf_out.display());

    // 4. Patch config.xml — sign host with AKSF (1024-bit patcher key)
    println!("Patching config.xml: {}", config_path.display());
    let config = fs::read_to_string(config_path)?;
    let host_sig = BASE64.encode(&build_aksf(&patcher_key, host.as_bytes())?);
    let patched = patch_config_xml(&config, host, port, &host_sig)?;
    let config_out = output.join("config.xml");
    fs::write(&config_out, &patched)?;
    println!("  -> {}", config_out.display());

    // 5. Re-sign signature.xmls — strip Ankama's AKSF, remove entries for missing/empty files, re-sign
    if let Some(xmls_path) = signature_xmls_path {
        println!("Re-signing signature.xmls: {}", xmls_path.display());
        let raw_file = fs::read(xmls_path)?;
        let xml_data = strip_aksf(&raw_file)?;
        let xml_str = std::str::from_utf8(xml_data).context("signature.xmls is not valid UTF-8")?;

        // Filter out <file> entries whose real file is empty or missing on disk
        // The theme dir is the parent of the signature.xmls file
        let theme_dir = xmls_path.parent().unwrap_or(xmls_path);
        let mut cleaned = String::new();
        let mut removed = 0u32;
        for line in xml_str.lines() {
            if line.contains("<file name=") {
                // Extract filename: <file name="path\to\file">size</file>
                if let (Some(start), Some(end)) = (line.find("name=\""), line.find("\">")) {
                    let name = &line[start + 6..end];
                    let path = theme_dir.join(name.replace('\\', "/"));
                    if !path.exists() || path.metadata().map(|m| m.len() == 0).unwrap_or(true) {
                        removed += 1;
                        continue; // skip this entry
                    }
                }
            }
            cleaned.push_str(line);
            cleaned.push('\n');
        }

        if removed > 0 {
            println!("  Removed {removed} entries for empty/missing files");
        }

        let cleaned_bytes = cleaned.as_bytes();
        let aksf = build_aksf(&patcher_key, cleaned_bytes)?;
        let mut signed_output = aksf;
        signed_output.extend_from_slice(cleaned_bytes);
        let xmls_out = output.join("signature.xmls");
        fs::write(&xmls_out, &signed_output)?;
        println!("  -> {} ({} bytes XML re-signed)", xmls_out.display(), cleaned_bytes.len());
    }

    println!("\nPatch complete. Copy files from {} into your Dofus client.", output.display());
    Ok(())
}

fn patch_config_xml(content: &str, host: &str, port: &str, signature: &str) -> Result<String> {
    let mut result = String::new();
    for line in content.lines() {
        if line.contains("\"connection.host\"") && !line.contains("signature") {
            result.push_str(&format!("\t<entry key=\"connection.host\">{host}</entry>\n"));
        } else if line.contains("\"connection.host.signature\"") {
            result.push_str(&format!("\t<entry key=\"connection.host.signature\">{signature}</entry>\n"));
        } else if line.contains("\"connection.port\"") {
            result.push_str(&format!("\t<entry key=\"connection.port\">{port}</entry>\n"));
        } else {
            result.push_str(line);
            result.push('\n');
        }
    }
    Ok(result)
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match &cli.command {
        Commands::Generate { output_path } => cmd_generate(output_path),
        Commands::Sign { private_key_path, hosts, file, output_path } => {
            cmd_sign(private_key_path, hosts, file, output_path)
        }
        Commands::AuthKeys { private_key, output_path } => {
            cmd_auth_keys(private_key, output_path)
        }
        Commands::Patch { private_key, sig_key, swf, config, signature_xmls, host, port, output } => {
            cmd_patch(private_key, sig_key, swf, config, signature_xmls.as_ref(), host, port, output)
        }
    }
}
