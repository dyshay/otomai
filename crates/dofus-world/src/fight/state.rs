use dofus_protocol::generated::types::EntityLook;

/// Fight phase.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FightPhase {
    Placement,
    Fighting,
    Ended,
}

/// Team side.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Team {
    Challengers = 0,
    Defenders = 1,
}

/// A fighter in a fight (player or monster).
#[derive(Debug, Clone)]
pub struct Fighter {
    pub id: f64,
    pub name: String,
    pub level: i16,
    pub breed: u8,
    pub look: EntityLook,
    pub cell_id: i16,
    pub team: Team,
    pub life_points: i32,
    pub max_life_points: i32,
    pub action_points: i16,
    pub max_action_points: i16,
    pub movement_points: i16,
    pub max_movement_points: i16,
    pub is_player: bool,
    pub is_alive: bool,
    pub monster_id: i32,
    pub monster_grade: u8,
}

/// A fight instance.
pub struct Fight {
    pub id: i16,
    pub map_id: i64,
    pub phase: FightPhase,
    pub fighters: Vec<Fighter>,
    pub current_fighter_index: usize,
    pub round: i32,
}

impl Fight {
    pub fn new(id: i16, map_id: i64) -> Self {
        Self {
            id,
            map_id,
            phase: FightPhase::Placement,
            fighters: Vec::new(),
            current_fighter_index: 0,
            round: 0,
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

    /// Get the player fighter.
    pub fn player_fighter(&self) -> Option<&Fighter> {
        self.fighters.iter().find(|f| f.is_player)
    }

    /// Check if one team is entirely dead.
    pub fn is_team_dead(&self, team: Team) -> bool {
        self.fighters
            .iter()
            .filter(|f| f.team == team)
            .all(|f| !f.is_alive)
    }

    /// Check if the fight should end.
    pub fn should_end(&self) -> bool {
        self.is_team_dead(Team::Challengers) || self.is_team_dead(Team::Defenders)
    }

    /// Advance to next alive fighter. Returns true if a full round was completed.
    pub fn advance_turn(&mut self) -> bool {
        let count = self.fighters.len();
        let start = self.current_fighter_index;

        for i in 1..=count {
            let idx = (start + i) % count;
            if self.fighters[idx].is_alive {
                let new_round = idx <= start;
                self.current_fighter_index = idx;
                if new_round {
                    self.round += 1;
                }
                return new_round;
            }
        }
        false
    }

    /// Check if challengers won.
    pub fn challengers_won(&self) -> bool {
        self.is_team_dead(Team::Defenders)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dofus_protocol::generated::types::EntityLook;

    fn make_fighter(id: f64, team: Team, is_player: bool) -> Fighter {
        Fighter {
            id,
            name: format!("Fighter{}", id),
            level: 1,
            breed: 0,
            look: EntityLook::default(),
            cell_id: 300,
            team,
            life_points: 100,
            max_life_points: 100,
            action_points: 6,
            max_action_points: 6,
            movement_points: 3,
            max_movement_points: 3,
            is_player,
            is_alive: true,
            monster_id: 0,
            monster_grade: 0,
        }
    }

    #[test]
    fn fight_should_end_when_team_dead() {
        let mut fight = Fight::new(1, 100);
        fight.add_fighter(make_fighter(1.0, Team::Challengers, true));
        let mut monster = make_fighter(-1.0, Team::Defenders, false);
        monster.is_alive = false;
        fight.add_fighter(monster);

        assert!(fight.should_end());
        assert!(fight.challengers_won());
    }

    #[test]
    fn fight_not_ended_when_both_alive() {
        let mut fight = Fight::new(1, 100);
        fight.add_fighter(make_fighter(1.0, Team::Challengers, true));
        fight.add_fighter(make_fighter(-1.0, Team::Defenders, false));

        assert!(!fight.should_end());
    }

    #[test]
    fn advance_turn_cycles() {
        let mut fight = Fight::new(1, 100);
        fight.add_fighter(make_fighter(1.0, Team::Challengers, true));
        fight.add_fighter(make_fighter(-1.0, Team::Defenders, false));

        assert_eq!(fight.current_fighter_index, 0);
        fight.advance_turn();
        assert_eq!(fight.current_fighter_index, 1);
        let new_round = fight.advance_turn();
        assert_eq!(fight.current_fighter_index, 0);
        assert!(new_round);
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
        // Should skip dead fighter at index 1, go to index 2
        assert_eq!(fight.current_fighter_index, 2);
    }
}
