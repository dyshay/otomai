# Otomai

Emulateur de serveur Dofus 2 ecrit en Rust, avec un toolkit complet pour lire, editer et exporter les fichiers de donnees du jeu.

## Architecture

```
crates/
  dofus-auth       Serveur d'authentification (port 5555)
  dofus-world      Serveur de jeu (port 5556)
  dofus-protocol   Definitions du protocole reseau
  dofus-network    Codec reseau + gestion des sessions
  dofus-io         I/O binaire BigEndian (serialization Dofus)
  dofus-common     Types communs, config, erreurs
  dofus-database   Couche SQLite (sqlx)
tools/
  data-reader      Lecteur/editeur de fichiers D2O, D2I, D2P
  protocol-gen     Generateur de code protocole depuis les SWF
  generator-rsa    Generation de cles RSA
```

## Data Reader / Editor

Outil CLI + interface web pour manipuler les fichiers de donnees Dofus 2.

### Formats supportes

| Format | Description | Read | Write |
|--------|-------------|------|-------|
| **D2O** | Game Data Objects (Items, Spells, Breeds, Monsters...) | oui | oui |
| **D2I** | Internationalization (traductions) | oui | - |
| **D2P** | Data Packs (archives de maps, gfx...) | oui | - |

### Interface Web

```bash
cargo run -p data-reader -- serve --data-dir /chemin/vers/dofus/Resources --port 8080
```

Ouvre `http://localhost:8080` pour :
- Naviguer dans tous les fichiers D2O/D2I/D2P
- Rechercher et filtrer les objets
- Editer les objets D2O en JSON
- Sauvegarder les modifications (backup automatique `.bak`)

### CLI

```bash
# Lire un D2O en JSON
cargo run -p data-reader -- d2o -i data/common/Items.d2o -o items.json

# Schema des classes
cargo run -p data-reader -- d2o -i data/common/Items.d2o --schema

# Traduction par ID
cargo run -p data-reader -- d2i -i data/i18n/i18n_fr.d2i --id 1

# Lister les fichiers d'une archive D2P
cargo run -p data-reader -- d2p -i content/maps/maps0.d2p

# Extraire une archive D2P
cargo run -p data-reader -- d2p -i content/maps/maps0.d2p --extract ./maps/

# Export batch de tous les D2O
cargo run -p data-reader -- export-all -i data/common/ -o ./export/
```

## Serveur

### Configuration

```bash
# Auth server
cargo run -p dofus-auth

# World server
cargo run -p dofus-world
```

Les fichiers de configuration sont dans `config/auth.toml` et `config/world.toml`.

## Build

```bash
# Tout compiler
cargo build --workspace

# Juste le data reader
cargo build -p data-reader

# Release
cargo build --release -p data-reader
```

## Licence

Usage prive / educatif uniquement.
