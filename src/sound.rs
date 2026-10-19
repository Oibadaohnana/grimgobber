//! The sounds of `../bankbobbery`, as its `audio.js` places them: quieter
//! with distance, and quieter still (`WALL`) where the hunter cannot see.
//! Doors are one-shots; footsteps a loop on each walker, at the pace he goes;
//! under it all, while there is a bank, the air: outside bims' temperate
//! air, inside a building's deep hum, faded into each other at the door.

use crate::crew::SoundLevel;
use crate::doors::Shut;
use crate::movement::{MARCH_SPEED, TILE};
use crate::player::{Controls, Dead, LocalPlayer, Player};
use crate::sight::Sight;
#[cfg(feature = "client")]
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
    Coin,
    Shot,
    Reload,
    Unholster,
    Holster,
    /// A loose thing landing.
    Thud,
    /// The banshee.
    Wail,
    /// An EMF reader reading high.
    Beep,
}

/// A noise at a point in the building: its sample, if it has one, and how
/// many tiles off the wraith hears it (half as far through walls; 0, never).
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
#[cfg(feature = "client")]
const DOOR_OPEN: &[u8] = include_bytes!("../sounds/door_open.ogg");
#[cfg(feature = "client")]
const DOOR_CLOSE: &[u8] = include_bytes!("../sounds/door_close.ogg");
#[cfg(feature = "client")]
const WALK: &[u8] = include_bytes!("../sounds/walking.ogg");
// Synthesised by `sounds/glass.py`.
#[cfg(feature = "client")]
const GLASS: &[u8] = include_bytes!("../sounds/glass.ogg");
// Synthesised by `sounds/coin.py`.
#[cfg(feature = "client")]
const COIN: &[u8] = include_bytes!("../sounds/coin.ogg");
// Synthesised by `sounds/beep.py`.
#[cfg(feature = "client")]
const BEEP: &[u8] = include_bytes!("../sounds/beep.ogg");
#[cfg(feature = "client")]
const SHOT: &[u8] = include_bytes!("../sounds/pistol_shot.ogg");
#[cfg(feature = "client")]
const RELOAD: &[u8] = include_bytes!("../sounds/reload.ogg");
#[cfg(feature = "client")]
const UNHOLSTER: &[u8] = include_bytes!("../sounds/unholster.ogg");
#[cfg(feature = "client")]
const HOLSTER: &[u8] = include_bytes!("../sounds/holster.ogg");
#[cfg(feature = "client")]
const KNOCKOUT: &[u8] = include_bytes!("../sounds/knockout.ogg");
// Loops at -30 LUFS: bims2_bevy's temperate air (`Sounds/game/temperate.ogg`),
// and a hum synthesised by `sounds/inside.py`.
#[cfg(feature = "client")]
const OUTSIDE: &[u8] = include_bytes!("../sounds/outside.ogg");
#[cfg(feature = "client")]
const INSIDE: &[u8] = include_bytes!("../sounds/inside.ogg");
/// The hum is played at half the outside air's level.
const HUM: f32 = 0.5;
/// Seconds to fade (95% of the way) from one to the other.
const FADE: f32 = 1.5;

/// The air's loops, by whether they are heard indoors.
#[cfg(feature = "client")]
#[derive(Component)]
struct Air(bool);

#[cfg(feature = "client")]
#[derive(Resource)]
struct Clips {
    door_open: Handle<AudioSource>,
    door_close: Handle<AudioSource>,
    walk: Handle<AudioSource>,
    glass: Handle<AudioSource>,
    coin: Handle<AudioSource>,
    beep: Handle<AudioSource>,
    shot: Handle<AudioSource>,
    reload: Handle<AudioSource>,
    unholster: Handle<AudioSource>,
    holster: Handle<AudioSource>,
    knockout: Handle<AudioSource>,
}

#[cfg(feature = "client")]
pub struct SoundPlugin;

#[cfg(feature = "client")]
impl Plugin for SoundPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<Noise>();
        // A smoke run is silent: nobody is listening, and somebody nearby might be.
        if crate::dev::smoke_frames().is_some() {
            return;
        }
        app.add_systems(Startup, load).add_systems(
            Update,
            (
                air,
                (feet, noises, steps)
                    .run_if(resource_exists::<Shut>.and_then(resource_exists::<Sight>)),
            ),
        );
    }
}

#[cfg(feature = "client")]
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
        coin: add(COIN),
        beep: add(BEEP),
        shot: add(SHOT),
        reload: add(RELOAD),
        unholster: add(UNHOLSTER),
        holster: add(HOLSTER),
        knockout: add(KNOCKOUT),
    });
    for (indoor, bytes) in [(false, OUTSIDE), (true, INSIDE)] {
        commands.spawn((
            Air(indoor),
            AudioPlayer::new(add(bytes)),
            PlaybackSettings::LOOP.with_volume(Volume::SILENT),
        ));
    }
}

/// The air where the camera is (on him, or free in the sim), while there
/// is a bank: not at the title or in the lobby.
#[cfg(feature = "client")]
fn air(
    time: Res<Time>,
    level: Res<SoundLevel>,
    bank: Option<Res<crate::smash::Bank>>,
    camera: Single<&Transform, With<crate::render::MainCamera>>,
    mut loops: Query<(&Air, &mut AudioSink)>,
    mut inside: Local<f32>,
) {
    let indoor = bank.as_ref().is_some_and(|b| {
        let (x, y) = crate::map::tile_of(camera.translation.truncate());
        b.0.inside(x, y) && b.0.at(x, y).indoor
    });
    let goal = if indoor { 1.0 } else { 0.0 };
    *inside += (goal - *inside) * crate::movement::approach(3.0 / FADE, time.delta_secs());
    let on = if bank.is_some() {
        MASTER * level.0
    } else {
        0.0
    };
    for (air, mut sink) in &mut loops {
        let share = if air.0 { HUM * *inside } else { 1.0 - *inside };
        sink.set_volume(Volume::Linear(on * share));
    }
}

/// What the hunter hears by: where he is, and what he can see.
#[cfg(feature = "client")]
#[derive(bevy::ecs::system::SystemParam)]
struct Ear<'w, 's> {
    me: Single<'w, 's, (&'static Player, &'static Controls), With<LocalPlayer>>,
    sight: Res<'w, Sight>,
    shut: Res<'w, Shut>,
    level: Res<'w, SoundLevel>,
}

#[cfg(feature = "client")]
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
            .hunter_reach(b.pos, b.heading, controls.torch, at);
        let seen = self.sight.seen(b.pos, at, reach, b.is_crouching(), doors) > 0.1;
        near * if seen { 1.0 } else { WALL }
    }
}

/// Every walker gets his footsteps, silent until he moves.
#[cfg(feature = "client")]
fn feet(clips: Res<Clips>, mut commands: Commands, hunters: Query<Entity, Added<Player>>) {
    for e in &hunters {
        commands.entity(e).insert((
            AudioPlayer::new(clips.walk.clone()),
            PlaybackSettings::LOOP.with_volume(Volume::SILENT),
        ));
    }
}

#[cfg(feature = "client")]
fn noises(clips: Res<Clips>, ear: Ear, mut noises: MessageReader<Noise>, mut commands: Commands) {
    for &Noise { clip, at, .. } in noises.read() {
        let Some(clip) = clip else {
            continue;
        };
        // Its sample, loudness, and the tiles it carries to the hunter's ear.
        let (handle, gain, range) = match clip {
            // The door's creak, slowed down to a moan.
            Clip::Wail => (&clips.door_open, 1.0, 40.0),
            Clip::Thud => (&clips.knockout, 0.7, 12.0),
            Clip::DoorOpen => (&clips.door_open, 0.7, 13.0),
            Clip::DoorClose => (&clips.door_close, 0.7, 13.0),
            Clip::Glass => (&clips.glass, 1.0, 24.0),
            Clip::Coin => (&clips.coin, 0.8, 14.0),
            Clip::Beep => (&clips.beep, 0.35, 10.0),
            Clip::Shot => (&clips.shot, 0.8, 30.0),
            Clip::Reload => (&clips.reload, 0.9, 8.0),
            Clip::Unholster => (&clips.unholster, 0.9, 8.0),
            Clip::Holster => (&clips.holster, 0.9, 8.0),
        };
        let level = MASTER * ear.level.0 * gain * ear.heard(at, range);
        if level > 0.002 {
            commands.spawn((
                AudioPlayer::new(handle.clone()),
                PlaybackSettings::DESPAWN
                    .with_volume(Volume::Linear(level))
                    .with_speed(if clip == Clip::Wail { 0.35 } else { 1.0 }),
            ));
        }
    }
}

#[cfg(feature = "client")]
fn steps(ear: Ear, mut hunters: Query<(&Player, Has<LocalPlayer>, Has<Dead>, &mut AudioSink)>) {
    // His own feet at a whisper crouched; anyone else's by where they are.
    let pace = |speed: f32| (speed / MARCH_SPEED).clamp(0.0, 1.8);
    for (p, me, dead, mut sink) in &mut hunters {
        let b = &p.body;
        // The dead's feet make no sound.
        let walking = !b.is_rolling() && b.speed > 1.0 && !dead;
        let soft = if b.is_crouching() { 0.4 } else { 1.0 };
        let level = match (walking, me) {
            (false, _) => 0.0,
            (true, true) => MASTER * ear.level.0 * 0.275 * soft,
            (true, false) => MASTER * ear.level.0 * 0.275 * ear.heard(b.pos, 9.0),
        };
        sink.set_volume(Volume::Linear(level));
        sink.set_speed(pace(b.speed).clamp(0.7, 1.4));
    }
}

// Decoding needs the audio half, which the server build leaves out.
#[cfg(all(test, feature = "client"))]
mod tests {
    use super::*;
    use bevy::audio::Decodable;

    #[test]
    fn every_clip_decodes() {
        for bytes in [
            DOOR_OPEN, DOOR_CLOSE, WALK, GLASS, COIN, BEEP, SHOT, RELOAD, UNHOLSTER, HOLSTER,
            KNOCKOUT, OUTSIDE, INSIDE,
        ] {
            let clip = AudioSource {
                bytes: bytes.into(),
            };
            assert_eq!(clip.decoder().take(4800).count(), 4800);
        }
    }
}
