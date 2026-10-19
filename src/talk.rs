//! Talking to the wraith: with every letter of its name found, T opens a box
//! with its name in it (to be changed at will), Enter speaks it (`Say`, to the server, which judges
//! it), Esc closes it. While it is open, keys and clicks are its alone.

use crate::dev::Script;
use crate::player::Living;
use crate::wraith::Wraith;
use bevy::input::keyboard::{Key, KeyboardInput};
use bevy::prelude::*;
use serde::{Deserialize, Serialize};

const LONGEST: usize = 12;

/// A word spoken to the wraith.
#[derive(Message, Serialize, Deserialize)]
pub struct Say(pub String);

/// The box: open, what is typed so far.
#[derive(Resource, Default)]
pub struct Talk {
    pub open: bool,
    text: String,
}

#[derive(Component)]
struct TalkBox;

pub struct TalkPlugin;

impl Plugin for TalkPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Talk>()
            .add_systems(Startup, spawn)
            .add_systems(PreUpdate, talk.after(crate::menu::escape))
            .add_systems(Update, show);
    }
}

#[allow(clippy::too_many_arguments)]
pub fn talk(
    mut keys: ResMut<ButtonInput<KeyCode>>,
    mut mouse: ResMut<ButtonInput<MouseButton>>,
    mut typed: MessageReader<KeyboardInput>,
    script: Res<Script>,
    me: Option<Single<(), Living>>,
    wraiths: Query<&Wraith>,
    mut box_: ResMut<Talk>,
    mut say: MessageWriter<Say>,
) {
    let known = wraiths.iter().find_map(|w| w.name().filter(|_| !w.caught));
    if script.pressed("Say")
        && let Some(word) = crate::dev::env("BB_SAY")
    {
        say.write(Say(word));
    }
    if !box_.open {
        typed.clear();
        if (keys.just_pressed(KeyCode::KeyT) || script.pressed("Talk"))
            && let Some(name) = known
            && me.is_some()
        {
            box_.open = true;
            box_.text = name;
            keys.reset_all();
        }
        return;
    }
    for k in typed.read().filter(|k| k.state.is_pressed()) {
        match &k.logical_key {
            Key::Enter => {
                say.write(Say(box_.text.clone()));
                box_.open = false;
            }
            Key::Escape => box_.open = false,
            Key::Backspace => {
                box_.text.pop();
            }
            Key::Character(s) => {
                for c in s.chars().filter(char::is_ascii_alphabetic) {
                    if box_.text.len() < LONGEST {
                        box_.text.push(c.to_ascii_uppercase());
                    }
                }
            }
            _ => {}
        }
    }
    // Nothing else hears the keys while it is open.
    keys.reset_all();
    mouse.clear_just_pressed(MouseButton::Left);
}

fn spawn(mut commands: Commands) {
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                width: Val::Percent(100.0),
                bottom: Val::Px(110.0),
                justify_content: JustifyContent::Center,
                ..default()
            },
            Visibility::Hidden,
            TalkBox,
        ))
        .with_child((
            Text::new(""),
            TextFont {
                font_size: FontSize::Px(22.0),
                ..default()
            },
            TextColor(Color::srgb(0.85, 0.92, 1.0)),
            Node {
                padding: UiRect::axes(Val::Px(14.0), Val::Px(8.0)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.02, 0.03, 0.05, 0.85)),
        ));
}

fn show(
    box_: Res<Talk>,
    mut node: Single<(&mut Visibility, &Children), With<TalkBox>>,
    mut text: Query<&mut Text>,
) {
    let (vis, children) = &mut *node;
    let want = if box_.open {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    };
    if **vis != want {
        **vis = want;
    }
    if let Some(mut t) = children.first().and_then(|&c| text.get_mut(c).ok()) {
        let line = format!(
            "Speak its name: {}_   (Enter speaks, Esc closes)",
            box_.text
        );
        if t.0 != line {
            t.0 = line;
        }
    }
}
