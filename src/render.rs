use crate::doors::Shut;
use crate::map::Map;
use crate::movement::TILE;
use crate::paint;
use crate::smash::Bank;
#[cfg(feature = "client")]
use bevy::asset::{RenderAssetUsages, load_internal_asset, uuid_handle};
#[cfg(feature = "client")]
use bevy::camera::visibility::RenderLayers;
#[cfg(feature = "client")]
use bevy::camera::{Hdr, ImageRenderTarget, RenderTarget, ScalingMode};
#[cfg(feature = "client")]
use bevy::core_pipeline::tonemapping::{DebandDither, Tonemapping};
#[cfg(feature = "client")]
use bevy::image::{ImageSampler, ImageSamplerDescriptor};
#[cfg(feature = "client")]
use bevy::mesh::MeshVertexBufferLayoutRef;
use bevy::prelude::*;
#[cfg(feature = "client")]
use bevy::render::render_resource::{
    AsBindGroup, Extent3d, PipelineCache, RenderPipelineDescriptor, ShaderType,
    SpecializedMeshPipelineError, TextureDimension, TextureFormat,
};
#[cfg(feature = "client")]
use bevy::render::{Render, RenderApp, RenderSystems};
#[cfg(feature = "client")]
use bevy::shader::{Shader, ShaderRef};
#[cfg(feature = "client")]
use bevy::sprite_render::{AlphaMode2d, Material2d, Material2dKey, Material2dPlugin};
#[cfg(feature = "client")]
use std::sync::Arc;
#[cfg(feature = "client")]
use std::sync::atomic::{AtomicBool, Ordering};

#[cfg(feature = "client")]
const WORLD_SHADER: Handle<Shader> = uuid_handle!("3f6c1b2e-8d4a-4e7b-9a51-2c7d0e9b6f41");

/// The most lamps a bank has (95 on 300 seeds).
pub const MAX_LIGHTS: usize = 128;
/// As in world.wgsl.
pub const MAX_TORCHES: usize = 6;
/// A light map's texels along a tile.
#[cfg(feature = "client")]
const BAKED: u32 = 16;

#[cfg(feature = "client")]
pub struct RenderPlugin;

#[cfg(feature = "client")]
impl Plugin for RenderPlugin {
    fn build(&self, app: &mut App) {
        load_internal_asset!(app, WORLD_SHADER, "shaders/world.wgsl", Shader::from_wgsl);
        let compiling = Compiling::default();
        app.add_plugins(Material2dPlugin::<WorldMaterial>::default())
            .insert_resource(compiling.clone())
            .add_systems(PostUpdate, bake);
        app.sub_app_mut(RenderApp)
            .insert_resource(compiling)
            .add_systems(Render, note_compiling.in_set(RenderSystems::Cleanup));
    }
}

/// The camera the player looks through, not a light map's.
#[derive(Component)]
pub struct MainCamera;

/// Draws one light map (0 the ground's, 1 things'): every lamp's light,
/// shadowed, traced once instead of every frame.
#[cfg(feature = "client")]
#[derive(Component)]
struct Bake(usize);

/// Whether a pipeline is still compiling, so that a bake may have drawn nothing.
#[cfg(feature = "client")]
#[derive(Resource, Clone, Default)]
struct Compiling(Arc<AtomicBool>);

#[cfg(feature = "client")]
fn note_compiling(cache: Res<PipelineCache>, compiling: Res<Compiling>) {
    let waiting = cache.waiting_pipelines().next().is_some();
    compiling.0.store(waiting, Ordering::Relaxed);
}

/// Bakes both maps for a new bank, going on a few frames: the light maps'
/// material may not be ready in the first. For a door moving or a window
/// smashed, one map a frame, taking turns, so a swing costs half as much.
#[cfg(feature = "client")]
fn bake(
    mut cameras: Query<(&mut Camera, Ref<Bake>)>,
    shut: Option<Res<Shut>>,
    bank: Option<Res<Bank>>,
    power: Query<Ref<crate::power::Power>>,
    compiling: Res<Compiling>,
    // Frames each map still wants, and the one drawn last.
    mut left: Local<[u8; 2]>,
    mut last: Local<usize>,
) {
    if compiling.0.load(Ordering::Relaxed) || cameras.iter().any(|(_, b)| b.is_added()) {
        *left = [3; 2];
    } else if shut.is_some_and(|s| s.is_changed())
        || bank.is_some_and(|b| b.is_changed())
        || power.iter().any(|p| p.is_changed())
    {
        *left = left.map(|n| n.max(1));
    }
    let both = left.iter().all(|&n| n > 1);
    let next = if left[1 - *last] > 0 {
        1 - *last
    } else {
        *last
    };
    for (mut camera, b) in &mut cameras {
        let on = left[b.0] > 0 && (both || b.0 == next);
        if camera.is_active != on {
            camera.is_active = on;
        }
    }
    if both {
        *left = left.map(|n| n - 1);
    } else if left[next] > 0 {
        left[next] -= 1;
        *last = next;
    }
}

#[cfg(feature = "client")]
#[derive(ShaderType, Clone, Debug)]
pub struct Params {
    pub eye: Vec4,
    pub map: Vec4,
    pub misc: Vec4,
    pub flicker: Vec4,
    /// x: 1, everything is seen (the sim's free camera).
    pub view: Vec4,
    pub torches: [Vec4; MAX_TORCHES],
    pub lights: [Vec4; 2 * MAX_LIGHTS],
}

#[cfg(feature = "client")]
#[derive(Asset, TypePath, AsBindGroup, Clone, Debug)]
pub struct WorldMaterial {
    #[uniform(0)]
    pub params: Params,
    #[texture(1)]
    pub tiles: Handle<Image>,
    #[texture(2)]
    pub occ: Handle<Image>,
    #[texture(3)]
    pub extra: Handle<Image>,
    #[texture(4)]
    pub low: Handle<Image>,
    #[texture(5)]
    pub doors: Handle<Image>,
    #[texture(6)]
    pub heard: Handle<Image>,
    #[texture(7)]
    #[sampler(8)]
    pub baked: Handle<Image>,
    pub blend: bool,
}

#[cfg(feature = "client")]
impl Material2d for WorldMaterial {
    fn fragment_shader() -> ShaderRef {
        ShaderRef::Handle(WORLD_SHADER)
    }

    fn alpha_mode(&self) -> AlphaMode2d {
        if self.blend {
            AlphaMode2d::Blend
        } else {
            AlphaMode2d::Opaque
        }
    }

    fn specialize(
        descriptor: &mut RenderPipelineDescriptor,
        _layout: &MeshVertexBufferLayoutRef,
        _key: Material2dKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        descriptor.primitive.cull_mode = None;
        Ok(())
    }
}

#[derive(Component)]
pub struct WorldPart;

#[cfg(feature = "client")]
#[derive(Resource, Default)]
pub struct WorldMaterials {
    pub all: Vec<Handle<WorldMaterial>>,
    /// Per tile: floor, wall, tint, deco. Rewritten as windows are smashed.
    pub tiles: Handle<Image>,
    /// Per doorway tile: how shut (r), lying along x (g), glass (b). Rewritten as doors swing.
    pub doors: Handle<Image>,
    /// Per tile, a smash's reach: sixteenths of a tile left (r), how strongly shown (g). Rewritten every frame.
    pub heard: Handle<Image>,
    /// Per tile: tall object, indoor, half-height object, lit (a). Rewritten as lights switch.
    pub extra: Handle<Image>,
}

#[cfg(feature = "client")]
fn grid_image(map: &Map, texel: impl Fn(i32, i32) -> [u8; 4]) -> Image {
    let mut data = Vec::with_capacity((map.w * map.h * 4) as usize);
    for y in 0..map.h {
        for x in 0..map.w {
            data.extend(texel(x, y));
        }
    }
    let mut image = Image::new(
        Extent3d {
            width: map.w as u32,
            height: map.h as u32,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8Unorm,
        RenderAssetUsages::RENDER_WORLD,
    );
    image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor::nearest());
    image
}

#[cfg(feature = "client")]
pub fn spawn_map(
    commands: &mut Commands,
    map: &Map,
    meshes: &mut Assets<Mesh>,
    images: &mut Assets<Image>,
    materials: &mut Assets<WorldMaterial>,
    world_materials: &mut WorldMaterials,
) {
    let mut tiles = grid_image(map, |x, y| {
        let t = map.at(x, y);
        [t.floor as u8, t.wall as u8, t.tint, t.deco]
    });
    tiles.asset_usage = RenderAssetUsages::default();
    let tiles = images.add(tiles);
    world_materials.tiles = tiles.clone();
    let (tall, half) = (map.occluders(false), map.occluders(true));
    let occ = images.add(grid_image(map, |x, y| tall.boxes[map.idx(x, y)]));
    let low = images.add(grid_image(map, |x, y| half.boxes[map.idx(x, y)]));
    let shut = Shut::new(map).0;
    let mut doors = grid_image(map, |x, y| {
        let i = map.idx(x, y) * 4;
        shut[i..i + 4].try_into().unwrap()
    });
    doors.asset_usage = RenderAssetUsages::default();
    let doors = images.add(doors);
    world_materials.doors = doors.clone();
    let mut heard = grid_image(map, |_, _| [0; 4]);
    heard.asset_usage = RenderAssetUsages::default();
    let heard = images.add(heard);
    world_materials.heard = heard.clone();
    let lit = map.lit();
    let mut extra = grid_image(map, |x, y| {
        let i = map.idx(x, y);
        let indoor = if map.at(x, y).indoor { 255 } else { 0 };
        [
            tall.objects[i],
            indoor,
            half.objects[i],
            if lit[i] { 255 } else { 0 },
        ]
    });
    extra.asset_usage = RenderAssetUsages::default();
    let extra = images.add(extra);
    world_materials.extra = extra.clone();
    if map.lights.len() > MAX_LIGHTS {
        warn!("{} lamps, only {MAX_LIGHTS} light", map.lights.len());
    }
    let mut lights = [Vec4::ZERO; 2 * MAX_LIGHTS];
    for (i, l) in map.lights.iter().take(MAX_LIGHTS).enumerate() {
        lights[2 * i] = l.pos.extend(l.radius).extend(l.intensity);
        lights[2 * i + 1] = Vec3::from(l.color).extend(0.0);
    }
    let lamps = map.lights.len().min(MAX_LIGHTS) as f32;
    // The ground's and things' light maps.
    let baked = [(); 2].map(|_| images.add(light_map(map)));
    let mut material = |mode: f32, baked: &Handle<Image>| {
        materials.add(WorldMaterial {
            params: Params {
                eye: vec4(map.spawn.x, map.spawn.y, 1.0, 0.0),
                map: vec4(map.w as f32, map.h as f32, TILE, 0.0),
                misc: vec4(mode, lamps, 0.0, 0.0),
                flicker: Vec4::ZERO,
                view: Vec4::ZERO,
                torches: [Vec4::ZERO; MAX_TORCHES],
                lights,
            },
            tiles: tiles.clone(),
            occ: occ.clone(),
            extra: extra.clone(),
            low: low.clone(),
            doors: doors.clone(),
            heard: heard.clone(),
            baked: baked.clone(),
            blend: mode > 0.5,
        })
    };
    let ground = material(0.0, &baked[0]);
    let things = material(1.0, &baked[1]);
    world_materials.all = vec![ground.clone(), things.clone(), material(2.0, &baked[1])];
    let bakers = [0, 1].map(|i| material(3.0 + i as f32, &baked[1 - i]));
    world_materials.all.extend(bakers.iter().cloned());

    let size = vec2(map.w as f32, map.h as f32) * TILE;
    let centre = Transform::from_xyz(size.x * 0.5, size.y * 0.5, 0.0);
    let whole = meshes.add(Rectangle::new(size.x, size.y));
    commands.spawn((
        WorldPart,
        Mesh2d(whole.clone()),
        MeshMaterial2d(ground),
        centre,
    ));
    // Each light map drawn by its own camera from a map-sized mesh only it
    // sees, its material bound to the other map.
    for i in 0..2 {
        let only = RenderLayers::layer(i + 1);
        commands.spawn((
            WorldPart,
            Mesh2d(whole.clone()),
            MeshMaterial2d(bakers[i].clone()),
            centre,
            only.clone(),
        ));
        commands.spawn((
            WorldPart,
            Bake(i),
            Camera2d,
            Camera {
                is_active: false,
                order: -1,
                clear_color: ClearColorConfig::Custom(Color::BLACK),
                ..default()
            },
            RenderTarget::Image(ImageRenderTarget::from(baked[i].clone())),
            // Half floats: lamps add up past white.
            Hdr,
            Tonemapping::None,
            DebandDither::Disabled,
            Msaa::Off,
            Projection::Orthographic(OrthographicProjection {
                scaling_mode: ScalingMode::Fixed {
                    width: size.x,
                    height: size.y,
                },
                ..OrthographicProjection::default_2d()
            }),
            centre,
            only,
        ));
    }
    for (layer, z) in paint::paint_props(&map.props)
        .into_iter()
        .zip([1.0, 2.0, 20.0])
    {
        if !layer.is_empty() {
            commands.spawn((
                WorldPart,
                Mesh2d(meshes.add(layer.build())),
                MeshMaterial2d(things.clone()),
                Transform::from_xyz(0.0, 0.0, z),
            ));
        }
    }
}

#[cfg(feature = "client")]
fn light_map(map: &Map) -> Image {
    let mut image = Image::new_target_texture(
        map.w as u32 * BAKED,
        map.h as u32 * BAKED,
        TextureFormat::Rgba16Float,
        None,
    );
    image.asset_usage = RenderAssetUsages::RENDER_WORLD;
    image.sampler = ImageSampler::linear();
    image
}

#[cfg(feature = "client")]
pub fn things_material(world_materials: &WorldMaterials) -> Handle<WorldMaterial> {
    world_materials.all[1].clone()
}

#[cfg(feature = "client")]
pub fn ghost_material(world_materials: &WorldMaterials) -> Handle<WorldMaterial> {
    world_materials.all[2].clone()
}

/// `flicker`: lights flickering, centre, reach and how bright now (none: zero).
/// `torches`: every flashlight on, the same way, the hunter's own first.
#[cfg(feature = "client")]
#[allow(clippy::too_many_arguments)]
pub fn write_params(
    eye: Vec2,
    heading: f32,
    torch: bool,
    crouch: bool,
    see_all: bool,
    flicker: Vec4,
    torches: &[Vec4],
    world_materials: &WorldMaterials,
    materials: &mut Assets<WorldMaterial>,
) {
    for handle in &world_materials.all {
        let Some(mut m) = materials.get_mut(handle) else {
            continue;
        };
        let p = &mut m.params;
        p.eye = vec4(eye.x, eye.y, heading.cos(), heading.sin());
        p.misc.z = if torch { 1.0 } else { 0.0 };
        p.misc.w = if crouch { 1.0 } else { 0.0 };
        p.map.w = torches.len().min(MAX_TORCHES) as f32;
        p.flicker = flicker;
        p.view.x = if see_all { 1.0 } else { 0.0 };
        for (t, &at) in p.torches.iter_mut().zip(torches) {
            *t = at;
        }
    }
}
