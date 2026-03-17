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
#[command(name = "generator-rsa", about = "Dofus RSA key generator and signer")]
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
}

fn generate(output_path: &PathBuf) -> Result<()> {
    fs::create_dir_all(output_path)?;

    let mut rng = rand::thread_rng();
    let private_key =
        RsaPrivateKey::new(&mut rng, 2048).context("Failed to generate RSA keypair")?;
    let public_key = private_key.to_public_key();

    // Extract n and e as hex strings
    let n_hex = BigUint::from_bytes_be(&public_key.n().to_bytes_be()).to_str_radix(16);
    let e_hex = BigUint::from_bytes_be(&public_key.e().to_bytes_be()).to_str_radix(16);

    // Write signature.bin: "DofusPublicKey" + n + e (as UTF strings)
    let mut writer = BigEndianWriter::new();
    writer.write_utf("DofusPublicKey");
    writer.write_utf(&n_hex);
    writer.write_utf(&e_hex);
    fs::write(output_path.join("signature.bin"), writer.data())?;

    // Write public.pem
    let der_bytes = public_key
        .to_public_key_der()
        .context("Failed to encode public key to DER")?;
    let b64 = BASE64.encode(der_bytes.as_bytes());
    let public_pem = format!("-----BEGIN PUBLIC KEY-----\n{b64}\n-----END PUBLIC KEY-----");
    fs::write(output_path.join("public.pem"), &public_pem)?;

    // Write private.pem (PKCS1)
    let private_pem = private_key
        .to_pkcs1_pem(pkcs1::LineEnding::LF)
        .context("Failed to encode private key to PEM")?;
    fs::write(output_path.join("private.pem"), private_pem.as_bytes())?;

    println!("Keys generated in {}", output_path.display());
    Ok(())
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

    let pem_content =
        fs::read_to_string(private_key_path).context("Failed to read private key file")?;
    let private_key = RsaPrivateKey::from_pkcs1_pem(&pem_content)
        .or_else(|_| {
            use pkcs8::DecodePrivateKey;
            RsaPrivateKey::from_pkcs8_pem(&pem_content)
        })
        .context("Failed to parse private key")?;

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
    }
}
