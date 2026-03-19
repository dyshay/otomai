# Otomai

Emulateur Dofus 2.57 en Rust. Protocole 2.57.1.1 (Cytrus 5).

## Architecture

```
crates/
  dofus-auth       Auth server (port 5555) — RSA, AES ticket, rate limiting
  dofus-world      World server (port 5556) — character selection, game context
  dofus-protocol   Protocole complet (1069 messages, 323 types, 100 enums)
  dofus-network    Codec TCP + sessions (tokio)
  dofus-io         Serialisation BigEndian (VarInt, VarShort, VarLong)
  dofus-common     Config, erreurs, types partages
  dofus-database   PostgreSQL (sqlx, migrations auto)
tools/
  data-reader      Lecteur/editeur D2O, D2I, D2P + interface web
  protocol-gen     Generateur de code depuis AS3 decompile
  generator-rsa    Patcher RSA client (cles, AKSF, SWF)
```

## Flow implemente

```
Auth: ProtocolRequired → HelloConnect → Identification (RSA+AES) → ServersList → Redirect
World: ProtocolRequired → HelloGame → Ticket (AES) → Capabilities → CharacterList
       → CharacterCreation/Selection → GameContext → CurrentMap → MapComplementary
```

## Lancement rapide

```bash
docker compose up -d                          # PostgreSQL
cargo run -p dofus-auth -- --auto-create &    # Auth
cargo run -p dofus-world &                    # World
```

## Patcher le client

```bash
cargo run -p generator-rsa -- patch \
  -k keys/priv.pem --sig-key keys/sig_priv.pem \
  --swf originals/DofusInvoker.swf \
  --config originals/config.xml \
  --signature-xmls originals/signature.xmls \
  --host localhost --port 5555 -o patched/
```

## Regenerer le protocole

```bash
cargo run -p protocol-gen -- generate-from-as \
  --input /path/to/decompiled-scripts/scripts \
  --output crates/dofus-protocol/src/generated/
```

Les IDs de messages se mettent a jour automatiquement via la macro `protocol_registry!`.

## Tests

```bash
cargo test --workspace    # 97 tests
```

## Licence

Usage prive / educatif uniquement.
