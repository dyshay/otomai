mod swf_patch;

use anyhow::{Context, Result};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use clap::{Parser, Subcommand};
use dofus_io::BigEndianWriter;
use pkcs1::{DecodeRsaPrivateKey, EncodeRsaPrivateKey};
use pkcs8::EncodePublicKey;
use rand::Rng;
use rsa::traits::PublicKeyParts;
use rsa::{BigUint, RsaPrivateKey};
use std::fs;
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "generator-rsa", about = "Dofus RSA key generator, signer, and client patcher")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Generate RSA 2048-bit keypair
    Generate {
        #[arg(short, long, default_value = "./keys")]
        output_path: PathBuf,
    },
    /// Sign data or hosts string
    Sign {
        /// Path to private key PEM file
        private_key_path: PathBuf,
        /// Hosts string to sign
        #[arg(long)]
        hosts: Option<String>,
        /// File path to sign
        #[arg(short, long)]
        file: Option<PathBuf>,
        #[arg(short, long, default_value = "./output")]
        output_path: PathBuf,
    },
    /// Patch a Dofus client to use our server
    Patch {
        /// Path to private key PEM file
        #[arg(short = 'k', long)]
        private_key: PathBuf,

        /// Path to DofusInvoker.swf
        #[arg(long)]
        swf: PathBuf,

        /// Path to config.xml
        #[arg(long)]
        config: PathBuf,

        /// Auth server host
        #[arg(long, default_value = "127.0.0.1")]
        host: String,

        /// Auth server port
        #[arg(long, default_value = "5555")]
        port: String,

        /// Output directory for patched files
        #[arg(short, long, default_value = "./patched")]
        output: PathBuf,
    },
}

fn load_private_key(path: &PathBuf) -> Result<RsaPrivateKey> {
    let pem = fs::read_to_string(path).context("Failed to read private key")?;
    RsaPrivateKey::from_pkcs1_pem(&pem)
        .or_else(|_| {
            use pkcs8::DecodePrivateKey;
            RsaPrivateKey::from_pkcs8_pem(&pem)
        })
        .context("Failed to parse private key")
}

fn generate(output_path: &PathBuf) -> Result<()> {
    fs::create_dir_all(output_path)?;

    let mut rng = rand::thread_rng();
    let private_key =
        RsaPrivateKey::new(&mut rng, 2048).context("Failed to generate RSA keypair")?;
    let public_key = private_key.to_public_key();

    let n_hex = BigUint::from_bytes_be(&public_key.n().to_bytes_be()).to_str_radix(16);
    let e_hex = BigUint::from_bytes_be(&public_key.e().to_bytes_be()).to_str_radix(16);

    // signature.bin: "DofusPublicKey" + n_hex + e_hex (BigEndian UTF)
    let mut writer = BigEndianWriter::new();
    writer.write_utf("DofusPublicKey");
    writer.write_utf(&n_hex);
    writer.write_utf(&e_hex);
    fs::write(output_path.join("signature.bin"), writer.data())?;

    // public.pem
    let der_bytes = public_key
        .to_public_key_der()
        .context("Failed to encode public key to DER")?;
    let b64 = BASE64.encode(der_bytes.as_bytes());
    let public_pem = format!("-----BEGIN PUBLIC KEY-----\n{b64}\n-----END PUBLIC KEY-----");
    fs::write(output_path.join("public.pem"), &public_pem)?;

    // private.pem (PKCS1)
    let private_pem = private_key
        .to_pkcs1_pem(pkcs1::LineEnding::LF)
        .context("Failed to encode private key to PEM")?;
    fs::write(output_path.join("private.pem"), private_pem.as_bytes())?;

    println!("Keys generated in {}", output_path.display());
    Ok(())
}

fn sign_hosts(private_key: &RsaPrivateKey, hosts: &str) -> Result<String> {
    let data = hosts.as_bytes();
    let random: u8 = rand::thread_rng().gen_range(0..=254);
    let md5_hex = format!("{:x}", md5::compute(data));

    let mut hash_writer = BigEndianWriter::new();
    hash_writer.write_byte(random);
    hash_writer.write_uint(data.len() as u32);
    hash_writer.write_utf_bytes(&md5_hex);

    let mut buffer = hash_writer.into_data();
    for byte in buffer[2..].iter_mut() {
        *byte ^= random;
    }

    let plaintext = BigUint::from_bytes_be(&buffer);
    let signed_data =
        rsa::hazmat::rsa_decrypt(None::<&mut rand::rngs::ThreadRng>, private_key, &plaintext)
            .context("RSA private encryption failed")?;
    let signed_bytes = signed_data.to_bytes_be();

    let key_size = private_key.size();
    let mut padded = vec![0u8; key_size];
    let offset = key_size.saturating_sub(signed_bytes.len());
    padded[offset..].copy_from_slice(&signed_bytes);

    // Build AKSF header
    let mut writer = BigEndianWriter::new();
    writer.write_utf("AKSF");
    writer.write_short(1);
    writer.write_int(padded.len() as i32);
    writer.write_bytes(&padded);

    Ok(BASE64.encode(writer.data()))
}

fn sign(
    private_key_path: &PathBuf,
    hosts: &Option<String>,
    file: &Option<PathBuf>,
    output_path: &PathBuf,
) -> Result<()> {
    if hosts.is_none() && file.is_none() {
        anyhow::bail!("Pass at least one option: --hosts or --file");
    }

    let private_key = load_private_key(private_key_path)?;

    let data: Vec<u8> = if let Some(hosts_str) = hosts {
        hosts_str.as_bytes().to_vec()
    } else {
        fs::read(file.as_ref().unwrap())?
    };

    let random: u8 = rand::thread_rng().gen_range(0..=254);
    let data_as_str = std::str::from_utf8(&data).unwrap_or("");
    let md5_hex = format!("{:x}", md5::compute(data_as_str.as_bytes()));

    let mut hash_writer = BigEndianWriter::new();
    hash_writer.write_byte(random);
    hash_writer.write_uint(data.len() as u32);
    hash_writer.write_utf_bytes(&md5_hex);

    let mut buffer = hash_writer.into_data();
    for byte in buffer[2..].iter_mut() {
        *byte ^= random;
    }

    let plaintext = BigUint::from_bytes_be(&buffer);
    let signed_data =
        rsa::hazmat::rsa_decrypt(None::<&mut rand::rngs::ThreadRng>, &private_key, &plaintext)
            .context("RSA private encryption failed")?;
    let signed_bytes = signed_data.to_bytes_be();

    let key_size = private_key.size();
    let mut padded_signed = vec![0u8; key_size];
    let offset = key_size.saturating_sub(signed_bytes.len());
    padded_signed[offset..].copy_from_slice(&signed_bytes);

    let mut writer = BigEndianWriter::new();
    writer.write_utf("AKSF");
    writer.write_short(1);
    writer.write_int(padded_signed.len() as i32);
    writer.write_bytes(&padded_signed);

    if hosts.is_some() {
        println!("{}", BASE64.encode(writer.data()));
    } else {
        fs::create_dir_all(output_path)?;
        writer.write_bytes(&data);
        fs::write(output_path.join("out.bin"), writer.data())?;
        println!("Signed file written to {}/out.bin", output_path.display());
    }

    Ok(())
}

fn patch(
    private_key_path: &PathBuf,
    swf_path: &PathBuf,
    config_path: &PathBuf,
    host: &str,
    port: &str,
    output: &PathBuf,
) -> Result<()> {
    let private_key = load_private_key(private_key_path)?;
    let public_key = private_key.to_public_key();

    fs::create_dir_all(output)?;

    // 1. Build our signature.bin (DofusPublicKey format)
    let n_hex = BigUint::from_bytes_be(&public_key.n().to_bytes_be()).to_str_radix(16);
    let e_hex = BigUint::from_bytes_be(&public_key.e().to_bytes_be()).to_str_radix(16);

    let mut sig_writer = BigEndianWriter::new();
    sig_writer.write_utf("DofusPublicKey");
    sig_writer.write_utf(&n_hex);
    sig_writer.write_utf(&e_hex);
    let signature_bin = sig_writer.into_data();

    // 2. Build our public.pem (for _verifyKey)
    let der_bytes = public_key.to_public_key_der()?;
    let b64 = BASE64.encode(der_bytes.as_bytes());
    let public_pem = format!("-----BEGIN PUBLIC KEY-----\n{b64}\n-----END PUBLIC KEY-----");
    let verify_key_bin = public_pem.into_bytes();

    // 3. Patch SWF
    println!("Patching SWF: {}", swf_path.display());
    let swf_data = fs::read(swf_path)?;
    let patched_swf = swf_patch::patch_swf(
        &swf_data,
        &signature_bin,
        &verify_key_bin,
    )?;
    let swf_out = output.join("DofusInvoker.swf");
    fs::write(&swf_out, &patched_swf)?;
    println!("  -> {}", swf_out.display());

    // 4. Patch config.xml
    println!("Patching config.xml: {}", config_path.display());
    let config_content = fs::read_to_string(config_path)?;
    let host_signature = sign_hosts(&private_key, host)?;

    let patched_config = patch_config_xml(&config_content, host, port, &host_signature)?;
    let config_out = output.join("config.xml");
    fs::write(&config_out, &patched_config)?;
    println!("  -> {}", config_out.display());

    println!("\nPatch complete. Copy the files from {} into your Dofus client.", output.display());
    Ok(())
}

fn patch_config_xml(content: &str, host: &str, port: &str, signature: &str) -> Result<String> {
    let mut result = String::new();
    for line in content.lines() {
        if line.contains("\"connection.host\"") && !line.contains("signature") {
            // Replace host
            result.push_str(&format!(
                "\t<entry key=\"connection.host\">{}</entry>\n",
                host
            ));
        } else if line.contains("\"connection.host.signature\"") {
            result.push_str(&format!(
                "\t<entry key=\"connection.host.signature\">{}</entry>\n",
                signature
            ));
        } else if line.contains("\"connection.port\"") {
            result.push_str(&format!(
                "\t<entry key=\"connection.port\">{}</entry>\n",
                port
            ));
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
        Commands::Generate { output_path } => generate(output_path),
        Commands::Sign {
            private_key_path,
            hosts,
            file,
            output_path,
        } => sign(private_key_path, hosts, file, output_path),
        Commands::Patch {
            private_key,
            swf,
            config,
            host,
            port,
            output,
        } => patch(private_key, swf, config, host, port, output),
    }
}
