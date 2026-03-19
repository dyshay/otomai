//! Core fight state: Fight, Fighter, SpellData, FighterStats.

use super::buffs::BuffList;
use super::marks::MarkManager;
use super::states::StateList;
use dofus_protocol::generated::types::EntityLook;
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FightPhase {
    Placement,
    Fighting,
    Ended,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Team {
    Challengers = 0,
    Defenders = 1,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Element {
    Neutral = 0,
    Earth = 1,
    Fire = 2,
    Water = 3,
    Air = 4,
}

// ─── Spell data (from SpellLevels D2O) ────────────────────────────

#[derive(Debug, Clone)]
pub struct SpellData {
    pub spell_id: i32,
    pub level: i32,
    pub ap_cost: i16,
    pub min_range: i16,
    pub range: i16,
    pub cast_in_line: bool,
    pub cast_in_diagonal: bool,
    pub cast_test_los: bool,
    pub max_cast_per_turn: i16,
    pub max_cast_per_target: i16,
    pub need_free_cell: bool,
    pub need_taken_cell: bool,
    pub critical_hit_probability: i16,
    pub effects: Vec<SpellEffect>,
    pub critical_effects: Vec<SpellEffect>,
}

#[derive(Debug, Clone)]
pub struct SpellEffect {
    pub effect_id: i32,
    pub dice_num: i32,
    pub dice_side: i32,
    pub value: i32,
    pub duration: i32,
    pub element: Element,
}

impl SpellEffect {
    pub fn min_damage(&self) -> i32 {
        self.dice_num
    }

    pub fn max_damage(&self) -> i32 {
        if self.dice_side > 0 { self.dice_num * self.dice_side } else { self.dice_num }
    }
}

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

/// Invisibility states.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum InvisibilityState {
    Visible = 0,
    Invisible = 1,
    Detected = 2, // Invisible but detected (can be seen but not targeted)
}

impl Fighter {
    pub fn reset_turn(&mut self) {
        self.action_points = self.max_action_points + self.buffs.stat_bonus(super::effects::StatType::AP);
        self.movement_points = self.max_movement_points + self.buffs.stat_bonus(super::effects::StatType::MP);
        self.spell_casts_this_turn.clear();
        self.spell_casts_on_target.clear();
    }
}

// ─── Fight ────────────────────────────────────────────────────────

pub struct Fight {
    pub id: i16,
    pub map_id: i64,
    pub phase: FightPhase,
    pub fighters: Vec<Fighter>,
    pub current_fighter_index: usize,
    pub round: i32,
    pub challenger_positions: Vec<i16>,
    pub defender_positions: Vec<i16>,
    pub marks: MarkManager,
}

impl Fight {
    pub fn new(id: i16, map_id: i64) -> Self {
        Self {
            id,
            map_id,
            phase: FightPhase::Placement,
            fighters: Vec::new(),
            current_fighter_index: 0,
            round: 1,
            challenger_positions: Vec::new(),
            defender_positions: Vec::new(),
            marks: MarkManager::default(),
        }
    }

    pub fn add_fighter(&mut self, fighter: Fighter) {
        self.fighters.push(fighter);
    }

    pub fn current_fighter(&self) -> Option<&Fighter> {
        self.fighters.get(self.current_fighter_index)
    }

    pub fn current_fighter_mut(&mut self) -> Option<&mut Fighter> {
        self.fighters.get_mut(self.current_fighter_index)
    }

    pub fn get_fighter(&self, id: f64) -> Option<&Fighter> {
        self.fighters.iter().find(|f| f.id == id)
    }

    pub fn get_fighter_mut(&mut self, id: f64) -> Option<&mut Fighter> {
        self.fighters.iter_mut().find(|f| f.id == id)
    }

    pub fn fighter_on_cell(&self, cell_id: i16) -> Option<&Fighter> {
        self.fighters.iter().find(|f| f.cell_id == cell_id && f.is_alive)
    }

    pub fn player_fighter(&self) -> Option<&Fighter> {
        self.fighters.iter().find(|f| f.is_player)
    }

    pub fn is_team_dead(&self, team: Team) -> bool {
        self.fighters.iter().filter(|f| f.team == team).all(|f| !f.is_alive)
    }

    pub fn should_end(&self) -> bool {
        self.is_team_dead(Team::Challengers) || self.is_team_dead(Team::Defenders)
    }

    pub fn challengers_won(&self) -> bool {
        self.is_team_dead(Team::Defenders)
    }

    pub fn turn_order(&self) -> Vec<f64> {
        self.fighters.iter().filter(|f| f.is_alive).map(|f| f.id).collect()
    }

    pub fn dead_ids(&self) -> Vec<f64> {
        self.fighters.iter().filter(|f| !f.is_alive).map(|f| f.id).collect()
    }

    pub fn advance_turn(&mut self) -> bool {
        let count = self.fighters.len();
        let start = self.current_fighter_index;
        for i in 1..=count {
            let idx = (start + i) % count;
            if self.fighters[idx].is_alive {
                let new_round = idx <= start;
                self.current_fighter_index = idx;
                if new_round { self.round += 1; }
                return new_round;
            }
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_fighter(id: f64, team: Team, is_player: bool) -> Fighter {
        Fighter {
            id,
            name: format!("F{}", id),
            level: 1,
            breed: 0,
            look: EntityLook::default(),
            cell_id: 300,
            direction: 1,
            team,
            life_points: 100,
            max_life_points: 100,
            shield_points: 0,
            invisible: false,
            states: StateList::default(),
            action_points: 6,
            max_action_points: 6,
            movement_points: 3,
            max_movement_points: 3,
            is_player,
            is_alive: true,
            monster_id: 0,
            monster_grade: 0,
            stats: FighterStats::default(),
            buffs: BuffList::default(),
            spell_casts_this_turn: HashMap::new(),
            spell_casts_on_target: HashMap::new(),
        }
    }

    #[test]
    fn fight_end_detection() {
        let mut fight = Fight::new(1, 100);
        fight.add_fighter(make_fighter(1.0, Team::Challengers, true));
        let mut m = make_fighter(-1.0, Team::Defenders, false);
        m.is_alive = false;
        fight.add_fighter(m);
        assert!(fight.should_end());
        assert!(fight.challengers_won());
    }

    #[test]
    fn advance_turn_skips_dead() {
        let mut fight = Fight::new(1, 100);
        fight.add_fighter(make_fighter(1.0, Team::Challengers, true));
        let mut dead = make_fighter(-1.0, Team::Defenders, false);
        dead.is_alive = false;
        fight.add_fighter(dead);
        fight.add_fighter(make_fighter(-2.0, Team::Defenders, false));
        fight.advance_turn();
        assert_eq!(fight.current_fighter_index, 2);
    }

    #[test]
    fn spell_effect_damage_range() {
        let e = SpellEffect { effect_id: 96, dice_num: 5, dice_side: 8, value: 0, duration: 0, element: Element::Earth };
        assert_eq!(e.min_damage(), 5);
        assert_eq!(e.max_damage(), 40);
    }

    #[test]
    fn stats_element_mapping() {
        let s = FighterStats { strength: 100, intelligence: 50, chance: 30, agility: 70, ..Default::default() };
        assert_eq!(s.stat_for_element(Element::Earth), 100);
        assert_eq!(s.stat_for_element(Element::Fire), 50);
        assert_eq!(s.stat_for_element(Element::Water), 30);
        assert_eq!(s.stat_for_element(Element::Air), 70);
    }
}
