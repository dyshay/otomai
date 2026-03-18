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
  dofus-database   Couche PostgreSQL (sqlx)
tools/
  data-reader      Lecteur/editeur de fichiers D2O, D2I, D2P + interface web
  protocol-gen     Generateur de code protocole depuis SWF ou sources AS3
  generator-rsa    Patcher RSA : generation de cles, signing AKSF, patching client
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

Architecture RSA a deux niveaux (fidele a la reference hetwanmod) :

- **Cle de signature** (2048-bit, `sig_priv.pem`) : signe la cle de session au demarrage
- **Cle de session** (1024-bit, ephemere) : generee une fois au startup, chiffre/dechiffre les credentials

Le handler auth implemente le flow complet tel que decrit dans `AuthentificationFrame.as` :

1. `ProtocolRequired` → client
2. `HelloConnectMessage` (session key DER signee PKCS1 + salt) → client
3. `IdentificationMessage` ← client (credentials chiffres RSA textbook avec session key)
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

## Client Patcher (generator-rsa)

Architecture deux cles alignee sur la reference hetwanmod :

| Cle | Taille | Fichier | Usage |
|-----|--------|---------|-------|
| Patcher | 1024-bit | `priv.pem` | AKSF signing + `SIGNATURE_KEY_DATA` dans SWF |
| Signature | 2048-bit | `sig_priv.pem` | Auth session signing + `_verifyKey` dans SWF |

Le SWF Dofus embarque 3 assets cles : `SIGNATURE_KEY_DATA` (1024-bit, verification AKSF V1), `_verifyKey` (2048-bit, verification auth session), `PUBLIC_KEY_V2` (2048-bit, verification AKSF V2).

```bash
# 1. Generer les deux paires de cles
cargo run -p generator-rsa -- gen -o keys/

# 2. Signer un host (AKSF, base64)
cargo run -p generator-rsa -- sign keys/priv.pem --hosts "localhost"

# 3. Signer un fichier (AKSF binaire)
cargo run -p generator-rsa -- sign keys/priv.pem --file signature.xml -o output/

# 4. Generer les cles de session auth
cargo run -p generator-rsa -- auth-keys -k keys/sig_priv.pem -o keys/auth/

# 5. Patcher le client complet (SWF + config.xml + signature.xmls)
cargo run -p generator-rsa -- patch \
  -k keys/priv.pem \
  --sig-key keys/sig_priv.pem \
  --swf originals/DofusInvoker.swf \
  --config originals/config.xml \
  --signature-xmls originals/signature.xmls \
  --host localhost --port 5555 \
  -o patched/
```

Le patcher strip le header AKSF d'Ankama sur `signature.xmls`, retire les entrees pour fichiers manquants/vides, et re-signe avec notre cle.

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
