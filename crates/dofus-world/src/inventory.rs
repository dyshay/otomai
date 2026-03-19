use dofus_database::models::Character;
use dofus_network::session::Session;
use dofus_protocol::messages::game::*;

/// Send InventoryContentMessage — items + kamas.
pub async fn send_inventory_content(
    session: &mut Session,
    character: &Character,
) -> anyhow::Result<()> {
    session
        .send(&InventoryContentMessage {
            objects: vec![], // empty inventory for now
            kamas: character.kamas,
        })
        .await?;
    Ok(())
}

/// Send InventoryWeightMessage — current/max weight.
pub async fn send_inventory_weight(session: &mut Session) -> anyhow::Result<()> {
    session
        .send(&InventoryWeightMessage {
            inventory_weight: 0,
            shop_weight: 0,
            weight_max: 1000,
        })
        .await?;
    Ok(())
}
