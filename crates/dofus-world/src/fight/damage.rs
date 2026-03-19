use super::state::Fight;
use dofus_network::session::Session;
use dofus_protocol::messages::game::*;

/// Apply damage from source to target.
/// element_id: 0=neutral, 1=earth, 2=fire, 3=water, 4=air
pub async fn apply_damage(
    session: &mut Session,
    fight: &mut Fight,
    source_id: f64,
    target_id: f64,
    base_damage: i32,
    element_id: i32,
) -> anyhow::Result<()> {
    // Simple damage calculation (no resistances for now)
    let damage = base_damage.max(1);

    // Apply to target
    let target_died = if let Some(target) = fight.get_fighter_mut(target_id) {
        target.life_points = (target.life_points - damage).max(0);
        if target.life_points == 0 {
            target.is_alive = false;
            true
        } else {
            false
        }
    } else {
        return Ok(());
    };

    // Send damage message
    session
        .send(&GameActionFightLifePointsLostMessage {
            action_id: 300,
            source_id,
            target_id,
            loss: damage,
            permanent_damages: 0,
            element_id,
        })
        .await?;

    // Send death if killed
    if target_died {
        session
            .send(&GameActionFightDeathMessage {
                action_id: 103, // ACTION_FIGHT_DEATH
                source_id,
                target_id,
            })
            .await?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::state::*;
    use dofus_protocol::generated::types::EntityLook;

    fn make_fight() -> Fight {
        let mut fight = Fight::new(1, 100);
        fight.add_fighter(Fighter {
            id: 1.0,
            name: "Player".to_string(),
            level: 1,
            breed: 8,
            look: EntityLook::default(),
            cell_id: 300,
            team: Team::Challengers,
            life_points: 100,
            max_life_points: 100,
            action_points: 6,
            max_action_points: 6,
            movement_points: 3,
            max_movement_points: 3,
            is_player: true,
            is_alive: true,
            monster_id: 0,
            monster_grade: 0,
        });
        fight.add_fighter(Fighter {
            id: -1.0,
            name: "Monster".to_string(),
            level: 1,
            breed: 0,
            look: EntityLook::default(),
            cell_id: 400,
            team: Team::Defenders,
            life_points: 50,
            max_life_points: 50,
            action_points: 6,
            max_action_points: 6,
            movement_points: 3,
            max_movement_points: 3,
            is_player: false,
            is_alive: true,
            monster_id: 0,
            monster_grade: 1,
        });
        fight
    }

    #[test]
    fn damage_reduces_hp() {
        let mut fight = make_fight();
        let target = fight.get_fighter_mut(-1.0).unwrap();
        target.life_points -= 30;
        assert_eq!(target.life_points, 20);
    }

    #[test]
    fn lethal_damage_kills() {
        let mut fight = make_fight();
        let target = fight.get_fighter_mut(-1.0).unwrap();
        target.life_points = 0;
        target.is_alive = false;
        assert!(!target.is_alive);
        assert!(fight.should_end());
        assert!(fight.challengers_won());
    }

    #[test]
    fn overkill_clamps_to_zero() {
        let mut fight = make_fight();
        let damage = 999;
        if let Some(target) = fight.get_fighter_mut(-1.0) {
            target.life_points = (target.life_points - damage).max(0);
            assert_eq!(target.life_points, 0);
        }
    }
}
