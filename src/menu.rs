//! The menus. The title: a lobby code (SCARE, the server's first) to join,
//! a new lobby to host (the door gives it a random code), or playing alone.
//! The lobby, between rounds (or for a robber who left one): the
//! crew, this player's name (typed straight in), colour and outfit with his
//! portrait, and Ready (alone, Start). In the bank, Esc opens a menu: Resume,
//! Back to the lobby, Quit; while it is open the game hears nothing. And the
//! names over the other robbers, while he sees them. Both menus go by
//! keys as well as the mouse (`Nav`): up and down move, left and right change
//! a colour or an outfit, Enter (or Space, in the Esc menu) chooses.

use crate::crew::{COLOURS, Crewmate, LONGEST_NAME, Leave, Look, Outfit, Ready, Settings};
use crate::door::{Asking, CODE_LEN, FIRST};
use crate::doors::Shut;
use crate::inventory::Inventory;
use crate::net::{Knocking, Mode, Owner, Seed, TitleLine, my_id};
use crate::paint::{self, Hex, hex};
use crate::player::{Controls, Dead, LocalPlayer, Player, Shown};
use crate::render::MainCamera;
use crate::sight::Sight;
use crate::talk::Talk;
use bevy::camera::visibility::RenderLayers;
use bevy::camera::{ImageRenderTarget, RenderTarget};
use bevy::input::keyboard::{Key, KeyboardInput};
use bevy::prelude::*;
use bevy::render::render_resource::TextureFormat;
use bevy::ui::UiSystems;
use bevy_replicon_renet::RenetClient;

const BG: Hex = hex(0x0b0c10);
const PANEL: Hex = hex(0x14161c);
const LINE: Hex = hex(0x2c3038);
const TEXT: Hex = hex(0xd8dcd2);
const DIM: Hex = hex(0x80868c);
/// The portrait's render layer, its image's side and how big he is in it.
const PORTRAIT_LAYER: usize = 3;
const PORTRAIT: u32 = 200;
const PORTRAIT_SCALE: f32 = 3.6;

#[derive(Component, Clone, Copy, PartialEq)]
enum Act {
    Colour(usize),
    Outfit(Outfit),
    Ready,
    Resume,
    Lobby,
    Quit,
    Join,
    Host,
    Alone,
    Leave,
}

#[derive(Component)]
struct LobbyRoot;

#[derive(Component)]
struct TitleRoot;

/// The title's code field, its heading, and its line.
#[derive(Component)]
struct CodeField;

#[derive(Component)]
struct CodeHeading;

#[derive(Component)]
struct TitleStatus;

/// The lobby's code, to give the crew.
#[derive(Component)]
struct CodeLine;

/// The code typed at the title.
#[derive(Resource)]
struct TitleCode(String);

#[derive(Component)]
struct EscRoot;

#[derive(Component)]
struct CrewList;

#[derive(Component)]
struct NameField;

#[derive(Component)]
struct ReadyLabel;

#[derive(Component)]
struct Status;

#[derive(Component)]
struct Portrait;

/// A name over that robber.
#[derive(Component)]
struct Plate(Entity);

/// The Esc menu is open.
#[derive(Resource, Default)]
pub struct EscMenu(pub bool);

/// Where the keys are: the lobby's row (`ROWS`, then Ready), the Esc menu's
/// button (`ESC`), and an action chosen by key, which `acts` does as a click.
#[derive(Resource, Default)]
pub struct Nav {
    row: usize,
    item: usize,
    /// The title's: 0 the code, then `TITLE`.
    title: usize,
    chosen: Option<Act>,
}

const ROWS: [&str; 3] = ["NAME", "COLOUR", "OUTFIT"];
const READY_ROW: usize = ROWS.len();
const LEAVE_ROW: usize = READY_ROW + 1;
const ESC: [Act; 3] = [Act::Resume, Act::Lobby, Act::Quit];
const TITLE: [Act; 4] = [Act::Join, Act::Host, Act::Alone, Act::Quit];

/// A lobby row's heading, lit while the keys are on it.
#[derive(Component)]
struct Heading(usize);

pub struct MenuPlugin;

impl Plugin for MenuPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<EscMenu>()
            .init_resource::<Nav>()
            .insert_resource(TitleCode(FIRST.into()))
            .add_systems(Startup, (title, lobby, esc_menu, portrait))
            .add_systems(PreUpdate, escape.after(UiSystems::Focus))
            .add_systems(
                Update,
                (
                    (auto, typing, hover, acts).chain(),
                    show,
                    crew_list,
                    lines,
                    buttons,
                    headings,
                    repaint,
                    plates,
                ),
            );
    }
}

fn colour(h: Hex) -> Color {
    let [r, g, b, a] = h.paint().rgba;
    Color::linear_rgba(r, g, b, a)
}

fn font(size: f32) -> TextFont {
    TextFont {
        font_size: FontSize::Px(size),
        ..default()
    }
}

fn label(text: &str, size: f32, c: Hex) -> impl Bundle {
    (Text::new(text), font(size), TextColor(colour(c)))
}

fn button(act: Act, text: &str) -> impl Bundle {
    (
        Button,
        act,
        Node {
            padding: UiRect::axes(Val::Px(14.0), Val::Px(7.0)),
            border: UiRect::all(Val::Px(1.0)),
            justify_content: JustifyContent::Center,
            ..default()
        },
        BorderColor::all(colour(LINE)),
        BackgroundColor(colour(PANEL)),
        children![label(text, 15.0, TEXT)],
    )
}

fn heading(text: &str) -> impl Bundle {
    label(text, 12.0, DIM)
}

fn lobby(mut commands: Commands, mut images: ResMut<Assets<Image>>) {
    let picture = images.add(Image::new_target_texture(
        PORTRAIT,
        PORTRAIT,
        TextureFormat::Rgba8UnormSrgb,
        None,
    ));
    commands.spawn((
        Camera2d,
        Camera {
            order: -2,
            clear_color: ClearColorConfig::Custom(Color::NONE),
            ..default()
        },
        RenderTarget::Image(ImageRenderTarget::from(picture.clone())),
        RenderLayers::layer(PORTRAIT_LAYER),
    ));
    let column = |gap: f32| Node {
        flex_direction: FlexDirection::Column,
        row_gap: Val::Px(gap),
        ..default()
    };
    let row = |gap: f32| Node {
        flex_direction: FlexDirection::Row,
        column_gap: Val::Px(gap),
        align_items: AlignItems::Center,
        ..default()
    };
    commands.spawn((
        LobbyRoot,
        Node {
            position_type: PositionType::Absolute,
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            flex_direction: FlexDirection::Column,
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            row_gap: Val::Px(28.0),
            ..default()
        },
        BackgroundColor(colour(BG)),
        GlobalZIndex(20),
        children![
            (
                Node {
                    align_items: AlignItems::Center,
                    ..column(4.0)
                },
                children![
                    label("BANK BOBBERY", 30.0, TEXT),
                    label("find its name, speak it, trap it", 13.0, DIM),
                    (CodeLine, label("", 15.0, TEXT)),
                ],
            ),
            (
                Node {
                    align_items: AlignItems::FlexStart,
                    ..row(48.0)
                },
                children![
                    (
                        Node {
                            width: Val::Px(300.0),
                            ..column(10.0)
                        },
                        children![heading("CREW"), (CrewList, column(8.0))],
                    ),
                    (
                        column(14.0),
                        children![
                            (Heading(0), heading("NAME")),
                            (
                                NameField,
                                Node {
                                    padding: UiRect::axes(Val::Px(8.0), Val::Px(5.0)),
                                    border: UiRect::bottom(Val::Px(1.0)),
                                    ..default()
                                },
                                BorderColor::all(colour(LINE)),
                                children![label("", 18.0, TEXT)],
                            ),
                            (Heading(1), heading("COLOUR")),
                            (
                                row(6.0),
                                Children::spawn(SpawnIter((0..COLOURS.len()).map(|i| {
                                    (
                                        Button,
                                        Act::Colour(i),
                                        Node {
                                            width: Val::Px(24.0),
                                            height: Val::Px(24.0),
                                            border: UiRect::all(Val::Px(2.0)),
                                            ..default()
                                        },
                                        BorderColor::all(colour(BG)),
                                        BackgroundColor(colour(COLOURS[i])),
                                    )
                                }))),
                            ),
                            (Heading(2), heading("OUTFIT")),
                            (
                                row(6.0),
                                Children::spawn(SpawnIter(
                                    Outfit::ALL
                                        .into_iter()
                                        .map(|o| button(Act::Outfit(o), o.name())),
                                )),
                            ),
                        ],
                    ),
                    (
                        Node {
                            width: Val::Px(PORTRAIT as f32),
                            height: Val::Px(PORTRAIT as f32),
                            border: UiRect::all(Val::Px(1.0)),
                            ..default()
                        },
                        BorderColor::all(colour(LINE)),
                        BackgroundColor(colour(PANEL)),
                        ImageNode::new(picture),
                    ),
                ],
            ),
            (
                Node {
                    align_items: AlignItems::Center,
                    ..column(8.0)
                },
                children![
                    (
                        row(10.0),
                        children![
                            (
                                Button,
                                Act::Ready,
                                Node {
                                    padding: UiRect::axes(Val::Px(36.0), Val::Px(10.0)),
                                    border: UiRect::all(Val::Px(1.0)),
                                    ..default()
                                },
                                BorderColor::all(colour(LINE)),
                                BackgroundColor(colour(PANEL)),
                                children![(ReadyLabel, label("", 18.0, TEXT))],
                            ),
                            button(Act::Leave, "Leave the lobby"),
                        ],
                    ),
                    (Status, label("", 13.0, DIM)),
                    label(
                        "up/down: move   left/right: change   enter: choose",
                        11.0,
                        LINE.shade(0.25),
                    ),
                ],
            ),
        ],
    ));
}

fn title(mut commands: Commands) {
    commands.spawn((
        TitleRoot,
        Node {
            position_type: PositionType::Absolute,
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            flex_direction: FlexDirection::Column,
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            row_gap: Val::Px(12.0),
            ..default()
        },
        BackgroundColor(colour(BG)),
        GlobalZIndex(25),
        Visibility::Hidden,
        children![
            label("BANK BOBBERY", 30.0, TEXT),
            label("find its name, speak it, trap it", 13.0, DIM),
            Node {
                height: Val::Px(18.0),
                ..default()
            },
            (CodeHeading, heading("LOBBY CODE")),
            (
                CodeField,
                Node {
                    padding: UiRect::axes(Val::Px(14.0), Val::Px(5.0)),
                    border: UiRect::bottom(Val::Px(1.0)),
                    ..default()
                },
                BorderColor::all(colour(LINE)),
                children![label("", 26.0, TEXT)],
            ),
            Node {
                height: Val::Px(6.0),
                ..default()
            },
            button(Act::Join, "Join"),
            button(Act::Host, "Host a new lobby"),
            button(Act::Alone, "Play alone"),
            button(Act::Quit, "Quit"),
            (TitleStatus, label("", 13.0, DIM)),
            label(
                "type a code   up/down: move   enter: choose",
                11.0,
                LINE.shade(0.25),
            ),
        ],
    ));
}

fn esc_menu(mut commands: Commands) {
    commands.spawn((
        EscRoot,
        Node {
            position_type: PositionType::Absolute,
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            flex_direction: FlexDirection::Column,
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            row_gap: Val::Px(10.0),
            ..default()
        },
        BackgroundColor(Color::srgba(0.02, 0.02, 0.03, 0.72)),
        GlobalZIndex(30),
        Visibility::Hidden,
        children![
            label("MENU", 13.0, DIM),
            button(Act::Resume, "Resume"),
            button(Act::Lobby, "Back to the lobby"),
            button(Act::Quit, "Quit"),
        ],
    ));
}

fn portrait(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    settings: Res<Settings>,
) {
    commands.spawn((
        Portrait,
        Mesh2d(meshes.add(paint::portrait(&settings.0, PORTRAIT_SCALE))),
        MeshMaterial2d(materials.add(ColorMaterial::default())),
        RenderLayers::layer(PORTRAIT_LAYER),
    ));
}

/// In the bank, Esc opens and shuts the menu (not over the inventory or the
/// talk box); while it is open nothing else hears a key or a click.
pub fn escape(
    mut keys: ResMut<ButtonInput<KeyCode>>,
    mut mouse: ResMut<ButtonInput<MouseButton>>,
    me: Option<Single<&Inventory, With<LocalPlayer>>>,
    talk: Res<Talk>,
    script: Res<crate::dev::Script>,
    mut menu: ResMut<EscMenu>,
    mut nav: ResMut<Nav>,
) {
    let Some(inv) = me else {
        menu.0 = false;
        return;
    };
    if (keys.just_pressed(KeyCode::Escape) || script.pressed("Menu")) && !inv.open && !talk.open {
        menu.0 = !menu.0;
        nav.item = 0;
        keys.reset_all();
    }
    if !menu.0 {
        return;
    }
    // Its own keys, read before nothing else may hear them.
    let n = ESC.len();
    if keys.any_just_pressed([KeyCode::ArrowUp, KeyCode::KeyW]) {
        nav.item = (nav.item + n - 1) % n;
    }
    if keys.any_just_pressed([KeyCode::ArrowDown, KeyCode::KeyS, KeyCode::Tab]) {
        nav.item = (nav.item + 1) % n;
    }
    if keys.any_just_pressed([KeyCode::Enter, KeyCode::NumpadEnter, KeyCode::Space]) {
        nav.chosen = Some(ESC[nav.item]);
    }
    keys.reset_all();
    mouse.reset_all();
}

/// What the title's actions need beside the rest.
type Talking<'w> = (
    Res<'w, TitleCode>,
    ResMut<'w, Mode>,
    ResMut<'w, TitleLine>,
    Option<Res<'w, Knocking>>,
    Option<ResMut<'w, RenetClient>>,
);

/// `BB_JOIN=CODE` or `BB_HOST=1`: the title knocks at once, for trying it.
fn auto(mut code: ResMut<TitleCode>, mut nav: ResMut<Nav>, mut done: Local<bool>) {
    if std::mem::replace(&mut *done, true) {
        return;
    }
    if let Some(c) = crate::dev::env("BB_JOIN") {
        code.0 = c.to_uppercase();
        nav.chosen = Some(Act::Join);
    } else if crate::dev::env("BB_HOST").is_some() {
        nav.chosen = Some(Act::Host);
    }
}

#[allow(clippy::too_many_arguments)]
fn acts(
    acts: Query<(&Interaction, &Act), Changed<Interaction>>,
    joined: Option<Res<crate::net::Joined>>,
    script: Res<crate::dev::Script>,
    crew: Query<&Crewmate>,
    mut settings: ResMut<Settings>,
    mut menu: ResMut<EscMenu>,
    mut nav: ResMut<Nav>,
    mut ready: MessageWriter<Ready>,
    mut leave: MessageWriter<Leave>,
    mut exit: MessageWriter<AppExit>,
    (code, mut mode, mut line, knocking, mut client): Talking,
    mut commands: Commands,
) {
    let me = my_id(joined.as_deref());
    if script.pressed("Leave") {
        leave.write(Leave);
    }
    if script.pressed("Ready") {
        let now = crew.iter().find(|c| c.id == me).is_some_and(|c| c.ready);
        ready.write(Ready(!now));
    }
    let clicked = acts
        .iter()
        .filter(|a| *a.0 == Interaction::Pressed)
        .map(|a| *a.1);
    for act in nav.chosen.take().into_iter().chain(clicked) {
        match act {
            Act::Colour(c) => settings.0.colour = c,
            Act::Outfit(o) => settings.0.outfit = o,
            Act::Ready => {
                let now = crew.iter().find(|c| c.id == me).is_some_and(|c| c.ready);
                ready.write(Ready(!now));
            }
            Act::Resume => menu.0 = false,
            Act::Lobby => {
                leave.write(Leave);
                menu.0 = false;
            }
            Act::Quit => {
                exit.write(AppExit::Success);
            }
            Act::Join | Act::Host if knocking.is_some() => {}
            Act::Join | Act::Host => {
                let join = (act == Act::Join).then_some(code.0.as_str());
                if join.is_some_and(|c| c.len() != CODE_LEN) {
                    line.0 = format!("A lobby code has {CODE_LEN} letters.");
                    continue;
                }
                let door = crate::net::door();
                match Asking::new(&door, join) {
                    Ok(asking) => {
                        line.0 = format!("Asking {door}...");
                        commands.insert_resource(Knocking(asking));
                    }
                    Err(e) => line.0 = e,
                }
            }
            Act::Alone => *mode = Mode::Alone,
            // Out of the lobby, to the title: off the server, or done alone.
            Act::Leave => {
                if let Some(c) = client.as_mut() {
                    c.disconnect();
                }
                line.0.clear();
                *mode = Mode::Title;
            }
        }
    }
}

/// In the lobby, letters type his name; the arrows, Tab and Enter go about it.
fn typing(
    mut typed: MessageReader<KeyboardInput>,
    me: Option<Single<(), With<LocalPlayer>>>,
    mode: Res<Mode>,
    mut code: ResMut<TitleCode>,
    mut settings: ResMut<Settings>,
    mut nav: ResMut<Nav>,
) {
    if *mode == Mode::Title {
        let rows = TITLE.len() + 1;
        for k in typed.read().filter(|k| k.state.is_pressed()) {
            match &k.logical_key {
                Key::ArrowUp => nav.title = (nav.title + rows - 1) % rows,
                Key::ArrowDown | Key::Tab => nav.title = (nav.title + 1) % rows,
                Key::Enter if nav.title == 0 => nav.chosen = Some(Act::Join),
                Key::Enter => nav.chosen = Some(TITLE[nav.title - 1]),
                Key::Backspace => {
                    code.0.pop();
                }
                Key::Character(s) => {
                    for c in s.chars().filter(char::is_ascii_alphabetic) {
                        if code.0.len() < CODE_LEN {
                            code.0.push(c.to_ascii_uppercase());
                        }
                    }
                    nav.title = 0;
                }
                _ => {}
            }
        }
        return;
    }
    if me.is_some() {
        typed.clear();
        return;
    }
    let rows = LEAVE_ROW + 1;
    for k in typed.read().filter(|k| k.state.is_pressed()) {
        // Letters are his name's only while it has the keys; elsewhere W A
        // S D go about as the arrows do.
        let name = nav.row == 0;
        let wasd = match &k.logical_key {
            Key::Character(c) if !name => c.to_lowercase(),
            _ => String::new(),
        };
        let key = match (&k.logical_key, wasd.as_str()) {
            (_, "w") => &Key::ArrowUp,
            (_, "s") => &Key::ArrowDown,
            (_, "a") => &Key::ArrowLeft,
            (_, "d") => &Key::ArrowRight,
            (key, _) => key,
        };
        match key {
            Key::ArrowUp => nav.row = (nav.row + rows - 1) % rows,
            Key::ArrowDown | Key::Tab => nav.row = (nav.row + 1) % rows,
            Key::ArrowLeft | Key::ArrowRight => {
                let right = *key == Key::ArrowRight;
                let step = |i: usize, n: usize| if right { (i + 1) % n } else { (i + n - 1) % n };
                let look = &mut settings.0;
                match nav.row {
                    1 => look.colour = step(look.colour, COLOURS.len()),
                    2 => {
                        let i = Outfit::ALL
                            .iter()
                            .position(|o| *o == look.outfit)
                            .unwrap_or(0);
                        look.outfit = Outfit::ALL[step(i, Outfit::ALL.len())];
                    }
                    _ => {}
                }
            }
            Key::Enter if nav.row == READY_ROW => nav.chosen = Some(Act::Ready),
            Key::Enter if nav.row == LEAVE_ROW => nav.chosen = Some(Act::Leave),
            Key::Enter => nav.row += 1,
            Key::Backspace if name => {
                settings.0.name.pop();
            }
            Key::Character(s) if name => {
                for c in s
                    .chars()
                    .filter(|c| c.is_alphanumeric() || " -_".contains(*c))
                {
                    if settings.0.name.chars().count() < LONGEST_NAME {
                        settings.0.name.push(c);
                    }
                }
            }
            _ => {}
        }
    }
}

/// The lobby shows while he has no robber; the Esc menu while it is open.
#[allow(clippy::type_complexity)]
fn show(
    me: Option<Single<(), With<LocalPlayer>>>,
    menu: Res<EscMenu>,
    mode: Res<Mode>,
    mut lobby: Single<&mut Visibility, (With<LobbyRoot>, Without<EscRoot>, Without<TitleRoot>)>,
    mut esc: Single<&mut Visibility, (With<EscRoot>, Without<LobbyRoot>, Without<TitleRoot>)>,
    mut title: Single<&mut Visibility, (With<TitleRoot>, Without<LobbyRoot>, Without<EscRoot>)>,
) {
    let vis = |on: bool| {
        if on {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        }
    };
    title.set_if_neq(vis(*mode == Mode::Title));
    lobby.set_if_neq(vis(*mode != Mode::Title && me.is_none()));
    esc.set_if_neq(vis(me.is_some() && menu.0));
}

/// The crew, a row each, rebuilt when anything in it changes.
fn crew_list(
    joined: Option<Res<crate::net::Joined>>,
    crew: Query<(&Crewmate, &Look)>,
    list: Single<Entity, With<CrewList>>,
    mut was: Local<String>,
    mut commands: Commands,
) {
    let me = my_id(joined.as_deref());
    let mut rows: Vec<(&Crewmate, &Look)> = crew.iter().collect();
    rows.sort_by_key(|r| r.0.id);
    let now: String = rows
        .iter()
        .map(|(c, l)| format!("{}{:?}{}{}{}{}", c.id, l, l.colour, c.ready, c.in_round, me))
        .collect();
    if *was == now {
        return;
    }
    *was = now;
    commands
        .entity(*list)
        .despawn_related::<Children>()
        .with_children(|list| {
            for (c, l) in rows {
                let state = match (c.in_round, c.ready) {
                    (true, _) => "in the bank",
                    (_, true) => "ready",
                    _ => "",
                };
                let you = if c.id == me { "  (you)" } else { "" };
                list.spawn((
                    Node {
                        column_gap: Val::Px(10.0),
                        align_items: AlignItems::Center,
                        ..default()
                    },
                    children![
                        (
                            Node {
                                width: Val::Px(12.0),
                                height: Val::Px(12.0),
                                ..default()
                            },
                            BackgroundColor(colour(l.colour())),
                        ),
                        label(&format!("{}{you}", l.name), 16.0, TEXT),
                        label(&format!("{}  {state}", l.outfit.name()), 12.0, DIM),
                    ],
                ));
            }
        });
}

/// The title's code field and line, and the lobby's code.
type Fields<'w, 's> = (
    Single<'w, 's, &'static Children, With<CodeField>>,
    Single<'w, 's, Entity, With<TitleStatus>>,
    Single<'w, 's, Entity, With<CodeLine>>,
);

/// The name field, the Ready button's word and what the crew waits for.
#[allow(clippy::too_many_arguments)]
fn lines(
    settings: Res<Settings>,
    joined: Option<Res<crate::net::Joined>>,
    crew: Query<&Crewmate>,
    seeds: Query<(), With<Seed>>,
    name: Single<&Children, With<NameField>>,
    ready: Single<Entity, With<ReadyLabel>>,
    status: Single<Entity, With<Status>>,
    (mode, title_code, title_line): (Res<Mode>, Res<TitleCode>, Res<TitleLine>),
    (field, title_status, code_line): Fields,
    mut texts: Query<&mut Text>,
) {
    let me = my_id(joined.as_deref());
    let alone = joined.is_none();
    let mine = crew.iter().find(|c| c.id == me);
    let ready_now = mine.is_some_and(|c| c.ready);
    let n = crew.iter().count();
    let ready_n = crew.iter().filter(|c| c.ready).count();
    let in_bank = crew.iter().filter(|c| c.in_round).count();
    let mut set = |e: Entity, s: String| {
        if let Ok(mut t) = texts.get_mut(e)
            && t.0 != s
        {
            t.0 = s;
        }
    };
    if let Some(&field) = name.first() {
        set(field, format!("{}_", settings.0.name));
    }
    let word = match (alone, ready_now) {
        (true, false) => "Start the heist",
        (true, true) => "Starting...",
        (false, false) => "Ready",
        (false, true) => "Not ready",
    };
    set(*ready, word.into());
    let line = if !seeds.is_empty() {
        format!("A round is on, {in_bank} in the bank: you join the next one.")
    } else if alone {
        String::new()
    } else {
        format!("{ready_n} of {n} ready: the round starts when everyone is.")
    };
    set(*status, line);
    if let Some(&f) = field.first() {
        set(f, format!("{}_", title_code.0));
    }
    set(*title_status, title_line.0.clone());
    let lobby = match &*mode {
        Mode::Online(Some(code)) => format!("LOBBY {code}   give your crew the code"),
        Mode::Online(None) => "LOBBY".into(),
        _ => "PLAYING ALONE".into(),
    };
    set(*code_line, lobby);
}

/// The mouse coming onto a button takes the keys there too.
fn hover(acts: Query<(&Interaction, &Act), Changed<Interaction>>, mut nav: ResMut<Nav>) {
    for (i, act) in &acts {
        if *i != Interaction::Hovered {
            continue;
        }
        match *act {
            Act::Colour(_) => nav.row = 1,
            Act::Outfit(_) => nav.row = 2,
            Act::Ready => nav.row = READY_ROW,
            Act::Leave => nav.row = LEAVE_ROW,
            Act::Join | Act::Host | Act::Alone => {
                nav.title = 1 + TITLE.iter().position(|t| *t == *act).unwrap_or(0);
            }
            a => {
                nav.item = ESC.iter().position(|e| *e == a).unwrap_or(nav.item);
                nav.title = 1 + TITLE.iter().position(|t| *t == a).unwrap_or(nav.title - 1);
            }
        }
    }
}

/// Buttons light up where the keys are, and the chosen colour, outfit and
/// Ready show.
fn buttons(
    settings: Res<Settings>,
    joined: Option<Res<crate::net::Joined>>,
    crew: Query<&Crewmate>,
    nav: Res<Nav>,
    mut buttons: Query<(&Act, &mut BackgroundColor, &mut BorderColor)>,
) {
    let me = my_id(joined.as_deref());
    let ready = crew.iter().find(|c| c.id == me).is_some_and(|c| c.ready);
    for (act, mut bg, mut border) in &mut buttons {
        let chosen = match *act {
            Act::Colour(c) => c == settings.0.colour,
            Act::Outfit(o) => o == settings.0.outfit,
            Act::Ready => ready,
            _ => false,
        };
        let on_title = nav.title > 0 && TITLE[nav.title - 1] == *act;
        let focused = match *act {
            Act::Ready => nav.row == READY_ROW,
            Act::Leave => nav.row == LEAVE_ROW,
            Act::Colour(_) | Act::Outfit(_) => false,
            Act::Join | Act::Host | Act::Alone => on_title,
            a => ESC[nav.item] == a || on_title,
        };
        let edge = if chosen || focused { TEXT } else { LINE };
        border.set_if_neq(BorderColor::all(colour(edge)));
        if !matches!(act, Act::Colour(_)) {
            let fill = match (focused, chosen) {
                (true, _) => LINE,
                (_, true) => hex(0x22262e),
                _ => PANEL,
            };
            bg.set_if_neq(BackgroundColor(colour(fill)));
        }
    }
}

/// The lobby row the keys are on: its heading lit, a mark before it.
fn headings(
    nav: Res<Nav>,
    mut headings: Query<(&Heading, &mut Text, &mut TextColor), Without<CodeHeading>>,
    mut code: Single<(&mut Text, &mut TextColor), With<CodeHeading>>,
) {
    let on = nav.title == 0;
    let word = format!("{}LOBBY CODE", if on { "> " } else { "" });
    if code.0.0 != word {
        code.0.0 = word;
    }
    code.1
        .set_if_neq(TextColor(colour(if on { TEXT } else { DIM })));
    for (h, mut text, mut tint) in &mut headings {
        let on = nav.row == h.0;
        let word = format!("{}{}", if on { "> " } else { "" }, ROWS[h.0]);
        if text.0 != word {
            text.0 = word;
        }
        tint.set_if_neq(TextColor(colour(if on { TEXT } else { DIM })));
    }
}

/// His portrait, repainted as he changes his look.
fn repaint(
    settings: Res<Settings>,
    portrait: Single<&Mesh2d, With<Portrait>>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    if settings.is_changed()
        && let Some(mut m) = meshes.get_mut(&portrait.0)
    {
        *m = paint::portrait(&settings.0, PORTRAIT_SCALE);
    }
}

type Other<'a> = (Entity, &'a Shown, &'a Look, Has<Dead>);
type Eye<'a> = (&'a Player, &'a Controls, Has<Dead>);

/// A name over each other robber he sees (the dead only once he is dead).
#[allow(clippy::too_many_arguments)]
fn plates(
    me: Option<Single<Eye, With<LocalPlayer>>>,
    others: Query<Other, (With<Owner>, Without<LocalPlayer>)>,
    camera: Single<(&Camera, &GlobalTransform), With<MainCamera>>,
    sight: Option<Res<Sight>>,
    shut: Option<Res<Shut>>,
    mut plates: Query<(Entity, &Plate, &mut Node, &mut Visibility, &Children)>,
    mut texts: Query<(&mut Text, &mut TextColor)>,
    mut commands: Commands,
) {
    for (e, plate, ..) in &plates {
        if others.get(plate.0).is_err() {
            commands.entity(e).despawn();
        }
    }
    for (e, ..) in &others {
        if !plates.iter().any(|p| p.1.0 == e) {
            commands.spawn((
                Plate(e),
                Node {
                    position_type: PositionType::Absolute,
                    width: Val::Px(160.0),
                    justify_content: JustifyContent::Center,
                    ..default()
                },
                GlobalZIndex(5),
                Visibility::Hidden,
                children![label("", 13.0, TEXT)],
            ));
        }
    }
    let (cam, at) = *camera;
    for (_, plate, mut node, mut vis, children) in &mut plates {
        let Ok((_, shown, look, dead)) = others.get(plate.0) else {
            continue;
        };
        let seen = match (&me, &sight, &shut) {
            (Some(me), Some(sight), Some(shut)) => {
                let (p, c, i_dead) = **me;
                let b = &p.body;
                let reach = sight.robber_reach(b.pos, b.heading, c.torch, shown.pos);
                (!dead || i_dead)
                    && sight.seen(b.pos, shown.pos, reach, b.is_crouching(), &shut.0) > 0.5
            }
            _ => false,
        };
        let spot = cam.world_to_viewport(at, (shown.pos + vec2(0.0, 34.0)).extend(0.0));
        let want = match (seen, spot) {
            (true, Ok(s)) => {
                node.left = Val::Px(s.x - 80.0);
                node.top = Val::Px(s.y - 10.0);
                Visibility::Inherited
            }
            _ => Visibility::Hidden,
        };
        vis.set_if_neq(want);
        if let Some(&child) = children.first()
            && let Ok((mut text, mut tint)) = texts.get_mut(child)
        {
            if text.0 != look.name {
                text.0 = look.name.clone();
            }
            tint.set_if_neq(TextColor(colour(look.colour().shade(0.35))));
        }
    }
}
