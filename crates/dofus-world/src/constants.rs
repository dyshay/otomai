//! Shared constants for the world server.

/// Default spawn map: Incarnam statue.
pub const DEFAULT_MAP_ID: i64 = 154010883;
pub const DEFAULT_SUB_AREA_ID: i16 = 449;
pub const DEFAULT_CELL_ID: i32 = 297;

/// Map encryption key (from config.xml: config.maps.encryptionKey).
pub const MAP_ENCRYPTION_KEY: &str = "649ae451ca33ec53bbcbcc33becf15f4";

/// MapComplementaryInformationsDataMessage ID (polymorphic, not in generated structs).
pub const MAP_COMPLEMENTARY_MSG_ID: u16 = 5176;

/// Breed skin IDs — maps (breed_id, sex) to the base skin.
pub const BREED_SKINS: [(i16, i16); 18] = [
    (10, 20),   // 1  Feca
    (30, 40),   // 2  Osamodas
    (50, 60),   // 3  Enutrof
    (70, 80),   // 4  Sram
    (90, 100),  // 5  Xelor
    (110, 120), // 6  Ecaflip
    (130, 140), // 7  Eniripsa
    (150, 160), // 8  Iop
    (170, 180), // 9  Cra
    (190, 200), // 10 Sadida
    (210, 220), // 11 Sacrieur
    (230, 240), // 12 Pandawa
    (250, 260), // 13 Roublard
    (270, 280), // 14 Zobal
    (290, 300), // 15 Steamer
    (310, 320), // 16 Eliotrope
    (330, 340), // 17 Huppermage
    (350, 360), // 18 Ouginak
];
