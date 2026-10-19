//! The crew's van on the street, where they start (`starts`): a hold with a
//! sliding door, its roof off while the hunter looking is inside, and a
//! shelf of instruments and capture tools at its back, opened with Tab in
//! the hold (inventory_ui.rs): one of each to take, and back again.

use crate::inventory::{Inventory, Kind};
use crate::map::PropKind;
use crate::movement::{Aabb, BODY_MARGIN};
use crate::paint;
use crate::player::{LocalPlayer, Player};
use crate::render::WorldPart;
#[cfg(feature = "client")]
use crate::render::{WorldMaterials, things_material};
use crate::smash::Bank;
use bevy::math::ivec2;
use bevy::prelude::*;

/// The shelf: each tool and its top-left cell on it, `SHELF` cells.
pub const SHELF: IVec2 = ivec2(4, 6);
pub const TOOLS: [(Kind, IVec2); 6] = [
    (Kind::Trap, ivec2(0, 0)),
    (Kind::Lantern, ivec2(2, 0)),
    (Kind::Thermometer, ivec2(0, 2)),
    (Kind::Emf, ivec2(2, 2)),
    (Kind::Infrared, ivec2(0, 3)),
    (Kind::Coffer, ivec2(0, 4)),
];

#[derive(Component)]
pub struct Roof;

pub struct VanPlugin;

impl Plugin for VanPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, roof.run_if(resource_exists::<Bank>));
    }
}

/// Where `n` hunters start: about the hold, three by two.
pub fn starts(hold: Aabb, n: usize) -> Vec<Vec2> {
    let inner = hold.expand(-BODY_MARGIN);
    (0..n)
        .map(|i| {
            let k = vec2((i % 3) as f32 / 2.0, ((i / 3) % 2) as f32);
            inner.min + (inner.max - inner.min) * k
        })
        .collect()
}

/// Whether he stands in the van's hold, at its shelf.
pub fn inside(bank: &Bank, at: Vec2) -> bool {
    bank.0.van.is_some_and(|h| h.contains(at))
}

/// Whether he has one of `kind` already: a set trap counts, its trigger in hand.
pub fn has(inv: &Inventory, kind: Kind) -> bool {
    inv.items
        .iter()
        .any(|i| i.kind == kind || (kind == Kind::Trap && i.kind == Kind::Trigger))
}

/// The roof, again for each new bank.
#[cfg(feature = "client")]
pub fn dress(
    bank: Option<Res<Bank>>,
    roofs: Query<(), With<Roof>>,
    mut meshes: ResMut<Assets<Mesh>>,
    world_materials: Res<WorldMaterials>,
    mut commands: Commands,
) {
    let Some(bank) = bank.filter(|_| roofs.is_empty()) else {
        return;
    };
    let Some(van) = bank.0.props.iter().find(|p| p.kind == PropKind::Van) else {
        return;
    };
    commands.spawn((
        WorldPart,
        Roof,
        Mesh2d(meshes.add(paint::van_roof(van))),
        MeshMaterial2d(things_material(&world_materials)),
        Transform::from_xyz(0.0, 0.0, 12.0),
    ));
}

/// Off while he is in the hold.
fn roof(
    bank: Res<Bank>,
    me: Option<Single<&Player, With<LocalPlayer>>>,
    mut roofs: Query<&mut Visibility, With<Roof>>,
) {
    let off = me.is_some_and(|p| inside(&bank, p.body.pos));
    for mut vis in &mut roofs {
        vis.set_if_neq(if off {
            Visibility::Hidden
        } else {
            Visibility::Inherited
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::{VAN_DOOR, generate, tile_of};
    use crate::movement::{Body, STEP, Solids, Steer, TILE};

    #[test]
    fn the_crew_starts_in_the_van_and_can_walk_out() {
        for seed in 0..20 {
            let map = generate(seed);
            let hold = map.van.expect("a van");
            let solids = map.solids();
            for at in starts(hold, 6) {
                assert!(hold.contains(at), "seed {seed}");
                let clear = |s: &Aabb| !s.expand(BODY_MARGIN - 1.0).contains(at);
                assert!(solids.iter().all(clear), "seed {seed}: {at}");
            }
            // Out through the door and a tile on, onto free floor.
            let door = vec2(
                hold.min.x + (VAN_DOOR.0 + VAN_DOOR.1 - 2.0) * TILE * 0.5,
                hold.max.y,
            );
            let out = door + vec2(0.0, TILE);
            let (x, y) = tile_of(out);
            assert!(map.free()[map.idx(x, y)], "seed {seed}: {x},{y}");
            let walls = Solids::new(map.bounds(), solids);
            let mut body = Body::new(hold.centre(), 0.0);
            for _ in 0..120 {
                body.set_steer(Steer {
                    walk: Some((out - body.pos).to_angle()),
                    ..default()
                });
                body.update(STEP, &walls);
            }
            assert!(
                body.pos.distance(out) < 8.0,
                "seed {seed}: stuck at {}",
                body.pos
            );
        }
    }
}
