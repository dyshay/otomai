//! Enums and spell-related types for the fight system.

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

/// Invisibility states.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum InvisibilityState {
    Visible = 0,
    Invisible = 1,
    Detected = 2,
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
