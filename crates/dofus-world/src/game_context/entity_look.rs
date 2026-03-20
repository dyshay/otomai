use dofus_database::models::Character;
use dofus_protocol::generated::types::EntityLook;
use crate::constants::BREED_SKINS;

/// Build an EntityLook from a DB Character.
pub fn build_entity_look(c: &Character) -> EntityLook {
    let indexed_colors: Vec<i32> = c
        .colors
        .as_array()
        .map(|arr| {
            arr.iter()
                .enumerate()
                .filter_map(|(i, v)| {
                    v.as_i64()
                        .map(|color| (((i + 1) as i32) << 24) | ((color as i32) & 0x00FFFFFF))
                })
                .collect()
        })
        .unwrap_or_default();

    let bones_id: i16 = if c.sex == 0 { 1 } else { 2 };
    let breed_idx = (c.breed_id as usize).saturating_sub(1).min(BREED_SKINS.len() - 1);
    let skin_id = if c.sex == 0 {
        BREED_SKINS[breed_idx].0
    } else {
        BREED_SKINS[breed_idx].1
    };

    EntityLook {
        bones_id,
        skins: vec![skin_id],
        indexed_colors,
        scales: vec![100],
        subentities: vec![],
    }
}
