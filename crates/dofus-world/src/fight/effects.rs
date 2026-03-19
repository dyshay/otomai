//! Effect classification and action IDs.
//!
//! Maps Dofus effect action IDs to typed enums for dispatch.

use super::state::Element;

/// Effect action IDs from Dofus ActionIds.
pub mod action_ids {
    // Damage
    pub const DAMAGE_NEUTRAL: i32 = 95;
    pub const DAMAGE_EARTH: i32 = 96;
    pub const DAMAGE_FIRE: i32 = 97;
    pub const DAMAGE_WATER: i32 = 91;
    pub const DAMAGE_AIR: i32 = 93;
    pub const DAMAGE_NEUTRAL_FIXED: i32 = 100;
    pub const DAMAGE_EARTH_FIXED: i32 = 92;
    pub const DAMAGE_FIRE_FIXED: i32 = 99;
    pub const DAMAGE_WATER_FIXED: i32 = 98;
    pub const DAMAGE_AIR_FIXED: i32 = 94;

    // Life steal
    pub const STEAL_NEUTRAL: i32 = 1092;
    pub const STEAL_EARTH: i32 = 1093;
    pub const STEAL_FIRE: i32 = 1094;
    pub const STEAL_WATER: i32 = 1091;
    pub const STEAL_AIR: i32 = 1095;

    // Heal
    pub const HEAL: i32 = 108;
    pub const HEAL_PERCENT: i32 = 143;

    // Boost stats
    pub const BOOST_STRENGTH: i32 = 118;
    pub const BOOST_INTELLIGENCE: i32 = 126;
    pub const BOOST_CHANCE: i32 = 123;
    pub const BOOST_AGILITY: i32 = 119;
    pub const BOOST_AP: i32 = 111;
    pub const BOOST_MP: i32 = 128;
    pub const BOOST_RANGE: i32 = 117;
    pub const BOOST_DAMAGE: i32 = 112;
    pub const BOOST_POWER: i32 = 138;
    pub const BOOST_CRITICAL: i32 = 115;

    // Malus stats
    pub const MALUS_STRENGTH: i32 = 157;
    pub const MALUS_INTELLIGENCE: i32 = 155;
    pub const MALUS_CHANCE: i32 = 154;
    pub const MALUS_AGILITY: i32 = 153;
    pub const MALUS_AP: i32 = 101;
    pub const MALUS_MP: i32 = 127;
    pub const MALUS_RANGE: i32 = 116;

    // Shield
    pub const SHIELD: i32 = 1040;
    pub const SHIELD_PERCENT: i32 = 1041;

    // Poison (damage over time)
    pub const POISON_NEUTRAL: i32 = 131;
    pub const POISON_EARTH: i32 = 132;
    pub const POISON_FIRE: i32 = 133;
    pub const POISON_WATER: i32 = 130;
    pub const POISON_AIR: i32 = 134;

    // Displacement
    pub const PUSH: i32 = 5;
    pub const PULL: i32 = 6;
    pub const TELEPORT: i32 = 4;
    pub const EXCHANGE_POSITIONS: i32 = 8;
    pub const TELEPORT_SYMMETRY: i32 = 1100;

    // Invisibility
    pub const INVISIBILITY: i32 = 150;

    // Marks (glyphs/traps)
    pub const GLYPH: i32 = 401;
    pub const GLYPH_COLORED: i32 = 402;
    pub const TRAP: i32 = 400;
}

/// What an effect does.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum EffectType {
    Damage(Element),
    LifeSteal(Element),
    Heal,
    HealPercent,
    BoostStat(StatType),
    MalusStat(StatType),
    Shield,
    ShieldPercent,
    Poison(Element),
    Push,
    Pull,
    Teleport,
    ExchangePositions,
    Invisibility,
    PlaceGlyph,
    PlaceTrap,
    DamageReflect,        // 107: Reflect fixed damage back to attacker
    DamageAbsorbPercent,  // 220: Absorb % of damage
    DamageReduction,      // 265: Reduce incoming damage by fixed amount
    DamageModifier,       // 792/793: Modify damage sent/received %
    TriggeredDamage,      // 1163/1164: Trigger damage on specific conditions
    Summon,               // 181: Summon creature
    SelfDamage,           // 90: Self-inflicted damage
    AddState,             // 950: Add a state (root, gravity, etc.)
    RemoveState,          // 952: Remove a state
    SpellModification,    // 283/293: Modify another spell
    StackingLimit,        // 1061: Limit buff stacking
    Portal,               // 1009: Eliotrope portal
    BombCast,             // 1008: Roublard bomb
    Unknown,
}

/// Which stat a boost/malus affects.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum StatType {
    Strength,
    Intelligence,
    Chance,
    Agility,
    AP,
    MP,
    Range,
    Damage,
    Power,
    Critical,
}

/// Classify what an effect action ID does.
pub fn classify(effect_id: i32) -> EffectType {
    use action_ids::*;
    match effect_id {
        DAMAGE_NEUTRAL | DAMAGE_NEUTRAL_FIXED => EffectType::Damage(Element::Neutral),
        DAMAGE_EARTH | DAMAGE_EARTH_FIXED => EffectType::Damage(Element::Earth),
        DAMAGE_FIRE | DAMAGE_FIRE_FIXED => EffectType::Damage(Element::Fire),
        DAMAGE_WATER | DAMAGE_WATER_FIXED => EffectType::Damage(Element::Water),
        DAMAGE_AIR | DAMAGE_AIR_FIXED => EffectType::Damage(Element::Air),

        STEAL_NEUTRAL => EffectType::LifeSteal(Element::Neutral),
        STEAL_EARTH => EffectType::LifeSteal(Element::Earth),
        STEAL_FIRE => EffectType::LifeSteal(Element::Fire),
        STEAL_WATER => EffectType::LifeSteal(Element::Water),
        STEAL_AIR => EffectType::LifeSteal(Element::Air),

        HEAL => EffectType::Heal,
        HEAL_PERCENT => EffectType::HealPercent,

        BOOST_STRENGTH => EffectType::BoostStat(StatType::Strength),
        BOOST_INTELLIGENCE => EffectType::BoostStat(StatType::Intelligence),
        BOOST_CHANCE => EffectType::BoostStat(StatType::Chance),
        BOOST_AGILITY => EffectType::BoostStat(StatType::Agility),
        BOOST_AP => EffectType::BoostStat(StatType::AP),
        BOOST_MP => EffectType::BoostStat(StatType::MP),
        BOOST_RANGE => EffectType::BoostStat(StatType::Range),
        BOOST_DAMAGE => EffectType::BoostStat(StatType::Damage),
        BOOST_POWER => EffectType::BoostStat(StatType::Power),
        BOOST_CRITICAL => EffectType::BoostStat(StatType::Critical),

        MALUS_STRENGTH => EffectType::MalusStat(StatType::Strength),
        MALUS_INTELLIGENCE => EffectType::MalusStat(StatType::Intelligence),
        MALUS_CHANCE => EffectType::MalusStat(StatType::Chance),
        MALUS_AGILITY => EffectType::MalusStat(StatType::Agility),
        MALUS_AP => EffectType::MalusStat(StatType::AP),
        MALUS_MP => EffectType::MalusStat(StatType::MP),
        MALUS_RANGE => EffectType::MalusStat(StatType::Range),

        SHIELD => EffectType::Shield,
        SHIELD_PERCENT => EffectType::ShieldPercent,

        POISON_NEUTRAL => EffectType::Poison(Element::Neutral),
        POISON_EARTH => EffectType::Poison(Element::Earth),
        POISON_FIRE => EffectType::Poison(Element::Fire),
        POISON_WATER => EffectType::Poison(Element::Water),
        POISON_AIR => EffectType::Poison(Element::Air),

        107 => EffectType::DamageReflect,
        220 => EffectType::DamageAbsorbPercent,
        265 => EffectType::DamageReduction,
        792 | 793 => EffectType::DamageModifier,
        1163 | 1164 => EffectType::TriggeredDamage,
        1223 | 1123 => EffectType::DamageModifier,

        181 => EffectType::Summon,
        90 => EffectType::SelfDamage,

        950 => EffectType::AddState,
        952 => EffectType::RemoveState,
        780 | 782 | 783 | 1027 | 1181 => EffectType::AddState, // State-related checks

        283 | 293 => EffectType::SpellModification,
        1061 => EffectType::StackingLimit,

        1009 => EffectType::Portal,
        1008 => EffectType::BombCast,

        1097 | 2023 => EffectType::DamageModifier, // Ranged bonus, special check

        PUSH => EffectType::Push,
        PULL => EffectType::Pull,
        TELEPORT | TELEPORT_SYMMETRY => EffectType::Teleport,
        EXCHANGE_POSITIONS => EffectType::ExchangePositions,
        INVISIBILITY => EffectType::Invisibility,
        GLYPH | GLYPH_COLORED => EffectType::PlaceGlyph,
        TRAP => EffectType::PlaceTrap,

        _ => EffectType::Unknown,
    }
}

/// Map effect_id to element for damage/steal/poison effects.
pub fn element_of(effect_id: i32) -> Option<Element> {
    match classify(effect_id) {
        EffectType::Damage(e) | EffectType::LifeSteal(e) | EffectType::Poison(e) => Some(e),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_damage_effects() {
        assert_eq!(classify(96), EffectType::Damage(Element::Earth));
        assert_eq!(classify(97), EffectType::Damage(Element::Fire));
        assert_eq!(classify(91), EffectType::Damage(Element::Water));
        assert_eq!(classify(93), EffectType::Damage(Element::Air));
    }

    #[test]
    fn classify_heal() {
        assert_eq!(classify(108), EffectType::Heal);
        assert_eq!(classify(143), EffectType::HealPercent);
    }

    #[test]
    fn classify_steal() {
        assert_eq!(classify(1093), EffectType::LifeSteal(Element::Earth));
    }

    #[test]
    fn classify_boost_malus() {
        assert_eq!(classify(118), EffectType::BoostStat(StatType::Strength));
        assert_eq!(classify(101), EffectType::MalusStat(StatType::AP));
    }

    #[test]
    fn classify_unknown() {
        assert_eq!(classify(99999), EffectType::Unknown);
    }

    #[test]
    fn element_of_damage() {
        assert_eq!(element_of(96), Some(Element::Earth));
        assert_eq!(element_of(108), None); // heal has no element
    }
}
