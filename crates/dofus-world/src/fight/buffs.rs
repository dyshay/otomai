//! Buff/debuff tracking system.
//!
//! Buffs are stat modifications with a duration (in turns).
//! They are applied when a spell effect has duration > 0,
//! and tick down each turn. Poisons deal damage each turn start.

use super::effects::{EffectType, StatType};
use super::state::{Element, Fighter};

/// Active buff on a fighter.
#[derive(Debug, Clone)]
pub struct Buff {
    pub id: u32,
    pub source_id: f64,
    pub effect_type: EffectType,
    pub value: i32,
    pub remaining_turns: i32,
}

/// Auto-increment buff IDs.
static NEXT_BUFF_ID: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(1);
fn next_buff_id() -> u32 {
    NEXT_BUFF_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

/// List of buffs on a fighter.
#[derive(Debug, Clone, Default)]
pub struct BuffList {
    pub buffs: Vec<Buff>,
}

impl BuffList {
    /// Add a new buff.
    pub fn add(&mut self, source_id: f64, effect_type: EffectType, value: i32, duration: i32) -> u32 {
        let id = next_buff_id();
        self.buffs.push(Buff {
            id,
            source_id,
            effect_type,
            value,
            remaining_turns: duration,
        });
        id
    }

    /// Tick all buffs at turn start. Returns expired buff IDs.
    pub fn tick(&mut self) -> Vec<u32> {
        let mut expired = Vec::new();
        self.buffs.retain(|b| {
            if b.remaining_turns <= 1 {
                expired.push(b.id);
                false
            } else {
                true
            }
        });
        for b in &mut self.buffs {
            b.remaining_turns -= 1;
        }
        expired
    }

    /// Get all active poison buffs (for turn-start damage).
    pub fn active_poisons(&self) -> Vec<&Buff> {
        self.buffs
            .iter()
            .filter(|b| matches!(b.effect_type, EffectType::Poison(_)))
            .collect()
    }

    /// Get total stat bonus from active buffs for a stat type.
    pub fn stat_bonus(&self, stat: StatType) -> i16 {
        let mut total = 0i32;
        for b in &self.buffs {
            match b.effect_type {
                EffectType::BoostStat(s) if s == stat => total += b.value,
                EffectType::MalusStat(s) if s == stat => total -= b.value,
                _ => {}
            }
        }
        total as i16
    }

    /// Get total shield points from active shield buffs.
    pub fn shield_points(&self) -> i32 {
        self.buffs
            .iter()
            .filter(|b| matches!(b.effect_type, EffectType::Shield))
            .map(|b| b.value)
            .sum()
    }

    /// Absorb damage into shield. Returns remaining damage after shield absorption.
    pub fn absorb_shield(&mut self, damage: i32) -> i32 {
        let mut remaining = damage;
        for b in &mut self.buffs {
            if matches!(b.effect_type, EffectType::Shield) && remaining > 0 {
                let absorbed = remaining.min(b.value);
                b.value -= absorbed;
                remaining -= absorbed;
            }
        }
        // Remove depleted shields
        self.buffs.retain(|b| {
            !matches!(b.effect_type, EffectType::Shield) || b.value > 0
        });
        remaining
    }

    /// Clear all buffs.
    pub fn clear(&mut self) {
        self.buffs.clear();
    }
}

/// Get the effective AP for a fighter including buff modifiers.
pub fn effective_ap(fighter: &Fighter) -> i16 {
    fighter.max_action_points + fighter.buffs.stat_bonus(StatType::AP)
}

/// Get the effective MP for a fighter including buff modifiers.
pub fn effective_mp(fighter: &Fighter) -> i16 {
    fighter.max_movement_points + fighter.buffs.stat_bonus(StatType::MP)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_and_tick_buff() {
        let mut list = BuffList::default();
        list.add(1.0, EffectType::BoostStat(StatType::Strength), 50, 3);

        assert_eq!(list.stat_bonus(StatType::Strength), 50);
        assert_eq!(list.buffs.len(), 1);

        let expired = list.tick(); // 3 → 2
        assert!(expired.is_empty());
        assert_eq!(list.buffs[0].remaining_turns, 2);

        list.tick(); // 2 → 1
        let expired = list.tick(); // 1 → expired
        assert_eq!(expired.len(), 1);
        assert!(list.buffs.is_empty());
    }

    #[test]
    fn malus_reduces_stat() {
        let mut list = BuffList::default();
        list.add(1.0, EffectType::BoostStat(StatType::AP), 2, 3);
        list.add(1.0, EffectType::MalusStat(StatType::AP), 1, 2);
        assert_eq!(list.stat_bonus(StatType::AP), 1); // +2 - 1 = +1
    }

    #[test]
    fn shield_absorbs_damage() {
        let mut list = BuffList::default();
        list.add(1.0, EffectType::Shield, 50, 3);

        let remaining = list.absorb_shield(30);
        assert_eq!(remaining, 0);
        assert_eq!(list.shield_points(), 20);

        let remaining = list.absorb_shield(40);
        assert_eq!(remaining, 20);
        assert_eq!(list.shield_points(), 0);
        assert!(list.buffs.is_empty()); // depleted shield removed
    }

    #[test]
    fn poison_tracking() {
        let mut list = BuffList::default();
        list.add(1.0, EffectType::Poison(Element::Fire), 15, 3);
        list.add(1.0, EffectType::BoostStat(StatType::Strength), 10, 2);

        assert_eq!(list.active_poisons().len(), 1);
        assert_eq!(list.active_poisons()[0].value, 15);
    }
}
