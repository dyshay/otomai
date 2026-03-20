use dofus_database::models::Character;
use dofus_io::{BigEndianWriter, DofusSerialize};
use dofus_network::codec::RawMessage;
use dofus_network::session::Session;
use super::state::{Fighter, FighterStats};

/// Build FighterStats from a DB Character.
pub fn build_player_stats(c: &Character) -> FighterStats {
    let get = |key: &str| c.stats.get(key).and_then(|v| v.as_i64()).unwrap_or(0) as i16;
    FighterStats {
        strength: get("strength"),
        intelligence: get("intelligence"),
        chance: get("chance"),
        agility: get("agility"),
        power: 0,
        ..Default::default()
    }
}

/// Send GameFightShowFighterMessage for a fighter (polymorphic, raw build).
pub async fn send_show_fighter(session: &mut Session, fighter: &Fighter) -> anyhow::Result<()> {
    let mut w = BigEndianWriter::new();

    if fighter.is_player {
        w.write_ushort(7807); // GameFightCharacterInformations
    } else {
        w.write_ushort(6096); // GameFightMonsterInformations
    }

    // contextual_id
    w.write_double(fighter.id);
    // disposition (EntityDispositionInformations, TYPE_ID 7114)
    w.write_ushort(7114);
    w.write_short(fighter.cell_id);
    w.write_byte(fighter.direction);
    // look
    fighter.look.serialize(&mut w);
    // spawnInfo (GameContextBasicSpawnInformation, TYPE_ID 7069)
    w.write_byte(fighter.team as u8);
    w.write_boolean(fighter.is_alive);
    // informations (GameContextActorPositionInformations, TYPE_ID 6271)
    w.write_ushort(6271);
    w.write_double(fighter.id);
    w.write_ushort(7114);
    w.write_short(fighter.cell_id);
    w.write_byte(fighter.direction);
    fighter.look.serialize(&mut w);

    w.write_byte(0); // wave
    // GameFightMinimalStats (TYPE_ID 8253)
    w.write_ushort(8253);
    write_minimal_stats(&mut w, fighter);
    w.write_short(0); // previousPositions count

    if fighter.is_player {
        w.write_utf(&fighter.name);
        w.write_byte(0); // PlayerStatus statusId
        w.write_var_short(0); // leagueId
        w.write_int(0); // ladderPosition
        w.write_boolean(false); // hiddenInPrefight
        w.write_var_short(fighter.level);
        // AlignmentInfos
        w.write_byte(0);
        w.write_byte(0);
        w.write_byte(0);
        w.write_double(0.0);
        w.write_byte(fighter.breed);
        w.write_boolean(false);
    } else {
        w.write_var_short(fighter.monster_id as i16); // creatureGenericId
        w.write_byte(fighter.monster_grade); // creatureGrade
        w.write_var_short(fighter.level); // creatureLevel
    }

    session
        .send_raw(RawMessage {
            message_id: 5525, // GameFightShowFighterMessage
            instance_id: 0,
            payload: w.into_data(),
        })
        .await?;

    Ok(())
}

pub fn write_minimal_stats(w: &mut BigEndianWriter, f: &Fighter) {
    w.write_var_int(f.life_points);
    w.write_var_int(f.max_life_points);
    w.write_var_int(f.max_life_points); // baseMaxLifePoints
    w.write_var_int(0); // permanentDamagePercent
    w.write_var_int(0); // shieldPoints
    w.write_var_short(f.action_points);
    w.write_var_short(f.max_action_points);
    w.write_var_short(f.movement_points);
    w.write_var_short(f.max_movement_points);
    w.write_double(0.0); // summoner
    w.write_boolean(false); // summoned
    // Resists (neutral, earth, water, air, fire) percent + flat = 10
    for _ in 0..10 { w.write_var_short(0); }
    w.write_var_short(0); // criticalDamageFixedResist
    w.write_var_short(0); // pushDamageFixedResist
    // PvP resists = 10
    for _ in 0..10 { w.write_var_short(0); }
    w.write_var_short(0); // dodgePALostProbability
    w.write_var_short(0); // dodgePMLostProbability
    w.write_var_short(0); // tackleBlock
    w.write_var_short(0); // tackleEvade
    w.write_var_short(0); // fixedDamageReflection
    w.write_byte(0); // invisibilityState
    w.write_var_short(0); // meleeDamageReceivedPercent
    w.write_var_short(0); // rangedDamageReceivedPercent
    w.write_var_short(0); // weaponDamageReceivedPercent
    w.write_var_short(0); // spellDamageReceivedPercent
}
