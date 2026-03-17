mod abc;
mod as3_parser;
mod codegen;
mod extractor;
mod swf;

use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "protocol-gen", about = "Extract Dofus protocol from SWF and generate Rust code")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Extract protocol from DofusInvoker.swf and generate Rust code
    Generate {
        /// Path to DofusInvoker.swf
        #[arg(short, long)]
        input: PathBuf,

        /// Output directory for generated Rust code
        #[arg(short, long, default_value = "crates/dofus-protocol/src/generated")]
        output: PathBuf,
    },
    /// Generate from decompiled .as files (more reliable than SWF parsing)
    GenerateFromAs {
        /// Path to decompiled scripts directory (containing com/ankamagames/...)
        #[arg(short, long)]
        input: PathBuf,

        /// Output directory for generated Rust code
        #[arg(short, long, default_value = "crates/dofus-protocol/src/generated")]
        output: PathBuf,
    },
    /// Inspect SWF/ABC structure (debug/exploration)
    Inspect {
        /// Path to DofusInvoker.swf
        #[arg(short, long)]
        input: PathBuf,

        /// Filter classes by name substring
        #[arg(short, long)]
        filter: Option<String>,
    },
}

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,protocol_gen=debug".into()),
        )
        .init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Generate { input, output } => {
            tracing::info!(input = %input.display(), output = %output.display(), "Starting protocol generation");

            let raw = std::fs::read(&input)?;
            let swf_file = swf::parse_swf(&raw)?;

            let mut all_classes = Vec::new();
            for block in &swf_file.abc_blocks {
                tracing::info!(name = %block.name, size = block.data.len(), "Parsing ABC block");
                let abc_file = abc::parse_abc(&block.data)?;
                let mut classes = extractor::extract_protocol(&abc_file);
                all_classes.append(&mut classes);
            }

            tracing::info!(total = all_classes.len(), "Total protocol classes extracted");

            let messages = all_classes.iter().filter(|c| c.is_message).count();
            let types = all_classes.iter().filter(|c| !c.is_message).count();
            tracing::info!(messages, types, "Breakdown");

            codegen::generate(&all_classes, &[], &[], &output)?;

            println!(
                "Generated {} messages + {} types in {}",
                messages,
                types,
                output.display()
            );
            Ok(())
        }
        Commands::GenerateFromAs { input, output } => {
            tracing::info!(input = %input.display(), output = %output.display(), "Generating from decompiled .as files");

            let all_classes = as3_parser::parse_protocol_dir(&input)?;
            let enums = as3_parser::parse_enums(&input)?;
            let hierarchies = as3_parser::build_type_hierarchies(&all_classes);

            let messages = all_classes.iter().filter(|c| c.is_message).count();
            let types = all_classes.iter().filter(|c| !c.is_message).count();
            tracing::info!(messages, types, enums = enums.len(), hierarchies = hierarchies.len(), "Extraction complete");

            codegen::generate(&all_classes, &enums, &hierarchies, &output)?;

            println!(
                "Generated {} messages + {} types + {} enums + {} polymorphic hierarchies in {}",
                messages, types, enums.len(), hierarchies.len(), output.display()
            );
            Ok(())
        }
        Commands::Inspect { input, filter } => {
            tracing::info!(input = %input.display(), "Inspecting SWF");

            let raw = std::fs::read(&input)?;
            let swf_file = swf::parse_swf(&raw)?;

            for block in &swf_file.abc_blocks {
                tracing::info!(name = %block.name, size = block.data.len(), "Parsing ABC block");
                let abc_file = abc::parse_abc(&block.data)?;

                let cp = &abc_file.constant_pool;

                println!("\n=== ABC Block: {} ===", block.name);
                println!(
                    "Version: {}.{}",
                    abc_file.major_version, abc_file.minor_version
                );
                println!(
                    "Constants: {} ints, {} uints, {} doubles, {} strings, {} namespaces, {} multinames",
                    cp.integers.len(),
                    cp.uintegers.len(),
                    cp.doubles.len(),
                    cp.strings.len(),
                    cp.namespaces.len(),
                    cp.multinames.len()
                );
                println!(
                    "Methods: {}, Classes: {}, Method bodies: {}",
                    abc_file.methods.len(),
                    abc_file.instances.len(),
                    abc_file.method_bodies.len()
                );

                println!("\n--- Classes ---");
                for (i, inst) in abc_file.instances.iter().enumerate() {
                    let name = cp.multiname_name(inst.name);
                    let full = cp.multiname_full(inst.name);
                    let parent = cp.multiname_name(inst.super_name);

                    if let Some(ref f) = filter {
                        if !name.contains(f.as_str()) && !full.contains(f.as_str()) {
                            continue;
                        }
                    }

                    println!("\n  [{}] {} (extends {})", i, full, parent);

                    // Instance traits
                    for t in &inst.traits {
                        let tname = cp.multiname_name(t.name);
                        match &t.data {
                            abc::TraitData::Slot { type_name, .. }
                            | abc::TraitData::Const { type_name, .. } => {
                                let tn = cp.multiname_name(*type_name);
                                println!("    slot: {} : {}", tname, tn);
                            }
                            abc::TraitData::Method { method, .. } => {
                                println!("    method: {} (method#{})", tname, method);
                            }
                            abc::TraitData::Getter { method, .. } => {
                                println!("    getter: {} (method#{})", tname, method);
                            }
                            abc::TraitData::Setter { method, .. } => {
                                println!("    setter: {} (method#{})", tname, method);
                            }
                            _ => {
                                println!("    trait: {} (kind={})", tname, t.kind);
                            }
                        }
                    }

                    // Static traits
                    let cls = &abc_file.classes[i];
                    for t in &cls.traits {
                        let tname = cp.multiname_name(t.name);
                        match &t.data {
                            abc::TraitData::Const { vindex, vkind, .. }
                            | abc::TraitData::Slot { vindex, vkind, .. } => {
                                let val = match vkind {
                                    0x03 => cp
                                        .integers
                                        .get(*vindex as usize)
                                        .map(|v| format!("{}", v)),
                                    0x04 => cp
                                        .uintegers
                                        .get(*vindex as usize)
                                        .map(|v| format!("{}", v)),
                                    0x06 => cp
                                        .strings
                                        .get(*vindex as usize)
                                        .map(|v| format!("\"{}\"", v)),
                                    _ => Some(format!(
                                        "(vkind=0x{:02x}, idx={})",
                                        vkind, vindex
                                    )),
                                };
                                println!(
                                    "    static: {} = {}",
                                    tname,
                                    val.unwrap_or_default()
                                );
                            }
                            _ => {
                                println!("    static: {} (kind={})", tname, t.kind);
                            }
                        }
                    }
                }

                // Extract protocol classes
                let classes = extractor::extract_protocol(&abc_file);
                println!("\n--- Protocol classes: {} ---", classes.len());
                for cls in &classes {
                    println!(
                        "  {} {} (id={}, fields={})",
                        if cls.is_message { "MSG" } else { "TYPE" },
                        cls.name,
                        cls.protocol_id,
                        cls.fields.len(),
                    );
                    for f in &cls.fields {
                        println!(
                            "    {} : {:?} (via {})",
                            f.name, f.field_type, f.write_method
                        );
                    }
                }
            }

            Ok(())
        }
    }
}
