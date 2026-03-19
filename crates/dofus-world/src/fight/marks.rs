//! Marks system: glyphs (trigger at turn start) and traps (trigger on movement).
//! Includes Sram trap network (connected traps propagate effects in chain).

use super::damage;
use super::effects::EffectType;
use super::state::{Element, Fight, FighterStats, SpellEffect};
use dofus_network::session::Session;
use dofus_protocol::messages::game::*;
use std::collections::HashSet;

/// Mark type.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MarkType {
    Glyph,  // Triggers at turn start of the caster
    Trap,   // Triggers when a fighter moves onto it
}

/// A mark on the battlefield.
#[derive(Debug, Clone)]
pub struct Mark {
    pub id: u32,
    pub mark_type: MarkType,
    pub caster_id: f64,
    pub cell_id: i16,
    pub zone_cells: Vec<i16>,  // All cells affected by this mark
    pub effects: Vec<SpellEffect>,
    pub remaining_turns: i32,
    pub team_only: bool,       // Only affects enemies (traps) vs all
    pub visible: bool,         // Visible to enemies (glyphs=true, traps=false until triggered)
    pub color: i32,            // Visual color
    pub spell_id: i32,         // Source spell for the mark
}

static NEXT_MARK_ID: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(1);
fn next_mark_id() -> u32 {
    NEXT_MARK_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

/// Mark manager for a fight.
#[derive(Debug, Clone, Default)]
pub struct MarkManager {
    pub marks: Vec<Mark>,
}

impl MarkManager {
    /// Place a new mark on the battlefield.
    pub fn place_mark(
        &mut self,
        mark_type: MarkType,
        caster_id: f64,
        cell_id: i16,
        zone_cells: Vec<i16>,
        effects: Vec<SpellEffect>,
        duration: i32,
        spell_id: i32,
    ) -> u32 {
        let id = next_mark_id();
        self.marks.push(Mark {
            id,
            mark_type,
            caster_id,
            cell_id,
            zone_cells,
            effects,
            remaining_turns: duration,
            team_only: mark_type == MarkType::Trap,
            visible: mark_type == MarkType::Glyph,
            color: match mark_type {
                MarkType::Glyph => 0xFF0000,
                MarkType::Trap => 0x00FF00,
            },
            spell_id,
        });
        id
    }

    /// Check if a cell has a trap. Returns mark IDs of traps on that cell.
    pub fn traps_on_cell(&self, cell_id: i16) -> Vec<u32> {
        self.marks
            .iter()
            .filter(|m| m.mark_type == MarkType::Trap && m.zone_cells.contains(&cell_id))
            .map(|m| m.id)
            .collect()
    }

    /// Get all glyph IDs owned by a caster.
    pub fn glyphs_by_caster(&self, caster_id: f64) -> Vec<u32> {
        self.marks
            .iter()
            .filter(|m| m.mark_type == MarkType::Glyph && m.caster_id == caster_id)
            .map(|m| m.id)
            .collect()
    }

    /// Find all traps connected to a trap (Sram trap network).
    /// Two traps are connected if their zones overlap or are adjacent.
    pub fn connected_traps(&self, start_mark_id: u32) -> Vec<u32> {
        let mut visited = HashSet::new();
        let mut queue = vec![start_mark_id];
        let mut result = Vec::new();

        while let Some(mark_id) = queue.pop() {
            if !visited.insert(mark_id) {
                continue;
            }
            result.push(mark_id);

            // Find adjacent/overlapping traps
            if let Some(mark) = self.marks.iter().find(|m| m.id == mark_id) {
                for other in &self.marks {
                    if other.id != mark_id
                        && other.mark_type == MarkType::Trap
                        && other.caster_id == mark.caster_id
                        && !visited.contains(&other.id)
                    {
                        // Connected if zones overlap or any cells are adjacent
                        let connected = mark.zone_cells.iter().any(|&c1| {
                            other.zone_cells.iter().any(|&c2| {
                                c1 == c2
                                    || dofus_common::pathfinding::distance(c1 as u16, c2 as u16) <= 1
                            })
                        });
                        if connected {
                            queue.push(other.id);
                        }
                    }
                }
            }
        }

        result
    }

    /// Remove a mark by ID.
    pub fn remove_mark(&mut self, mark_id: u32) {
        self.marks.retain(|m| m.id != mark_id);
    }

    /// Tick all marks (decrement durations). Returns expired mark IDs.
    pub fn tick(&mut self) -> Vec<u32> {
        let mut expired = Vec::new();
        self.marks.retain(|m| {
            if m.remaining_turns <= 1 {
                expired.push(m.id);
                false
            } else {
                true
            }
        });
        for m in &mut self.marks {
            m.remaining_turns -= 1;
        }
        expired
    }
}

/// Trigger traps on a cell (called when a fighter moves onto it).
/// Handles Sram trap network: triggers all connected traps in chain.
pub async fn trigger_traps_on_cell(
    session: &mut Session,
    fight: &mut Fight,
    cell_id: i16,
    trigger_fighter_id: f64,
) -> anyhow::Result<()> {
    let trap_ids = fight.marks.traps_on_cell(cell_id);
    if trap_ids.is_empty() {
        return Ok(());
    }

    // For each trap, find connected traps (Sram network)
    let mut all_trap_ids: Vec<u32> = Vec::new();
    let mut seen = HashSet::new();
    for tid in &trap_ids {
        let connected = fight.marks.connected_traps(*tid);
        for cid in connected {
            if seen.insert(cid) {
                all_trap_ids.push(cid);
            }
        }
    }

    // Trigger all traps in the network
    for trap_id in &all_trap_ids {
        let mark = match fight.marks.marks.iter().find(|m| m.id == *trap_id) {
            Some(m) => m.clone(),
            None => continue,
        };

        // Apply each effect to the triggering fighter
        let caster_stats = fight.get_fighter(mark.caster_id)
            .map(|f| f.stats.clone())
            .unwrap_or_default();

        for effect in &mark.effects {
            let effect_type = super::effects::classify(effect.effect_id);
            match effect_type {
                EffectType::Damage(elem) => {
                    let target_stats = fight.get_fighter(trigger_fighter_id)
                        .map(|f| f.stats.clone())
                        .unwrap_or_default();
                    let dmg = super::damage::calculate_damage(effect, &caster_stats, &target_stats, false);
                    damage::apply_damage(session, fight, mark.caster_id, trigger_fighter_id, dmg, elem).await?;
                }
                EffectType::Poison(elem) => {
                    if let Some(target) = fight.get_fighter_mut(trigger_fighter_id) {
                        let value = (effect.min_damage() + effect.max_damage()) / 2;
                        target.buffs.add(mark.caster_id, effect_type, value, effect.duration.max(1));
                    }
                }
                _ => {} // Other trap effects (push, etc.) handled via displacement
            }
        }
    }

    // Remove triggered traps
    for trap_id in &all_trap_ids {
        fight.marks.remove_mark(*trap_id);
    }

    Ok(())
}

/// Trigger glyphs at turn start (for the current fighter's glyphs).
pub async fn trigger_glyphs_for_turn(
    session: &mut Session,
    fight: &mut Fight,
    caster_id: f64,
) -> anyhow::Result<()> {
    let glyph_ids = fight.marks.glyphs_by_caster(caster_id);

    for glyph_id in &glyph_ids {
        let mark = match fight.marks.marks.iter().find(|m| m.id == *glyph_id) {
            Some(m) => m.clone(),
            None => continue,
        };

        let caster_stats = fight.get_fighter(caster_id)
            .map(|f| f.stats.clone())
            .unwrap_or_default();

        // Find all fighters standing on glyph zone
        let affected: Vec<f64> = fight.fighters
            .iter()
            .filter(|f| f.is_alive && mark.zone_cells.contains(&f.cell_id))
            .map(|f| f.id)
            .collect();

        for fighter_id in &affected {
            for effect in &mark.effects {
                let effect_type = super::effects::classify(effect.effect_id);
                if let EffectType::Damage(elem) = effect_type {
                    let target_stats = fight.get_fighter(*fighter_id)
                        .map(|f| f.stats.clone())
                        .unwrap_or_default();
                    let dmg = super::damage::calculate_damage(effect, &caster_stats, &target_stats, false);
                    damage::apply_damage(session, fight, caster_id, *fighter_id, dmg, elem).await?;
                }
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::state::Element;

    #[test]
    fn place_and_find_trap() {
        let mut mgr = MarkManager::default();
        mgr.place_mark(MarkType::Trap, 1.0, 300, vec![300, 301, 314], vec![], 3, 100);

        assert_eq!(mgr.traps_on_cell(300).len(), 1);
        assert_eq!(mgr.traps_on_cell(301).len(), 1);
        assert!(mgr.traps_on_cell(200).is_empty());
    }

    #[test]
    fn connected_traps_sram_network() {
        let mut mgr = MarkManager::default();
        // Two traps by same caster with adjacent zones
        let id1 = mgr.place_mark(MarkType::Trap, 1.0, 300, vec![300, 301], vec![], 3, 100);
        let id2 = mgr.place_mark(MarkType::Trap, 1.0, 302, vec![301, 302], vec![], 3, 100);
        // Third trap not adjacent
        let id3 = mgr.place_mark(MarkType::Trap, 1.0, 500, vec![500], vec![], 3, 100);

        let connected = mgr.connected_traps(id1);
        assert!(connected.contains(&id1));
        assert!(connected.contains(&id2)); // Connected via cell 301
        assert!(!connected.contains(&id3)); // Not connected
    }

    #[test]
    fn different_caster_traps_not_connected() {
        let mut mgr = MarkManager::default();
        let id1 = mgr.place_mark(MarkType::Trap, 1.0, 300, vec![300, 301], vec![], 3, 100);
        let id2 = mgr.place_mark(MarkType::Trap, 2.0, 301, vec![301, 302], vec![], 3, 100);

        let connected = mgr.connected_traps(id1);
        assert!(!connected.contains(&id2)); // Different caster
    }

    #[test]
    fn tick_removes_expired_marks() {
        let mut mgr = MarkManager::default();
        mgr.place_mark(MarkType::Glyph, 1.0, 300, vec![300], vec![], 2, 100);
        mgr.place_mark(MarkType::Trap, 1.0, 400, vec![400], vec![], 1, 200);

        let expired = mgr.tick();
        assert_eq!(expired.len(), 1); // Trap with duration 1 expired
        assert_eq!(mgr.marks.len(), 1); // Only glyph remains
    }

    #[test]
    fn glyphs_by_caster() {
        let mut mgr = MarkManager::default();
        mgr.place_mark(MarkType::Glyph, 1.0, 300, vec![300], vec![], 3, 100);
        mgr.place_mark(MarkType::Trap, 1.0, 400, vec![400], vec![], 3, 200);
        mgr.place_mark(MarkType::Glyph, 2.0, 500, vec![500], vec![], 3, 100);

        assert_eq!(mgr.glyphs_by_caster(1.0).len(), 1);
        assert_eq!(mgr.glyphs_by_caster(2.0).len(), 1);
    }
}
