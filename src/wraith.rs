//! The wraith that haunts the bank, and that the robbers come to catch.
//! Unseen, it keeps to its room, now and then a trip out of it. When it sees
//! or hears a robber it rolls against his sanity (`haunt_chance`), at most
//! every `ROLL_EACH` for him and `ROLL_GAP` for anyone: a haunt, or a sign he
//! can see (footprints coming at him, a door swung open and slammed, the
//! lights flickering round him). In a haunt it shows itself, the lamps and
//! flashlights round it flicker, and after `WARN` it hunts whoever it sees or
//! hears: its touch kills.
//!
//! It is one of three kinds (`Ghost`), each showing three of six signs
//! (`Ghost::signs`): the crew tells which from what they see (phone.rs), and
//! catches it with that kind's tool (capture.rs, trap.rs); the wrong one
//! enrages it (`Mind::enrage`).
//!
//! Its name has five letters, one on each of five notes about the bank
//! (`Note`, read by walking over it, the letter put in its place). With all
//! five found, a robber may speak (T, `talk::Say`): the right name forces a
//! long haunt at him. A wrong word costs his sanity.
//!
//! Its mind (`Mind`, its name and kind in it) is the server's; the players
//! get where it is and what it shows (`Wraith`), the notes, the footprints
//! (`Prints`) and the other signs (`Omen`).

use crate::doors::{Leaf, Shut};
use crate::hearing::feet;
use crate::junk::Loose;
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
use crate::sound::{Clip, Noise};
use crate::talk::Say;
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
/// How near a noise a robber must be to have made it.
const MADE: f32 = 1.5 * TILE;
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
/// A robber this close reads a note.
const READS: f32 = 0.6 * TILE;
/// Frost lasts this long; a wail is heard this far, in tiles.
const FROST_LASTS: f32 = 20.0;
const WAIL: f32 = 14.0;
/// Its name is one of these.
const NAMES: [&str; 17] = [
    "AGNES", "EDGAR", "EDITH", "ETHEL", "HAZEL", "MABEL", "MAUDE", "CLARA", "GRETA", "IRENE",
    "SILAS", "ELIAS", "FELIX", "OSCAR", "HENRY", "NORAH", "BERTA",
];
const NAME_LEN: usize = 5;

/// What the players get of it.
#[derive(Component, Serialize, Deserialize)]
pub struct Wraith {
    pub pos: Vec2,
    pub heading: f32,
    /// Showing itself, in a haunt.
    pub shown: bool,
    /// Where the lights flicker, and how far round, in tiles.
    pub flicker: Option<(Vec2, f32)>,
    /// The letters of its name, those found so far.
    pub letters: [Option<char>; NAME_LEN],
    /// Its name spoken.
    pub named: bool,
    pub caught: bool,
    /// Shot down, lying still.
    pub down: bool,
    /// How far burned in a UV lantern's beam, in tenths.
    pub burn: u8,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Ghost {
    Poltergeist,
    Shade,
    Banshee,
}

impl Ghost {
    pub const ALL: [Ghost; 3] = [Ghost::Poltergeist, Ghost::Shade, Ghost::Banshee];

    pub fn signs(self) -> [Sign; 3] {
        match self {
            Ghost::Poltergeist => [Sign::Prints, Sign::Door, Sign::Thrown],
            Ghost::Shade => [Sign::Door, Sign::Lights, Sign::Frost],
            Ghost::Banshee => [Sign::Prints, Sign::Lights, Sign::Wail],
        }
    }

    /// What it is caught with.
    pub fn tool(self) -> &'static str {
        match self {
            Ghost::Poltergeist => "Pistol + coffer",
            Ghost::Shade => "UV lantern",
            Ghost::Banshee => "Net trap",
        }
    }

    /// How it is caught.
    pub fn catch(self) -> &'static str {
        match self {
            Ghost::Poltergeist => "shoot it down, seal it in a coffer",
            Ghost::Shade => "burn it in a UV lantern's beam",
            Ghost::Banshee => "lure it over a net trap, pull",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Sign {
    Prints,
    Door,
    Lights,
    Thrown,
    Frost,
    Wail,
}

impl Sign {
    pub const ALL: [Sign; 6] = [
        Sign::Prints,
        Sign::Door,
        Sign::Lights,
        Sign::Thrown,
        Sign::Frost,
        Sign::Wail,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Sign::Prints => "Footprints",
            Sign::Door => "A door slammed",
            Sign::Lights => "Flickering lights",
            Sign::Thrown => "Things thrown",
            Sign::Frost => "Frost on the floor",
            Sign::Wail => "A wail",
        }
    }
}

/// Signs the players see drawn or hear: frost where a robber stands, a wail where it is.
#[derive(Message, Clone, Serialize, Deserialize)]
pub enum Omen {
    Frost(Vec2),
    Wail(Vec2),
}

/// A note with a letter of its name on it, lying where it was left.
#[derive(Component, Serialize, Deserialize)]
pub struct Note {
    pub at: Vec2,
    pub letter: char,
    /// Its place in the name.
    pub place: usize,
}

impl Wraith {
    /// Its name, once every letter is found.
    pub fn name(&self) -> Option<String> {
        self.letters.iter().copied().collect()
    }
}

/// Footprints appearing one after another: where, and which way the toes point.
#[derive(Message, Clone, Serialize, Deserialize)]
pub struct Prints(pub Vec<(Vec2, f32)>);

/// Its mind, the server's alone.
#[derive(Component)]
pub struct Mind {
    name: &'static str,
    pub ghost: Ghost,
    /// Shot: hits in a row, seconds till they are forgotten, and seconds left down.
    pub hits: u32,
    pub hit_for: f32,
    pub down: f32,
    /// Seconds in a UV lantern's beam.
    pub burn: f32,
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
    /// It had him when he went into a hideout, so it follows him in.
    follows: bool,
    respite: f32,
    /// Seconds left of a sign's flicker.
    flicker: f32,
}

impl Mind {
    pub fn true_name(&self) -> &'static str {
        self.name
    }

    /// What it is up to, for the sim.
    pub fn state(&self, w: &Wraith) -> String {
        let doing = match self.haunt {
            _ if w.caught => "caught".into(),
            Some(t) if t < WARN => format!("haunting, showing itself: hunts in {:.1} s", WARN - t),
            Some(t) => format!("hunting, {:.0} s left", WARN + self.hunt - t),
            None if self.respite > 0.0 => format!("roaming, no haunt for {:.0} s", self.respite),
            None => "roaming".into(),
        };
        let doing = if w.down {
            format!("shot down, up in {:.1} s", self.down)
        } else {
            doing
        };
        let named = if w.named { ", named" } else { "" };
        format!(
            "{}, a {:?} (home a {:?}{named})\n{doing}",
            self.name, self.ghost, self.home.kind
        )
    }
}

impl Mind {
    /// It hunts `prey`, last at `at`, at once: a wrong tool, or shot down and up again.
    pub fn enrage(&mut self, w: &mut Wraith, prey: Entity, at: Vec2) {
        info!("the wraith is enraged");
        self.haunt = Some(WARN);
        self.hunt = HUNT;
        self.prey = Some(prey);
        self.last = Some(at);
        self.path.clear();
        self.retarget = 0.0;
        self.down = 0.0;
        self.hits = 0;
        w.down = false;
        w.shown = true;
        w.flicker = Some((self.body.pos, HAUNT_FLICKER));
    }
}

/// A footprint or frost, seconds since it showed (below 0: not yet): its
/// look (left foot, right, frost) and how long it lasts.
#[derive(Component)]
struct Print {
    age: f32,
    look: usize,
    lasts: f32,
}

/// The footprints' and frost's pictures, fading in three steps.
#[derive(Resource)]
struct PrintLooks([[Handle<Mesh>; 3]; 3]);

pub struct WraithPlugin;

impl Plugin for WraithPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<Prints>()
            .add_message::<Omen>()
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
                .add_systems(Update, (show, prints, omens, age, ghosts, pale));
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
    let mut letters: Vec<(usize, char)> = name.chars().enumerate().collect();
    rng.shuffle(&mut letters);
    for (&at, (place, letter)) in dev::tile("BB_NOTES")
        .map(|t| {
            (0..NAME_LEN)
                .map(|i| t + vec2(i as f32 * TILE, 0.0))
                .collect()
        })
        .unwrap_or(tiles)
        .iter()
        .zip(letters)
    {
        commands.spawn((WorldPart, Replicated, Note { at, letter, place }));
    }
    // Loose things about the rooms, for a poltergeist to throw.
    let mut free: Vec<Vec2> = rooms.iter().flat_map(|r| spots(&walkable, r)).collect();
    rng.shuffle(&mut free);
    for (i, &at) in free.iter().take(crate::junk::JUNK).enumerate() {
        let off = vec2(rng.f32() - 0.5, rng.f32() - 0.5) * 0.5 * TILE;
        commands.spawn((WorldPart, Replicated, Loose::new(at + off, i)));
    }
    let ghost = dev::env("BB_GHOST")
        .and_then(|g| {
            Ghost::ALL
                .into_iter()
                .find(|k| format!("{k:?}").eq_ignore_ascii_case(&g))
        })
        .unwrap_or_else(|| rng.pick(&Ghost::ALL));
    info!("the wraith is a {ghost:?}");
    commands.spawn((
        WorldPart,
        Replicated,
        Wraith {
            pos: at,
            heading: 0.0,
            shown: false,
            flicker: None,
            letters: [None; NAME_LEN],
            named: false,
            caught: false,
            down: false,
            burn: 0,
        },
        Mind {
            name,
            ghost,
            hits: 0,
            hit_for: 0.0,
            down: 0.0,
            burn: 0.0,
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
            follows: false,
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
        // Shown and still while it warns, and while it lies shot down.
        let still = m.haunt.is_some_and(|t| t < WARN) || m.down > 0.0;
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
    m.follows = false;
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
    mut wraiths: Query<(&mut Mind, &mut Wraith)>,
    mut loose: Query<&mut Loose>,
    mut prints: MessageWriter<Prints>,
    mut omens: MessageWriter<Omen>,
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
        m.hit_for -= dt;
        if m.hit_for <= 0.0 {
            m.hits = 0;
        }
        // Down, it lies still till it gets up, enraged, at whoever downed it.
        if m.down > 0.0 {
            m.down -= dt;
            if m.down <= 0.0
                && let (Some(prey), Some(last)) = (m.prey, m.last)
            {
                m.enrage(w, prey, last);
            }
            continue;
        }
        if let Some(t) = &mut m.haunt {
            *t += dt;
            if *t >= WARN + m.hunt {
                end_haunt(m, w);
                continue;
            }
        }
        // A banshee goes to see what it hears.
        let ear = sight.index(at);
        if m.ghost == Ghost::Banshee
            && m.haunt.is_none()
            && let Some(&(to, _)) = sounds.iter().find(|&&(s, loud)| {
                s.distance(at) < (loud + 1.0) * 1.5 * TILE
                    && ear.is_some_and(|i| sight.carry(s, loud, doors)[i] > 0.0)
            })
        {
            m.path = walkable.path(at, to);
        }
        if m.haunt.is_none() && m.gap > 0.0 {
            continue;
        }
        let noticed = notice(m, &sight, &level, doors, &sounds, &robbers);

        if let Some(t) = m.haunt {
            w.flicker = Some((at, HAUNT_FLICKER));
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
            let hiding = m
                .prey
                .and_then(|e| robbers.get(e).ok())
                .map(|r| r.1.body.pos);
            match (seen, hiding) {
                (Some(&(e, p)), _) => {
                    m.prey = Some(e);
                    m.last = Some(p);
                    m.follows = true;
                }
                (None, Some(p)) if m.follows && level.hideout(p).is_some() => m.last = Some(p),
                _ => m.follows = false,
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
        // A sign of its kind where he can see it, if one can be shown there.
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
        let thing = loose
            .iter_mut()
            .filter(|l| {
                l.at.distance(p) < 6.0 * TILE && sight.seen(p, l.at, VIEW, false, doors) > 0.3
            })
            .min_by(|a, b| a.at.distance(p).total_cmp(&b.at.distance(p)));
        let signs: Vec<Sign> = m
            .ghost
            .signs()
            .into_iter()
            .filter(|s| match s {
                Sign::Prints => trail.len() >= 4,
                Sign::Door => door.is_some(),
                Sign::Thrown => thing.is_some(),
                _ => true,
            })
            .collect();
        if signs.is_empty() {
            continue;
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
            // Flung at him, landing a tile short.
            Sign::Thrown => {
                let mut thing = thing.unwrap();
                let short = p + (thing.at - p).normalize_or_zero() * TILE;
                let to = crate::throw::landing(thing.at, short, &level.solids);
                thing.fling(to);
                to
            }
            Sign::Frost => {
                omens.write(Omen::Frost(p));
                p
            }
            Sign::Wail => {
                omens.write(Omen::Wail(at));
                at
            }
        };
        for (_, r, _, mut s) in &mut robbers {
            let b = &r.body;
            let heard = sign == Sign::Wail && b.pos.distance(at) < WAIL * TILE;
            if heard || sight.seen(b.pos, seen_at, VIEW, b.is_crouching(), doors) > 0.5 {
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
                w.letters[note.place] = Some(note.letter);
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
            if w.caught || w.name().is_none() {
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
            .map(|r| (r.0, r.1.body.pos.distance(p)))
    };
    // A noise gives away only who made it: a coin's landing or the breaker
    // nobody.
    let noisy: Vec<Entity> = sounds
        .iter()
        .filter(|&&(p, loud)| carries(p, loud))
        .filter_map(|&(p, _)| nearest_to(p).filter(|r| r.1 < MADE))
        .map(|r| r.0)
        .collect();
    let mut noticed: Vec<(Entity, Vec2)> = robbers
        .iter()
        .filter(|(e, r, c, _)| {
            let b = &r.body;
            let reach = if c.torch || c.uv || sight.lit_at(b.pos) {
                SEES
            } else {
                SEES_DARK
            };
            // In a bush or thicket he is not seen, and none is seen through:
            // a hunt follows him in only if it had him as he went (`follows`).
            let hidden = level.hideout(b.pos).is_some_and(|h| !h.contains(at));
            let seen = !hidden
                && !level.screened(b.pos, at)
                && b.pos.distance(at) < reach * TILE
                && sight.seen(b.pos, at, reach, b.is_crouching(), doors) > 0.5;
            // A bush hushes his feet.
            let heard = level.hideout(b.pos).is_none() && carries(b.pos, feet(b));
            seen || heard || noisy.contains(e)
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
/// Shot down, it lies flat; burning, it shrinks; caught, it is drawn into
/// the trap, shrinking away. In the sim it is always drawn, pale while it
/// does not show itself.
fn show(
    time: Res<Time>,
    sim: Option<Res<crate::sim::Sim>>,
    world_materials: Res<WorldMaterials>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut wraiths: Query<(
        &Wraith,
        &Mesh2d,
        &mut MeshMaterial2d<WorldMaterial>,
        &mut Transform,
        &mut Visibility,
    )>,
) {
    for (w, mesh, mut material, mut t, mut vis) in &mut wraiths {
        if w.caught {
            t.scale *= 1.0 - approach(3.0, time.delta_secs());
        } else {
            let k = 1.0 - 0.05 * w.burn as f32;
            t.scale = vec3(k, if w.down { 0.45 * k } else { k }, 1.0);
        }
        if sim.is_some() {
            let m = if w.shown {
                things_material(&world_materials)
            } else {
                ghost_material(&world_materials)
            };
            if material.0 != m {
                material.0 = m;
            }
        }
        let want = if (w.shown || sim.is_some()) && t.scale.x > 0.05 {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        if *vis != want {
            *vis = want;
        }
        let at = t.translation.truncate();
        let to = if at.distance(w.pos) < 2.0 * TILE {
            let rate = if w.caught { 6.0 } else { 20.0 };
            at + (w.pos - at) * approach(rate, time.delta_secs())
        } else {
            w.pos
        };
        t.translation = to.extend(9.5);
        t.rotation = Quat::from_rotation_z(w.heading);
        if *vis != Visibility::Hidden
            && let Some(mut m) = meshes.get_mut(&mesh.0)
        {
            *m = paint::wraith(time.elapsed_secs());
        }
    }
}

fn looks(mut meshes: ResMut<Assets<Mesh>>, mut commands: Commands) {
    let mut fading = |paint: &dyn Fn(f32) -> Mesh| [1.0, 0.6, 0.3].map(|k| meshes.add(paint(k)));
    commands.insert_resource(PrintLooks([
        fading(&|k| paint::footprint(true, k)),
        fading(&|k| paint::footprint(false, k)),
        fading(&paint::frost),
    ]));
}

fn prints(
    mut trails: MessageReader<Prints>,
    looks: Res<PrintLooks>,
    world_materials: Res<WorldMaterials>,
    mut commands: Commands,
) {
    for Prints(trail) in trails.read() {
        for (i, &(at, toes)) in trail.iter().enumerate() {
            let look = i % 2;
            commands.spawn((
                WorldPart,
                Print {
                    age: -(i as f32) * PRINT_EVERY,
                    look,
                    lasts: PRINT_LASTS,
                },
                Mesh2d(looks.0[look][0].clone()),
                MeshMaterial2d(things_material(&world_materials)),
                Transform::from_translation(at.extend(0.5))
                    .with_rotation(Quat::from_rotation_z(toes)),
                Visibility::Hidden,
            ));
        }
    }
}

/// Frost spreads where it was seen; a wail is heard.
fn omens(
    mut omens: MessageReader<Omen>,
    looks: Res<PrintLooks>,
    world_materials: Res<WorldMaterials>,
    mut noises: MessageWriter<Noise>,
    mut commands: Commands,
) {
    for omen in omens.read() {
        match *omen {
            Omen::Frost(at) => {
                commands.spawn((
                    WorldPart,
                    Print {
                        age: 0.0,
                        look: 2,
                        lasts: FROST_LASTS,
                    },
                    Mesh2d(looks.0[2][0].clone()),
                    MeshMaterial2d(things_material(&world_materials)),
                    Transform::from_translation(at.extend(0.4))
                        .with_rotation(Quat::from_rotation_z(paint::noise1(at.x) * TAU)),
                    Visibility::Hidden,
                ));
            }
            Omen::Wail(at) => {
                noises.write(Noise {
                    clip: Some(Clip::Wail),
                    at,
                    loud: 0.0,
                });
            }
        }
    }
}

/// Footprints and frost show, fade and go.
fn age(
    time: Res<Time>,
    looks: Res<PrintLooks>,
    mut prints: Query<(Entity, &mut Print, &mut Mesh2d, &mut Visibility)>,
    mut commands: Commands,
) {
    for (e, mut p, mut mesh, mut vis) in &mut prints {
        p.age += time.delta_secs();
        if p.age >= p.lasts {
            commands.entity(e).despawn();
        } else if p.age >= 0.0 {
            *vis = Visibility::Inherited;
            let step = ((p.age / p.lasts) * 3.0) as usize;
            let look = &looks.0[p.look][step.min(2)];
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
    fn three_signs_tell_the_kind() {
        for g in Ghost::ALL {
            let shows = |o: &&Ghost| g.signs().iter().all(|s| o.signs().contains(s));
            assert_eq!(Ghost::ALL.iter().filter(shows).count(), 1, "{g:?}");
        }
    }

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
