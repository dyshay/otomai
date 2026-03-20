use super::*;
use super::super::buffs::BuffList;
use super::super::states::StateList;
use dofus_protocol::generated::types::EntityLook;
use std::collections::HashMap;

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
