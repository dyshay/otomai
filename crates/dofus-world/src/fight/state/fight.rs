//! Fight struct and its methods.

use super::types::{FightPhase, Team};
use super::fighter::Fighter;
use super::super::marks::MarkManager;

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
