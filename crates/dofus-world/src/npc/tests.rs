use super::actors::npc_contextual_id;
use super::look::parse_npc_look;

#[test]
fn parse_npc_look_basic() {
    let look = parse_npc_look("{1|100,200|16777215|120}");
    assert_eq!(look.bones_id, 1);
    assert_eq!(look.skins, vec![100, 200]);
    assert_eq!(look.scales, vec![120]);
}

#[test]
fn parse_npc_look_empty() {
    let look = parse_npc_look("");
    assert_eq!(look.bones_id, 1);
    assert!(look.skins.is_empty());
}

#[test]
fn npc_contextual_id_is_negative() {
    assert!(npc_contextual_id(1) < 0.0);
    assert!(npc_contextual_id(100) < 0.0);
    assert_ne!(npc_contextual_id(1), npc_contextual_id(2));
}
