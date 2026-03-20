mod fight;
pub mod fighter;
pub mod types;
#[cfg(test)]
mod tests;

pub use fight::Fight;
pub use fighter::{Fighter, FighterStats};
pub use types::{Element, FightPhase, InvisibilityState, SpellData, SpellEffect, Team};
