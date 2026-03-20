use super::*;
use crate::constants::{BREED_SKINS, DEFAULT_MAP_ID, DEFAULT_SUB_AREA_ID};
use crate::world::MapPlayer;
use dofus_database::models::Character;
use dofus_io::{BigEndianReader, BigEndianWriter};
use dofus_protocol::generated::types::EntityLook;
use chrono::Utc;

fn make_character(breed_id: i32, sex: i32, colors: serde_json::Value) -> Character {
    Character {
        id: 1,
        account_id: 1,
        name: "TestChar".to_string(),
        breed_id,
        sex,
        level: 1,
        experience: 0,
        kamas: 0,
        map_id: 154010883,
        cell_id: 297,
        direction: 3,
        colors,
        stats: serde_json::json!({}),
        created_at: Utc::now(),
        last_login: None,
    }
}

#[test]
fn entity_look_male_iop() {
    let c = make_character(8, 0, serde_json::json!([0xFF0000, 0x00FF00]));
    let look = build_entity_look(&c);

    assert_eq!(look.bones_id, 1); // male
    assert_eq!(look.skins, vec![150]); // Iop male skin
    assert_eq!(look.scales, vec![100]);
    // Color 0: (1 << 24) | 0xFF0000 = 0x01FF0000
    assert_eq!(look.indexed_colors[0], 0x01FF0000u32 as i32);
    // Color 1: (2 << 24) | 0x00FF00 = 0x0200FF00
    assert_eq!(look.indexed_colors[1], 0x0200FF00u32 as i32);
}

#[test]
fn entity_look_female_cra() {
    let c = make_character(9, 1, serde_json::json!([]));
    let look = build_entity_look(&c);

    assert_eq!(look.bones_id, 2); // female
    assert_eq!(look.skins, vec![180]); // Cra female skin
    assert!(look.indexed_colors.is_empty());
}

#[test]
fn entity_look_all_breeds() {
    for breed_id in 1..=18 {
        let c = make_character(breed_id, 0, serde_json::json!([]));
        let look = build_entity_look(&c);
        assert_eq!(look.skins[0], BREED_SKINS[(breed_id - 1) as usize].0);

        let c = make_character(breed_id, 1, serde_json::json!([]));
        let look = build_entity_look(&c);
        assert_eq!(look.skins[0], BREED_SKINS[(breed_id - 1) as usize].1);
    }
}

#[test]
fn map_complementary_empty_is_valid() {
    let data = build_map_complementary_payload(449, 154010883.0, &[], &[]);
    let mut r = BigEndianReader::new(data);

    let sub_area = r.read_var_short().unwrap();
    assert_eq!(sub_area, 449);

    let map_id = r.read_double().unwrap();
    assert_eq!(map_id, 154010883.0);

    let houses = r.read_short().unwrap();
    assert_eq!(houses, 0);

    let actors = r.read_short().unwrap();
    assert_eq!(actors, 0);

    let interactives = r.read_short().unwrap();
    assert_eq!(interactives, 0);

    let stated = r.read_short().unwrap();
    assert_eq!(stated, 0);

    let obstacles = r.read_short().unwrap();
    assert_eq!(obstacles, 0);

    let fights = r.read_short().unwrap();
    assert_eq!(fights, 0);

    let aggressive = r.read_boolean().unwrap();
    assert!(!aggressive);
}

#[test]
fn map_complementary_with_player() {
    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
    let player = MapPlayer {
        character_id: 42,
        account_id: 1,
        name: "Hero".to_string(),
        entity_look: EntityLook {
            bones_id: 1,
            skins: vec![150],
            indexed_colors: vec![],
            scales: vec![100],
            subentities: vec![],
        },
        cell_id: 300,
        direction: 3,
        level: 1,
        breed: 8,
        sex: false,
        tx,
    };

    let data = build_map_complementary_payload(449, 154010883.0, &[player], &[]);
    let mut r = BigEndianReader::new(data);

    let _sub_area = r.read_var_short().unwrap();
    let _map_id = r.read_double().unwrap();
    let _houses = r.read_short().unwrap();

    // Actors: count should be 1
    let actor_count = r.read_short().unwrap();
    assert_eq!(actor_count, 1);

    // TYPE_ID for GameRolePlayCharacterInformations = 5268
    let type_id = r.read_ushort().unwrap();
    assert_eq!(type_id, 5268);

    // contextual_id = 42.0
    let ctx_id = r.read_double().unwrap();
    assert_eq!(ctx_id, 42.0);
}

#[test]
fn default_map_used_when_zero() {
    assert_eq!(DEFAULT_MAP_ID, 154010883);
    assert_eq!(DEFAULT_SUB_AREA_ID, 449);
}
