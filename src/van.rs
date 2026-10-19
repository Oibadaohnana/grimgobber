//! The crew's van on the street: the capture tools lie at its side, one of
//! each; E by one takes it, if he has none of it yet, onto a free quick slot.

use crate::inventory::{Inventory, Kind};
use crate::map::{Map, PropKind, tile_centre, tile_of};
use crate::movement::TILE;
use crate::paint;
use crate::player::{Living, Player};
use crate::render::{WorldMaterials, WorldPart, things_material};
use crate::smash::Bank;
use bevy::prelude::*;

const TOOLS: [Kind; 3] = [Kind::Trap, Kind::Lantern, Kind::Coffer];
const REACH: f32 = 0.8 * TILE;

#[derive(Component)]
pub struct Tool;

/// Where the tools lie in this bank.
#[derive(Resource)]
pub struct Spots(Vec<(Vec2, Kind)>);

pub struct VanPlugin;

impl Plugin for VanPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, take.run_if(resource_exists::<Spots>));
    }
}

/// Where each tool lies: on every other free tile along the van's side
/// towards the bank (street lamps stand there too).
fn spots(map: &Map) -> Vec<(Vec2, Kind)> {
    let Some(van) = map.props.iter().find(|p| p.kind == PropKind::Van) else {
        return Vec::new();
    };
    let free = map.free();
    let (x0, _) = tile_of(van.block.min + 1.0);
    let (x1, y1) = tile_of(van.block.max - 1.0);
    (y1 + 1..y1 + 3)
        .flat_map(|y| (x0 - 1..=x1 + 1).map(move |x| (x, y)))
        .filter(|&(x, y)| map.inside(x, y) && free[map.idx(x, y)])
        .step_by(2)
        .zip(TOOLS)
        .map(|((x, y), kind)| (tile_centre(x, y), kind))
        .collect()
}

/// The tools drawn by the van, again for each new bank.
pub fn dress(
    bank: Option<Res<Bank>>,
    tools: Query<(), With<Tool>>,
    mut meshes: ResMut<Assets<Mesh>>,
    world_materials: Res<WorldMaterials>,
    mut commands: Commands,
) {
    let Some(bank) = bank.filter(|_| tools.is_empty()) else {
        return;
    };
    let spots = spots(&bank.0);
    for &(at, kind) in &spots {
        commands.spawn((
            WorldPart,
            Tool,
            Mesh2d(meshes.add(paint::loot(kind))),
            MeshMaterial2d(things_material(&world_materials)),
            Transform::from_translation(at.extend(0.6)),
        ));
    }
    commands.insert_resource(Spots(spots));
}

fn take(
    keys: Res<ButtonInput<KeyCode>>,
    script: Res<crate::dev::Script>,
    spots: Res<Spots>,
    me: Single<(&Player, &mut Inventory), Living>,
    mut prompt: ResMut<crate::prompt::Prompt>,
) {
    let (player, mut inv) = me.into_inner();
    if prompt.0.is_some() || inv.open {
        return;
    }
    let pos = player.body.pos;
    let has = |kind| inv.items.iter().any(|i| i.kind == kind);
    let Some(&(at, kind)) = spots.0.iter().find(|&&(at, kind)| {
        at.distance(pos) < REACH && !has(kind) && !(kind == Kind::Trap && has(Kind::Trigger))
    }) else {
        return;
    };
    let what = match kind {
        Kind::Trap => "Take the ghost trap",
        Kind::Lantern => "Take the UV lantern",
        _ => "Take the coffer",
    };
    prompt.0 = Some((at + vec2(0.0, 30.0), what));
    if (keys.just_pressed(KeyCode::KeyE) || script.pressed("Use")) && inv.add(kind, 1, None) {
        let id = inv.items.len() - 1;
        inv.bind(id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::{generate, tile_of};

    #[test]
    fn the_tools_lie_where_he_can_stand() {
        for seed in 0..20 {
            let map = generate(seed);
            let free = map.free();
            let spots = spots(&map);
            assert_eq!(spots.len(), TOOLS.len(), "seed {seed}");
            for (at, _) in spots {
                let (x, y) = tile_of(at);
                assert!(free[map.idx(x, y)], "seed {seed}: {x},{y}");
            }
        }
    }
}
