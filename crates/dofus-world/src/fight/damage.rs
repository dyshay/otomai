use super::state::{Element, Fight, FighterStats, SpellEffect};
use dofus_protocol::messages::game::GameActionFightLifePointsGainMessage;
use dofus_network::session::Session;
use dofus_protocol::messages::game::*;

/// Calculate damage for a spell effect, applying stats and resistances.
pub fn calculate_damage(
    effect: &SpellEffect,
    caster_stats: &FighterStats,
    target_stats: &FighterStats,
    is_critical: bool,
) -> i32 {
    // Base damage: random between min and max
    let base = if effect.dice_side > 0 {
        // In a real server we'd use rand, but for deterministic first pass use average
        (effect.min_damage() + effect.max_damage()) / 2
    } else {
        effect.dice_num
    };

    // Element bonus: stat / 100
    let elem_stat = caster_stats.stat_for_element(effect.element) as f64;
    let power = caster_stats.power as f64;
    let elem_bonus = (elem_stat + power) / 100.0;

    // Critical bonus
    let crit_bonus = if is_critical {
        caster_stats.critical_damage_bonus as f64
    } else {
        0.0
    };

    // Total before resist
    let total = (base as f64) * (1.0 + elem_bonus) + crit_bonus;

    // Target resistances
    let resist_pct = target_stats.resist_percent_for_element(effect.element) as f64;
    let resist_flat = target_stats.resist_flat_for_element(effect.element) as f64;

    let after_resist = total * (1.0 - resist_pct / 100.0) - resist_flat;

    after_resist.max(0.0) as i32
}

/// Calculate heal amount from a heal spell effect.
pub fn calculate_heal(effect: &SpellEffect, caster_stats: &FighterStats) -> i32 {
    let base = if effect.dice_side > 0 {
        (effect.min_damage() + effect.max_damage()) / 2
    } else {
        effect.dice_num
    };
    let intel_bonus = caster_stats.intelligence as f64 / 100.0;
    (base as f64 * (1.0 + intel_bonus)).max(0.0) as i32
}

/// Apply healing to a fighter.
pub async fn apply_heal(
    session: &mut Session,
    fight: &mut Fight,
    target_id: f64,
    heal: i32,
) -> anyhow::Result<()> {
    let actual_heal = if let Some(target) = fight.get_fighter_mut(target_id) {
        let missing = target.max_life_points - target.life_points;
        let actual = heal.min(missing).max(0);
        target.life_points += actual;
        actual
    } else {
        return Ok(());
    };

    if actual_heal > 0 {
        session
            .send(&GameActionFightLifePointsGainMessage {
                action_id: 300,
                source_id: target_id,
                target_id,
                delta: actual_heal,
            })
            .await?;
    }
    Ok(())
}

/// Apply damage from source to target in a fight.
pub async fn apply_damage(
    session: &mut Session,
    fight: &mut Fight,
    source_id: f64,
    target_id: f64,
    damage: i32,
    element: Element,
) -> anyhow::Result<bool> {
    let clamped = damage.max(0);

    // Shield absorbs damage first
    let (hp_damage, shield_damage) = if let Some(target) = fight.get_fighter_mut(target_id) {
        let after_shield = target.buffs.absorb_shield(clamped);
        let shield_absorbed = clamped - after_shield;
        target.shield_points = target.buffs.shield_points() as i32;
        target.life_points = (target.life_points - after_shield).max(0);
        if target.life_points == 0 {
            target.is_alive = false;
        }
        (after_shield, shield_absorbed)
    } else {
        return Ok(false);
    };

    let target_died = fight.get_fighter(target_id).map(|f| !f.is_alive).unwrap_or(false);

    // GameActionFightLifePointsLostMessage
    session
        .send(&GameActionFightLifePointsLostMessage {
            action_id: 300, // ACTION_FIGHT_CAST_SPELL
            source_id,
            target_id,
            loss: clamped,
            permanent_damages: 0,
            element_id: element as i32,
        })
        .await?;

    if target_died {
        session
            .send(&GameActionFightDeathMessage {
                action_id: 103,
                source_id,
                target_id,
            })
            .await?;
    }

    Ok(target_died)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn damage_no_stats_no_resist() {
        let effect = SpellEffect {
            effect_id: 96,
            dice_num: 10,
            dice_side: 0,
            value: 0,
            duration: 0,
            element: Element::Earth,
        };
        let caster = FighterStats::default();
        let target = FighterStats::default();
        let dmg = calculate_damage(&effect, &caster, &target, false);
        assert_eq!(dmg, 10); // base damage, no bonuses
    }

    #[test]
    fn damage_with_stat_bonus() {
        let effect = SpellEffect {
            effect_id: 96,
            dice_num: 10,
            dice_side: 0,
            value: 0,
            duration: 0,
            element: Element::Earth,
        };
        let caster = FighterStats {
            strength: 100, // +100% earth bonus
            ..Default::default()
        };
        let target = FighterStats::default();
        let dmg = calculate_damage(&effect, &caster, &target, false);
        assert_eq!(dmg, 20); // 10 * (1 + 100/100) = 20
    }

    #[test]
    fn damage_with_resist_percent() {
        let effect = SpellEffect {
            effect_id: 96,
            dice_num: 100,
            dice_side: 0,
            value: 0,
            duration: 0,
            element: Element::Earth,
        };
        let caster = FighterStats::default();
        let target = FighterStats {
            earth_resist_percent: 50, // 50% resist
            ..Default::default()
        };
        let dmg = calculate_damage(&effect, &caster, &target, false);
        assert_eq!(dmg, 50); // 100 * (1 - 50/100) = 50
    }

    #[test]
    fn damage_with_resist_flat() {
        let effect = SpellEffect {
            effect_id: 96,
            dice_num: 100,
            dice_side: 0,
            value: 0,
            duration: 0,
            element: Element::Earth,
        };
        let caster = FighterStats::default();
        let target = FighterStats {
            earth_resist_flat: 30,
            ..Default::default()
        };
        let dmg = calculate_damage(&effect, &caster, &target, false);
        assert_eq!(dmg, 70); // 100 - 30 = 70
    }

    #[test]
    fn damage_clamped_to_zero() {
        let effect = SpellEffect {
            effect_id: 96,
            dice_num: 5,
            dice_side: 0,
            value: 0,
            duration: 0,
            element: Element::Earth,
        };
        let caster = FighterStats::default();
        let target = FighterStats {
            earth_resist_flat: 100,
            ..Default::default()
        };
        let dmg = calculate_damage(&effect, &caster, &target, false);
        assert_eq!(dmg, 0);
    }

    #[test]
    fn damage_dice_range() {
        let effect = SpellEffect {
            effect_id: 96,
            dice_num: 5,
            dice_side: 10,
            value: 0,
            duration: 0,
            element: Element::Earth,
        };
        let caster = FighterStats::default();
        let target = FighterStats::default();
        let dmg = calculate_damage(&effect, &caster, &target, false);
        // Average of 5..50 = 27
        assert_eq!(dmg, 27);
    }

    #[test]
    fn damage_critical_bonus() {
        let effect = SpellEffect {
            effect_id: 96,
            dice_num: 10,
            dice_side: 0,
            value: 0,
            duration: 0,
            element: Element::Earth,
        };
        let caster = FighterStats {
            critical_damage_bonus: 20,
            ..Default::default()
        };
        let target = FighterStats::default();
        let dmg = calculate_damage(&effect, &caster, &target, true);
        assert_eq!(dmg, 30); // 10 + 20 crit bonus
    }
}
