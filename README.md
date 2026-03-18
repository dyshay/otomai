# Otomai

Emulateur Dofus 2.55 en Rust.

## Architecture

```
crates/
  dofus-auth       Auth server (port 5555)
  dofus-world      World server (port 5556)
  dofus-protocol   Protocole complet (1081 messages, 327 types, 89 enums)
  dofus-network    Codec TCP + sessions
  dofus-io         Serialisation BigEndian
  dofus-common     Config, erreurs, types partages
  dofus-database   PostgreSQL (sqlx, migrations auto)
tools/
  data-reader      Lecteur/editeur D2O, D2I, D2P + interface web
  protocol-gen     Generateur de code depuis AS3
  generator-rsa    Patcher RSA client (cles, AKSF, SWF)
```

## Lancement rapide

```bash
docker compose up -d                          # PostgreSQL
cargo run -p dofus-auth -- --auto-create &    # Auth (auto-creation comptes)
cargo run -p dofus-world &                    # World
```

## Patcher le client

```bash
cargo run -p generator-rsa -- gen -o keys/
cargo run -p generator-rsa -- patch \
  -k keys/priv.pem --sig-key keys/sig_priv.pem \
  --swf originals/DofusInvoker.swf \
  --config originals/config.xml \
  --signature-xmls originals/signature.xmls \
  --host localhost --port 5555 -o patched/
```

## Tests

```bash
cargo test --workspace
```

## Licence

Usage prive / educatif uniquement.
