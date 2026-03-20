//! Fighter and FighterStats definitions.

use super::types::Element;
use super::super::buffs::BuffList;
use super::super::states::StateList;
use dofus_protocol::generated::types::EntityLook;
use super::types::Team;
use std::collections::HashMap;

// ─── Fighter stats ────────────────────────────────────────────────

#[derive(Debug, Clone, Default)]
pub struct FighterStats {
    pub strength: i16,
    pub intelligence: i16,
    pub chance: i16,
    pub agility: i16,
    pub power: i16,
    pub neutral_resist_percent: i16,
    pub earth_resist_percent: i16,
    pub fire_resist_percent: i16,
    pub water_resist_percent: i16,
    pub air_resist_percent: i16,
    pub neutral_resist_flat: i16,
    pub earth_resist_flat: i16,
    pub fire_resist_flat: i16,
    pub water_resist_flat: i16,
    pub air_resist_flat: i16,
    pub critical_damage_bonus: i16,
}

impl FighterStats {
    pub fn stat_for_element(&self, elem: Element) -> i16 {
        match elem {
            Element::Neutral | Element::Earth => self.strength,
            Element::Fire => self.intelligence,
            Element::Water => self.chance,
            Element::Air => self.agility,
        }
    }

    pub fn resist_percent_for_element(&self, elem: Element) -> i16 {
        match elem {
            Element::Neutral => self.neutral_resist_percent,
            Element::Earth => self.earth_resist_percent,
            Element::Fire => self.fire_resist_percent,
            Element::Water => self.water_resist_percent,
            Element::Air => self.air_resist_percent,
        }
    }

    pub fn resist_flat_for_element(&self, elem: Element) -> i16 {
        match elem {
            Element::Neutral => self.neutral_resist_flat,
            Element::Earth => self.earth_resist_flat,
            Element::Fire => self.fire_resist_flat,
            Element::Water => self.water_resist_flat,
            Element::Air => self.air_resist_flat,
        }
    }
}

// ─── Fighter ──────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct Fighter {
    pub id: f64,
    pub name: String,
    pub level: i16,
    pub breed: u8,
    pub look: EntityLook,
    pub cell_id: i16,
    pub direction: u8,
    pub team: Team,
    pub life_points: i32,
    pub max_life_points: i32,
    pub shield_points: i32,
    pub action_points: i16,
    pub max_action_points: i16,
    pub movement_points: i16,
    pub max_movement_points: i16,
    pub is_player: bool,
    pub is_alive: bool,
    pub monster_id: i32,
    pub monster_grade: u8,
    pub stats: FighterStats,
    pub buffs: BuffList,
    pub invisible: bool,
    pub states: StateList,
    pub spell_casts_this_turn: HashMap<i32, i16>,
    pub spell_casts_on_target: HashMap<(i32, i64), i16>,
}

impl Fighter {
    pub fn reset_turn(&mut self) {
        self.action_points = self.max_action_points + self.buffs.stat_bonus(super::super::effects::StatType::AP);
        self.movement_points = self.max_movement_points + self.buffs.stat_bonus(super::super::effects::StatType::MP);
        self.spell_casts_this_turn.clear();
        self.spell_casts_on_target.clear();
    }
}
