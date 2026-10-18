//! The "E ..." prompt: whatever the robber could do with E now offers itself
//! each frame (the first to has it), and the prompt
//! shows over where it says.

use crate::render::MainCamera;
use bevy::prelude::*;
use bevy::ui::UiSystems;

/// Where to show the prompt this frame, and what E would do.
#[derive(Resource, Default)]
pub struct Prompt(pub Option<(Vec2, &'static str)>);

#[derive(Component)]
struct Shown;

#[derive(Component)]
struct Label;

pub struct PromptPlugin;

impl Plugin for PromptPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Prompt>()
            .add_systems(Startup, spawn_prompt)
            .add_systems(First, |mut prompt: ResMut<Prompt>| prompt.0 = None)
            .add_systems(PostUpdate, show.before(UiSystems::Layout));
    }
}

fn spawn_prompt(mut commands: Commands) {
    let font = |size| TextFont {
        font_size: FontSize::Px(size),
        ..default()
    };
    commands
        .spawn((
            Shown,
            Node {
                position_type: PositionType::Absolute,
                align_items: AlignItems::Center,
                column_gap: Val::Px(6.0),
                padding: UiRect::axes(Val::Px(6.0), Val::Px(4.0)),
                border_radius: BorderRadius::all(Val::Px(5.0)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.02, 0.02, 0.04, 0.75)),
            Visibility::Hidden,
        ))
        .with_children(|p| {
            p.spawn((
                Node {
                    width: Val::Px(20.0),
                    height: Val::Px(20.0),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    border: UiRect::all(Val::Px(1.0)),
                    border_radius: BorderRadius::all(Val::Px(3.0)),
                    ..default()
                },
                BackgroundColor(Color::srgb(0.88, 0.88, 0.84)),
                BorderColor::all(Color::srgb(0.45, 0.45, 0.42)),
            ))
            .with_child((
                Text::new("E"),
                font(13.0),
                TextColor(Color::srgb(0.08, 0.08, 0.1)),
            ));
            p.spawn((
                Label,
                Text::new(""),
                font(13.0),
                TextColor(Color::srgb(0.92, 0.92, 0.88)),
            ));
        });
}

/// On the screen where it says, saying what it says.
fn show(
    prompt: Res<Prompt>,
    camera: Query<(&Camera, &GlobalTransform), With<MainCamera>>,
    mut shown: Query<(&mut Node, &mut Visibility, &ComputedNode), With<Shown>>,
    mut label: Query<&mut Text, With<Label>>,
) {
    let (Ok((camera, eye)), Ok((mut node, mut vis, computed)), Ok(mut label)) =
        (camera.single(), shown.single_mut(), label.single_mut())
    else {
        return;
    };
    let at = prompt
        .0
        .and_then(|(at, what)| Some((camera.world_to_viewport(eye, at.extend(0.0)).ok()?, what)));
    *vis = if at.is_some() {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    };
    if let Some((at, what)) = at {
        if label.0 != what {
            label.0 = what.into();
        }
        let size = computed.size() * computed.inverse_scale_factor;
        node.left = Val::Px(at.x - size.x * 0.5);
        node.top = Val::Px(at.y - size.y * 0.5);
    }
}
