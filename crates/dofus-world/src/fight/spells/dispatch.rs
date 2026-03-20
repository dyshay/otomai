use super::super::{damage, displacement, effects, marks, summons, state::{Element, Fight, SpellData, SpellEffect, FighterStats}};
use crate::WorldState;
use dofus_network::session::Session;
use dofus_protocol::messages::game::*;
use std::sync::Arc;
use super::loading::load_spell_data;
use super::validation::validate_cast;

/// Validate and execute a spell cast.
pub async fn handle_spell_cast(
    session: &mut Session,
    state: &Arc<WorldState>,
    fight: &mut Fight,
    player_id: f64,
    spell_id: i16,
    cell_id: i16,
) -> anyhow::Result<()> {
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

    // Validate cast
    if validate_cast(fight, state, player_id, spell_id, cell_id, &spell_data).is_err() {
        return Ok(());
    }

    // Deduct AP + reveal if invisible
    if let Some(f) = fight.current_fighter_mut() {
        f.action_points -= spell_data.ap_cost;
        *f.spell_casts_this_turn.entry(spell_id as i32).or_insert(0) += 1;

        if f.invisible {
            f.invisible = false;
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
    let applied_effects = if is_critical && !spell_data.critical_effects.is_empty() {
        &spell_data.critical_effects
    } else {
        &spell_data.effects
    };

    let caster_stats = fight.get_fighter(player_id).map(|f| f.stats.clone()).unwrap_or_default();

    for effect in applied_effects {
        apply_effect(session, fight, player_id, cell_id, effect, &caster_stats, is_critical, spell_id).await?;
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

async fn apply_effect(
    session: &mut Session,
    fight: &mut Fight,
    player_id: f64,
    cell_id: i16,
    effect: &SpellEffect,
    caster_stats: &FighterStats,
    is_critical: bool,
    spell_id: i16,
) -> anyhow::Result<()> {
    let effect_type = effects::classify(effect.effect_id);

    match effect_type {
        effects::EffectType::Damage(elem) => {
            let target = fight.fighter_on_cell(cell_id).cloned();
            if let Some(target) = target {
                let dmg = damage::calculate_damage(effect, caster_stats, &target.stats, is_critical);
                damage::apply_damage(session, fight, player_id, target.id, dmg, elem).await?;
            }
        }
        effects::EffectType::LifeSteal(elem) => {
            let target = fight.fighter_on_cell(cell_id).cloned();
            if let Some(target) = target {
                let dmg = damage::calculate_damage(effect, caster_stats, &target.stats, is_critical);
                damage::apply_damage(session, fight, player_id, target.id, dmg, elem).await?;
                let heal = dmg / 2;
                damage::apply_heal(session, fight, player_id, heal).await?;
            }
        }
        effects::EffectType::Heal => {
            let target_id = fight.fighter_on_cell(cell_id).map(|f| f.id).unwrap_or(player_id);
            let heal = damage::calculate_heal(effect, caster_stats);
            damage::apply_heal(session, fight, target_id, heal).await?;
        }
        effects::EffectType::HealPercent => {
            let target_id = fight.fighter_on_cell(cell_id).map(|f| f.id).unwrap_or(player_id);
            if let Some(target) = fight.get_fighter(target_id) {
                let heal = (target.max_life_points as f64 * effect.dice_num as f64 / 100.0) as i32;
                damage::apply_heal(session, fight, target_id, heal).await?;
            }
        }
        effects::EffectType::Shield => {
            let target_id = fight.fighter_on_cell(cell_id).map(|f| f.id).unwrap_or(player_id);
            let shield_value = effect.dice_num;
            if let Some(target) = fight.get_fighter_mut(target_id) {
                target.shield_points += shield_value;
                target.buffs.add(player_id, effect_type, shield_value, effect.duration.max(1));
            }
        }
        effects::EffectType::BoostStat(_) | effects::EffectType::MalusStat(_) => {
            let target_id = fight.fighter_on_cell(cell_id).map(|f| f.id).unwrap_or(player_id);
            let value = effect.dice_num;
            let duration = effect.duration.max(1);
            if let Some(target) = fight.get_fighter_mut(target_id) {
                target.buffs.add(player_id, effect_type, value, duration);
            }
        }
        effects::EffectType::Poison(_elem) => {
            let target_id = fight.fighter_on_cell(cell_id).map(|f| f.id);
            if let Some(tid) = target_id {
                let value = (effect.min_damage() + effect.max_damage()) / 2;
                let duration = effect.duration.max(1);
                if let Some(target) = fight.get_fighter_mut(tid) {
                    target.buffs.add(player_id, effect_type, value, duration);
                }
            }
        }
        effects::EffectType::Push => {
            let target = fight.fighter_on_cell(cell_id).cloned();
            if let Some(target) = target {
                displacement::apply_push(session, fight, player_id, target.id, effect.dice_num).await?;
                let new_cell = fight.get_fighter(target.id).map(|f| f.cell_id).unwrap_or(cell_id);
                marks::trigger_traps_on_cell(session, fight, new_cell, target.id).await?;
            }
        }
        effects::EffectType::Pull => {
            let target = fight.fighter_on_cell(cell_id).cloned();
            if let Some(target) = target {
                displacement::apply_pull(session, fight, player_id, target.id, effect.dice_num).await?;
                let new_cell = fight.get_fighter(target.id).map(|f| f.cell_id).unwrap_or(cell_id);
                marks::trigger_traps_on_cell(session, fight, new_cell, target.id).await?;
            }
        }
        effects::EffectType::Teleport => {
            displacement::apply_teleport(session, fight, player_id, player_id, cell_id).await?;
            marks::trigger_traps_on_cell(session, fight, cell_id, player_id).await?;
        }
        effects::EffectType::ExchangePositions => {
            let target = fight.fighter_on_cell(cell_id).cloned();
            if let Some(target) = target {
                displacement::apply_exchange(session, fight, player_id, target.id).await?;
            }
        }
        effects::EffectType::Invisibility => {
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
        effects::EffectType::PlaceGlyph => {
            let zone_cells = vec![cell_id];
            fight.marks.place_mark(
                marks::MarkType::Glyph, player_id, cell_id,
                zone_cells, vec![effect.clone()], effect.duration.max(1), spell_id as i32,
            );
        }
        effects::EffectType::PlaceTrap => {
            let zone_cells = vec![cell_id];
            fight.marks.place_mark(
                marks::MarkType::Trap, player_id, cell_id,
                zone_cells, vec![effect.clone()], effect.duration.max(1), spell_id as i32,
            );
        }
        effects::EffectType::Summon => {
            summons::summon_creature(
                session, fight, player_id, cell_id,
                effect.dice_num,
                effect.dice_side as u8,
                fight.get_fighter(player_id).map(|f| f.level).unwrap_or(1),
            ).await?;
        }
        effects::EffectType::SelfDamage => {
            let dmg = damage::calculate_damage(effect, caster_stats, caster_stats, is_critical);
            damage::apply_damage(session, fight, player_id, player_id, dmg, effect.element).await?;
        }
        effects::EffectType::AddState => {
            let target_id = fight.fighter_on_cell(cell_id).map(|f| f.id).unwrap_or(player_id);
            let state_id = effect.dice_num;
            if let Some(target) = fight.get_fighter_mut(target_id) {
                target.states.add(state_id);
            }
        }
        effects::EffectType::RemoveState => {
            let target_id = fight.fighter_on_cell(cell_id).map(|f| f.id).unwrap_or(player_id);
            let state_id = effect.dice_num;
            if let Some(target) = fight.get_fighter_mut(target_id) {
                target.states.remove(state_id);
            }
        }
        effects::EffectType::DamageReflect
        | effects::EffectType::DamageAbsorbPercent
        | effects::EffectType::DamageReduction
        | effects::EffectType::DamageModifier
        | effects::EffectType::TriggeredDamage
        | effects::EffectType::SpellModification
        | effects::EffectType::StackingLimit => {
            let target_id = fight.fighter_on_cell(cell_id).map(|f| f.id).unwrap_or(player_id);
            if let Some(target) = fight.get_fighter_mut(target_id) {
                target.buffs.add(player_id, effect_type, effect.dice_num, effect.duration.max(1));
            }
        }
        effects::EffectType::ShieldPercent => {
            let target_id = fight.fighter_on_cell(cell_id).map(|f| f.id).unwrap_or(player_id);
            if let Some(target) = fight.get_fighter_mut(target_id) {
                let shield_val = (target.max_life_points as f64 * effect.dice_num as f64 / 100.0) as i32;
                target.shield_points += shield_val;
                target.buffs.add(player_id, effects::EffectType::Shield, shield_val, effect.duration.max(1));
            }
        }
        effects::EffectType::Portal => {
            fight.marks.place_mark(
                marks::MarkType::Glyph, player_id, cell_id,
                vec![cell_id], vec![], effect.duration.max(1), spell_id as i32,
            );
        }
        effects::EffectType::BombCast => {
            summons::summon_creature(
                session, fight, player_id, cell_id,
                effect.dice_num, 1,
                fight.get_fighter(player_id).map(|f| f.level).unwrap_or(1),
            ).await?;
        }
        _ => {}
    }

    Ok(())
}
