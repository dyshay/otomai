use dofus_database::models::Character;
use dofus_network::session::Session;
use dofus_protocol::generated::types::{
    ActorExtendedAlignmentInformations, CharacterBaseCharacteristic,
    CharacterCharacteristicsInformations,
};
use dofus_protocol::messages::game::*;

/// Base HP at level 1.
const BASE_HP: i32 = 50;

/// Calculate max HP for a character.
pub fn base_hp(c: &Character) -> i32 {
    let vitality = c.stats.get("vitality").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
    BASE_HP + (c.level as i32 - 1) * 5 + vitality
}

/// XP thresholds for levels 1 and 2 (level floor / next floor).
const LEVEL_1_XP: i64 = 0;
const LEVEL_2_XP: i64 = 110;

fn stat(base: i16) -> CharacterBaseCharacteristic {
    CharacterBaseCharacteristic {
        base,
        ..Default::default()
    }
}

/// Build the full CharacterCharacteristicsInformations from DB character.
pub fn build_stats(c: &Character) -> CharacterCharacteristicsInformations {
    let vitality_base = c.stats.get("vitality").and_then(|v| v.as_i64()).unwrap_or(0) as i16;
    let wisdom_base = c.stats.get("wisdom").and_then(|v| v.as_i64()).unwrap_or(0) as i16;
    let strength_base = c.stats.get("strength").and_then(|v| v.as_i64()).unwrap_or(0) as i16;
    let intelligence_base = c.stats.get("intelligence").and_then(|v| v.as_i64()).unwrap_or(0) as i16;
    let chance_base = c.stats.get("chance").and_then(|v| v.as_i64()).unwrap_or(0) as i16;
    let agility_base = c.stats.get("agility").and_then(|v| v.as_i64()).unwrap_or(0) as i16;

    let max_hp = BASE_HP + (c.level as i32 - 1) * 5 + vitality_base as i32;

    // Stats points: 5 per level (level 1 gets 0 extra)
    let stats_points = ((c.level - 1) * 5) as i16;
    let spells_points = (c.level - 1) as i16;

    // XP floor for current level (simplified: level * 100 for now)
    let xp_floor = if c.level <= 1 { LEVEL_1_XP } else { ((c.level as i64 - 1) * 100) };
    let xp_next = c.level as i64 * 100;

    CharacterCharacteristicsInformations {
        experience: c.experience,
        experience_level_floor: xp_floor,
        experience_next_level_floor: xp_next,
        experience_bonus_limit: 0,
        kamas: c.kamas,
        stats_points,
        additionnal_points: 0,
        spells_points,
        alignment_infos: ActorExtendedAlignmentInformations::default(),
        life_points: max_hp,
        max_life_points: max_hp,
        energy_points: 10000,
        max_energy_points: 10000,
        action_points_current: 6,
        movement_points_current: 3,
        initiative: stat(strength_base + intelligence_base + chance_base + agility_base),
        prospecting: stat(100), // base prospecting
        action_points: stat(6),
        movement_points: stat(3),
        strength: stat(strength_base),
        vitality: stat(vitality_base),
        wisdom: stat(wisdom_base),
        chance: stat(chance_base),
        agility: stat(agility_base),
        intelligence: stat(intelligence_base),
        range: stat(0),
        summonable_creatures_boost: stat(1),
        reflect: stat(0),
        critical_hit: stat(0),
        critical_hit_weapon: 0,
        critical_miss: stat(0),
        heal_bonus: stat(0),
        all_damages_bonus: stat(0),
        weapon_damages_bonus_percent: stat(0),
        damages_bonus_percent: stat(0),
        trap_bonus: stat(0),
        trap_bonus_percent: stat(0),
        glyph_bonus_percent: stat(0),
        rune_bonus_percent: stat(0),
        permanent_damage_percent: stat(0),
        tackle_block: stat(0),
        tackle_evade: stat(0),
        p_a_attack: stat(0),
        p_m_attack: stat(0),
        push_damage_bonus: stat(0),
        critical_damage_bonus: stat(0),
        neutral_damage_bonus: stat(0),
        earth_damage_bonus: stat(0),
        water_damage_bonus: stat(0),
        air_damage_bonus: stat(0),
        fire_damage_bonus: stat(0),
        dodge_p_a_lost_probability: stat(0),
        dodge_p_m_lost_probability: stat(0),
        neutral_element_resist_percent: stat(0),
        earth_element_resist_percent: stat(0),
        water_element_resist_percent: stat(0),
        air_element_resist_percent: stat(0),
        fire_element_resist_percent: stat(0),
        neutral_element_reduction: stat(0),
        earth_element_reduction: stat(0),
        water_element_reduction: stat(0),
        air_element_reduction: stat(0),
        fire_element_reduction: stat(0),
        push_damage_reduction: stat(0),
        critical_damage_reduction: stat(0),
        pvp_neutral_element_resist_percent: stat(0),
        pvp_earth_element_resist_percent: stat(0),
        pvp_water_element_resist_percent: stat(0),
        pvp_air_element_resist_percent: stat(0),
        pvp_fire_element_resist_percent: stat(0),
        pvp_neutral_element_reduction: stat(0),
        pvp_earth_element_reduction: stat(0),
        pvp_water_element_reduction: stat(0),
        pvp_air_element_reduction: stat(0),
        pvp_fire_element_reduction: stat(0),
        melee_damage_done_percent: stat(0),
        melee_damage_received_percent: stat(0),
        ranged_damage_done_percent: stat(0),
        ranged_damage_received_percent: stat(0),
        weapon_damage_done_percent: stat(0),
        weapon_damage_received_percent: stat(0),
        spell_damage_done_percent: stat(0),
        spell_damage_received_percent: stat(0),
        spell_modifications: vec![],
        probation_time: 0,
    }
}

/// Send CharacterStatsListMessage to the client.
pub async fn send_stats(session: &mut Session, character: &Character) -> anyhow::Result<()> {
    let stats = build_stats(character);
    session
        .send(&CharacterStatsListMessage { stats })
        .await?;
    Ok(())
}

/// Send LifePointsRegenBeginMessage — 10 HP per 2 seconds out of combat.
pub async fn send_regen_begin(session: &mut Session) -> anyhow::Result<()> {
    session
        .send(&LifePointsRegenBeginMessage { regen_rate: 20 })
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn make_character(level: i32, stats_json: serde_json::Value) -> Character {
        Character {
            id: 1,
            account_id: 1,
            name: "TestChar".to_string(),
            breed_id: 8, // Iop
            sex: 0,
            level,
            experience: 0,
            kamas: 500,
            map_id: 154010883,
            cell_id: 297,
            direction: 1,
            colors: serde_json::json!([0xFF0000, 0x00FF00, 0x0000FF]),
            stats: stats_json,
            created_at: Utc::now(),
            last_login: None,
        }
    }

    #[test]
    fn build_stats_level_1_defaults() {
        let c = make_character(1, serde_json::json!({"vitality":0,"strength":0,"intelligence":0,"chance":0,"agility":0,"wisdom":0}));
        let stats = build_stats(&c);

        assert_eq!(stats.life_points, 50); // BASE_HP
        assert_eq!(stats.max_life_points, 50);
        assert_eq!(stats.action_points_current, 6);
        assert_eq!(stats.movement_points_current, 3);
        assert_eq!(stats.kamas, 500);
        assert_eq!(stats.stats_points, 0); // no extra at level 1
        assert_eq!(stats.spells_points, 0);
        assert_eq!(stats.experience, 0);
        assert_eq!(stats.prospecting.base, 100);
    }

    #[test]
    fn build_stats_level_10_with_vitality() {
        let c = make_character(10, serde_json::json!({"vitality":30,"strength":20,"intelligence":0,"chance":0,"agility":0,"wisdom":0}));
        let stats = build_stats(&c);

        // HP = 50 + (10-1)*5 + 30 = 50 + 45 + 30 = 125
        assert_eq!(stats.max_life_points, 125);
        assert_eq!(stats.life_points, 125);
        // stats_points = (10-1)*5 = 45
        assert_eq!(stats.stats_points, 45);
        // spells_points = 10-1 = 9
        assert_eq!(stats.spells_points, 9);
        // strength base = 20
        assert_eq!(stats.strength.base, 20);
        assert_eq!(stats.vitality.base, 30);
        // initiative = sum of all 4 stats = 20 + 0 + 0 + 0 = 20
        assert_eq!(stats.initiative.base, 20);
    }

    #[test]
    fn build_stats_empty_json() {
        let c = make_character(1, serde_json::json!({}));
        let stats = build_stats(&c);

        // All stats default to 0 with empty JSON
        assert_eq!(stats.life_points, 50);
        assert_eq!(stats.strength.base, 0);
        assert_eq!(stats.vitality.base, 0);
    }

    #[test]
    fn stats_serialization_roundtrip() {
        use dofus_io::{BigEndianReader, BigEndianWriter, DofusDeserialize, DofusSerialize};

        let c = make_character(5, serde_json::json!({"vitality":10,"strength":15,"intelligence":0,"chance":0,"agility":0,"wisdom":5}));
        let stats = build_stats(&c);

        let msg = CharacterStatsListMessage { stats };
        let mut w = BigEndianWriter::new();
        msg.serialize(&mut w);
        let data = w.into_data();

        let mut r = BigEndianReader::new(data);
        let decoded = CharacterStatsListMessage::deserialize(&mut r).unwrap();

        assert_eq!(decoded.stats.life_points, msg.stats.life_points);
        assert_eq!(decoded.stats.kamas, 500);
        assert_eq!(decoded.stats.strength.base, 15);
        assert_eq!(decoded.stats.wisdom.base, 5);
        assert_eq!(decoded.stats.action_points_current, 6);
    }
}
