//! Fighter state system: root, gravity, invulnerable, etc.
//!
//! States are conditions applied to fighters that modify behavior.
//! Tracked as a set of active state IDs per fighter.

use std::collections::HashSet;

/// Known state IDs from the Dofus client.
pub mod state_ids {
    pub const ROOTED: i32 = 1;        // Can't move
    pub const GRAVITY: i32 = 2;       // Can't be moved/pushed
    pub const INVULNERABLE: i32 = 3;  // Can't take damage
    pub const CARRIED: i32 = 4;       // Being carried
    pub const INVISIBLE: i32 = 6;     // Invisible
    pub const UNTARGETABLE: i32 = 7;  // Can't be targeted
}

/// Active states on a fighter.
#[derive(Debug, Clone, Default)]
pub struct StateList {
    pub states: HashSet<i32>,
}

impl StateList {
    pub fn add(&mut self, state_id: i32) {
        self.states.insert(state_id);
    }

    pub fn remove(&mut self, state_id: i32) {
        self.states.remove(&state_id);
    }

    pub fn has(&self, state_id: i32) -> bool {
        self.states.contains(&state_id)
    }

    pub fn is_rooted(&self) -> bool {
        self.has(state_ids::ROOTED)
    }

    pub fn has_gravity(&self) -> bool {
        self.has(state_ids::GRAVITY)
    }

    pub fn is_invulnerable(&self) -> bool {
        self.has(state_ids::INVULNERABLE)
    }

    pub fn is_untargetable(&self) -> bool {
        self.has(state_ids::UNTARGETABLE)
    }

    pub fn clear(&mut self) {
        self.states.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_remove_state() {
        let mut list = StateList::default();
        list.add(state_ids::ROOTED);
        assert!(list.is_rooted());
        list.remove(state_ids::ROOTED);
        assert!(!list.is_rooted());
    }

    #[test]
    fn gravity_prevents_push() {
        let mut list = StateList::default();
        list.add(state_ids::GRAVITY);
        assert!(list.has_gravity());
    }

    #[test]
    fn invulnerable_check() {
        let mut list = StateList::default();
        assert!(!list.is_invulnerable());
        list.add(state_ids::INVULNERABLE);
        assert!(list.is_invulnerable());
    }
}
