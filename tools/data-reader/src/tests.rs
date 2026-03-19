//! Roundtrip tests for D2O, D2I, and D2P readers/writers.

mod d2o_tests {
    use crate::d2o::*;
    use crate::d2o_writer;
    use serde_json::json;
    use std::collections::HashMap;

    fn sample_classes() -> HashMap<i32, D2OClassDef> {
        let mut classes = HashMap::new();
        classes.insert(
            1,
            D2OClassDef {
                class_id: 1,
                name: "Item".to_string(),
                package: "com.ankamagames.dofus.datacenter.items".to_string(),
                fields: vec![
                    D2OFieldDef {
                        name: "id".to_string(),
                        field_type: D2OFieldType::Int,
                    },
                    D2OFieldDef {
                        name: "name".to_string(),
                        field_type: D2OFieldType::String,
                    },
                    D2OFieldDef {
                        name: "level".to_string(),
                        field_type: D2OFieldType::UInt,
                    },
                    D2OFieldDef {
                        name: "weight".to_string(),
                        field_type: D2OFieldType::Number,
                    },
                    D2OFieldDef {
                        name: "usable".to_string(),
                        field_type: D2OFieldType::Bool,
                    },
                    D2OFieldDef {
                        name: "nameId".to_string(),
                        field_type: D2OFieldType::I18n,
                    },
                ],
            },
        );
        classes
    }

    #[test]
    fn d2o_roundtrip_simple() {
        let classes = sample_classes();
        let objects = vec![
            (
                100,
                json!({
                    "_class": "Item",
                    "id": 100,
                    "name": "Epée de Bois",
                    "level": 1,
                    "weight": 3.5,
                    "usable": true,
                    "nameId": 42,
                }),
            ),
            (
                101,
                json!({
                    "_class": "Item",
                    "id": 101,
                    "name": "Bouclier en Cuir",
                    "level": 5,
                    "weight": 10.0,
                    "usable": false,
                    "nameId": 43,
                }),
            ),
        ];

        // Write
        let bytes = d2o_writer::write_d2o(&classes, &objects).expect("write_d2o failed");

        // Verify magic
        assert_eq!(&bytes[0..3], b"D2O");

        // Read back
        let reader = D2OReader::from_bytes(bytes).expect("from_bytes failed");

        // Verify class definitions survived
        assert_eq!(reader.classes().len(), 1);
        let cls = reader.classes().get(&1).unwrap();
        assert_eq!(cls.name, "Item");
        assert_eq!(cls.fields.len(), 6);

        // Verify object IDs
        let ids = reader.object_ids();
        assert_eq!(ids.len(), 2);

        // Verify object data
        let obj0 = reader.read_object(100).expect("read object 100");
        assert_eq!(obj0["id"], 100);
        assert_eq!(obj0["name"], "Epée de Bois");
        assert_eq!(obj0["level"], 1);
        assert_eq!(obj0["weight"], 3.5);
        assert_eq!(obj0["usable"], true);
        assert_eq!(obj0["nameId"], 42);

        let obj1 = reader.read_object(101).expect("read object 101");
        assert_eq!(obj1["id"], 101);
        assert_eq!(obj1["name"], "Bouclier en Cuir");
        assert_eq!(obj1["level"], 5);
        assert_eq!(obj1["usable"], false);
    }

    #[test]
    fn d2o_roundtrip_with_vectors() {
        let mut classes = HashMap::new();
        classes.insert(
            10,
            D2OClassDef {
                class_id: 10,
                name: "Spell".to_string(),
                package: "com.ankamagames.dofus.datacenter.spells".to_string(),
                fields: vec![
                    D2OFieldDef {
                        name: "id".to_string(),
                        field_type: D2OFieldType::Int,
                    },
                    D2OFieldDef {
                        name: "levels".to_string(),
                        field_type: D2OFieldType::Vector(
                            "int".to_string(),
                            Box::new(D2OFieldType::Int),
                        ),
                    },
                    D2OFieldDef {
                        name: "tags".to_string(),
                        field_type: D2OFieldType::Vector(
                            "String".to_string(),
                            Box::new(D2OFieldType::String),
                        ),
                    },
                ],
            },
        );

        let objects = vec![(
            1,
            json!({
                "_class": "Spell",
                "id": 42,
                "levels": [1, 2, 3, 4, 5, 6],
                "tags": ["fire", "damage", "aoe"],
            }),
        )];

        let bytes = d2o_writer::write_d2o(&classes, &objects).expect("write");
        let reader = D2OReader::from_bytes(bytes).expect("read");

        let obj = reader.read_object(1).expect("read object 1");
        assert_eq!(obj["id"], 42);
        assert_eq!(obj["levels"], json!([1, 2, 3, 4, 5, 6]));
        assert_eq!(obj["tags"], json!(["fire", "damage", "aoe"]));
    }

    #[test]
    fn d2o_roundtrip_with_nested_objects() {
        let mut classes = HashMap::new();
        classes.insert(
            20,
            D2OClassDef {
                class_id: 20,
                name: "Monster".to_string(),
                package: "com.ankamagames.dofus.datacenter.monsters".to_string(),
                fields: vec![
                    D2OFieldDef {
                        name: "id".to_string(),
                        field_type: D2OFieldType::Int,
                    },
                    D2OFieldDef {
                        name: "stats".to_string(),
                        field_type: D2OFieldType::Object(21),
                    },
                ],
            },
        );
        classes.insert(
            21,
            D2OClassDef {
                class_id: 21,
                name: "MonsterStats".to_string(),
                package: "com.ankamagames.dofus.datacenter.monsters".to_string(),
                fields: vec![
                    D2OFieldDef {
                        name: "hp".to_string(),
                        field_type: D2OFieldType::Int,
                    },
                    D2OFieldDef {
                        name: "xp".to_string(),
                        field_type: D2OFieldType::UInt,
                    },
                ],
            },
        );

        let objects = vec![(
            1,
            json!({
                "_class": "Monster",
                "id": 7,
                "stats": {
                    "_class": "MonsterStats",
                    "hp": 500,
                    "xp": 1200,
                },
            }),
        )];

        let bytes = d2o_writer::write_d2o(&classes, &objects).expect("write");
        let reader = D2OReader::from_bytes(bytes).expect("read");

        let obj = reader.read_object(1).expect("read object 1");
        assert_eq!(obj["id"], 7);
        assert_eq!(obj["stats"]["_class"], "MonsterStats");
        assert_eq!(obj["stats"]["hp"], 500);
        assert_eq!(obj["stats"]["xp"], 1200);
    }

    #[test]
    fn d2o_roundtrip_null_object() {
        let mut classes = HashMap::new();
        classes.insert(
            30,
            D2OClassDef {
                class_id: 30,
                name: "QuestStep".to_string(),
                package: "com.ankamagames.dofus.datacenter.quest".to_string(),
                fields: vec![
                    D2OFieldDef {
                        name: "id".to_string(),
                        field_type: D2OFieldType::Int,
                    },
                    D2OFieldDef {
                        name: "reward".to_string(),
                        field_type: D2OFieldType::Object(31),
                    },
                ],
            },
        );
        classes.insert(
            31,
            D2OClassDef {
                class_id: 31,
                name: "Reward".to_string(),
                package: "com.ankamagames.dofus.datacenter.quest".to_string(),
                fields: vec![D2OFieldDef {
                    name: "kamas".to_string(),
                    field_type: D2OFieldType::Int,
                }],
            },
        );

        let objects = vec![(
            1,
            json!({
                "_class": "QuestStep",
                "id": 99,
                "reward": null,
            }),
        )];

        let bytes = d2o_writer::write_d2o(&classes, &objects).expect("write");
        let reader = D2OReader::from_bytes(bytes).expect("read");

        let obj = reader.read_object(1).expect("read object 1");
        assert_eq!(obj["id"], 99);
        assert!(obj["reward"].is_null());
    }

    #[test]
    fn d2o_roundtrip_empty_strings() {
        let mut classes = HashMap::new();
        classes.insert(
            40,
            D2OClassDef {
                class_id: 40,
                name: "Simple".to_string(),
                package: "test".to_string(),
                fields: vec![D2OFieldDef {
                    name: "text".to_string(),
                    field_type: D2OFieldType::String,
                }],
            },
        );

        let objects = vec![(
            1,
            json!({
                "_class": "Simple",
                "text": "",
            }),
        )];

        let bytes = d2o_writer::write_d2o(&classes, &objects).expect("write");
        let reader = D2OReader::from_bytes(bytes).expect("read");

        let obj = reader.read_object(1).expect("read");
        assert_eq!(obj["text"], "");
    }

    #[test]
    fn d2o_roundtrip_many_objects() {
        let mut classes = HashMap::new();
        classes.insert(
            50,
            D2OClassDef {
                class_id: 50,
                name: "Entry".to_string(),
                package: "test".to_string(),
                fields: vec![
                    D2OFieldDef {
                        name: "id".to_string(),
                        field_type: D2OFieldType::Int,
                    },
                    D2OFieldDef {
                        name: "value".to_string(),
                        field_type: D2OFieldType::Int,
                    },
                ],
            },
        );

        let objects: Vec<(i32, serde_json::Value)> = (0..100)
            .map(|i| {
                (
                    i,
                    json!({
                        "_class": "Entry",
                        "id": i,
                        "value": i * i,
                    }),
                )
            })
            .collect();

        let bytes = d2o_writer::write_d2o(&classes, &objects).expect("write");
        let reader = D2OReader::from_bytes(bytes).expect("read");

        assert_eq!(reader.object_ids().len(), 100);

        for i in 0..100 {
            let obj = reader.read_object(i).expect("read");
            assert_eq!(obj["id"], i);
            assert_eq!(obj["value"], i * i);
        }
    }
}

mod d2i_tests {
    use crate::d2i::D2IReader;
    use crate::d2i_writer;
    use std::collections::HashMap;

    #[test]
    fn d2i_roundtrip_simple() {
        let mut texts = HashMap::new();
        texts.insert(1, "Iop".to_string());
        texts.insert(2, "Cra".to_string());
        texts.insert(3, "Eniripsa".to_string());
        texts.insert(100, "Épée de bois flotté".to_string());
        texts.insert(200, "Bouclier du Bouftou".to_string());

        let undiacritical = HashMap::new();
        let named_texts = HashMap::new();

        let bytes =
            d2i_writer::write_d2i(&texts, &undiacritical, &named_texts).expect("write_d2i");
        let reader = D2IReader::from_bytes(bytes).expect("from_bytes");

        assert_eq!(reader.len(), 5);

        assert_eq!(reader.get_text(1).unwrap(), "Iop");
        assert_eq!(reader.get_text(2).unwrap(), "Cra");
        assert_eq!(reader.get_text(3).unwrap(), "Eniripsa");
        assert_eq!(reader.get_text(100).unwrap(), "Épée de bois flotté");
        assert_eq!(reader.get_text(200).unwrap(), "Bouclier du Bouftou");
    }

    #[test]
    fn d2i_roundtrip_with_diacritical() {
        let mut texts = HashMap::new();
        texts.insert(1, "Épée légendaire".to_string());
        texts.insert(2, "Château".to_string());

        let mut undiacritical = HashMap::new();
        undiacritical.insert(1, "Epee legendaire".to_string());
        undiacritical.insert(2, "Chateau".to_string());

        let named_texts = HashMap::new();

        let bytes =
            d2i_writer::write_d2i(&texts, &undiacritical, &named_texts).expect("write_d2i");
        let reader = D2IReader::from_bytes(bytes).expect("from_bytes");

        assert_eq!(reader.get_text(1).unwrap(), "Épée légendaire");
        assert_eq!(
            reader.get_undiacritical_text(1).unwrap(),
            Some("Epee legendaire".to_string())
        );
        assert_eq!(reader.get_text(2).unwrap(), "Château");
        assert_eq!(
            reader.get_undiacritical_text(2).unwrap(),
            Some("Chateau".to_string())
        );
    }

    #[test]
    fn d2i_roundtrip_with_named_texts() {
        let texts = HashMap::new();
        let undiacritical = HashMap::new();

        let mut named_texts = HashMap::new();
        named_texts.insert("ui.common.yes".to_string(), "Oui".to_string());
        named_texts.insert("ui.common.no".to_string(), "Non".to_string());
        named_texts.insert(
            "ui.fight.challenge".to_string(),
            "Défi".to_string(),
        );

        let bytes =
            d2i_writer::write_d2i(&texts, &undiacritical, &named_texts).expect("write_d2i");
        let reader = D2IReader::from_bytes(bytes).expect("from_bytes");

        assert_eq!(reader.get_named_text("ui.common.yes").unwrap(), "Oui");
        assert_eq!(reader.get_named_text("ui.common.no").unwrap(), "Non");
        assert_eq!(reader.get_named_text("ui.fight.challenge").unwrap(), "Défi");
    }

    #[test]
    fn d2i_roundtrip_all_texts() {
        let mut texts = HashMap::new();
        for i in 0..50 {
            texts.insert(i, format!("Text #{}", i));
        }

        let bytes =
            d2i_writer::write_d2i(&texts, &HashMap::new(), &HashMap::new()).expect("write_d2i");
        let reader = D2IReader::from_bytes(bytes).expect("from_bytes");

        let all = reader.all_texts().unwrap();
        assert_eq!(all.len(), 50);
        for i in 0..50 {
            assert_eq!(all[&i], format!("Text #{}", i));
        }
    }

    #[test]
    fn d2i_roundtrip_unicode() {
        let mut texts = HashMap::new();
        texts.insert(1, "日本語テスト".to_string());
        texts.insert(2, "Ça fait plaisir 🎮".to_string());
        texts.insert(3, "Кириллица".to_string());

        let bytes =
            d2i_writer::write_d2i(&texts, &HashMap::new(), &HashMap::new()).expect("write_d2i");
        let reader = D2IReader::from_bytes(bytes).expect("from_bytes");

        assert_eq!(reader.get_text(1).unwrap(), "日本語テスト");
        assert_eq!(reader.get_text(2).unwrap(), "Ça fait plaisir 🎮");
        assert_eq!(reader.get_text(3).unwrap(), "Кириллица");
    }

    #[test]
    fn d2i_empty() {
        let bytes = d2i_writer::write_d2i(&HashMap::new(), &HashMap::new(), &HashMap::new())
            .expect("write_d2i");
        let reader = D2IReader::from_bytes(bytes).expect("from_bytes");
        assert!(reader.is_empty());
        assert_eq!(reader.len(), 0);
    }
}

mod d2p_tests {
    use crate::d2p::D2PReader;
    use crate::d2p_writer;
    use std::collections::HashMap;

    #[test]
    fn d2p_roundtrip_simple() {
        let mut files = HashMap::new();
        files.insert("maps/1234.dlm".to_string(), vec![0xDE, 0xAD, 0xBE, 0xEF]);
        files.insert("maps/5678.dlm".to_string(), vec![0xCA, 0xFE, 0xBA, 0xBE]);
        files.insert(
            "audio/ambient.mp3".to_string(),
            b"fake mp3 data here".to_vec(),
        );

        let properties = HashMap::new();

        let bytes = d2p_writer::write_d2p(&files, &properties).expect("write_d2p");
        let reader = D2PReader::from_bytes(bytes).expect("from_bytes");

        assert_eq!(reader.len(), 3);

        let names = reader.filenames();
        assert!(names.contains(&"maps/1234.dlm"));
        assert!(names.contains(&"maps/5678.dlm"));
        assert!(names.contains(&"audio/ambient.mp3"));

        assert_eq!(
            reader.read_file("maps/1234.dlm").unwrap(),
            vec![0xDE, 0xAD, 0xBE, 0xEF]
        );
        assert_eq!(
            reader.read_file("maps/5678.dlm").unwrap(),
            vec![0xCA, 0xFE, 0xBA, 0xBE]
        );
        assert_eq!(
            reader.read_file("audio/ambient.mp3").unwrap(),
            b"fake mp3 data here".to_vec()
        );
    }

    #[test]
    fn d2p_roundtrip_with_properties() {
        let mut files = HashMap::new();
        files.insert("test.txt".to_string(), b"hello world".to_vec());

        let mut properties = HashMap::new();
        properties.insert("contentOffset".to_string(), "2".to_string());
        properties.insert("version".to_string(), "2.69".to_string());

        let bytes = d2p_writer::write_d2p(&files, &properties).expect("write_d2p");
        let reader = D2PReader::from_bytes(bytes).expect("from_bytes");

        assert_eq!(reader.len(), 1);
        assert_eq!(reader.read_file("test.txt").unwrap(), b"hello world");

        let props = reader.properties();
        assert_eq!(props.get("version").unwrap(), "2.69");
    }

    #[test]
    fn d2p_roundtrip_large_files() {
        let mut files = HashMap::new();

        // 10 KB file
        let big_data: Vec<u8> = (0..10_000).map(|i| (i % 256) as u8).collect();
        files.insert("big.bin".to_string(), big_data.clone());

        // Small file
        files.insert("small.txt".to_string(), b"tiny".to_vec());

        let bytes = d2p_writer::write_d2p(&files, &HashMap::new()).expect("write_d2p");
        let reader = D2PReader::from_bytes(bytes).expect("from_bytes");

        assert_eq!(reader.len(), 2);
        assert_eq!(reader.read_file("big.bin").unwrap(), big_data);
        assert_eq!(reader.read_file("small.txt").unwrap(), b"tiny");
    }

    #[test]
    fn d2p_roundtrip_empty_archive() {
        let bytes =
            d2p_writer::write_d2p(&HashMap::new(), &HashMap::new()).expect("write_d2p");
        let reader = D2PReader::from_bytes(bytes).expect("from_bytes");
        assert!(reader.is_empty());
        assert_eq!(reader.len(), 0);
    }

    #[test]
    fn d2p_roundtrip_many_files() {
        let mut files = HashMap::new();
        for i in 0..50 {
            files.insert(
                format!("file_{:04}.dat", i),
                format!("content of file {}", i).into_bytes(),
            );
        }

        let bytes = d2p_writer::write_d2p(&files, &HashMap::new()).expect("write_d2p");
        let reader = D2PReader::from_bytes(bytes).expect("from_bytes");

        assert_eq!(reader.len(), 50);

        for i in 0..50 {
            let name = format!("file_{:04}.dat", i);
            let expected = format!("content of file {}", i).into_bytes();
            assert_eq!(reader.read_file(&name).unwrap(), expected);
        }
    }

    #[test]
    fn d2p_file_not_found() {
        let mut files = HashMap::new();
        files.insert("exists.txt".to_string(), b"data".to_vec());

        let bytes = d2p_writer::write_d2p(&files, &HashMap::new()).expect("write_d2p");
        let reader = D2PReader::from_bytes(bytes).expect("from_bytes");

        assert!(reader.read_file("nope.txt").is_err());
    }

    #[test]
    fn d2p_roundtrip_binary_data() {
        let mut files = HashMap::new();
        // All possible byte values
        let all_bytes: Vec<u8> = (0..=255).collect();
        files.insert("all_bytes.bin".to_string(), all_bytes.clone());

        // Empty file
        files.insert("empty.bin".to_string(), vec![]);

        let bytes = d2p_writer::write_d2p(&files, &HashMap::new()).expect("write_d2p");
        let reader = D2PReader::from_bytes(bytes).expect("from_bytes");

        assert_eq!(reader.read_file("all_bytes.bin").unwrap(), all_bytes);
        assert_eq!(reader.read_file("empty.bin").unwrap(), vec![] as Vec<u8>);
    }
}

mod dlm_tests {
    use crate::d2p::D2PReader;
    use dofus_common::dlm;
    use std::path::Path;

    const MAPS_D2P: &str = "/Users/dys/Projects/DofusClient/original-client/dofus/5.0_2.57.1.1/darwin/main/Dofus.app/Contents/Resources/content/maps/maps0.d2p";
    const INCARNAM_DLM: &str = "3/154010883.dlm";

    fn load_dlm(d2p_path: &str, dlm_path: &str) -> Option<Vec<u8>> {
        let path = Path::new(d2p_path);
        if !path.exists() {
            return None;
        }
        let reader = D2PReader::open(path).ok()?;
        reader.read_file(dlm_path).ok()
    }

    #[test]
    fn debug_dlm_header() {
        let Some(data) = load_dlm(MAPS_D2P, INCARNAM_DLM) else { return; };

        use byteorder::{BigEndian, ReadBytesExt};
        use flate2::read::ZlibDecoder;
        use std::io::{Cursor, Read};

        let mut decoder = ZlibDecoder::new(&data[..]);
        let mut decompressed = Vec::new();
        decoder.read_to_end(&mut decompressed).unwrap();

        let mut out = String::new();
        out.push_str(&format!("Total decompressed: {} bytes\n", decompressed.len()));

        let mut c = Cursor::new(&decompressed[..]);
        let header = c.read_u8().unwrap();
        let version = c.read_u8().unwrap();
        let id = c.read_u32::<BigEndian>().unwrap();
        out.push_str(&format!("header=0x{:02X} version={} id={}\n", header, version, id));

        // Encryption
        let encrypted = c.read_u8().unwrap() != 0;
        let enc_version = c.read_u8().unwrap();
        let data_len = c.read_i32::<BigEndian>().unwrap();
        out.push_str(&format!("encrypted={} enc_version={} data_len={}\n", encrypted, enc_version, data_len));

        if encrypted {
            // Decrypt
            let key_hex = "649ae451ca33ec53bbcbcc33becf15f4";
            let key_bytes: Vec<u8> = (0..key_hex.len()).step_by(2)
                .filter_map(|i| u8::from_str_radix(&key_hex[i..i+2], 16).ok())
                .collect();
            out.push_str(&format!("key_bytes ({} bytes): {:02X?}\n", key_bytes.len(), &key_bytes));

            let pos = c.position() as usize;
            let mut enc_data = decompressed[pos..pos + data_len as usize].to_vec();
            for (i, byte) in enc_data.iter_mut().enumerate() {
                *byte ^= key_bytes[i % key_bytes.len()];
            }

            // Dump first 80 bytes of decrypted data
            out.push_str("Decrypted data (first 80 bytes):\n");
            for (i, chunk) in enc_data[..80.min(enc_data.len())].chunks(16).enumerate() {
                out.push_str(&format!("{:04X}: ", i * 16));
                for b in chunk {
                    out.push_str(&format!("{:02X} ", b));
                }
                out.push('\n');
            }

            // Parse from decrypted
            let mut dc = Cursor::new(&enc_data[..]);
            let rel_id = dc.read_u32::<BigEndian>().unwrap();
            let map_type = dc.read_u8().unwrap();
            let sub_area = dc.read_i32::<BigEndian>().unwrap();
            let top = dc.read_i32::<BigEndian>().unwrap();
            let bot = dc.read_i32::<BigEndian>().unwrap();
            let left = dc.read_i32::<BigEndian>().unwrap();
            let right = dc.read_i32::<BigEndian>().unwrap();
            out.push_str(&format!("rel_id={} map_type={} sub_area={}\n", rel_id, map_type, sub_area));
            out.push_str(&format!("neighbors: T={} B={} L={} R={}\n", top, bot, left, right));
        }

        std::fs::write("/tmp/dlm_debug.txt", &out).unwrap();
    }

    #[test]
    fn parse_incarnam_map() {
        let Some(data) = load_dlm(MAPS_D2P, INCARNAM_DLM) else {
            eprintln!("Skipping: client files not found");
            return;
        };

        let map = dlm::parse_dlm(&data).expect("Failed to parse Incarnam DLM");

        // Incarnam statue map
        assert_eq!(map.id, 154010883);
        assert_eq!(map.cells.len(), dlm::MAP_CELLS_COUNT);

        // Should have some walkable cells
        let walkable = map.cells.iter().filter(|c| c.is_walkable()).count();
        assert!(walkable > 0, "No walkable cells found");
        assert!(walkable < dlm::MAP_CELLS_COUNT, "All cells walkable (suspicious)");

        std::fs::write("/tmp/dlm_incarnam.txt", format!(
            "Map {} v{}: {} cells, {} walkable, sub_area={}, neighbors: T={} B={} L={} R={}\n",
            map.id, map.version, map.cells.len(), walkable, map.sub_area_id,
            map.top_neighbour_id, map.bottom_neighbour_id,
            map.left_neighbour_id, map.right_neighbour_id,
        )).unwrap();
    }

    #[test]
    fn parse_multiple_maps() {
        let path = Path::new(MAPS_D2P);
        if !path.exists() {
            eprintln!("Skipping: client files not found");
            return;
        }
        let reader = D2PReader::open(path).expect("Failed to open D2P");

        let mut success = 0;
        let mut failed = 0;

        for name in reader.filenames().iter().take(50) {
            let data = reader.read_file(name).expect("Failed to read DLM");
            match dlm::parse_dlm(&data) {
                Ok(map) => {
                    assert_eq!(map.cells.len(), dlm::MAP_CELLS_COUNT);
                    success += 1;
                }
                Err(e) => {
                    eprintln!("Failed to parse {}: {}", name, e);
                    failed += 1;
                }
            }
        }

        std::fs::write("/tmp/dlm_multi.txt", format!(
            "Parsed {}/{} maps successfully\n", success, success + failed
        )).unwrap();
        assert!(success > 0, "No maps parsed successfully");
        // Allow some failures for exotic/encrypted maps
        assert!(failed <= 5, "Too many failures: {}/{}", failed, success + failed);
    }

    #[test]
    fn pathfinding_on_real_map() {
        let Some(data) = load_dlm(MAPS_D2P, INCARNAM_DLM) else { return; };
        let map = dlm::parse_dlm(&data).expect("Failed to parse DLM");

        use dofus_common::pathfinding;

        // Find two walkable cells
        let walkable: Vec<u16> = (0..dlm::MAP_CELLS_COUNT as u16)
            .filter(|&c| map.cells[c as usize].is_walkable())
            .collect();
        assert!(walkable.len() > 10, "Not enough walkable cells");

        let start = walkable[0];
        let end = walkable[walkable.len() / 2];

        let path = pathfinding::find_path(&map, start, end, None);
        assert!(path.is_some(), "Should find path on real Incarnam map from {} to {}", start, end);

        let path = path.unwrap();
        assert_eq!(*path.first().unwrap(), start);
        assert_eq!(*path.last().unwrap(), end);
        assert!(pathfinding::validate_path(&map, &path), "Path should be valid");

        std::fs::write("/tmp/dlm_pathfind.txt", format!(
            "Path from {} to {}: {} steps, path: {:?}\n",
            start, end, path.len() - 1, &path[..path.len().min(20)]
        )).unwrap();
    }
}
