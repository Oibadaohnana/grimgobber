//! The wraith that haunts the bank, and that the robbers come to catch.
//! Unseen, it keeps to its room, now and then a trip out of it. When it sees
//! or hears a robber it rolls against his sanity (`haunt_chance`), at most
//! every `ROLL_EACH` for him and `ROLL_GAP` for anyone: a haunt, or a sign he
//! can see (footprints coming at him, a door swung open and slammed, the
//! lights flickering round him). In a haunt it shows itself, the lamps and
//! flashlights round it flicker, and after `WARN` it hunts whoever it sees or
//! hears: its touch kills.
//!
//! Its name has five letters, one on each of five notes about the bank
//! (`Note`, read by walking over it). With all five found, a robber may speak
//! (T, `talk::Say`): the right name forces a long haunt at him and binds it
//! (`Wraith::named`), and a bound wraith shown over a set trap is caught
//! (`trap::Trap`), which wins the round. A wrong word costs his sanity.
//!
//! Its mind (`Mind`, the name in it) is the server's; the players get where
//! it is and what it shows (`Wraith`), the notes and the footprints (`Prints`).

use crate::doors::{Leaf, Shut};
use crate::hearing::feet;
use crate::map::{Map, Room, tile_centre};
use crate::movement::{Body, STEP, Solids, Steer, TILE, approach};
use crate::net::{Owner, sent_by};
use crate::paths::Walkable;
use crate::player::{Controls, Dead, Level, LocalPlayer, Player};
use crate::render::{WorldMaterial, WorldMaterials, WorldPart, ghost_material, things_material};
use crate::rng::Rng;
use crate::sanity::Sanity;
use crate::sight::{Sight, VIEW};
use crate::smash::Bank;
use crate::sound::Noise;
use crate::talk::Say;
use crate::trap::Trap;
use crate::{dev, paint};
use bevy::prelude::*;
use bevy_replicon::prelude::{ClientState, FromClient, Replicated};
use bevy_replicon::shared::backend::connected_client::NetworkId;
use serde::{Deserialize, Serialize};
use std::f32::consts::TAU;

/// It sees all round this far, in tiles: a robber in the light (a lamp's or
/// his own flashlight's), and one in the dark.
const SEES: f32 = 8.0;
const SEES_DARK: f32 = 4.0;
const ROLL_EACH: f32 = 10.0;
const ROLL_GAP: f32 = 4.0;
/// A haunt: shown and still for `WARN` seconds, then hunting for `HUNT`;
/// after one, no other for `RESPITE`.
const WARN: f32 = 3.0;
const HUNT: f32 = 20.0;
const RESPITE: f32 = 30.0;
/// How far the lights flicker, in tiles: round it in a haunt, round a robber
/// at a sign (for `SIGN_FLICKER_FOR` seconds).
const HAUNT_FLICKER: f32 = 8.0;
const SIGN_FLICKER: f32 = 4.0;
const SIGN_FLICKER_FOR: f32 = 2.5;
/// This close in a hunt, it kills.
const TOUCH: f32 = 0.7 * TILE;
/// Sanity lost seeing a sign, and being near as a haunt starts.
const SIGN_SHOCK: f32 = 5.0;
const HAUNT_SHOCK: f32 = 15.0;
/// Footprints: so far apart, the last so many, stopping so short of him.
const STRIDE: f32 = 0.45 * TILE;
const PRINTS: usize = 24;
const SHORT: f32 = 1.5 * TILE;
/// Each footprint shows this long after the one before, and lasts so long.
const PRINT_EVERY: f32 = 0.3;
const PRINT_LASTS: f32 = 12.0;
/// Its name spoken, it hunts this long; a wrong word costs so much sanity.
const NAMED_HUNT: f32 = 40.0;
const WRONG_NAME: f32 = 5.0;
/// A robber this close reads a note; a trap this close to it, shown, takes it.
const READS: f32 = 0.6 * TILE;
const TRAPS: f32 = 0.8 * TILE;
/// Its name is one of these.
const NAMES: [&str; 17] = [
    "AGNES", "EDGAR", "EDITH", "ETHEL", "HAZEL", "MABEL", "MAUDE", "CLARA", "GRETA", "IRENE",
    "SILAS", "ELIAS", "FELIX", "OSCAR", "HENRY", "NORAH", "BERTA",
];
pub const NAME_LEN: usize = 5;

/// What the players get of it.
#[derive(Component, Serialize, Deserialize)]
pub struct Wraith {
    pub pos: Vec2,
    pub heading: f32,
    /// Showing itself, in a haunt.
    pub shown: bool,
    /// Where the lights flicker, and how far round, in tiles.
    pub flicker: Option<(Vec2, f32)>,
    /// The letters of its name found so far, in the order found.
    pub letters: Vec<char>,
    /// Its name spoken: a trap can take it.
    pub named: bool,
    pub caught: bool,
}

/// A note with a letter of its name on it, lying where it was left.
#[derive(Component, Serialize, Deserialize)]
pub struct Note {
    pub at: Vec2,
    pub letter: char,
}

/// Footprints appearing one after another: where, and which way the toes point.
#[derive(Message, Clone, Serialize, Deserialize)]
pub struct Prints(pub Vec<(Vec2, f32)>);

/// Its mind, the server's alone.
#[derive(Component)]
struct Mind {
    name: &'static str,
    home: Room,
    body: Body,
    /// Tile centres to go through, the next last.
    path: Vec<Vec2>,
    rng: Rng,
    /// Seconds before it moves on, and stuck on its way.
    idle: f32,
    stuck: f32,
    /// Seconds till each robber may be rolled for again, and till anyone may.
    waits: Vec<(Entity, f32)>,
    gap: f32,
    /// Seconds into a haunt, how long it hunts in it, and till it next
    /// picks its way.
    haunt: Option<f32>,
    hunt: f32,
    retarget: f32,
    /// Whom it hunts, and where he was last seen or heard.
    prey: Option<Entity>,
    last: Option<Vec2>,
    respite: f32,
    /// Seconds left of a sign's flicker.
    flicker: f32,
}

#[derive(Clone, Copy, Debug)]
enum Sign {
    Prints,
    Door,
    Lights,
}

/// A footprint, seconds since it showed (below 0: not yet).
#[derive(Component)]
struct Print {
    age: f32,
    left: bool,
}

/// The footprints' pictures, left and right, fading in three steps.
#[derive(Resource)]
struct PrintLooks([[Handle<Mesh>; 3]; 2]);

pub struct WraithPlugin;

impl Plugin for WraithPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<Prints>()
            .add_systems(
                FixedUpdate,
                walk.run_if(
                    resource_exists::<Level>
                        .and_then(resource_exists::<Walkable>)
                        .and_then(in_state(ClientState::Disconnected)),
                ),
            )
            .add_systems(
                Update,
                (read, hear, sense).chain().distributive_run_if(
                    resource_exists::<Level>
                        .and_then(resource_exists::<Sight>)
                        .and_then(resource_exists::<Walkable>)
                        .and_then(resource_exists::<Shut>)
                        .and_then(in_state(ClientState::Disconnected)),
                ),
            );
        if crate::drawn(app) {
            app.add_systems(Startup, looks)
                .add_systems(Update, (show, prints, age, ghosts, pale));
        }
    }
}

/// The chance a roll comes up a haunt, at `sanity` percent: 3% sane, 53% at none.
pub fn haunt_chance(sanity: f32) -> f32 {
    0.03 + 0.5 * (1.0 - sanity / 100.0).powi(2)
}

/// How bright flickering lights are at `t` seconds: mostly on, dropping out in fits.
pub fn flicker_level(t: f32) -> f32 {
    match paint::noise1((t * 14.0).floor()) {
        n if n < 0.3 => 0.05,
        n if n < 0.5 => 0.45,
        _ => 1.0,
    }
}

pub fn spawn_wraith(commands: &mut Commands, map: &Map, seed: u64) {
    let mut rng = Rng::new(seed ^ 0x7a17);
    let name = rng.pick(&NAMES);
    let walkable = Walkable::new(map);
    let rooms: Vec<Room> = map
        .rooms
        .iter()
        .copied()
        .filter(|r| !spots(&walkable, r).is_empty())
        .collect();
    let home = rng.pick(&rooms);
    let at = dev::tile("BB_WRAITH").unwrap_or_else(|| rng.pick(&spots(&walkable, &home)));
    info!("the wraith: {name}, at home in a {:?}", home.kind);
    // Its letters on notes, each on a free tile somewhere in the bank.
    let mut tiles: Vec<Vec2> = rooms.iter().flat_map(|r| spots(&walkable, r)).collect();
    rng.shuffle(&mut tiles);
    let mut letters: Vec<char> = name.chars().collect();
    rng.shuffle(&mut letters);
    for (&at, letter) in dev::tile("BB_NOTES")
        .map(|t| {
            (0..NAME_LEN)
                .map(|i| t + vec2(i as f32 * TILE, 0.0))
                .collect()
        })
        .unwrap_or(tiles)
        .iter()
        .zip(letters)
    {
        commands.spawn((WorldPart, Replicated, Note { at, letter }));
    }
    commands.spawn((
        WorldPart,
        Replicated,
        Wraith {
            pos: at,
            heading: 0.0,
            shown: false,
            flicker: None,
            letters: Vec::new(),
            named: false,
            caught: false,
        },
        Mind {
            name,
            home,
            body: Body::new(at, 0.0),
            path: Vec::new(),
            rng,
            idle: 2.0,
            stuck: 0.0,
            waits: Vec::new(),
            gap: ROLL_GAP,
            haunt: None,
            hunt: HUNT,
            retarget: 0.0,
            prey: None,
            last: None,
            respite: 0.0,
            flicker: 0.0,
        },
    ));
}

/// The free tiles of a room, their centres.
fn spots(walkable: &Walkable, r: &Room) -> Vec<Vec2> {
    (r.y0..=r.y1)
        .flat_map(|y| (r.x0..=r.x1).map(move |x| (x, y)))
        .filter(|&(x, y)| walkable.is_free(x, y))
        .map(|(x, y)| tile_centre(x, y))
        .collect()
}

/// One fixed step along `path` (the next last): on to a corner's tile from
/// a third of a tile out, all the way to the last; creeping at a crouch's pace.
fn step_along(body: &mut Body, path: &mut Vec<Vec2>, creep: bool, solids: &Solids) {
    while let Some(&next) = path.last() {
        let close = if path.len() > 1 { TILE / 3.0 } else { 2.0 };
        if next.distance(body.pos) > close {
            break;
        }
        path.pop();
    }
    let walk = path.last().map(|&p| (p - body.pos).to_angle());
    body.set_steer(Steer {
        walk,
        aim: walk.unwrap_or(body.heading),
        crouch: creep,
        ..default()
    });
    body.update(STEP, solids);
}

/// Where it goes next: a spot in its room, now and then in another.
fn roam(m: &mut Mind, map: &Map, walkable: &Walkable) -> Option<Vec2> {
    let room = if m.rng.chance(0.85) {
        m.home
    } else {
        m.rng.pick(&map.rooms)
    };
    let spots = spots(walkable, &room);
    (!spots.is_empty()).then(|| m.rng.pick(&spots))
}

fn walk(
    level: Res<Level>,
    walkable: Res<Walkable>,
    bank: Res<Bank>,
    mut wraiths: Query<(&mut Mind, &mut Wraith)>,
) {
    for (mut m, mut w) in &mut wraiths {
        let m = &mut *m;
        if w.caught {
            continue;
        }
        if m.haunt.is_none() && m.path.is_empty() {
            m.idle -= STEP;
            if m.idle <= 0.0 {
                m.idle = 2.0 + 4.0 * m.rng.f32();
                if let Some(to) = roam(m, &bank.0, &walkable) {
                    m.path = walkable.path(m.body.pos, to);
                }
            }
        }
        // Shown and still while it warns.
        let still = m.haunt.is_some_and(|t| t < WARN);
        let was = m.body.pos;
        let mut none = Vec::new();
        let path = if still { &mut none } else { &mut m.path };
        step_along(&mut m.body, path, m.haunt.is_none(), &level.solids);
        // Pushing against something it cannot pass, it gives up the way.
        m.stuck = if !m.path.is_empty() && m.body.pos.distance(was) < 0.2 {
            m.stuck + STEP
        } else {
            0.0
        };
        if m.stuck > 2.0 {
            m.path.clear();
        }
        // Untouched unless it moves: a changed component goes to every player.
        if w.pos != m.body.pos {
            w.pos = m.body.pos;
        }
        if w.heading != m.body.heading {
            w.heading = m.body.heading;
        }
    }
}

fn end_haunt(m: &mut Mind, w: &mut Wraith) {
    m.haunt = None;
    m.prey = None;
    m.last = None;
    m.path.clear();
    m.respite = RESPITE;
    w.shown = false;
    w.flicker = None;
}

#[allow(clippy::too_many_arguments)]
fn sense(
    time: Res<Time>,
    sight: Res<Sight>,
    level: Res<Level>,
    walkable: Res<Walkable>,
    shut: Res<Shut>,
    mut noises: MessageReader<Noise>,
    mut robbers: Query<(Entity, &Player, &Controls, &mut Sanity), Without<Dead>>,
    mut leaves: Query<&mut Leaf>,
    mut traps: Query<&mut Trap>,
    mut wraiths: Query<(&mut Mind, &mut Wraith)>,
    mut prints: MessageWriter<Prints>,
    mut commands: Commands,
) {
    let dt = time.delta_secs();
    let doors = &shut.0;
    let sounds: Vec<(Vec2, f32)> = noises
        .read()
        .filter(|n| n.loud > 0.0)
        .map(|n| (n.at, n.loud))
        .collect();
    for (mut m, mut w) in &mut wraiths {
        let (m, w) = (&mut *m, &mut *w);
        if w.caught {
            continue;
        }
        let at = m.body.pos;
        m.gap -= dt;
        m.respite -= dt;
        for wait in &mut m.waits {
            wait.1 -= dt;
        }
        if m.flicker > 0.0 {
            m.flicker -= dt;
            if m.flicker <= 0.0 && m.haunt.is_none() {
                w.flicker = None;
            }
        }
        if let Some(t) = &mut m.haunt {
            *t += dt;
            if *t >= WARN + m.hunt {
                end_haunt(m, w);
                continue;
            }
        }
        if m.haunt.is_none() && m.gap > 0.0 {
            continue;
        }
        let noticed = notice(m, &sight, &level, doors, &sounds, &robbers);

        if let Some(t) = m.haunt {
            w.flicker = Some((at, HAUNT_FLICKER));
            // Bound and shown, over a set trap: caught.
            if w.named
                && let Some(mut trap) = traps
                    .iter_mut()
                    .find(|t| !t.sprung && t.at.distance(at) < TRAPS)
            {
                info!("the wraith is caught");
                trap.sprung = true;
                end_haunt(m, w);
                w.caught = true;
                continue;
            }
            if t < WARN {
                continue;
            }
            if let Some((e, ..)) = robbers.iter().find(|r| r.1.body.pos.distance(at) < TOUCH) {
                info!("the wraith caught a robber");
                commands.entity(e).insert(Dead);
                end_haunt(m, w);
                continue;
            }
            let seen = noticed
                .iter()
                .find(|n| Some(n.0) == m.prey)
                .or(noticed.first());
            if let Some(&(e, p)) = seen {
                m.prey = Some(e);
                m.last = Some(p);
            }
            m.retarget -= dt;
            if m.retarget <= 0.0 {
                m.retarget = 0.3;
                match m.last {
                    Some(p) if p.distance(at) > TILE * 0.5 => m.path = walkable.path_to(at, p),
                    _ if m.path.is_empty() => {
                        // Lost him: it casts about where it is.
                        m.last = None;
                        let off = Vec2::from_angle(m.rng.f32() * TAU) * 3.0 * TILE;
                        m.path = walkable.path(at, at + off);
                    }
                    _ => {}
                }
            }
            continue;
        }

        // Not haunting: a roll for the first it notices who may be rolled for.
        let ready = |e: Entity| m.waits.iter().all(|w| w.0 != e || w.1 <= 0.0);
        let Some(&(e, p)) = noticed.iter().find(|n| ready(n.0)) else {
            continue;
        };
        m.waits.retain(|w| w.0 != e);
        m.waits.push((e, ROLL_EACH));
        m.gap = ROLL_GAP;
        let sanity = robbers.get(e).map_or(100.0, |r| r.3.0);
        if m.respite <= 0.0 && m.rng.chance(haunt_chance(sanity)) {
            info!("the wraith haunts, at {sanity:.0}% sanity");
            m.haunt = Some(0.0);
            m.hunt = HUNT;
            m.prey = Some(e);
            m.last = Some(p);
            m.path.clear();
            m.retarget = 0.0;
            w.shown = true;
            w.flicker = Some((at, HAUNT_FLICKER));
            for (.., r, _, mut s) in &mut robbers {
                if r.body.pos.distance(at) < HAUNT_FLICKER * TILE {
                    s.lose(HAUNT_SHOCK);
                }
            }
            continue;
        }
        // A sign where he can see it.
        let door = leaves
            .iter_mut()
            .filter(|l| {
                l.is_shut()
                    && l.centre().distance(p) < 7.0 * TILE
                    && sight.seen(p, l.centre(), VIEW, false, doors) > 0.3
            })
            .min_by(|a, b| a.centre().distance(p).total_cmp(&b.centre().distance(p)));
        let mut way: Vec<Vec2> = walkable.path(at, p).into_iter().rev().collect();
        way.insert(0, at);
        let trail = footprints(&way, p);
        let mut signs = vec![Sign::Lights];
        if door.is_some() {
            signs.push(Sign::Door);
        }
        if trail.len() >= 4 {
            signs.push(Sign::Prints);
        }
        let sign = m.rng.pick(&signs);
        info!("the wraith shows a sign: {sign:?}");
        let seen_at = match sign {
            Sign::Door => {
                let mut leaf = door.unwrap();
                leaf.haunt();
                leaf.centre()
            }
            Sign::Prints => {
                let end = trail.last().unwrap().0;
                prints.write(Prints(trail));
                end
            }
            Sign::Lights => {
                m.flicker = SIGN_FLICKER_FOR;
                w.flicker = Some((p, SIGN_FLICKER));
                p
            }
        };
        for (_, r, _, mut s) in &mut robbers {
            let b = &r.body;
            if sight.seen(b.pos, seen_at, VIEW, b.is_crouching(), doors) > 0.5 {
                s.lose(SIGN_SHOCK);
            }
        }
    }
}

/// A living robber walking over a note reads it: its letter is found.
fn read(
    robbers: Query<&Player, Without<Dead>>,
    notes: Query<(Entity, &Note)>,
    mut wraiths: Query<&mut Wraith>,
    mut commands: Commands,
) {
    for (e, note) in &notes {
        if robbers.iter().any(|p| p.body.pos.distance(note.at) < READS) {
            for mut w in &mut wraiths {
                w.letters.push(note.letter);
            }
            commands.entity(e).despawn();
        }
    }
}

/// A robber who may have spoken: who he is, where, and his sanity.
type Speaker<'a> = (
    Entity,
    &'a Player,
    Option<&'a Owner>,
    Has<LocalPlayer>,
    &'a mut Sanity,
);

/// A word spoken, with every letter found: its name forces a haunt at the
/// speaker and binds it; anything else costs him.
fn hear(
    mut said: MessageReader<FromClient<Say>>,
    ids: Query<&NetworkId>,
    mut robbers: Query<Speaker, Without<Dead>>,
    mut wraiths: Query<(&mut Mind, &mut Wraith)>,
) {
    for FromClient { client_id, message } in said.read() {
        let Some((e, p, .., mut sanity)) = robbers
            .iter_mut()
            .find(|r| sent_by(*client_id, &ids, r.2, r.3))
        else {
            continue;
        };
        for (mut m, mut w) in &mut wraiths {
            if w.caught || w.letters.len() < NAME_LEN {
                continue;
            }
            if !message.0.trim().eq_ignore_ascii_case(m.name) {
                sanity.lose(WRONG_NAME);
                continue;
            }
            info!("its name is spoken: {}", m.name);
            w.named = true;
            w.shown = true;
            m.haunt = Some(0.0);
            m.hunt = NAMED_HUNT;
            m.prey = Some(e);
            m.last = Some(p.body.pos);
            m.path.clear();
            m.retarget = 0.0;
        }
    }
}

/// The robbers it sees or hears now, the nearest first: seen all round, in
/// a thicket only one it is after; heard by his feet, or the nearest robber
/// to a noise that reaches it.
fn notice(
    m: &Mind,
    sight: &Sight,
    level: &Level,
    doors: &[u8],
    sounds: &[(Vec2, f32)],
    robbers: &Query<(Entity, &Player, &Controls, &mut Sanity), Without<Dead>>,
) -> Vec<(Entity, Vec2)> {
    let at = m.body.pos;
    let ear = sight.index(at);
    let carries = |from: Vec2, loud: f32| {
        from.distance(at) < (loud + 1.0) * 1.5 * TILE
            && ear.is_some_and(|i| sight.carry(from, loud, doors)[i] > 0.0)
    };
    let nearest_to = |p: Vec2| {
        robbers
            .iter()
            .min_by(|a, b| {
                a.1.body
                    .pos
                    .distance(p)
                    .total_cmp(&b.1.body.pos.distance(p))
            })
            .map(|r| r.0)
    };
    let noisy: Vec<Entity> = sounds
        .iter()
        .filter(|&&(p, loud)| carries(p, loud))
        .filter_map(|&(p, _)| nearest_to(p))
        .collect();
    let mut noticed: Vec<(Entity, Vec2)> = robbers
        .iter()
        .filter(|(e, r, c, _)| {
            let b = &r.body;
            let reach = if c.torch || sight.lit_at(b.pos) {
                SEES
            } else {
                SEES_DARK
            };
            let hidden =
                level.hideout(b.pos).is_some_and(|h| !h.contains(at)) && m.prey != Some(*e);
            let seen = !hidden
                && b.pos.distance(at) < reach * TILE
                && sight.seen(b.pos, at, reach, b.is_crouching(), doors) > 0.5;
            seen || noisy.contains(e) || carries(b.pos, feet(b))
        })
        .map(|(e, r, ..)| (e, r.body.pos))
        .collect();
    noticed.sort_by(|a, b| a.1.distance(at).total_cmp(&b.1.distance(at)));
    noticed
}

/// Footprints along `way` (from the wraith) towards `end`, left and right
/// in turn, stopping short of it: the last `PRINTS` of them.
fn footprints(way: &[Vec2], end: Vec2) -> Vec<(Vec2, f32)> {
    let mut out = Vec::new();
    let mut into = 0.0;
    'walk: for seg in way.windows(2) {
        let d = seg[1] - seg[0];
        let len = d.length();
        if len < 1e-3 {
            continue;
        }
        let dir = d / len;
        let mut s = into;
        while s < len {
            let p = seg[0] + dir * s;
            if p.distance(end) < SHORT {
                break 'walk;
            }
            let side = if out.len() % 2 == 0 { 4.0 } else { -4.0 };
            out.push((p + dir.perp() * side, dir.to_angle()));
            s += STRIDE;
        }
        into = s - len;
    }
    let n = out.len().saturating_sub(PRINTS);
    out.split_off(n)
}

pub fn dress(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    world_materials: Res<WorldMaterials>,
    wraiths: Query<(Entity, &Wraith), Added<Wraith>>,
) {
    for (e, w) in &wraiths {
        commands.entity(e).insert((
            Mesh2d(meshes.add(paint::wraith(0.0))),
            MeshMaterial2d(things_material(&world_materials)),
            Transform::from_translation(w.pos.extend(9.5)),
            Visibility::Hidden,
        ));
    }
}

/// Shown only in a haunt, repainted every frame then: it never holds still.
fn show(
    time: Res<Time>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut wraiths: Query<(&Wraith, &Mesh2d, &mut Transform, &mut Visibility)>,
) {
    for (w, mesh, mut t, mut vis) in &mut wraiths {
        let want = if w.shown {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        if *vis != want {
            *vis = want;
        }
        let at = t.translation.truncate();
        let to = if at.distance(w.pos) < 2.0 * TILE {
            at + (w.pos - at) * approach(20.0, time.delta_secs())
        } else {
            w.pos
        };
        t.translation = to.extend(9.5);
        t.rotation = Quat::from_rotation_z(w.heading);
        if w.shown
            && let Some(mut m) = meshes.get_mut(&mesh.0)
        {
            *m = paint::wraith(time.elapsed_secs());
        }
    }
}

fn looks(mut meshes: ResMut<Assets<Mesh>>, mut commands: Commands) {
    let mut fading = |left| [1.0, 0.6, 0.3].map(|k| meshes.add(paint::footprint(left, k)));
    commands.insert_resource(PrintLooks([fading(true), fading(false)]));
}

fn prints(
    mut trails: MessageReader<Prints>,
    looks: Res<PrintLooks>,
    world_materials: Res<WorldMaterials>,
    mut commands: Commands,
) {
    for Prints(trail) in trails.read() {
        for (i, &(at, toes)) in trail.iter().enumerate() {
            let left = i % 2 == 0;
            commands.spawn((
                WorldPart,
                Print {
                    age: -(i as f32) * PRINT_EVERY,
                    left,
                },
                Mesh2d(looks.0[left as usize][0].clone()),
                MeshMaterial2d(things_material(&world_materials)),
                Transform::from_translation(at.extend(0.5))
                    .with_rotation(Quat::from_rotation_z(toes)),
                Visibility::Hidden,
            ));
        }
    }
}

/// Footprints show one by one, fade and go.
fn age(
    time: Res<Time>,
    looks: Res<PrintLooks>,
    mut prints: Query<(Entity, &mut Print, &mut Mesh2d, &mut Visibility)>,
    mut commands: Commands,
) {
    for (e, mut p, mut mesh, mut vis) in &mut prints {
        p.age += time.delta_secs();
        if p.age >= PRINT_LASTS {
            commands.entity(e).despawn();
        } else if p.age >= 0.0 {
            *vis = Visibility::Inherited;
            let step = ((p.age / PRINT_LASTS) * 3.0) as usize;
            let look = &looks.0[p.left as usize][step.min(2)];
            if mesh.0 != *look {
                mesh.0 = look.clone();
            }
        }
    }
}

pub fn dress_notes(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    world_materials: Res<WorldMaterials>,
    notes: Query<(Entity, &Note), Added<Note>>,
) {
    for (e, note) in &notes {
        let turn = paint::noise1(note.at.x + note.at.y) * TAU;
        commands.entity(e).insert((
            Mesh2d(meshes.add(paint::note())),
            MeshMaterial2d(things_material(&world_materials)),
            Transform::from_translation(note.at.extend(0.6))
                .with_rotation(Quat::from_rotation_z(turn)),
            Visibility::default(),
        ));
    }
}

/// The dead walk on unseen by the living; the dead see them.
fn ghosts(
    me: Option<Single<Has<Dead>, With<LocalPlayer>>>,
    mut robbers: Query<(&mut Visibility, Has<Dead>), With<Player>>,
) {
    let dead_me = me.is_some_and(|d| *d);
    for (mut vis, dead) in &mut robbers {
        let want = if dead && !dead_me {
            Visibility::Hidden
        } else {
            Visibility::Inherited
        };
        if *vis != want {
            *vis = want;
        }
    }
}

/// A robber who dies is drawn pale and see-through from then on.
fn pale(
    dead: Query<&Children, Added<Dead>>,
    mut parts: Query<&mut MeshMaterial2d<WorldMaterial>>,
    world_materials: Res<WorldMaterials>,
) {
    for children in &dead {
        for &part in children {
            if let Ok(mut m) = parts.get_mut(part) {
                m.0 = ghost_material(&world_materials);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::{RoomKind, generate, tile_of};

    #[test]
    fn a_sane_robber_is_seldom_haunted() {
        assert!((haunt_chance(100.0) - 0.03).abs() < 1e-6);
        assert!((haunt_chance(0.0) - 0.53).abs() < 1e-6);
        assert!(haunt_chance(50.0) < haunt_chance(25.0));
    }

    #[test]
    fn it_walks_from_the_street_into_the_vault() {
        for seed in 0..8 {
            let map = generate(seed);
            let v = *map
                .rooms
                .iter()
                .find(|r| r.kind == RoomKind::Vault)
                .unwrap();
            let mut path = Walkable::new(&map).path(map.spawn, tile_centre(v.x0, v.y0));
            let end = path[0];
            let solids = Solids::new(map.bounds(), map.solids());
            let mut body = Body::new(map.spawn, 0.0);
            for _ in 0..60 * 200 {
                step_along(&mut body, &mut path, true, &solids);
            }
            let at = tile_of(body.pos);
            assert!(body.pos.distance(end) < 3.0, "seed {seed}: stuck at {at:?}");
        }
    }

    #[test]
    fn footprints_come_at_him_and_stop_short() {
        let way = [Vec2::ZERO, vec2(20.0 * TILE, 0.0)];
        let end = way[1];
        let prints = footprints(&way, end);
        assert_eq!(prints.len(), PRINTS);
        assert!(prints.iter().all(|p| p.0.distance(end) >= SHORT - 5.0));
        assert!(prints[0].0.x < prints[1].0.x, "towards him");
    }
}
