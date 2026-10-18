//! The crowbar in hand, a left click swings it, and a window right in
//! front of the robber breaks: an opening with glass on the floor, which
//! sight, walking and paths all take in, and a crash heard from far off.

use crate::dev::Script;
use crate::inventory::{Inventory, Kind};
use crate::map::{Map, Wall, deco, tile_centre, tile_of};
use crate::movement::TILE;
use crate::paths::Walkable;
use crate::player::{Level, Living, Player, Swing};
use crate::render::WorldMaterials;
use crate::sight::Sight;
use crate::sound::{Clip, Noise};
use bevy::prelude::*;
use bevy_replicon::prelude::ClientState;

/// The bank as it stands, windows broken and all.
#[derive(Resource)]
pub struct Bank(pub Map);

/// How far off a window going is heard, in tiles.
const LOUD: f32 = 14.0;

pub struct SmashPlugin;

impl Plugin for SmashPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            smash.run_if(resource_exists::<Bank>.and_then(in_state(ClientState::Disconnected))),
        );
    }
}

/// The window the robber faces within a crowbar's reach, if any.
fn window(map: &Map, pos: Vec2, heading: f32) -> Option<(i32, i32)> {
    let (x0, y0) = tile_of(pos);
    let ahead = Vec2::from_angle(heading);
    (-1..=1)
        .flat_map(|dy| (-1..=1).map(move |dx| (x0 + dx, y0 + dy)))
        .filter(|&(x, y)| map.inside(x, y) && map.at(x, y).wall == Wall::Glass)
        .map(|(x, y)| (x, y, tile_centre(x, y) - pos))
        .filter(|&(_, _, off)| off.length() < 1.3 * TILE && off.normalize().dot(ahead) > 0.5)
        .min_by(|a, b| a.2.length().total_cmp(&b.2.length()))
        .map(|(x, y, _)| (x, y))
}

#[allow(clippy::too_many_arguments)]
fn smash(
    mouse: Res<ButtonInput<MouseButton>>,
    script: Res<Script>,
    me: Single<(&Player, &Inventory, &mut Swing), Living>,
    mut bank: ResMut<Bank>,
    world_materials: Res<WorldMaterials>,
    mut images: ResMut<Assets<Image>>,
    mut noises: MessageWriter<Noise>,
    mut commands: Commands,
) {
    let (player, inventory, mut swing) = me.into_inner();
    let click = mouse.just_pressed(MouseButton::Left) || script.pressed("Smash");
    if inventory.open || !inventory.holds(Kind::Crowbar) || !click {
        return;
    }
    swing.0 = Some(0.0);
    let b = &player.body;
    let Some((x, y)) = window(&bank.0, b.pos, b.heading) else {
        return;
    };
    let map = &mut bank.0;
    map.smash(x, y);
    noises.write(Noise {
        clip: Some(Clip::Glass),
        at: tile_centre(x, y),
        loud: LOUD,
    });
    commands.insert_resource(Level::new(map));
    commands.insert_resource(Walkable::new(map));
    commands.insert_resource(Sight::new(map));
    if let Some(mut image) = images.get_mut(&world_materials.tiles)
        && let Some(data) = image.data.as_mut()
    {
        let i = map.idx(x, y) * 4;
        data[i + 1] = Wall::None as u8;
        data[i + 3] |= deco::SHARDS;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::generate;
    use crate::movement::Solids;
    use std::f32::consts::FRAC_PI_2;

    #[test]
    fn a_window_in_front_breaks_into_an_opening() {
        let mut map = generate(6);
        // A window in the bank's front with the pavement below it.
        let free = map.free();
        let (x, y) = (0..map.w)
            .flat_map(|x| (1..map.h).map(move |y| (x, y)))
            .find(|&(x, y)| {
                map.at(x, y).wall == Wall::Glass
                    && !map.at(x, y - 1).indoor
                    && free[map.idx(x, y - 1)]
            })
            .unwrap();
        let below = tile_centre(x, y - 1);
        assert_eq!(window(&map, below, -FRAC_PI_2), None, "facing away");
        assert_eq!(window(&map, below, FRAC_PI_2), Some((x, y)));
        map.smash(x, y);
        assert!(map.free()[map.idx(x, y)]);
        let solids = Solids::new(map.bounds(), map.solids());
        assert!(!solids.solid_at(tile_centre(x, y)));
    }
}
