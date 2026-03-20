//! Player session state tracked during a connection.

use crate::fight;
use crate::movement;
use crate::npc;
use dofus_network::codec::RawMessage;
use tokio::sync::mpsc;

pub struct PlayerSession {
    pub account_id: i64,
    pub character_id: Option<i64>,
    pub character_name: Option<String>,
    pub map_id: Option<i64>,
    pub movement: Option<movement::MovementState>,
    pub dialog: Option<npc::NpcDialogState>,
    pub fight: Option<fight::state::Fight>,
    pub broadcast_tx: mpsc::UnboundedSender<RawMessage>,
    pub broadcast_rx: mpsc::UnboundedReceiver<RawMessage>,
}

impl PlayerSession {
    pub fn new(account_id: i64) -> Self {
        let (broadcast_tx, broadcast_rx) = crate::world::new_broadcast_channel();
        Self {
            account_id,
            character_id: None,
            character_name: None,
            map_id: None,
            movement: None,
            dialog: None,
            fight: None,
            broadcast_tx,
            broadcast_rx,
        }
    }
}
