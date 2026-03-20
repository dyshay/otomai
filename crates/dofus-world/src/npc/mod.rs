pub mod actors;
pub mod dialogue;
pub mod look;
#[cfg(test)]
mod tests;

pub use actors::{build_npc_actor, write_npc_actors};
pub use dialogue::{handle_npc_action, handle_npc_dialog_reply, NpcDialogState};
pub use look::get_npc_look;
