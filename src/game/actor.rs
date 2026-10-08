//! Heroes and monsters.

use glam::Vec2;

slotmap::new_key_type! {
    pub struct ActorId;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Faction {
    Hero,
    Enemy,
}

#[derive(Clone, Debug)]
pub struct Actor {
    pub faction: Faction,
    /// Name of the 3D figure that draws this actor.
    pub figure: String,
    pub pos: Vec2,
    /// Height above the ground (bats fly, slimes hop).
    pub y: f32,
    /// Facing angle on the ground plane (radians, atan2(z, x)).
    pub facing: f32,
    pub radius: f32,
    pub speed: f32,
    pub moving: bool,
    pub move_target: Option<Vec2>,
}

impl Actor {
    pub fn hero(pos: Vec2) -> Actor {
        Actor {
            faction: Faction::Hero,
            figure: "knight".into(),
            pos,
            y: 0.0,
            facing: 0.0,
            radius: 0.32,
            speed: 4.0,
            moving: false,
            move_target: None,
        }
    }
}
