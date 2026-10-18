//! Switches for headless runs (`./hidden` runs one on a headless compositor):
//!
//! - `BB_SEED=n`: the map's seed (else the clock's)
//! - `BB_SMOKE_FRAMES=n`: run n frames of exactly 1/60 s each, then exit, silently
//! - `BB_SCREENSHOT=path.png`: save the frame 30 before the end
//! - `BB_KEYS="10:+W,40:-W,45:Roll"`: hold (+) or let go (-) of W A S D Shift
//!   Up Down Left Right at those frames, or press `Roll`, `Torch`, `Crouch`,
//!   `Inventory`, `Slot1`–`Slot4` or `Smash`, `Throw`, `Fire` or `SetTrap` (a left click), `Use` (E), `Reload`, `NewBank` (F5), `Talk` (T), `Say`, `Ready` (in the lobby), `Menu` (Esc) or `Leave` (back to the lobby) once
//!   or `Key:ArrowUp` (`ArrowDown`, `ArrowLeft`, `ArrowRight`, `Enter`, `Tab`,
//!   `Escape`, `Space`, `W`, `A`, `S`, `D`): a real key press, as the keyboard sends it
//! - `BB_AIM=degrees`: aim there instead of at the pointer (0 east, 90 north)
//! - `BB_ZOOM=scale`: world units a pixel to start at (else zoomed out all the way)
//! - `BB_WINDOW=1400x900`: the window's size
//! - `BB_SPAWN=x,y`: start on that tile
//! - `BB_WRAITH=x,y`: the wraith starts on that tile (else in its room)
//! - `BB_NOTES=x,y`: the five notes in a row from that tile east (else about the bank)
//! - `BB_TITLE=1`: a smoke run starts at the title (else alone, at once);
//!   `BB_JOIN=CODE` joins that lobby from it, `BB_HOST=1` opens a new one
//! - `BB_IDLE=seconds`: how long a lobby other than SCARE stays open empty (600)
//! - `BB_LOBBY=1`: a smoke run stays in the lobby (else it is ready at once)
//! - `BB_SAY=word`: what the `Say` key in `BB_KEYS` speaks to the wraith
//! - `BB_SANITY=n`: every robber starts at n% sanity
//! - `BB_UNPACED=1`: smoke frames as fast as they go, logging the average fps
//! - `BB_PLAYERS=n`: n-1 more robbers by the spawn, flashlights on, walking in slow circles

use crate::map::{Map, tile_centre};
use crate::movement::TILE;
use crate::player::Controls;
use bevy::input::ButtonState;
use bevy::input::keyboard::{Key, KeyboardInput};
use bevy::prelude::*;
use bevy::render::view::screenshot::{Screenshot, save_to_disk};
use std::collections::HashSet;
use std::time::{Duration, Instant};

pub fn env(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.is_empty())
}

/// A tile's centre from "x,y".
pub fn tile(name: &str) -> Option<Vec2> {
    let v = env(name)?;
    let (x, y) = v.split_once(',')?;
    Some(crate::map::tile_centre(x.parse().ok()?, y.parse().ok()?))
}

/// Where the `BB_PLAYERS` crew start: free tiles 2 or more from the spawn, nearest first.
pub fn crew(map: &Map, spawn: Vec2) -> Vec<Vec2> {
    let n = env("BB_PLAYERS")
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(1);
    let free = map.free();
    let mut tiles: Vec<Vec2> = (0..map.h)
        .flat_map(|y| (0..map.w).map(move |x| (x, y)))
        .filter(|&(x, y)| free[map.idx(x, y)])
        .map(|(x, y)| tile_centre(x, y))
        .filter(|p| p.distance(spawn) >= 2.0 * TILE)
        .collect();
    tiles.sort_by(|a, b| a.distance(spawn).total_cmp(&b.distance(spawn)));
    tiles.truncate(n.saturating_sub(1));
    tiles
}

#[derive(Component)]
pub struct Bot(pub f32);

fn bots(time: Res<Time>, mut bots: Query<(&Bot, &mut Controls)>) {
    for (bot, mut c) in &mut bots {
        c.aim = bot.0 + time.elapsed_secs() * 0.5;
        c.walk = Some(c.aim + 1.2);
        c.torch = true;
    }
}

pub fn smoke_frames() -> Option<u32> {
    env("BB_SMOKE_FRAMES").and_then(|v| v.parse().ok())
}

pub fn window_size() -> (u32, u32) {
    env("BB_WINDOW")
        .and_then(|v| {
            let (w, h) = v.split_once('x')?;
            Some((w.parse().ok()?, h.parse().ok()?))
        })
        .unwrap_or((1400, 900))
}

#[derive(Resource, Default)]
pub struct Script {
    events: Vec<(u32, String)>,
    held: HashSet<String>,
    pressed: HashSet<String>,
    pub aim: Option<f32>,
}

impl Script {
    pub fn from_env() -> Self {
        let mut s = Script::default();
        if let Some(keys) = env("BB_KEYS") {
            for item in keys.split(',') {
                if let Some((frame, what)) = item.trim().split_once(':')
                    && let Ok(frame) = frame.parse()
                {
                    s.events.push((frame, what.to_string()));
                }
            }
        }
        s.aim = env("BB_AIM")
            .and_then(|v| v.parse::<f32>().ok())
            .map(f32::to_radians);
        s
    }

    pub fn held(&self, name: &str) -> bool {
        self.held.contains(name)
    }

    pub fn pressed(&self, name: &str) -> bool {
        self.pressed.contains(name)
    }
}

#[derive(Resource, Default)]
pub struct Frames(pub u32);

pub struct DevPlugin;

impl Plugin for DevPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(Script::from_env())
            .init_resource::<Frames>()
            .add_systems(First, (count, play_script).chain())
            .add_systems(PreUpdate, bots)
            .add_systems(Last, smoke);
        if crate::drawn(app) {
            app.add_systems(First, typed.after(play_script));
        }
        if smoke_frames().is_some() {
            app.insert_resource(bevy::time::TimeUpdateStrategy::ManualDuration(
                Duration::from_secs_f64(1.0 / 60.0),
            ));
        }
    }
}

fn count(mut frames: ResMut<Frames>) {
    frames.0 += 1;
}

fn play_script(frames: Res<Frames>, mut script: ResMut<Script>) {
    let s = &mut *script;
    s.pressed.clear();
    for (_, what) in s.events.iter().filter(|(f, _)| *f == frames.0) {
        if let Some(key) = what.strip_prefix('+') {
            s.held.insert(key.to_string());
        } else if let Some(key) = what.strip_prefix('-') {
            s.held.remove(key);
        } else {
            s.pressed.insert(what.clone());
        }
    }
}

/// `Key:Name` in the script: that key pressed for a frame, as the keyboard
/// would, so menus and text boxes hear it too.
fn typed(
    script: Res<Script>,
    window: Query<Entity, With<bevy::window::PrimaryWindow>>,
    mut keys: MessageWriter<KeyboardInput>,
    mut down: Local<Vec<(KeyCode, Key)>>,
) {
    let Ok(window) = window.single() else {
        return;
    };
    let mut send = |key_code, logical_key, state| {
        keys.write(KeyboardInput {
            key_code,
            logical_key,
            state,
            text: None,
            repeat: false,
            window,
        });
    };
    for (code, key) in down.drain(..) {
        send(code, key, ButtonState::Released);
    }
    for name in script.pressed.iter().filter_map(|p| p.strip_prefix("Key:")) {
        let (code, key) = match name {
            "ArrowUp" => (KeyCode::ArrowUp, Key::ArrowUp),
            "ArrowDown" => (KeyCode::ArrowDown, Key::ArrowDown),
            "ArrowLeft" => (KeyCode::ArrowLeft, Key::ArrowLeft),
            "ArrowRight" => (KeyCode::ArrowRight, Key::ArrowRight),
            "Enter" => (KeyCode::Enter, Key::Enter),
            "Tab" => (KeyCode::Tab, Key::Tab),
            "Escape" => (KeyCode::Escape, Key::Escape),
            "Space" => (KeyCode::Space, Key::Space),
            "W" => (KeyCode::KeyW, Key::Character("w".into())),
            "A" => (KeyCode::KeyA, Key::Character("a".into())),
            "S" => (KeyCode::KeyS, Key::Character("s".into())),
            "D" => (KeyCode::KeyD, Key::Character("d".into())),
            _ => continue,
        };
        send(code, key.clone(), ButtonState::Pressed);
        down.push((code, key));
    }
}

fn smoke(
    mut commands: Commands,
    frames: Res<Frames>,
    mut exit: MessageWriter<AppExit>,
    mut last: Local<Option<Instant>>,
    mut warm: Local<Option<Instant>>,
) {
    // Timed from here, once the shaders have compiled.
    const WARM: u32 = 120;
    if frames.0 == WARM {
        *warm = Some(Instant::now());
    }
    let Some(limit) = smoke_frames() else {
        return;
    };
    // Paced at 60 fps: a headless compositor has no vblank, and a frame count
    // is only a time if each frame takes one.
    if env("BB_UNPACED").is_none()
        && let Some(at) = *last
    {
        std::thread::sleep(Duration::from_secs_f64(1.0 / 60.0).saturating_sub(at.elapsed()));
    }
    *last = Some(Instant::now());
    if frames.0 + 30 == limit
        && let Some(path) = env("BB_SCREENSHOT")
    {
        commands
            .spawn(Screenshot::primary_window())
            .observe(save_to_disk(path));
    }
    if frames.0 >= limit {
        if let Some(at) = *warm {
            let n = frames.0.saturating_sub(WARM);
            let secs = at.elapsed().as_secs_f64();
            info!(
                "smoke run: {n} frames after the first {WARM} in {secs:.2} s, {:.0} fps",
                n as f64 / secs
            );
        }
        exit.write(AppExit::Success);
    }
}
