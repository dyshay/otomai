use super::damage;
use super::state::{Element, Fight, SpellData, SpellEffect};
use crate::WorldState;
use dofus_common::pathfinding;
use dofus_database::repository;
use dofus_network::session::Session;
use dofus_protocol::messages::game::*;
use std::sync::Arc;

/// Load spell data from SpellLevels D2O in game_data table.
pub async fn load_spell_data(
    state: &Arc<WorldState>,
    spell_id: i32,
    spell_level: i32,
) -> Option<SpellData> {
    // First get the Spell to find the SpellLevel ID
    let spell_data = repository::get_game_data(&state.pool, "Spells", spell_id).await.ok()??;
    let level_ids = spell_data.data.get("spellLevels")?.as_array()?;

    let level_idx = (spell_level as usize).saturating_sub(1).min(level_ids.len().saturating_sub(1));
    let level_id = level_ids.get(level_idx)?.as_i64()? as i32;

    // Load the SpellLevel
    let level_data = repository::get_game_data(&state.pool, "SpellLevels", level_id).await.ok()??;
    let d = &level_data.data;

    let effects = parse_effects(d.get("effects"));
    let critical_effects = parse_effects(d.get("criticalEffect"));

    Some(SpellData {
        spell_id,
        level: spell_level,
        ap_cost: d.get("apCost").and_then(|v| v.as_i64()).unwrap_or(3) as i16,
        min_range: d.get("minRange").and_then(|v| v.as_i64()).unwrap_or(1) as i16,
        range: d.get("range").and_then(|v| v.as_i64()).unwrap_or(6) as i16,
        cast_in_line: d.get("castInLine").and_then(|v| v.as_bool()).unwrap_or(false),
        cast_in_diagonal: d.get("castInDiagonal").and_then(|v| v.as_bool()).unwrap_or(false),
        cast_test_los: d.get("castTestLos").and_then(|v| v.as_bool()).unwrap_or(false),
        max_cast_per_turn: d.get("maxCastPerTurn").and_then(|v| v.as_i64()).unwrap_or(0) as i16,
        max_cast_per_target: d.get("maxCastPerTarget").and_then(|v| v.as_i64()).unwrap_or(0) as i16,
        need_free_cell: d.get("needFreeCell").and_then(|v| v.as_bool()).unwrap_or(false),
        need_taken_cell: d.get("needTakenCell").and_then(|v| v.as_bool()).unwrap_or(false),
        critical_hit_probability: d.get("criticalHitProbability").and_then(|v| v.as_i64()).unwrap_or(0) as i16,
        effects,
        critical_effects,
    })
}

fn parse_effects(val: Option<&serde_json::Value>) -> Vec<SpellEffect> {
    let arr = match val.and_then(|v| v.as_array()) {
        Some(a) => a,
        None => return vec![],
    };

    arr.iter()
        .filter_map(|e| {
            let effect_id = e.get("effectId").and_then(|v| v.as_i64())? as i32;
            let dice_num = e.get("diceNum").or_else(|| e.get("value"))
                .and_then(|v| v.as_i64()).unwrap_or(0) as i32;
            let dice_side = e.get("diceSide").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
            let value = e.get("diceConst").or_else(|| e.get("parameter2"))
                .and_then(|v| v.as_i64()).unwrap_or(0) as i32;
            let duration = e.get("duration").and_then(|v| v.as_i64()).unwrap_or(0) as i32;

            let element = super::effects::element_of(effect_id).unwrap_or(Element::Neutral);

            Some(SpellEffect {
                effect_id,
                dice_num,
                dice_side,
                value,
                duration,
                element,
            })
        })
        .collect()
}

/// Validate and execute a spell cast.
pub async fn handle_spell_cast(
    session: &mut Session,
    state: &Arc<WorldState>,
    fight: &mut Fight,
    player_id: f64,
    spell_id: i16,
    cell_id: i16,
) -> anyhow::Result<()> {
    // Verify it's the player's turn
    let current = match fight.current_fighter() {
        Some(f) if f.id == player_id && f.is_player => f.clone(),
        _ => return Ok(()),
    };

    // Load spell data from D2O
    let spell_data = match load_spell_data(state, spell_id as i32, 1).await {
        Some(d) => d,
        None => {
            // Fallback: generic spell (3 AP, 10 damage)
            SpellData {
                spell_id: spell_id as i32,
                level: 1,
                ap_cost: 3,
                min_range: 1,
                range: 6,
                cast_in_line: false,
                cast_in_diagonal: false,
                cast_test_los: false,
                max_cast_per_turn: 0,
                max_cast_per_target: 0,
                need_free_cell: false,
                need_taken_cell: false,
                critical_hit_probability: 0,
                effects: vec![SpellEffect {
                    effect_id: 96,
                    dice_num: 5,
                    dice_side: 8,
                    value: 0,
                    duration: 0,
                    element: Element::Earth,
                }],
                critical_effects: vec![],
            }
        }
    };

    // --- Validations ---

    // AP cost
    if current.action_points < spell_data.ap_cost {
        return Ok(());
    }

    // Range check
    let dist = pathfinding::distance(current.cell_id as u16, cell_id as u16);
    if (dist as i16) < spell_data.min_range || (dist as i16) > spell_data.range {
        return Ok(());
    }

    // LOS check
    if spell_data.cast_test_los {
        if let Some(map_data) = state.maps.get(fight.map_id) {
            if !pathfinding::has_line_of_sight(&map_data, current.cell_id as u16, cell_id as u16) {
                return Ok(());
            }
        }
    }

    // Max cast per turn
    if spell_data.max_cast_per_turn > 0 {
        let count = current.spell_casts_this_turn.get(&(spell_id as i32)).copied().unwrap_or(0);
        if count >= spell_data.max_cast_per_turn {
            return Ok(());
        }
    }

    // --- Execute ---

    // Deduct AP + reveal if invisible
    if let Some(f) = fight.current_fighter_mut() {
        f.action_points -= spell_data.ap_cost;
        *f.spell_casts_this_turn.entry(spell_id as i32).or_insert(0) += 1;

        if f.invisible {
            f.invisible = false;
            // Send reveal message (will be sent after sequence start)
        }
    }

    // Reveal invisibility
    let was_invisible = fight.get_fighter(player_id).map(|f| !f.invisible).unwrap_or(false);
    if was_invisible {
        session.send(&GameActionFightInvisibilityMessage {
            action_id: 150,
            source_id: player_id,
            target_id: player_id,
            state: 0, // VISIBLE
        }).await?;
    }

    // Critical hit check
    let is_critical = spell_data.critical_hit_probability > 0
        && (rand::random::<u16>() % 100) < spell_data.critical_hit_probability as u16;

    let critical_flag = if is_critical { 1u8 } else { 0u8 };

    // Sequence start
    session
        .send(&SequenceStartMessage {
            sequence_type: 1, // SPELL
            author_id: player_id,
        })
        .await?;

    // Spell cast visual
    session
        .send(&GameActionFightSpellCastMessage {
            action_id: 300,
            source_id: player_id,
            silent_cast: false,
            verbose_cast: true,
            target_id: 0.0,
            destination_cell_id: cell_id,
            critical: critical_flag,
            spell_id,
            spell_level: 1,
            portals_ids: vec![],
        })
        .await?;

    // Apply effects
    let effects = if is_critical && !spell_data.critical_effects.is_empty() {
        &spell_data.critical_effects
    } else {
        &spell_data.effects
    };

    let caster_stats = fight.get_fighter(player_id).map(|f| f.stats.clone()).unwrap_or_default();

    for effect in effects {
        let effect_type = super::effects::classify(effect.effect_id);

        match effect_type {
            super::effects::EffectType::Damage(elem) => {
                let target = fight.fighter_on_cell(cell_id).cloned();
                if let Some(target) = target {
                    let dmg = damage::calculate_damage(effect, &caster_stats, &target.stats, is_critical);
                    damage::apply_damage(session, fight, player_id, target.id, dmg, elem).await?;
                }
            }
            super::effects::EffectType::LifeSteal(elem) => {
                let target = fight.fighter_on_cell(cell_id).cloned();
                if let Some(target) = target {
                    let dmg = damage::calculate_damage(effect, &caster_stats, &target.stats, is_critical);
                    damage::apply_damage(session, fight, player_id, target.id, dmg, elem).await?;
                    // Heal caster for half the damage dealt
                    let heal = dmg / 2;
                    damage::apply_heal(session, fight, player_id, heal).await?;
                }
            }
            super::effects::EffectType::Heal => {
                // Heal the target (or self if cell is caster's)
                let target_id = fight.fighter_on_cell(cell_id).map(|f| f.id).unwrap_or(player_id);
                let heal = damage::calculate_heal(effect, &caster_stats);
                damage::apply_heal(session, fight, target_id, heal).await?;
            }
            super::effects::EffectType::HealPercent => {
                let target_id = fight.fighter_on_cell(cell_id).map(|f| f.id).unwrap_or(player_id);
                if let Some(target) = fight.get_fighter(target_id) {
                    let heal = (target.max_life_points as f64 * effect.dice_num as f64 / 100.0) as i32;
                    damage::apply_heal(session, fight, target_id, heal).await?;
                }
            }
            super::effects::EffectType::Shield => {
                let target_id = fight.fighter_on_cell(cell_id).map(|f| f.id).unwrap_or(player_id);
                let shield_value = effect.dice_num;
                if let Some(target) = fight.get_fighter_mut(target_id) {
                    target.shield_points += shield_value;
                    target.buffs.add(player_id, effect_type, shield_value, effect.duration.max(1));
                }
            }
            super::effects::EffectType::BoostStat(_) | super::effects::EffectType::MalusStat(_) => {
                let target_id = fight.fighter_on_cell(cell_id).map(|f| f.id).unwrap_or(player_id);
                let value = effect.dice_num;
                let duration = effect.duration.max(1);
                if let Some(target) = fight.get_fighter_mut(target_id) {
                    target.buffs.add(player_id, effect_type, value, duration);
                }
            }
            super::effects::EffectType::Poison(elem) => {
                let target_id = fight.fighter_on_cell(cell_id).map(|f| f.id);
                if let Some(tid) = target_id {
                    let value = (effect.min_damage() + effect.max_damage()) / 2;
                    let duration = effect.duration.max(1);
                    if let Some(target) = fight.get_fighter_mut(tid) {
                        target.buffs.add(player_id, effect_type, value, duration);
                    }
                }
            }
            super::effects::EffectType::Push => {
                let target = fight.fighter_on_cell(cell_id).cloned();
                if let Some(target) = target {
                    super::displacement::apply_push(session, fight, player_id, target.id, effect.dice_num).await?;
                    // Check traps after displacement
                    let new_cell = fight.get_fighter(target.id).map(|f| f.cell_id).unwrap_or(cell_id);
                    super::marks::trigger_traps_on_cell(session, fight, new_cell, target.id).await?;
                }
            }
            super::effects::EffectType::Pull => {
                let target = fight.fighter_on_cell(cell_id).cloned();
                if let Some(target) = target {
                    super::displacement::apply_pull(session, fight, player_id, target.id, effect.dice_num).await?;
                    let new_cell = fight.get_fighter(target.id).map(|f| f.cell_id).unwrap_or(cell_id);
                    super::marks::trigger_traps_on_cell(session, fight, new_cell, target.id).await?;
                }
            }
            super::effects::EffectType::Teleport => {
                super::displacement::apply_teleport(session, fight, player_id, player_id, cell_id).await?;
                super::marks::trigger_traps_on_cell(session, fight, cell_id, player_id).await?;
            }
            super::effects::EffectType::ExchangePositions => {
                let target = fight.fighter_on_cell(cell_id).cloned();
                if let Some(target) = target {
                    super::displacement::apply_exchange(session, fight, player_id, target.id).await?;
                }
            }
            super::effects::EffectType::Invisibility => {
                if let Some(f) = fight.get_fighter_mut(player_id) {
                    f.invisible = true;
                }
                session.send(&GameActionFightInvisibilityMessage {
                    action_id: 150,
                    source_id: player_id,
                    target_id: player_id,
                    state: 1, // INVISIBLE
                }).await?;
            }
            super::effects::EffectType::PlaceGlyph => {
                let zone_cells = vec![cell_id]; // Simplified: single-cell zone
                fight.marks.place_mark(
                    super::marks::MarkType::Glyph, player_id, cell_id,
                    zone_cells, vec![effect.clone()], effect.duration.max(1), spell_id as i32,
                );
            }
            super::effects::EffectType::PlaceTrap => {
                let zone_cells = vec![cell_id];
                fight.marks.place_mark(
                    super::marks::MarkType::Trap, player_id, cell_id,
                    zone_cells, vec![effect.clone()], effect.duration.max(1), spell_id as i32,
                );
            }
            super::effects::EffectType::Summon => {
                super::summons::summon_creature(
                    session, fight, player_id, cell_id,
                    effect.dice_num, // monster_id = param1
                    effect.dice_side as u8, // grade = param2
                    fight.get_fighter(player_id).map(|f| f.level).unwrap_or(1),
                ).await?;
            }
            super::effects::EffectType::SelfDamage => {
                let dmg = damage::calculate_damage(effect, &caster_stats, &caster_stats, is_critical);
                damage::apply_damage(session, fight, player_id, player_id, dmg, effect.element).await?;
            }
            super::effects::EffectType::AddState => {
                let target_id = fight.fighter_on_cell(cell_id).map(|f| f.id).unwrap_or(player_id);
                let state_id = effect.dice_num;
                if let Some(target) = fight.get_fighter_mut(target_id) {
                    target.states.add(state_id);
                }
            }
            super::effects::EffectType::RemoveState => {
                let target_id = fight.fighter_on_cell(cell_id).map(|f| f.id).unwrap_or(player_id);
                let state_id = effect.dice_num;
                if let Some(target) = fight.get_fighter_mut(target_id) {
                    target.states.remove(state_id);
                }
            }
            super::effects::EffectType::DamageReflect => {
                let target_id = fight.fighter_on_cell(cell_id).map(|f| f.id).unwrap_or(player_id);
                if let Some(target) = fight.get_fighter_mut(target_id) {
                    target.buffs.add(player_id, effect_type, effect.dice_num, effect.duration.max(1));
                }
            }
            super::effects::EffectType::DamageAbsorbPercent
            | super::effects::EffectType::DamageReduction
            | super::effects::EffectType::DamageModifier
            | super::effects::EffectType::TriggeredDamage => {
                let target_id = fight.fighter_on_cell(cell_id).map(|f| f.id).unwrap_or(player_id);
                if let Some(target) = fight.get_fighter_mut(target_id) {
                    target.buffs.add(player_id, effect_type, effect.dice_num, effect.duration.max(1));
                }
            }
            super::effects::EffectType::ShieldPercent => {
                let target_id = fight.fighter_on_cell(cell_id).map(|f| f.id).unwrap_or(player_id);
                if let Some(target) = fight.get_fighter_mut(target_id) {
                    let shield_val = (target.max_life_points as f64 * effect.dice_num as f64 / 100.0) as i32;
                    target.shield_points += shield_val;
                    target.buffs.add(player_id, super::effects::EffectType::Shield, shield_val, effect.duration.max(1));
                }
            }
            super::effects::EffectType::SpellModification
            | super::effects::EffectType::StackingLimit => {
                // Store as buff — spell modifications tracked via buff system
                let target_id = fight.fighter_on_cell(cell_id).map(|f| f.id).unwrap_or(player_id);
                if let Some(target) = fight.get_fighter_mut(target_id) {
                    target.buffs.add(player_id, effect_type, effect.dice_num, effect.duration.max(1));
                }
            }
            super::effects::EffectType::Portal => {
                // Eliotrope portal: place a portal mark on the cell
                fight.marks.place_mark(
                    super::marks::MarkType::Glyph, player_id, cell_id,
                    vec![cell_id], vec![], effect.duration.max(1), spell_id as i32,
                );
            }
            super::effects::EffectType::BombCast => {
                // Roublard bomb: summon a bomb entity
                super::summons::summon_creature(
                    session, fight, player_id, cell_id,
                    effect.dice_num, 1, // bomb = monster_id from param1
                    fight.get_fighter(player_id).map(|f| f.level).unwrap_or(1),
                ).await?;
            }
            _ => {} // Unknown — skip silently
        }
    }

    // AP variation
    session
        .send(&GameActionFightPointsVariationMessage {
            action_id: 168,
            source_id: player_id,
            target_id: player_id,
            delta: -spell_data.ap_cost,
        })
        .await?;

    // Sequence end
    session
        .send(&SequenceEndMessage {
            action_id: 300,
            author_id: player_id,
            sequence_type: 1,
        })
        .await?;

    Ok(())
}
