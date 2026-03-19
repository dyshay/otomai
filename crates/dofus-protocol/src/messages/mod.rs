pub mod auth {
    pub use crate::generated::messages::connection::*;
    pub use crate::generated::messages::handshake::*;
}

pub mod game {
    pub use crate::generated::messages::game_approach::*;
    pub use crate::generated::messages::common_basic::*;
    pub use crate::generated::messages::game_basic::*;
    pub use crate::generated::messages::game_character_choice::*;
    pub use crate::generated::messages::game_character_creation::*;
    pub use crate::generated::messages::game_character_stats::*;
    pub use crate::generated::messages::game_chat::*;
    pub use crate::generated::messages::game_context::*;
    pub use crate::generated::messages::game_context_roleplay::*;
    pub use crate::generated::messages::game_context_roleplay_emote::*;
    pub use crate::generated::messages::game_friend::*;
    pub use crate::generated::messages::game_initialization::*;
    pub use crate::generated::messages::game_inventory_items::*;
    pub use crate::generated::messages::game_inventory_spells::*;
}
