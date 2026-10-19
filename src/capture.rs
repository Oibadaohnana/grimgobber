//! Catching it, each kind its own way; any other way enrages it
//! (`Mind::enrage`). A poltergeist, shown, shot `HITS` times within
//! `HIT_FOR` seconds goes down for `DOWN`, and a hunter with a coffer seals
//! it in with E (`Seal`). A shade, shown, held `BURN` seconds in a UV
//! lantern's beam (shining while the left button is held, its cone drawn
//! for all to see) burns away into it. A banshee is netted (trap.rs). Shots
//! are traced again on the server (`Shot`); the client draws its own.

use crate::doors::Shut;
use crate::guns;
use crate::inventory::{Inventory, Kind};
use crate::movement::TILE;
use crate::net::{Owner, sent_by};
use crate::paint;
use crate::player::{Controls, Dead, Living, LocalPlayer, Player, Shown};
use crate::render::WorldPart;
use crate::sight::Sight;
use crate::sound::Noise;
use crate::wraith::{Ghost, Mind, Wraith};
use bevy::prelude::*;
use bevy_replicon::prelude::{ClientState, FromClient};
use bevy_replicon::shared::backend::connected_client::NetworkId;
use serde::{Deserialize, Serialize};

const HITS: u32 = 3;
const HIT_FOR: f32 = 4.0;
const DOWN: f32 = 6.0;
const BURN: f32 = 3.0;
/// Any other kind held this long in the beam turns on whoever holds it.
const WRONG_BURN: f32 = 1.0;
/// The lantern's reach in tiles, and half its beam's angle.
const BURNS: f32 = 6.0;
const BEAM: f32 = 0.45;
const SEALS: f32 = 1.5 * TILE;

/// A shot fired, asked of the server: from the muzzle, along `dir`.
#[derive(Message, Serialize, Deserialize)]
pub struct Shot {
    pub from: Vec2,
    pub dir: Vec2,
}

/// The poltergeist lying by him sealed in his coffer, asked of the server.
#[derive(Message, Serialize, Deserialize)]
pub struct Seal;

pub struct CapturePlugin;

impl Plugin for CapturePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (shots, seal, burn).distributive_run_if(
                resource_exists::<Sight>
                    .and_then(resource_exists::<Shut>)
                    .and_then(in_state(ClientState::Disconnected)),
            ),
        );
        #[cfg(feature = "client")]
        if crate::drawn(app) {
            app.add_systems(Startup, beam_look).add_systems(
                Update,
                (
                    offer,
                    beams.run_if(resource_exists::<Sight>.and_then(resource_exists::<Shut>)),
                ),
            );
        }
    }
}

type Asker<'a> = (
    Entity,
    &'a Player,
    Option<&'a Owner>,
    Has<LocalPlayer>,
    &'a Inventory,
);

fn shots(
    mut asks: MessageReader<FromClient<Shot>>,
    ids: Query<&NetworkId>,
    sight: Res<Sight>,
    shut: Res<Shut>,
    hunters: Query<Asker, Without<Dead>>,
    mut wraiths: Query<(&mut Mind, &mut Wraith)>,
    mut noises: MessageWriter<Noise>,
) {
    for FromClient { client_id, message } in asks.read() {
        let Some((e, p, ..)) = hunters.iter().find(|r| sent_by(*client_id, &ids, r.2, r.3)) else {
            continue;
        };
        let (from, at) = (message.from, p.body.pos);
        if from.distance(at) > 2.0 * TILE {
            continue;
        }
        noises.write(Noise {
            clip: None,
            at: from,
            loud: guns::LOUD,
        });
        let dir = message.dir.normalize_or_zero();
        for (mut m, mut w) in &mut wraiths {
            if !w.shown || w.down || w.caught {
                continue;
            }
            if guns::shoot(&sight, &shut.0, from, dir, &[w.pos])
                .1
                .is_none()
            {
                continue;
            }
            if m.ghost != Ghost::Poltergeist {
                m.enrage(&mut w, e, at);
                continue;
            }
            m.hits += 1;
            m.hit_for = HIT_FOR;
            if m.hits >= HITS {
                info!("the poltergeist is down");
                m.hits = 0;
                m.down = DOWN;
                w.down = true;
            }
        }
    }
}

fn seal(
    mut asks: MessageReader<FromClient<Seal>>,
    ids: Query<&NetworkId>,
    hunters: Query<Asker, Without<Dead>>,
    mut wraiths: Query<&mut Wraith>,
) {
    for FromClient { client_id, .. } in asks.read() {
        let Some((_, p, .., inv)) = hunters.iter().find(|r| sent_by(*client_id, &ids, r.2, r.3))
        else {
            continue;
        };
        if !inv.items.iter().any(|i| i.kind == Kind::Coffer) {
            continue;
        }
        for mut w in &mut wraiths {
            if w.down && !w.caught && w.pos.distance(p.body.pos) < SEALS {
                info!("the poltergeist is sealed in a coffer");
                w.caught = true;
            }
        }
    }
}

/// A lit UV lantern on it while it shows itself: a shade burns away, any
/// other kind turns on him.
fn burn(
    time: Res<Time>,
    sight: Res<Sight>,
    shut: Res<Shut>,
    hunters: Query<(Entity, &Player, &Controls, &Inventory), Without<Dead>>,
    mut wraiths: Query<(&mut Mind, &mut Wraith)>,
) {
    let dt = time.delta_secs();
    for (mut m, mut w) in &mut wraiths {
        if w.caught {
            continue;
        }
        let lit = hunters.iter().find(|(_, p, c, inv)| {
            let b = &p.body;
            let off = w.pos - b.pos;
            w.shown
                && c.uv
                && inv.holds(Kind::Lantern)
                && off.length() < BURNS * TILE
                && Vec2::from_angle(b.heading).angle_to(off).abs() < BEAM
                && sight.seen(b.pos, w.pos, BURNS, false, &shut.0) > 0.5
        });
        m.burn = match lit {
            Some(_) => m.burn + dt,
            None => (m.burn - dt).max(0.0),
        };
        let shade = m.ghost == Ghost::Shade;
        if let Some((e, p, ..)) = lit {
            if shade && m.burn >= BURN {
                info!("the shade is burned into the lantern");
                w.caught = true;
            } else if !shade && m.burn >= WRONG_BURN {
                m.burn = 0.0;
                m.enrage(&mut w, e, p.body.pos);
            }
        }
        // Only a shade shows it burning.
        let tenths = if shade {
            (m.burn / BURN * 10.0).min(10.0) as u8
        } else {
            0
        };
        if w.burn != tenths {
            w.burn = tenths;
        }
    }
}

/// A lit UV lantern's beam, drawn for the hunter holding it.
#[derive(Component)]
struct Beam(Entity);

#[cfg(feature = "client")]
#[derive(Resource)]
struct BeamLook(Handle<Mesh>, Handle<ColorMaterial>);

#[cfg(feature = "client")]
fn beam_look(
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    mut commands: Commands,
) {
    commands.insert_resource(BeamLook(
        meshes.add(paint::beam(BEAM, {
            let mut violet = paint::hex(0x9a6cff).glow(1.4);
            violet.rgba[3] = 0.3;
            violet
        })),
        materials.add(ColorMaterial::default()),
    ));
}

/// Each lit lantern's beam out to where it stops, or `BURNS` tiles; gone when it goes out.
#[cfg(feature = "client")]
fn beams(
    sight: Res<Sight>,
    shut: Res<Shut>,
    look: Res<BeamLook>,
    hunters: Query<(Entity, &Shown, &Player, &Controls, &Inventory)>,
    mut drawn: Query<(Entity, &Beam, &mut Transform)>,
    mut commands: Commands,
) {
    let lit = |c: &Controls, inv: &Inventory| c.uv && inv.holds(Kind::Lantern);
    for (e, beam, mut t) in &mut drawn {
        let Some((_, shown, p, ..)) = hunters.get(beam.0).ok().filter(|r| lit(r.3, r.4)) else {
            commands.entity(e).despawn();
            continue;
        };
        let dir = Vec2::from_angle(p.body.heading);
        let from = shown.pos + dir * 16.0;
        let len = (1..=(BURNS * TILE / 4.0) as usize)
            .map(|k| k as f32 * 4.0)
            .find(|&d| sight.stops(from + dir * d, &shut.0))
            .unwrap_or(BURNS * TILE);
        *t = Transform::from_translation(from.extend(24.0))
            .with_rotation(Quat::from_rotation_z(p.body.heading))
            .with_scale(Vec3::splat(len));
    }
    for (e, .., c, inv) in &hunters {
        if lit(c, inv) && !drawn.iter().any(|d| d.1.0 == e) {
            commands.spawn((
                WorldPart,
                Beam(e),
                Mesh2d(look.0.clone()),
                MeshMaterial2d(look.1.clone()),
                Transform::from_scale(Vec3::ZERO),
            ));
        }
    }
}

/// The poltergeist down within his reach, and a coffer on him: E seals it.
fn offer(
    keys: Res<ButtonInput<KeyCode>>,
    script: Res<crate::dev::Script>,
    me: Single<(&Player, &Inventory), Living>,
    wraiths: Query<&Wraith>,
    mut prompt: ResMut<crate::prompt::Prompt>,
    mut seal: MessageWriter<Seal>,
) {
    let (p, inv) = *me;
    if prompt.0.is_some() || inv.open || !inv.items.iter().any(|i| i.kind == Kind::Coffer) {
        return;
    }
    let Some(w) = wraiths
        .iter()
        .find(|w| w.down && !w.caught && w.pos.distance(p.body.pos) < SEALS)
    else {
        return;
    };
    prompt.0 = Some((w.pos + vec2(0.0, 34.0), "Seal it in the coffer"));
    if keys.just_pressed(KeyCode::KeyE) || script.pressed("Use") {
        seal.write(Seal);
    }
}
