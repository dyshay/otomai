# Otomai

Emulateur de serveur Dofus 2.55 ecrit en Rust, avec un toolkit complet pour lire, editer et exporter les fichiers de donnees du jeu.

## Architecture

```
crates/
  dofus-auth       Serveur d'authentification (port 5555)
  dofus-world      Serveur de jeu (port 5556)
  dofus-protocol   Protocole reseau complet (1081 messages, 327 types, 89 enums)
  dofus-network    Codec reseau + gestion des sessions
  dofus-io         I/O binaire BigEndian (serialization Dofus)
  dofus-common     Types communs, config, erreurs
  dofus-database   Couche SQLite (sqlx)
tools/
  data-reader      Lecteur/editeur de fichiers D2O, D2I, D2P + interface web
  protocol-gen     Generateur de code protocole depuis SWF ou sources AS3
  generator-rsa    Generation de cles RSA
```

## Protocole

Le protocole est genere automatiquement depuis les sources AS3 decompilees du client Dofus 2.55 via `protocol-gen`. Toutes les definitions (messages, types, enums) sont extraites avec les bons IDs et la serialisation exacte.

```bash
# Regenerer le protocole depuis les sources AS3
cargo run -p protocol-gen -- generate-from-as \
  --input /chemin/vers/decompiled-scripts/scripts \
  --output crates/dofus-protocol/src/generated/
```

## Serveur d'authentification

Le handler auth implemente le flow complet tel que decrit dans `AuthentificationFrame.as` :

1. `ProtocolRequired` → client
2. `HelloConnectMessage` (cle RSA + salt) → client
3. `IdentificationMessage` ← client (credentials chiffres RSA)
4. Dechiffrement RSA (format : `salt(32) + AES_key(32) + [cert] + username_len(1) + username + password`)
5. Verification du compte (auto-creation optionnelle en mode dev)
6. `IdentificationSuccessMessage` → client
7. `ServersListMessage` → client (multi-serveur via DB)
8. `ServerSelectionMessage` ← client
9. `SelectedServerDataMessage` (ticket + redirection) → client

### Features

- **Rate limiting** : max N tentatives/minute par IP, echecs comptent double
- **Queue de connexion** : semaphore tokio (`--max-connections`)
- **Mode maintenance** : rejette les connexions avec `IN_MAINTENANCE` (`--maintenance`)
- **Multi-serveur** : tous les serveurs en DB, statut dynamique
- **Auto-creation de comptes** : mode dev (`--auto-create`)

```bash
# Production
cargo run -p dofus-auth

# Dev (auto-creation de comptes)
cargo run -p dofus-auth -- --auto-create

# Avec limites custom
cargo run -p dofus-auth -- --auto-create --max-connections 50 --rate-limit 5
```

## Data Reader / Editor

Outil CLI + interface web pour manipuler les fichiers de donnees Dofus 2.

### Formats supportes

| Format | Description | Read | Write |
|--------|-------------|------|-------|
| **D2O** | Game Data Objects (Items, Spells, Breeds, Monsters...) | oui | oui |
| **D2I** | Internationalization (traductions) | oui | oui |
| **D2P** | Data Packs (archives de maps, gfx...) | oui | oui |

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

## Client Patcher

Patch automatique du client Dofus pour se connecter a notre serveur :

```bash
cargo run -p generator-rsa -- patch \
  --private-key keys/private.pem \
  --swf /chemin/vers/DofusInvoker.swf \
  --config /chemin/vers/config.xml \
  --host 127.0.0.1 --port 5555 \
  --output ./patched/
```

Remplace les cles RSA embarquees dans le SWF et met a jour le config.xml (host, port, signature).

## Base de donnees

PostgreSQL 16+ requis.

```bash
# Lancer avec Docker
docker compose up -d

# Ou setup manuel
createdb otomai
createuser dofus -P  # password: dofus
```

Les tables sont creees automatiquement au lancement du serveur.

Connection string par defaut : `postgresql://dofus:dofus@localhost:5432/otomai`

## Tests

```bash
# Tous les tests (70 tests, 15 suites)
cargo test --workspace

# Tests data-reader uniquement (roundtrip D2O/D2I/D2P)
cargo test -p data-reader

# Tests auth (credentials, rate limiting, maintenance)
cargo test -p dofus-auth
```

## Build

```bash
cargo build --workspace
cargo build --release -p data-reader
cargo build --release -p dofus-auth
```

## Licence

Usage prive / educatif uniquement.
