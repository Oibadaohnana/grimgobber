//! The sounds of `../bankbobbery`, as its `audio.js` places them: quieter
//! with distance, and quieter still (`WALL`) where the robber cannot see.
//! Doors are one-shots; footsteps a loop on each walker, at the pace he goes.

use crate::doors::Shut;
use crate::guard::Guard;
use crate::movement::{MARCH_SPEED, TILE};
use crate::player::{Controls, LocalPlayer, Player};
use crate::sight::Sight;
use bevy::audio::{
    AudioPlayer, AudioSink, AudioSinkPlayback, AudioSource, PlaybackSettings, Volume,
};
use bevy::prelude::*;
use serde::{Deserialize, Serialize};

const MASTER: f32 = 0.7;
const WALL: f32 = 0.55;

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Clip {
    DoorOpen,
    DoorClose,
    Glass,
    Knockout,
    Coin,
    Shot,
    Reload,
    Unholster,
    Holster,
}

/// A noise at a point in the building: its sample, if it has one, and how
/// many tiles off a guard hears it (half as far through walls; 0, never).
#[derive(Message, Clone, Serialize, Deserialize)]
pub struct Noise {
    pub clip: Option<Clip>,
    pub at: Vec2,
    pub loud: f32,
}

// Cut from the old samples so the latch knocks as a door starts to open and
// the slam lands as it shuts, 0.4 s on: `ffmpeg -i Opening_door.mp3 -ss 0.29`
// (Closing_door.mp3 `-ss 0.59`) `-af afade=t=in:d=0.005 -ac 1 -ar 48000
// -c:a libvorbis -q:a 4`.
const DOOR_OPEN: &[u8] = include_bytes!("../sounds/door_open.ogg");
const DOOR_CLOSE: &[u8] = include_bytes!("../sounds/door_close.ogg");
const WALK: &[u8] = include_bytes!("../sounds/walking.ogg");
// Synthesised by `sounds/glass.py`.
const GLASS: &[u8] = include_bytes!("../sounds/glass.ogg");
// bims2's punch, and a slowed copy of it as the body hits the floor.
const KNOCKOUT: &[u8] = include_bytes!("../sounds/knockout.ogg");
// Synthesised by `sounds/coin.py`.
const COIN: &[u8] = include_bytes!("../sounds/coin.ogg");
const SHOT: &[u8] = include_bytes!("../sounds/pistol_shot.ogg");
const RELOAD: &[u8] = include_bytes!("../sounds/reload.ogg");
const UNHOLSTER: &[u8] = include_bytes!("../sounds/unholster.ogg");
const HOLSTER: &[u8] = include_bytes!("../sounds/holster.ogg");

#[derive(Resource)]
struct Clips {
    door_open: Handle<AudioSource>,
    door_close: Handle<AudioSource>,
    walk: Handle<AudioSource>,
    glass: Handle<AudioSource>,
    knockout: Handle<AudioSource>,
    coin: Handle<AudioSource>,
    shot: Handle<AudioSource>,
    reload: Handle<AudioSource>,
    unholster: Handle<AudioSource>,
    holster: Handle<AudioSource>,
}

pub struct SoundPlugin;

impl Plugin for SoundPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<Noise>();
        // A smoke run is silent: nobody is listening, and somebody nearby might be.
        if crate::dev::smoke_frames().is_some() {
            return;
        }
        app.add_systems(Startup, load).add_systems(
            Update,
            (feet, noises, steps)
                .run_if(resource_exists::<Shut>.and_then(resource_exists::<Sight>)),
        );
    }
}

fn load(mut assets: ResMut<Assets<AudioSource>>, mut commands: Commands) {
    let mut add = |bytes: &'static [u8]| {
        assets.add(AudioSource {
            bytes: bytes.into(),
        })
    };
    commands.insert_resource(Clips {
        door_open: add(DOOR_OPEN),
        door_close: add(DOOR_CLOSE),
        walk: add(WALK),
        glass: add(GLASS),
        knockout: add(KNOCKOUT),
        coin: add(COIN),
        shot: add(SHOT),
        reload: add(RELOAD),
        unholster: add(UNHOLSTER),
        holster: add(HOLSTER),
    });
}

/// What the robber hears by: where he is, and what he can see.
#[derive(bevy::ecs::system::SystemParam)]
struct Ear<'w, 's> {
    me: Single<'w, 's, (&'static Player, &'static Controls), With<LocalPlayer>>,
    sight: Res<'w, Sight>,
    shut: Res<'w, Shut>,
}

impl Ear<'_, '_> {
    /// How loud a sound carrying `range` tiles is from `at`, as `place` in audio.js.
    fn heard(&self, at: Vec2, range: f32) -> f32 {
        let (player, controls) = *self.me;
        let b = &player.body;
        let near = (1.0 - b.pos.distance(at) / (range * TILE))
            .max(0.0)
            .powf(1.5);
        let doors = &self.shut.0;
        let reach = self
            .sight
            .robber_reach(b.pos, b.heading, controls.torch, at);
        let seen = self.sight.seen(b.pos, at, reach, b.is_crouching(), doors) > 0.1;
        near * if seen { 1.0 } else { WALL }
    }
}

/// Every walker gets his footsteps, silent until he moves.
fn feet(
    clips: Res<Clips>,
    mut commands: Commands,
    robbers: Query<Entity, Added<Player>>,
    guards: Query<Entity, Added<Guard>>,
) {
    for e in robbers.iter().chain(&guards) {
        commands.entity(e).insert((
            AudioPlayer::new(clips.walk.clone()),
            PlaybackSettings::LOOP.with_volume(Volume::SILENT),
        ));
    }
}

fn noises(clips: Res<Clips>, ear: Ear, mut noises: MessageReader<Noise>, mut commands: Commands) {
    for &Noise { clip, at, .. } in noises.read() {
        let Some(clip) = clip else {
            continue;
        };
        // Its sample, loudness, and the tiles it carries to the robber's ear.
        let (handle, gain, range) = match clip {
            Clip::DoorOpen => (&clips.door_open, 0.7, 13.0),
            Clip::DoorClose => (&clips.door_close, 0.7, 13.0),
            Clip::Glass => (&clips.glass, 1.0, 24.0),
            Clip::Knockout => (&clips.knockout, 0.9, 12.0),
            Clip::Coin => (&clips.coin, 0.8, 14.0),
            Clip::Shot => (&clips.shot, 0.8, 30.0),
            Clip::Reload => (&clips.reload, 0.9, 8.0),
            Clip::Unholster => (&clips.unholster, 0.9, 8.0),
            Clip::Holster => (&clips.holster, 0.9, 8.0),
        };
        let level = MASTER * gain * ear.heard(at, range);
        if level > 0.002 {
            commands.spawn((
                AudioPlayer::new(handle.clone()),
                PlaybackSettings::DESPAWN.with_volume(Volume::Linear(level)),
            ));
        }
    }
}

fn steps(
    ear: Ear,
    mut robbers: Query<(&Player, Has<LocalPlayer>, &mut AudioSink), Without<Guard>>,
    mut guards: Query<(&Guard, &mut AudioSink)>,
) {
    // His own feet at a whisper crouched; anyone else's by where they are.
    let pace = |speed: f32| (speed / MARCH_SPEED).clamp(0.0, 1.8);
    let walkers = robbers
        .iter_mut()
        .map(|(p, me, sink)| (&p.body, me, sink))
        .chain(guards.iter_mut().map(|(g, sink)| (&g.body, false, sink)));
    for (b, me, mut sink) in walkers {
        let walking = !b.is_rolling() && b.speed > 1.0;
        let soft = if b.is_crouching() { 0.4 } else { 1.0 };
        let level = match (walking, me) {
            (false, _) => 0.0,
            (true, true) => MASTER * 0.275 * soft,
            (true, false) => MASTER * 0.275 * ear.heard(b.pos, 9.0),
        };
        sink.set_volume(Volume::Linear(level));
        sink.set_speed(pace(b.speed).clamp(0.7, 1.4));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::audio::Decodable;

    #[test]
    fn every_clip_decodes() {
        for bytes in [
            DOOR_OPEN, DOOR_CLOSE, WALK, GLASS, KNOCKOUT, COIN, SHOT, RELOAD, UNHOLSTER, HOLSTER,
        ] {
            let clip = AudioSource {
                bytes: bytes.into(),
            };
            assert_eq!(clip.decoder().take(4800).count(), 4800);
        }
    }
}
