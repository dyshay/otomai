mod d2i;
mod d2i_writer;
mod d2o;
mod d2o_writer;
mod d2p;
mod d2p_writer;
mod serve;

#[cfg(test)]
mod tests;

use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "dofrust-data", about = "Otomai — Lire, editer et exporter les fichiers Dofus 2 (D2O, D2I, D2P)")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Lancer l'editeur web
    Serve {
        /// Dossier contenant les fichiers de donnees (data/, content/, etc.)
        #[arg(short = 'd', long)]
        data_dir: PathBuf,

        /// Port du serveur web
        #[arg(short, long, default_value_t = 8080)]
        port: u16,
    },

    /// Lire un fichier D2O et exporter en JSON
    D2o {
        #[arg(short, long)]
        input: PathBuf,
        #[arg(short, long)]
        output: Option<PathBuf>,
        #[arg(long)]
        schema: bool,
        #[arg(long, default_value_t = true)]
        pretty: bool,
    },

    /// Lire un fichier D2I et exporter les traductions en JSON
    D2i {
        #[arg(short, long)]
        input: PathBuf,
        #[arg(short, long)]
        output: Option<PathBuf>,
        #[arg(long)]
        id: Option<i32>,
        #[arg(long)]
        name: Option<String>,
    },

    /// Lire une archive D2P et lister/extraire les fichiers
    D2p {
        #[arg(short, long)]
        input: PathBuf,
        #[arg(short, long)]
        extract: Option<PathBuf>,
        #[arg(long)]
        file: Option<String>,
        #[arg(short, long)]
        output: Option<PathBuf>,
    },

    /// Export batch de tous les D2O d'un dossier en JSON
    ExportAll {
        #[arg(short, long)]
        input: PathBuf,
        #[arg(short, long)]
        output: PathBuf,
    },

    /// Importer les fichiers D2O dans PostgreSQL
    ImportDb {
        /// Dossier contenant les fichiers D2O (data/common/)
        #[arg(short, long)]
        input: PathBuf,

        /// URL PostgreSQL
        #[arg(long, default_value = "postgresql://dofus:dofus@localhost:5432/otomai")]
        db: String,

        /// Fichier specifique a importer (sinon tous les .d2o)
        #[arg(long)]
        file: Option<String>,
    },

    /// Exporter des donnees de la DB vers un fichier D2O
    ExportDb {
        /// Nom du fichier source (ex: Items.d2o)
        #[arg(long)]
        file: String,

        /// Fichier D2O original (pour les class definitions)
        #[arg(short, long)]
        template: PathBuf,

        /// Fichier D2O de sortie
        #[arg(short, long)]
        output: PathBuf,

        /// URL PostgreSQL
        #[arg(long, default_value = "postgresql://dofus:dofus@localhost:5432/otomai")]
        db: String,
    },
}

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        )
        .init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Serve { data_dir, port } => {
            let rt = tokio::runtime::Runtime::new()?;
            rt.block_on(serve::run(data_dir, port))?;
        }

        Commands::D2o { input, output, schema, pretty } => {
            let reader = d2o::D2OReader::open(&input)?;

            if schema {
                let classes: Vec<_> = reader.classes().values().collect();
                let json = if pretty {
                    serde_json::to_string_pretty(&serde_json::json!({
                        "classes": classes.iter().map(|c| serde_json::json!({
                            "id": c.class_id,
                            "name": &c.name,
                            "package": &c.package,
                            "fields": c.fields.iter().map(|f| serde_json::json!({
                                "name": &f.name,
                                "type": format!("{:?}", f.field_type),
                            })).collect::<Vec<_>>(),
                        })).collect::<Vec<_>>(),
                    }))?
                } else {
                    serde_json::to_string(&serde_json::json!({"class_count": classes.len()}))?
                };
                write_output(&json, output.as_deref())?;
            } else {
                let objects = reader.read_all_objects()?;
                let json = if pretty {
                    serde_json::to_string_pretty(&objects)?
                } else {
                    serde_json::to_string(&objects)?
                };
                write_output(&json, output.as_deref())?;
                eprintln!("Exported {} objects from {}", objects.len(), input.display());
            }
        }

        Commands::D2i { input, output, id, name } => {
            let reader = d2i::D2IReader::open(&input)?;

            if let Some(id) = id {
                let text = reader.get_text(id)?;
                println!("{}", text);
            } else if let Some(name) = name {
                let text = reader.get_named_text(&name)?;
                println!("{}", text);
            } else {
                let texts = reader.all_texts()?;
                let json = serde_json::to_string_pretty(&texts)?;
                write_output(&json, output.as_deref())?;
                eprintln!("Exported {} texts from {}", texts.len(), input.display());
            }
        }

        Commands::D2p { input, extract, file, output } => {
            let reader = d2p::D2PReader::open(&input)?;

            if let Some(extract_dir) = extract {
                let count = reader.extract_all(&extract_dir)?;
                eprintln!("Extracted {} files to {}", count, extract_dir.display());
            } else if let Some(filename) = file {
                let data = reader.read_file(&filename)?;
                if let Some(out) = output {
                    std::fs::write(&out, &data)?;
                    eprintln!("Extracted {} ({} bytes)", filename, data.len());
                } else {
                    std::io::Write::write_all(&mut std::io::stdout(), &data)?;
                }
            } else {
                let files = reader.filenames();
                println!("{} files in archive:", files.len());
                for f in &files {
                    println!("  {}", f);
                }
                if let Some(props) = reader.properties().get("contentOffset") {
                    println!("\nContent offset: {}", props);
                }
            }
        }

        Commands::ImportDb { input, db, file } => {
            let rt = tokio::runtime::Runtime::new()?;
            rt.block_on(async {
                let pool = dofus_database::create_pool(&db).await?;
                dofus_database::run_migrations(&pool).await?;

                let files_to_import: Vec<std::path::PathBuf> = if let Some(name) = &file {
                    vec![input.join(name)]
                } else {
                    let mut files = Vec::new();
                    for entry in std::fs::read_dir(&input)? {
                        let path = entry?.path();
                        if path.extension().map(|e| e == "d2o").unwrap_or(false) {
                            files.push(path);
                        }
                    }
                    files.sort();
                    files
                };

                let mut total = 0usize;
                for path in &files_to_import {
                    let stem = path.file_name().unwrap().to_string_lossy().to_string();
                    match d2o::D2OReader::open(path) {
                        Ok(reader) => {
                            let ids = reader.object_ids();
                            let mut count = 0;
                            for id in &ids {
                                match reader.read_object(*id) {
                                    Ok(obj) => {
                                        let class_name = obj.get("_class")
                                            .and_then(|v| v.as_str())
                                            .unwrap_or("Unknown")
                                            .to_string();
                                        dofus_database::repository::upsert_game_data(
                                            &pool, &stem, *id, &class_name, &obj,
                                        ).await?;
                                        count += 1;
                                    }
                                    Err(e) => eprintln!("  {} #{} -> ERROR: {}", stem, id, e),
                                }
                            }
                            eprintln!("  {} -> {} objets importes", stem, count);
                            total += count;
                        }
                        Err(e) => eprintln!("  {} -> ERROR: {}", stem, e),
                    }
                }

                eprintln!("\nTotal: {} objets importes dans la DB", total);
                Ok::<_, anyhow::Error>(())
            })?;
        }

        Commands::ExportDb { file, template, output, db } => {
            let rt = tokio::runtime::Runtime::new()?;
            rt.block_on(async {
                let pool = dofus_database::create_pool(&db).await?;

                // Load class definitions from the template D2O
                let template_reader = d2o::D2OReader::open(&template)?;
                let classes = template_reader.classes().clone();

                // Fetch all objects from DB
                let rows = dofus_database::repository::get_all_game_data(&pool, &file).await?;

                if rows.is_empty() {
                    anyhow::bail!("Aucune donnee trouvee pour '{}' dans la DB", file);
                }

                let objects: Vec<(i32, serde_json::Value)> = rows
                    .into_iter()
                    .map(|r| (r.object_id, r.data))
                    .collect();

                let bytes = d2o_writer::write_d2o(&classes, &objects)?;
                std::fs::write(&output, &bytes)?;

                eprintln!("Exporte {} objets -> {} ({} bytes)",
                    objects.len(), output.display(), bytes.len());
                Ok::<_, anyhow::Error>(())
            })?;
        }

        Commands::ExportAll { input, output } => {
            std::fs::create_dir_all(&output)?;
            let mut total = 0;

            for entry in std::fs::read_dir(&input)? {
                let entry = entry?;
                let path = entry.path();
                if path.extension().map(|e| e == "d2o").unwrap_or(false) {
                    let stem = path.file_stem().unwrap().to_string_lossy();
                    let out_path = output.join(format!("{}.json", stem));

                    match d2o::D2OReader::open(&path) {
                        Ok(reader) => {
                            match reader.read_all_objects() {
                                Ok(objects) => {
                                    let json = serde_json::to_string_pretty(&objects)?;
                                    std::fs::write(&out_path, json)?;
                                    eprintln!("  {} -> {} objects", stem, objects.len());
                                    total += objects.len();
                                }
                                Err(e) => eprintln!("  {} -> ERROR: {}", stem, e),
                            }
                        }
                        Err(e) => eprintln!("  {} -> ERROR: {}", stem, e),
                    }
                }
            }

            eprintln!("\nTotal: {} objects exported to {}", total, output.display());
        }
    }

    Ok(())
}

fn write_output(content: &str, path: Option<&std::path::Path>) -> anyhow::Result<()> {
    if let Some(path) = path {
        std::fs::write(path, content)?;
    } else {
        println!("{}", content);
    }
    Ok(())
}
