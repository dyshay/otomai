pub mod auth {
    pub use crate::generated::messages::connection::*;
    pub use crate::generated::messages::handshake::*;
}

pub mod game {
    pub use crate::generated::messages::game_approach::*;
    pub use crate::generated::messages::common_basic::*;
}
