use crate::map::{DoorKind, Map};
use crate::movement::TILE;
use crate::paint;
use bevy::asset::{RenderAssetUsages, load_internal_asset, uuid_handle};
use bevy::image::{ImageSampler, ImageSamplerDescriptor};
use bevy::mesh::MeshVertexBufferLayoutRef;
use bevy::prelude::*;
use bevy::render::render_resource::{
    AsBindGroup, Extent3d, RenderPipelineDescriptor, ShaderType, SpecializedMeshPipelineError,
    TextureDimension, TextureFormat,
};
use bevy::shader::{Shader, ShaderRef};
use bevy::sprite_render::{AlphaMode2d, Material2d, Material2dKey, Material2dPlugin};

const WORLD_SHADER: Handle<Shader> = uuid_handle!("3f6c1b2e-8d4a-4e7b-9a51-2c7d0e9b6f41");

/// The most lamps a frame lights: the nearest to the player.
pub const MAX_LIGHTS: usize = 128;

pub struct RenderPlugin;

impl Plugin for RenderPlugin {
    fn build(&self, app: &mut App) {
        load_internal_asset!(app, WORLD_SHADER, "shaders/world.wgsl", Shader::from_wgsl);
        app.add_plugins(Material2dPlugin::<WorldMaterial>::default());
    }
}

#[derive(ShaderType, Clone, Debug)]
pub struct Params {
    pub eye: Vec4,
    pub map: Vec4,
    pub misc: Vec4,
    pub guard: Vec4,
    pub lights: [Vec4; 2 * MAX_LIGHTS],
}

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
    pub blend: bool,
}

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

#[derive(Resource, Default)]
pub struct WorldMaterials {
    pub all: Vec<Handle<WorldMaterial>>,
    /// Per tile: floor, wall, tint, deco. Rewritten as windows are smashed.
    pub tiles: Handle<Image>,
    /// Per doorway tile: how shut (r), lying along x (g), glass (b). Rewritten as doors swing.
    pub doors: Handle<Image>,
    /// Per tile, a smash's reach: sixteenths of a tile left (r), how strongly shown (g). Rewritten every frame.
    pub heard: Handle<Image>,
}

#[derive(Resource, Default)]
pub struct Lamps {
    pub all: Vec<(Vec4, Vec4)>,
}

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

pub fn spawn_map(
    commands: &mut Commands,
    map: &Map,
    meshes: &mut Assets<Mesh>,
    images: &mut Assets<Image>,
    materials: &mut Assets<WorldMaterial>,
    world_materials: &mut WorldMaterials,
    lamps: &mut Lamps,
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
    let mut shut = vec![[0u8; 4]; map.tiles.len()];
    for d in map.doors() {
        let glass = d.kind == DoorKind::Glass;
        shut[map.idx(d.x, d.y)] = [
            255,
            if d.along.x != 0.0 { 255 } else { 0 },
            if glass { 255 } else { 0 },
            0,
        ];
    }
    let mut doors = grid_image(map, |x, y| shut[map.idx(x, y)]);
    doors.asset_usage = RenderAssetUsages::default();
    let doors = images.add(doors);
    world_materials.doors = doors.clone();
    let mut heard = grid_image(map, |_, _| [0; 4]);
    heard.asset_usage = RenderAssetUsages::default();
    let heard = images.add(heard);
    world_materials.heard = heard.clone();
    let lit = map.lit();
    let extra = images.add(grid_image(map, |x, y| {
        let i = map.idx(x, y);
        let indoor = if map.at(x, y).indoor { 255 } else { 0 };
        [
            tall.objects[i],
            indoor,
            half.objects[i],
            if lit[i] { 255 } else { 0 },
        ]
    }));
    let mut material = |mode: f32| {
        materials.add(WorldMaterial {
            params: Params {
                eye: vec4(map.spawn.x, map.spawn.y, 1.0, 0.0),
                map: vec4(map.w as f32, map.h as f32, TILE, 0.0),
                misc: vec4(mode, 0.0, 0.0, 0.0),
                guard: Vec4::ZERO,
                lights: [Vec4::ZERO; 2 * MAX_LIGHTS],
            },
            tiles: tiles.clone(),
            occ: occ.clone(),
            extra: extra.clone(),
            low: low.clone(),
            doors: doors.clone(),
            heard: heard.clone(),
            blend: mode > 0.5,
        })
    };
    let ground = material(0.0);
    let things = material(1.0);
    world_materials.all = vec![ground.clone(), things.clone(), material(2.0)];

    let size = vec2(map.w as f32, map.h as f32) * TILE;
    commands.spawn((
        WorldPart,
        Mesh2d(meshes.add(Rectangle::new(size.x, size.y))),
        MeshMaterial2d(ground),
        Transform::from_xyz(size.x * 0.5, size.y * 0.5, 0.0),
    ));
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

    lamps.all = map
        .lights
        .iter()
        .map(|l| {
            (
                l.pos.extend(l.radius).extend(l.intensity),
                Vec3::from(l.color).extend(0.0),
            )
        })
        .collect();
}

pub fn things_material(world_materials: &WorldMaterials) -> Handle<WorldMaterial> {
    world_materials.all[1].clone()
}

pub fn ghost_material(world_materials: &WorldMaterials) -> Handle<WorldMaterial> {
    world_materials.all[2].clone()
}

/// `guard`: his eye and the way he faces (cos, sin); zero for none.
#[allow(clippy::too_many_arguments)]
pub fn write_params(
    eye: Vec2,
    heading: f32,
    torch: bool,
    crouch: bool,
    guard: Vec4,
    world_materials: &WorldMaterials,
    lamps: &Lamps,
    materials: &mut Assets<WorldMaterial>,
) {
    let mut near: Vec<_> = lamps.all.iter().collect();
    if near.len() > MAX_LIGHTS {
        near.sort_by(|a, b| {
            a.0.xy()
                .distance_squared(eye)
                .total_cmp(&b.0.xy().distance_squared(eye))
        });
        near.truncate(MAX_LIGHTS);
    }
    for handle in &world_materials.all {
        let Some(mut m) = materials.get_mut(handle) else {
            continue;
        };
        let p = &mut m.params;
        p.eye = vec4(eye.x, eye.y, heading.cos(), heading.sin());
        p.misc.y = near.len() as f32;
        p.misc.z = if torch { 1.0 } else { 0.0 };
        p.misc.w = if crouch { 1.0 } else { 0.0 };
        p.guard = guard;
        for (i, &&(at, colour)) in near.iter().enumerate() {
            p.lights[2 * i] = at;
            p.lights[2 * i + 1] = colour;
        }
    }
}
