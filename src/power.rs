//! The bank's lights. A switch by a door of each room with lamps turns them
//! on and off (E, `Flip`), and every lamp on drains a generator nobody sees
//! (`Generator`, the server's), which slowly recovers. Run dry, its breaker
//! trips and every lamp in the bank goes out (the street's stay on), till a
//! hunter resets it at the fuse box, on a wall of a room picked at random
//! (`Reset`; it only holds once the generator has recovered `HOLDS`). The
//! server keeps what is on (`Power`, replicated). Every machine follows it in
//! `Sight` (sight, the wraith's and sanity's dark); a client also in the
//! lamps' light (baked again, render.rs) and the shader's lit rooms.

use crate::map::{Map, Wall, deco, tile_centre};
use crate::movement::TILE;
use crate::net::{Owner, sent_by};
use crate::paint;
use crate::player::{Dead, Living, LocalPlayer, Player};
use crate::render::WorldPart;
#[cfg(feature = "client")]
use crate::render::{WorldMaterial, WorldMaterials, things_material};
use crate::rng::Rng;
use crate::sight::Sight;
use crate::smash::Bank;
use crate::sound::{Clip, Noise};
use bevy::prelude::*;
use bevy_replicon::prelude::{ClientState, FromClient};
use bevy_replicon::shared::backend::connected_client::NetworkId;
use serde::{Deserialize, Serialize};

/// The generator's charge drained a second with every lamp on, and
/// recovered a second whatever: all on runs it dry in about three minutes.
const DRAIN: f32 = 1.0;
const RECOVER: f32 = 0.45;
const FULL: f32 = 100.0;
const HOLDS: f32 = 25.0;
const REACH: f32 = 0.9 * TILE;
/// The breaker's trip is heard this far, in tiles.
const LOUD: f32 = 10.0;

/// Per room, its lights switched on; the breaker tripped; where the fuse box is.
#[derive(Component, Clone, Serialize, Deserialize)]
pub struct Power {
    rooms: Vec<bool>,
    pub tripped: bool,
    fuse: Vec2,
}

impl Power {
    pub fn new(map: &Map, seed: u64) -> Self {
        let mut rng = Rng::new(seed ^ 0xf05e);
        let free = map.free();
        let walls: Vec<Vec2> = (0..map.rooms.len())
            .filter_map(|r| on_wall(map, &free, r, false))
            .collect();
        Power {
            rooms: vec![true; map.rooms.len()],
            tripped: false,
            fuse: rng.pick(&walls),
        }
    }

    /// Per lamp, whether it shines.
    fn lamps(&self, map: &Map) -> Vec<bool> {
        map.lights
            .iter()
            .map(|l| {
                !map.lamp_indoors(l)
                    || (!self.tripped && map.lamp_room(l).is_none_or(|r| self.rooms[r]))
            })
            .collect()
    }
}

/// The generator's charge, the server's alone.
#[derive(Resource)]
struct Generator(f32);

/// Asked of the server: a room's switch flipped, the breaker reset.
#[derive(Message, Serialize, Deserialize)]
pub struct Flip(pub u16);

#[derive(Message, Serialize, Deserialize)]
pub struct Reset;

/// A switch's or the fuse box's picture, by what it is.
#[derive(Component)]
pub enum Mark {
    Switch(usize),
    Fuse,
}

pub struct PowerPlugin;

impl Plugin for PowerPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                (flip, reset, run).distributive_run_if(
                    resource_exists::<Bank>.and_then(in_state(ClientState::Disconnected)),
                ),
                follow.run_if(resource_exists::<Bank>.and_then(resource_exists::<Sight>)),
            )
                .chain(),
        );
        #[cfg(feature = "client")]
        if crate::drawn(app) {
            app.add_systems(
                Update,
                (relight, offer, looks).distributive_run_if(
                    resource_exists::<Bank>.and_then(resource_exists::<Switches>),
                ),
            );
        }
    }
}

/// Each room with lamps, and where its switch is: worked out once a bank.
#[derive(Resource)]
pub struct Switches(Vec<(usize, Vec2)>);

/// A spot on room `r`'s wall, facing in, before a `free` tile one can stand
/// on: beside a door if `by_door`.
fn on_wall(map: &Map, free: &[bool], r: usize, by_door: bool) -> Option<Vec2> {
    let room = map.rooms[r];
    let door = |x, y| map.inside(x, y) && map.at(x, y).deco & deco::DOOR != 0;
    for y in room.y0..=room.y1 {
        for x in room.x0..=room.x1 {
            if map.at(x, y).room as usize != r || !free[map.idx(x, y)] {
                continue;
            }
            for (dx, dy) in [(0, 1), (1, 0), (0, -1), (-1, 0)] {
                let (wx, wy) = (x + dx, y + dy);
                if !map.inside(wx, wy) || map.at(wx, wy).wall == Wall::None || door(wx, wy) {
                    continue;
                }
                if by_door && !door(wx + dy, wy + dx) && !door(wx - dy, wy - dx) {
                    continue;
                }
                return Some(tile_centre(x, y) + vec2(dx as f32, dy as f32) * 0.42 * TILE);
            }
        }
    }
    None
}

impl Switches {
    pub fn new(map: &Map) -> Self {
        let free = map.free();
        let mut lamped = vec![false; map.rooms.len()];
        for l in &map.lights {
            if let Some(r) = map.lamp_room(l) {
                lamped[r] = true;
            }
        }
        Switches(
            (0..map.rooms.len())
                .filter(|&r| lamped[r])
                .filter_map(|r| {
                    on_wall(map, &free, r, true)
                        .or_else(|| on_wall(map, &free, r, false))
                        .map(|at| (r, at))
                })
                .collect(),
        )
    }
}

type Asker<'a> = (&'a Player, Option<&'a Owner>, Has<LocalPlayer>);

fn flip(
    mut asks: MessageReader<FromClient<Flip>>,
    ids: Query<&NetworkId>,
    switched: Option<Res<Switches>>,
    hunters: Query<Asker, Without<Dead>>,
    mut power: Single<&mut Power>,
    mut noises: MessageWriter<Noise>,
) {
    for FromClient { client_id, message } in asks.read() {
        let Some((p, ..)) = hunters.iter().find(|r| sent_by(*client_id, &ids, r.1, r.2)) else {
            continue;
        };
        let r = message.0 as usize;
        if let Some(&(_, at)) = switched.iter().flat_map(|s| &s.0).find(|s| s.0 == r)
            && at.distance(p.body.pos) < REACH
        {
            power.rooms[r] ^= true;
            noises.write(Noise {
                clip: Some(Clip::Unholster),
                at,
                loud: 1.0,
            });
        }
    }
}

fn reset(
    mut asks: MessageReader<FromClient<Reset>>,
    ids: Query<&NetworkId>,
    hunters: Query<Asker, Without<Dead>>,
    generator: Option<Res<Generator>>,
    mut power: Single<&mut Power>,
    mut noises: MessageWriter<Noise>,
) {
    for FromClient { client_id, .. } in asks.read() {
        let Some((p, ..)) = hunters.iter().find(|r| sent_by(*client_id, &ids, r.1, r.2)) else {
            continue;
        };
        if !power.tripped || power.fuse.distance(p.body.pos) >= REACH {
            continue;
        }
        let holds = generator.as_ref().is_some_and(|g| g.0 >= HOLDS);
        if holds {
            info!("the breaker is reset");
            power.tripped = false;
        }
        noises.write(Noise {
            clip: Some(if holds { Clip::Thud } else { Clip::Unholster }),
            at: power.fuse,
            loud: 2.0,
        });
    }
}

/// The generator drained by the lamps on, recovering; dry, the breaker trips.
fn run(
    time: Res<Time>,
    bank: Res<Bank>,
    generator: Option<ResMut<Generator>>,
    power: Option<Single<&mut Power>>,
    mut noises: MessageWriter<Noise>,
    mut commands: Commands,
) {
    let Some(mut power) = power else {
        return;
    };
    let Some(mut generator) = generator.filter(|_| !power.is_added()) else {
        commands.insert_resource(Generator(FULL));
        return;
    };
    let map = &bank.0;
    let lamps = power.lamps(map);
    let indoor: Vec<bool> = map.lights.iter().map(|l| map.lamp_indoors(l)).collect();
    let total = indoor.iter().filter(|&&i| i).count().max(1) as f32;
    let on = lamps
        .iter()
        .zip(&indoor)
        .filter(|(on, i)| **on && **i)
        .count() as f32;
    let dt = time.delta_secs();
    generator.0 = (generator.0 + (RECOVER - DRAIN * on / total) * dt).clamp(0.0, FULL);
    if generator.0 <= 0.0 && !power.tripped {
        info!("the breaker trips");
        power.tripped = true;
        noises.write(Noise {
            clip: Some(Clip::Thud),
            at: power.fuse,
            loud: LOUD,
        });
    }
}

/// Sight follows the lamps (and a new `Sight`, after a smash, again).
fn follow(bank: Res<Bank>, power: Option<Single<Ref<Power>>>, mut sight: ResMut<Sight>) {
    let Some(power) = power else {
        return;
    };
    if power.is_changed() || sight.is_added() {
        sight.set_lit(bank.0.lit_with(&power.lamps(&bank.0)));
    }
}

/// The lamps' power in every world material, and the lit rooms in the
/// shader's texture.
#[cfg(feature = "client")]
fn relight(
    bank: Res<Bank>,
    power: Option<Single<Ref<Power>>>,
    world_materials: Res<WorldMaterials>,
    mut materials: ResMut<Assets<WorldMaterial>>,
    mut images: ResMut<Assets<Image>>,
) {
    let Some(power) = power else {
        return;
    };
    if !power.is_changed() && !world_materials.is_changed() {
        return;
    }
    let map = &bank.0;
    let lamps = power.lamps(map);
    for handle in &world_materials.all {
        if let Some(mut m) = materials.get_mut(handle) {
            for (i, (l, &on)) in map
                .lights
                .iter()
                .zip(&lamps)
                .enumerate()
                .take(m.params.lights.len() / 2)
            {
                m.params.lights[2 * i].w = if on { l.intensity } else { 0.0 };
            }
        }
    }
    let lit = map.lit_with(&lamps);
    if let Some(mut image) = images.get_mut(&world_materials.extra)
        && let Some(data) = image.data.as_mut()
    {
        for (i, &l) in lit.iter().enumerate() {
            data[i * 4 + 3] = if l { 255 } else { 0 };
        }
    }
}

/// The switch or fuse box within his reach, if nothing else wants E.
#[allow(clippy::too_many_arguments)]
fn offer(
    keys: Res<ButtonInput<KeyCode>>,
    script: Res<crate::dev::Script>,
    switched: Res<Switches>,
    me: Single<&Player, Living>,
    power: Option<Single<&Power>>,
    mut prompt: ResMut<crate::prompt::Prompt>,
    mut flips: MessageWriter<Flip>,
    mut resets: MessageWriter<Reset>,
) {
    let Some(power) = power else {
        return;
    };
    if prompt.0.is_some() {
        return;
    }
    let pos = me.body.pos;
    let pressed = keys.just_pressed(KeyCode::KeyE) || script.pressed("Use");
    if power.tripped && power.fuse.distance(pos) < REACH {
        prompt.0 = Some((power.fuse + vec2(0.0, 30.0), "Reset the breaker"));
        if pressed {
            resets.write(Reset);
        }
        return;
    }
    let Some(&(r, at)) = switched
        .0
        .iter()
        .filter(|s| s.1.distance(pos) < REACH)
        .min_by(|a, b| a.1.distance(pos).total_cmp(&b.1.distance(pos)))
    else {
        return;
    };
    let on = power.rooms[r] && !power.tripped;
    prompt.0 = Some((
        at + vec2(0.0, 30.0),
        if on { "Lights off" } else { "Lights on" },
    ));
    if pressed {
        flips.write(Flip(r as u16));
    }
}

/// The switches and the fuse box drawn (again for each new bank), showing
/// whether each is on.
#[cfg(feature = "client")]
pub fn dress(
    switched: Option<Res<Switches>>,
    power: Option<Single<&Power>>,
    marks: Query<(), With<Mark>>,
    mut meshes: ResMut<Assets<Mesh>>,
    world_materials: Res<WorldMaterials>,
    mut commands: Commands,
) {
    let (Some(switched), Some(power)) = (switched, power) else {
        return;
    };
    if !marks.is_empty() {
        return;
    }
    let mut spawn = |mark: Mark, at: Vec2, mesh: Mesh| {
        commands.spawn((
            WorldPart,
            mark,
            Mesh2d(meshes.add(mesh)),
            MeshMaterial2d(things_material(&world_materials)),
            Transform::from_translation(at.extend(4.0)),
        ));
    };
    for &(r, at) in &switched.0 {
        spawn(Mark::Switch(r), at, paint::switch(power.rooms[r]));
    }
    spawn(Mark::Fuse, power.fuse, paint::fuse_box(power.tripped));
}

fn looks(
    power: Option<Single<Ref<Power>>>,
    mut marks: Query<(&Mark, &Mesh2d)>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    let Some(power) = power.filter(|p| p.is_changed()) else {
        return;
    };
    for (mark, mesh) in &mut marks {
        let look = match *mark {
            Mark::Switch(r) => paint::switch(power.rooms[r] && !power.tripped),
            Mark::Fuse => paint::fuse_box(power.tripped),
        };
        if let Some(mut m) = meshes.get_mut(&mesh.0) {
            *m = look;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::{generate, tile_of};

    #[test]
    fn every_lit_room_has_a_switch() {
        for seed in 0..12 {
            let map = generate(seed);
            let lit: std::collections::HashSet<usize> =
                map.lights.iter().filter_map(|l| map.lamp_room(l)).collect();
            let switched = Switches::new(&map).0;
            assert_eq!(switched.len(), lit.len(), "seed {seed}");
            for (r, at) in switched {
                if seed == 6 {
                    println!("{:?} {r}: {:?}", map.rooms[r].kind, tile_of(at));
                }
            }
            let fuse = Power::new(&map, seed).fuse;
            if seed == 6 {
                println!("fuse box {:?}", tile_of(fuse));
            }
        }
    }
}
