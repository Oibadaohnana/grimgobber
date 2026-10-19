//! The crew's instruments, from the van. The thermometer reads the air: the
//! bank's rooms are about 20 °C and the street 8, the wraith's own room cold
//! (below zero if it leaves frost), and the air chills round it wherever it
//! goes. The EMF reader reads 1 to 5 where the wraith has lately touched
//! something (`Emf`, laid by the server): 2 where it went by, 3 a door it
//! slammed or a thing it threw, 4 where a haunt began. Both are read on
//! every client from what is replicated: his own on a dial left of the quick
//! slots, anyone's reader lit and beeping in his hand. The infrared lamp,
//! while in hand, shows the prints a poltergeist or shade leaves wherever it
//! walks (`Infrared`) in its beam; it lights nothing else, and the wraith
//! does not see it.

use crate::doors::Shut;
use crate::inventory::{Inventory, Kind};
use crate::map::{Map, tile_of};
use crate::movement::{TILE, approach};
use crate::paint;
use crate::player::{Dead, Living, Player, Shown};
use crate::render::WorldPart;
use crate::sight::Sight;
use crate::smash::Bank;
use crate::sound::{Clip, Noise};
use crate::wraith::Wraith;
use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use bevy_replicon::prelude::{ClientState, Replicated};
use serde::{Deserialize, Serialize};

/// The air, in °C: outside, and in the bank's rooms.
const OUTSIDE: f32 = 8.0;
const ROOM: f32 = 19.5;
/// The air chills this far round the wraith, in tiles.
const CHILL: f32 = 3.0;
/// A trace lasts this long, and reads its level this far off (one less up
/// to twice as far), in tiles.
const LASTS: f32 = 25.0;
const REACH: f32 = 1.5;
/// Seconds between a reader's beeps, by its level.
const BEEPS: [f32; 6] = [0.0, 0.0, 0.9, 0.5, 0.25, 0.12];
/// The infrared lamp's reach, in tiles, and half its beam's angle.
const IR_REACH: f32 = 5.0;
const IR_BEAM: f32 = 0.4;
pub const LEDS: [u32; 5] = [0x3ad06a, 0x3ad06a, 0xe8d23a, 0xf08a2a, 0xf03a3a];

/// Where the wraith touched something, and how hard.
#[derive(Component, Serialize, Deserialize)]
pub struct Emf {
    pub at: Vec2,
    pub level: u8,
}

/// Seconds left of a trace, on the server.
#[derive(Component)]
struct Lasts(f32);

/// A print seen only in an infrared lamp's beam.
#[derive(Component)]
pub struct Infrared;

/// An infrared lamp's beam, on every hunter, shown while he holds one.
#[derive(Component)]
struct Cone;

#[cfg(feature = "client")]
#[derive(Resource)]
struct ConeLook(Handle<Mesh>, Handle<ColorMaterial>);

/// An EMF reader's light in a hunter's hand: lit from this level.
#[derive(Component)]
pub struct Led(pub u8, pub Entity);

pub fn trace(commands: &mut Commands, at: Vec2, level: u8) {
    commands.spawn((WorldPart, Replicated, Emf { at, level }, Lasts(LASTS)));
}

/// The air at `at`, in °C.
pub fn temperature(map: &Map, w: &Wraith, at: Vec2) -> f32 {
    let (x, y) = tile_of(at);
    let air = if !map.inside(x, y) || !map.at(x, y).indoor {
        OUTSIDE
    } else {
        match map.rooms.iter().position(|r| r.contains(x, y)) {
            Some(i) if i == w.home as usize => w.cold,
            Some(i) => ROOM + 3.0 * (paint::noise1(i as f32 * 3.7) - 0.5),
            None => ROOM,
        }
    };
    let near = (1.0 - at.distance(w.pos) / (CHILL * TILE)).max(0.0);
    air + (w.cold - air).min(0.0) * near
}

/// What an EMF reader at `at` reads.
pub fn emf<'a>(traces: impl IntoIterator<Item = &'a Emf>, at: Vec2) -> u8 {
    traces
        .into_iter()
        .map(|t| match t.at.distance(at) / (REACH * TILE) {
            d if d < 1.0 => t.level,
            d if d < 2.0 => t.level - 1,
            _ => 1,
        })
        .fold(1, u8::max)
}

pub struct InstrumentsPlugin;

impl Plugin for InstrumentsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, fade.run_if(in_state(ClientState::Disconnected)));
        #[cfg(feature = "client")]
        if crate::drawn(app) {
            app.add_systems(Startup, (spawn_dial, cone_look))
                .add_systems(
                    Update,
                    (
                        dial,
                        leds,
                        beep,
                        cones,
                        infrared.run_if(resource_exists::<Sight>.and_then(resource_exists::<Shut>)),
                    ),
                );
        }
    }
}

fn fade(time: Res<Time>, mut traces: Query<(Entity, &mut Lasts)>, mut commands: Commands) {
    for (e, mut left) in &mut traces {
        left.0 -= time.delta_secs();
        if left.0 <= 0.0 {
            commands.entity(e).despawn();
        }
    }
}

#[cfg(feature = "client")]
fn cone_look(
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    mut commands: Commands,
) {
    let mut red = paint::hex(0xc0281c).glow(1.0);
    red.rgba[3] = 0.16;
    commands.insert_resource(ConeLook(
        meshes.add(paint::beam(IR_BEAM, red)),
        materials.add(ColorMaterial::default()),
    ));
}

#[cfg(feature = "client")]
fn cones(look: Res<ConeLook>, hunters: Query<Entity, Added<Shown>>, mut commands: Commands) {
    for e in &hunters {
        commands.entity(e).with_child((
            Cone,
            Mesh2d(look.0.clone()),
            MeshMaterial2d(look.1.clone()),
            Transform::from_xyz(16.0, 0.0, 14.0),
            Visibility::Hidden,
        ));
    }
}

/// Each infrared lamp in hand: its beam out to where it stops, and the
/// prints in it shown.
#[allow(clippy::type_complexity)]
fn infrared(
    sight: Res<Sight>,
    shut: Res<Shut>,
    hunters: Query<
        (
            &Shown,
            &Player,
            &Inventory,
            Has<Dead>,
            &Children,
            &Transform,
        ),
        Without<Cone>,
    >,
    mut cones: Query<(&mut Transform, &mut Visibility), With<Cone>>,
    mut prints: Query<(&Transform, &mut Visibility), (With<Infrared>, Without<Cone>)>,
) {
    let doors = &shut.0;
    let clear = |from: Vec2, d: Vec2| {
        let n = (d.length() / 6.0) as usize;
        (1..n).all(|k| !sight.stops(from + d * k as f32 / n as f32, doors))
    };
    let mut lamps = Vec::new();
    for (shown, p, inv, dead, children, root) in &hunters {
        let on = inv.holds(Kind::Infrared) && !dead;
        let dir = Vec2::from_angle(p.body.heading);
        let from = shown.pos + dir * 16.0;
        let len = (1..=(IR_REACH * TILE / 4.0) as usize)
            .map(|k| k as f32 * 4.0)
            .find(|&d| sight.stops(from + dir * d, doors))
            .unwrap_or(IR_REACH * TILE);
        for &c in children {
            if let Ok((mut t, mut vis)) = cones.get_mut(c) {
                // His own scale (crouched, a little smaller) taken out.
                t.scale = Vec3::splat(len / root.scale.x);
                vis.set_if_neq(if on {
                    Visibility::Inherited
                } else {
                    Visibility::Hidden
                });
            }
        }
        if on {
            lamps.push((from, dir));
        }
    }
    for (t, mut vis) in &mut prints {
        let at = t.translation.truncate();
        let lit = lamps.iter().any(|&(from, dir)| {
            let d = at - from;
            d.length() < IR_REACH * TILE && d.angle_to(dir).abs() < IR_BEAM && clear(from, d)
        });
        vis.set_if_neq(if lit {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        });
    }
}

#[derive(Component)]
struct Dial;

#[derive(Component)]
struct Label;

#[derive(Component)]
struct Reading;

#[derive(Component)]
struct Lights;

#[derive(Component)]
struct Light(u8);

/// Left of the quick slots, mirroring the sanity bar on their right.
fn spawn_dial(mut commands: Commands) {
    let px = Val::Px;
    let font = |size| TextFont {
        font_size: FontSize::Px(size),
        ..default()
    };
    let light = |n: u8| {
        (
            Light(n),
            Node {
                width: px(28.0),
                height: px(10.0),
                border_radius: BorderRadius::all(px(3.0)),
                ..default()
            },
            BackgroundColor(Color::NONE),
        )
    };
    commands.spawn((
        Dial,
        Node {
            position_type: PositionType::Absolute,
            right: Val::Percent(50.0),
            margin: UiRect::right(px(144.0)),
            bottom: px(18.0),
            width: px(200.0),
            height: px(60.0),
            flex_direction: FlexDirection::Column,
            justify_content: JustifyContent::Center,
            row_gap: px(7.0),
            padding: UiRect::axes(px(12.0), px(8.0)),
            border: UiRect::all(px(1.0)),
            ..default()
        },
        BackgroundColor(Color::srgb_u8(0x1d, 0x20, 0x24)),
        BorderColor::all(Color::srgb_u8(0x55, 0x5c, 0x66)),
        Visibility::Hidden,
        children![
            (
                Node {
                    justify_content: JustifyContent::SpaceBetween,
                    ..default()
                },
                children![
                    (
                        Label,
                        Text::new(""),
                        font(11.0),
                        TextColor(Color::srgb(0.6, 0.63, 0.68))
                    ),
                    (Reading, Text::new(""), font(13.0), TextColor(Color::WHITE)),
                ],
            ),
            (
                Lights,
                Node {
                    justify_content: JustifyContent::SpaceBetween,
                    ..default()
                },
                children![light(1), light(2), light(3), light(4), light(5)],
            ),
        ],
    ));
}

/// His instrument in hand, read: the thermometer slow to settle.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn dial(
    time: Res<Time>,
    bank: Option<Res<Bank>>,
    me: Option<Single<(&Player, &Inventory), Living>>,
    wraiths: Query<&Wraith>,
    traces: Query<&Emf>,
    mut root: Single<&mut Visibility, With<Dial>>,
    mut label: Single<&mut Text, (With<Label>, Without<Reading>)>,
    mut reading: Single<(&mut Text, &mut TextColor), With<Reading>>,
    mut lights: Single<&mut Node, With<Lights>>,
    mut leds: Query<(&Light, &mut BackgroundColor)>,
    mut air: Local<Option<f32>>,
) {
    let held = me.as_ref().and_then(|m| {
        [Kind::Thermometer, Kind::Emf]
            .into_iter()
            .find(|&k| m.1.holds(k))
    });
    let (Some(kind), Some(me), Some(bank), Some(w)) = (held, me, bank, wraiths.iter().next())
    else {
        root.set_if_neq(Visibility::Hidden);
        *air = None;
        return;
    };
    root.set_if_neq(Visibility::Inherited);
    let at = me.0.body.pos;
    let (name, line, colour, display) = if kind == Kind::Thermometer {
        let t = time.elapsed_secs();
        let goal = temperature(&bank.0, w, at) + 0.4 * (paint::noise1((t * 1.5).floor()) - 0.5);
        let c = air.get_or_insert(goal);
        *c += (goal - *c) * approach(0.6, time.delta_secs());
        let colour = if *c < 0.0 {
            Color::srgb(0.55, 0.8, 1.0)
        } else {
            Color::WHITE
        };
        ("THERMOMETER", format!("{:.1} C", *c), colour, Display::None)
    } else {
        let level = emf(traces.iter(), at);
        for (l, mut bg) in &mut leds {
            let c = LEDS[l.0 as usize - 1];
            let c = Color::srgb_u8((c >> 16) as u8, (c >> 8) as u8, c as u8);
            bg.0 = if l.0 <= level { c } else { c.with_alpha(0.12) };
        }
        let line = format!("{level}");
        ("EMF READER", line, Color::WHITE, Display::Flex)
    };
    if label.0 != name {
        label.0 = name.into();
    }
    if reading.0.0 != line {
        reading.0.0 = line;
    }
    reading.1.0 = colour;
    if lights.display != display {
        lights.display = display;
    }
}

/// Every reader in a hunter's hand lit to what it reads.
fn leds(hunters: Query<&Player>, traces: Query<&Emf>, mut lights: Query<(&Led, &mut Visibility)>) {
    for (led, mut vis) in &mut lights {
        let Ok(p) = hunters.get(led.1) else {
            continue;
        };
        let want = if emf(traces.iter(), p.body.pos) >= led.0 {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        vis.set_if_neq(want);
    }
}

/// A reader reading 2 or more beeps, faster the higher it reads.
fn beep(
    time: Res<Time>,
    hunters: Query<(Entity, &Player, &Inventory), Without<Dead>>,
    traces: Query<&Emf>,
    mut since: Local<HashMap<Entity, f32>>,
    mut noises: MessageWriter<Noise>,
) {
    for (e, p, inv) in &hunters {
        let level = emf(traces.iter(), p.body.pos);
        if !inv.holds(Kind::Emf) || level < 2 {
            since.remove(&e);
            continue;
        }
        let s = since.entry(e).or_insert(f32::MAX);
        *s += time.delta_secs();
        if *s >= BEEPS[level as usize] {
            *s = 0.0;
            noises.write(Noise {
                clip: Some(Clip::Beep),
                at: p.body.pos,
                loud: 0.0,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::{generate, tile_centre};

    #[test]
    fn its_room_is_cold_and_traces_read_near() {
        let map = generate(6);
        let r = map.rooms[1];
        let w = |home: usize, cold: f32| Wraith {
            pos: Vec2::splat(-1e4),
            heading: 0.0,
            shown: false,
            flicker: None,
            letters: [None; 5],
            named: false,
            caught: false,
            down: false,
            burn: 0,
            home: home as u8,
            cold,
        };
        let inside = tile_centre(r.x0, r.y0);
        assert_eq!(temperature(&map, &w(1, -3.0), inside), -3.0);
        let warm = temperature(&map, &w(0, -3.0), inside);
        assert!((18.0..21.0).contains(&warm), "{warm}");
        let traces = [Emf {
            at: Vec2::ZERO,
            level: 3,
        }];
        assert_eq!(emf(traces.iter(), vec2(TILE, 0.0)), 3);
        assert_eq!(emf(traces.iter(), vec2(2.5 * TILE, 0.0)), 2);
        assert_eq!(emf(traces.iter(), vec2(4.0 * TILE, 0.0)), 1);
    }
}
