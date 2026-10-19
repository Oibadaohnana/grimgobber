//! The crew: who plays (`Crewmate`, one a player, kept by the server and
//! seen by all) and how each looks (`Look`, chosen in the lobby and kept in
//! `~/.config/grimgobber/player.txt`). And the rounds: when every crewmate
//! is ready the server builds a new bank with a robber for each, wearing his
//! look; a round ends when the wraith is caught, everyone in it is dead, or
//! everyone has gone back to the lobby (`Leave`).

use crate::map::{Map, tile_centre};
use crate::movement::TILE;
use crate::net::{Mode, Owner, Seed, client_u64, my_id};
use crate::paint::{Hex, hex};
use crate::player::{Dead, Player};
use crate::render::WorldPart;
use crate::wraith::Wraith;
use bevy::prelude::*;
use bevy_replicon::prelude::{ClientState, FromClient, Replicated};
use bevy_replicon::shared::backend::connected_client::NetworkId;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// A round's end shown this long: the wraith caught, or the crew dead.
const LINGER: f32 = 6.0;
pub const LONGEST_NAME: usize = 16;

pub const COLOURS: [Hex; 8] = [
    hex(0xb0302a),
    hex(0xd0782a),
    hex(0xd8b83a),
    hex(0x3f8a46),
    hex(0x2f8f8f),
    hex(0x2f5fb0),
    hex(0x7a3fa0),
    hex(0xc8507a),
];

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub enum Outfit {
    #[default]
    Stripes,
    Hoodie,
    Suit,
    Tracksuit,
}

impl Outfit {
    pub const ALL: [Outfit; 4] = [
        Outfit::Stripes,
        Outfit::Hoodie,
        Outfit::Suit,
        Outfit::Tracksuit,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Outfit::Stripes => "Stripes",
            Outfit::Hoodie => "Hoodie",
            Outfit::Suit => "Suit",
            Outfit::Tracksuit => "Tracksuit",
        }
    }
}

/// How a robber looks: on his crewmate in the lobby, and on him in the bank.
#[derive(Component, Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct Look {
    pub name: String,
    pub colour: usize,
    pub outfit: Outfit,
}

impl Default for Look {
    fn default() -> Self {
        Look {
            name: "Robber".into(),
            colour: 0,
            outfit: Outfit::Stripes,
        }
    }
}

impl Look {
    pub fn colour(&self) -> Hex {
        COLOURS[self.colour % COLOURS.len()]
    }
}

/// A player: his client id (0 alone), ready for the next round, in this one.
#[derive(Component, Serialize, Deserialize)]
#[require(Look)]
pub struct Crewmate {
    pub id: u64,
    pub ready: bool,
    pub in_round: bool,
}

/// Asked of the server: to look so, to be ready (or not), to leave the bank.
#[derive(Message, Serialize, Deserialize)]
pub struct SetLook(pub Look);

#[derive(Message, Serialize, Deserialize)]
pub struct Ready(pub bool);

#[derive(Message, Serialize, Deserialize)]
pub struct Leave;

/// The seed of the next bank.
#[derive(Resource)]
pub struct NextSeed(pub u64);

/// This machine's look, as saved.
#[derive(Resource, Clone, PartialEq)]
pub struct Settings(pub Look);

/// How loud this machine plays, 0 to 1 (the Esc menu's), saved beside the look.
#[derive(Resource, Clone, Copy, PartialEq)]
pub struct SoundLevel(pub f32);

pub struct CrewPlugin;

impl Plugin for CrewPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                alone,
                looks,
                readies,
                leaves,
                start,
                // The sim's bank stays, with or without hunters.
                finish.run_if(|mode: Res<Mode>| *mode != Mode::Sim),
            )
                .run_if(in_state(ClientState::Disconnected)),
        );
        if crate::drawn(app) {
            let (look, level) = load();
            app.insert_resource(Settings(look))
                .insert_resource(SoundLevel(level))
                .add_systems(Update, (introduce, save));
        }
    }
}

/// Playing alone, the one crewmate is this machine's; back at the title,
/// he goes.
fn alone(mode: Res<Mode>, crew: Query<(Entity, &Crewmate)>, mut commands: Commands) {
    if !mode.is_changed() {
        return;
    }
    let mine = crew.iter().find(|c| c.1.id == 0).map(|c| c.0);
    match (&*mode, mine) {
        (Mode::Alone, None) => {
            commands.spawn((
                Replicated,
                Crewmate {
                    id: 0,
                    ready: false,
                    in_round: false,
                },
            ));
        }
        (Mode::Title, Some(e)) => {
            commands.entity(e).despawn();
        }
        _ => {}
    }
}

/// Once this machine's crewmate is there, it gets the saved look; a smoke
/// run is ready at once (unless `BB_LOBBY` keeps it there).
fn introduce(
    settings: Res<Settings>,
    joined: Option<Res<crate::net::Joined>>,
    crew: Query<&Crewmate>,
    mut set: MessageWriter<SetLook>,
    mut ready: MessageWriter<Ready>,
    mut done: Local<bool>,
) {
    let me = my_id(joined.as_deref());
    // Again for each lobby joined.
    if !crew.iter().any(|c| c.id == me) {
        *done = false;
        return;
    }
    if std::mem::replace(&mut *done, true) {
        return;
    }
    set.write(SetLook(settings.0.clone()));
    if crate::dev::smoke_frames().is_some() && crate::dev::env("BB_LOBBY").is_none() {
        ready.write(Ready(true));
    }
}

fn mate<'a>(
    crew: &'a mut Query<(&mut Crewmate, &mut Look)>,
    id: u64,
) -> Option<(Mut<'a, Crewmate>, Mut<'a, Look>)> {
    crew.iter_mut().find(|c| c.0.id == id)
}

fn looks(
    mut asks: MessageReader<FromClient<SetLook>>,
    ids: Query<&NetworkId>,
    mut crew: Query<(&mut Crewmate, &mut Look)>,
) {
    for FromClient { client_id, message } in asks.read() {
        if let Some((_, mut look)) = mate(&mut crew, client_u64(*client_id, &ids)) {
            let mut new = message.0.clone();
            new.name = new.name.chars().take(LONGEST_NAME).collect();
            if *look != new {
                *look = new;
            }
        }
    }
}

fn readies(
    mut asks: MessageReader<FromClient<Ready>>,
    ids: Query<&NetworkId>,
    mut crew: Query<(&mut Crewmate, &mut Look)>,
) {
    for FromClient { client_id, message } in asks.read() {
        if let Some((mut c, _)) = mate(&mut crew, client_u64(*client_id, &ids))
            && c.ready != message.0
        {
            c.ready = message.0;
        }
    }
}

/// Back to the lobby: his robber goes, the round goes on without him.
fn leaves(
    mut asks: MessageReader<FromClient<Leave>>,
    ids: Query<&NetworkId>,
    mut crew: Query<(&mut Crewmate, &mut Look)>,
    robbers: Query<(Entity, &Owner), With<Player>>,
    mut commands: Commands,
) {
    for FromClient { client_id, .. } in asks.read() {
        let id = client_u64(*client_id, &ids);
        for (e, owner) in &robbers {
            if owner.0 == id {
                commands.entity(e).despawn();
            }
        }
        if let Some((mut c, _)) = mate(&mut crew, id) {
            c.in_round = false;
        }
    }
}

/// Every crewmate ready, and no round on: a new bank.
fn start(
    crew: Query<&Crewmate>,
    seeds: Query<(), With<Seed>>,
    mut next: ResMut<NextSeed>,
    mut new_map: ResMut<crate::NewMap>,
) {
    if crew.is_empty() || !seeds.is_empty() || new_map.0.is_some() || crew.iter().any(|c| !c.ready)
    {
        return;
    }
    new_map.0 = Some(next.0);
    next.0 = next
        .0
        .wrapping_mul(6364136223846793005)
        .wrapping_add(1442695040888963407)
        % 100_000;
}

/// The round over: everything built for it goes, and the crew is back in the lobby.
#[allow(clippy::too_many_arguments)]
fn finish(
    time: Res<Time>,
    seeds: Query<(), With<Seed>>,
    wraiths: Query<&Wraith>,
    robbers: Query<Has<Dead>, (With<Player>, With<Owner>)>,
    parts: Query<Entity, With<WorldPart>>,
    mut crew: Query<&mut Crewmate>,
    mut ending: Local<f32>,
    mut commands: Commands,
) {
    if seeds.is_empty() {
        *ending = 0.0;
        return;
    }
    let caught = wraiths.iter().any(|w| w.caught);
    let all_dead = !robbers.is_empty() && robbers.iter().all(|dead| dead);
    let over = if robbers.is_empty() {
        true
    } else if caught || all_dead {
        *ending += time.delta_secs();
        *ending >= LINGER
    } else {
        false
    };
    if !over {
        return;
    }
    info!("the round is over");
    *ending = 0.0;
    for e in &parts {
        commands.entity(e).despawn();
    }
    for mut c in &mut crew {
        c.ready = false;
        c.in_round = false;
    }
}

/// Where a round's robbers start: free tiles nearest `spawn`, a tile apart.
pub fn starts(map: &Map, spawn: Vec2, n: usize) -> Vec<Vec2> {
    let free = map.free();
    let mut tiles: Vec<Vec2> = (0..map.h)
        .flat_map(|y| (0..map.w).map(move |x| (x, y)))
        .filter(|&(x, y)| free[map.idx(x, y)])
        .map(|(x, y)| tile_centre(x, y))
        .collect();
    tiles.sort_by(|a, b| a.distance(spawn).total_cmp(&b.distance(spawn)));
    let mut picked: Vec<Vec2> = Vec::new();
    for t in tiles {
        if picked.len() == n {
            break;
        }
        if picked.iter().all(|p| p.distance(t) >= TILE) {
            picked.push(t);
        }
    }
    picked
}

fn path() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
    Some(base.join("grimgobber").join("player.txt"))
}

/// The saved look and sound level, or the defaults: lines of `name=`,
/// `colour=`, `outfit=`, `volume=` (percent, 80 unless saved).
fn load() -> (Look, f32) {
    let mut look = Look::default();
    let mut level = 0.8;
    let text = path()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .unwrap_or_default();
    for line in text.lines() {
        match line.split_once('=') {
            Some(("name", v)) if !v.trim().is_empty() => {
                look.name = v.trim().chars().take(LONGEST_NAME).collect();
            }
            Some(("colour", v)) => look.colour = v.trim().parse().unwrap_or(0) % COLOURS.len(),
            Some(("outfit", v)) => {
                if let Some(o) = Outfit::ALL.into_iter().find(|o| o.name() == v.trim()) {
                    look.outfit = o;
                }
            }
            Some(("volume", v)) => {
                level = v
                    .trim()
                    .parse::<f32>()
                    .map_or(level, |p| (p / 100.0).clamp(0.0, 1.0));
            }
            _ => {}
        }
    }
    (look, level)
}

/// A changed look or sound level is saved; a look is sent to the server.
fn save(settings: Res<Settings>, level: Res<SoundLevel>, mut set: MessageWriter<SetLook>) {
    let look = settings.is_changed() && !settings.is_added();
    if !look && !(level.is_changed() && !level.is_added()) {
        return;
    }
    let l = &settings.0;
    let text = format!(
        "name={}\ncolour={}\noutfit={}\nvolume={:.0}\n",
        l.name,
        l.colour,
        l.outfit.name(),
        level.0 * 100.0
    );
    if let Some(p) = path()
        && let Some(dir) = p.parent()
        && std::fs::create_dir_all(dir).is_ok()
        && let Err(e) = std::fs::write(&p, text)
    {
        warn!("could not save {}: {e}", p.display());
    }
    if look {
        set.write(SetLook(l.clone()));
    }
}
