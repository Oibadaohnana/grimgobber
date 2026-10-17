use crate::movement::{Aabb, TILE};
use crate::rng::Rng;
use bevy::math::{Vec2, vec2};
use std::collections::VecDeque;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Floor {
    Grass = 0,
    Asphalt = 1,
    Sidewalk = 2,
    Parking = 3,
    Marble = 4,
    Carpet = 5,
    Wood = 6,
    WashroomTile = 7,
    Concrete = 8,
    Steel = 9,
    Lino = 10,
    Raised = 11,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Wall {
    None = 0,
    Brick = 1,
    Drywall = 2,
    Vault = 3,
    Glass = 4,
}

/// Decoration flags on a tile, read by the ground shader.
pub mod deco {
    pub const DOOR: u8 = 1;
    /// Along the tile's bottom edge.
    pub const LANE: u8 = 2;
    /// Along the tile's left edge.
    pub const BAY: u8 = 4;
    pub const ZEBRA: u8 = 8;
    pub const VAULT_DOOR: u8 = 16;
    pub const RUG: u8 = 32;
    /// A smashed window: glass on the floor, the frame's posts at its ends.
    pub const SHARDS: u8 = 64;
}

#[derive(Clone, Copy, Debug)]
pub struct Tile {
    pub floor: Floor,
    pub wall: Wall,
    pub tint: u8,
    pub deco: u8,
    pub room: u16,
    pub indoor: bool,
}

pub const NO_ROOM: u16 = u16::MAX;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DoorKind {
    Wood,
    Steel,
    Glass,
}

/// One leaf in a doorway: it swings about `hinge`, lying along `along` shut.
#[derive(Clone, Copy, Debug)]
pub struct Door {
    pub x: i32,
    pub y: i32,
    pub hinge: Vec2,
    pub along: Vec2,
    pub across: Vec2,
    pub kind: DoorKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RoomKind {
    Lobby,
    Corridor,
    OpenOffice,
    Office,
    Manager,
    Break,
    Washroom,
    Server,
    Security,
    Archive,
    Vault,
}

/// Floor tiles, inclusive.
#[derive(Clone, Copy, Debug)]
pub struct Room {
    pub kind: RoomKind,
    pub x0: i32,
    pub y0: i32,
    pub x1: i32,
    pub y1: i32,
}

impl Room {
    fn w(&self) -> i32 {
        self.x1 - self.x0 + 1
    }
    fn h(&self) -> i32 {
        self.y1 - self.y0 + 1
    }
    fn mid(&self) -> (i32, i32) {
        ((self.x0 + self.x1) / 2, (self.y0 + self.y1) / 2)
    }
    fn contains(&self, x: i32, y: i32) -> bool {
        x >= self.x0 && x <= self.x1 && y >= self.y0 && y <= self.y1
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Light {
    pub pos: Vec2,
    pub radius: f32,
    pub color: [f32; 3],
    pub intensity: f32,
}

impl PropKind {
    /// Cover for a crouched body, and in the way of its eyes.
    pub fn is_low(self) -> bool {
        use PropKind::*;
        matches!(
            self,
            Desk | BigDesk | MonitorDesk | Table | Counter | Sofa | Car
        )
    }

    /// Thick enough to walk into and hide in.
    pub fn is_hideout(self) -> bool {
        self == PropKind::Thicket
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PropKind {
    Desk,
    BigDesk,
    Chair,
    Cabinet,
    Bookshelf,
    Plant,
    /// A potted palm, its fronds over whoever passes.
    BigPlant,
    /// A planter of tall, dense plants along a wall, to hide in.
    Thicket,
    Pillar,
    Bench,
    Counter,
    Atm,
    Cooler,
    Rack,
    Table,
    Fridge,
    Vending,
    SinkCounter,
    Toilet,
    Lockers,
    Pallet,
    Gold,
    Safe,
    MonitorDesk,
    Sofa,
    Copier,
    Rug,
    Stanchion,
    VaultDoor,
    Tree,
    Car,
    Van,
    StreetLamp,
    Hydrant,
    Bin,
    Bush,
}

/// Which way a prop's front faces; north is +y.
pub type Face = u8;
pub const NORTH: Face = 0;
pub const EAST: Face = 1;
pub const SOUTH: Face = 2;
pub const WEST: Face = 3;

#[derive(Clone, Copy, Debug)]
pub struct Prop {
    pub kind: PropKind,
    /// The tiles it stands on; `body` is the thing itself inside them.
    pub block: Aabb,
    pub body: Aabb,
    pub face: Face,
    pub seed: u32,
    pub solid: bool,
    pub opaque: bool,
    room: u16,
}

pub fn face_dir(face: Face) -> Vec2 {
    match face {
        NORTH => vec2(0.0, 1.0),
        EAST => vec2(1.0, 0.0),
        SOUTH => vec2(0.0, -1.0),
        _ => vec2(-1.0, 0.0),
    }
}

/// How a kind sits in its block: backed against the wall behind it or
/// centred, inset from the ends, and how deep.
struct Spec {
    backed: bool,
    inset: f32,
    depth: f32,
    solid: bool,
    opaque: bool,
}

fn spec(kind: PropKind) -> Spec {
    use PropKind::*;
    let s = |backed, inset, depth, solid, opaque| Spec {
        backed,
        inset,
        depth,
        solid,
        opaque,
    };
    match kind {
        Desk => s(false, 3.0, 34.0, true, false),
        BigDesk => s(false, 2.0, 40.0, true, false),
        Chair => s(false, 14.0, 24.0, false, false),
        Cabinet => s(true, 7.0, 32.0, true, true),
        Bookshelf => s(true, 2.0, 22.0, true, true),
        Plant => s(false, 12.0, 28.0, true, false),
        BigPlant => s(false, 10.0, 32.0, true, false),
        Thicket => s(true, 0.0, TILE, false, false),
        Pillar => s(false, 6.0, 40.0, true, true),
        Bench => s(false, 4.0, 22.0, true, false),
        Counter => s(false, 0.0, 30.0, true, false),
        Atm => s(true, 9.0, 26.0, true, true),
        Cooler => s(true, 14.0, 22.0, true, false),
        Rack => s(false, 5.0, 34.0, true, true),
        Table => s(false, 12.0, 80.0, true, false),
        Fridge => s(true, 6.0, 36.0, true, true),
        Vending => s(true, 4.0, 34.0, true, true),
        SinkCounter => s(true, 0.0, 26.0, true, false),
        Toilet => s(true, 14.0, 30.0, true, false),
        Lockers => s(true, 0.0, 18.0, true, true),
        Pallet => s(false, 6.0, 40.0, true, false),
        Gold => s(false, 12.0, 26.0, true, false),
        Safe => s(true, 8.0, 36.0, true, true),
        MonitorDesk => s(true, 2.0, 30.0, true, false),
        Sofa => s(true, 4.0, 30.0, true, false),
        Copier => s(true, 8.0, 30.0, true, false),
        Rug | Stanchion | VaultDoor => s(false, 0.0, TILE, false, false),
        Tree => s(false, 18.0, 16.0, true, true),
        Car => s(false, 9.0, 4.0 * TILE - 16.0, true, false),
        Van => s(false, 5.0, 5.0 * TILE - 12.0, true, true),
        StreetLamp => s(false, 21.0, 10.0, true, false),
        Hydrant => s(false, 17.0, 18.0, true, false),
        Bin => s(false, 14.0, 24.0, true, false),
        Bush => s(false, 4.0, 40.0, true, false),
    }
}

pub struct Map {
    pub w: i32,
    pub h: i32,
    pub tiles: Vec<Tile>,
    pub rooms: Vec<Room>,
    pub props: Vec<Prop>,
    pub lights: Vec<Light>,
    pub spawn: Vec2,
    /// x0, y0, x1, y1, walls included.
    pub building: (i32, i32, i32, i32),
}

/// Per tile, the box that stops a ray in 0..=255 of the tile (min x, min y,
/// max x, max y; empty when max ≤ min), and its object (0 for a wall).
pub struct Occluders {
    pub boxes: Vec<[u8; 4]>,
    pub objects: Vec<u8>,
}

const DIRS: [(i32, i32); 4] = [(1, 0), (-1, 0), (0, 1), (0, -1)];

impl Map {
    pub fn idx(&self, x: i32, y: i32) -> usize {
        (y * self.w + x) as usize
    }

    pub fn inside(&self, x: i32, y: i32) -> bool {
        x >= 0 && y >= 0 && x < self.w && y < self.h
    }

    pub fn at(&self, x: i32, y: i32) -> Tile {
        self.tiles[self.idx(x, y)]
    }

    /// Per tile: outdoors, in a room with a lamp, or a door or wall beside one.
    pub fn lit(&self) -> Vec<bool> {
        let mut rooms = vec![false; self.rooms.len()];
        for l in &self.lights {
            let (x, y) = tile_of(l.pos);
            if self.inside(x, y) && self.at(x, y).room != NO_ROOM {
                rooms[self.at(x, y).room as usize] = true;
            }
        }
        let room_lit = |x: i32, y: i32| {
            self.inside(x, y) && self.at(x, y).room != NO_ROOM && rooms[self.at(x, y).room as usize]
        };
        let mut out = Vec::with_capacity(self.tiles.len());
        for y in 0..self.h {
            for x in 0..self.w {
                let t = self.at(x, y);
                out.push(
                    !t.indoor
                        || room_lit(x, y)
                        || (t.room == NO_ROOM
                            && DIRS.iter().any(|(dx, dy)| room_lit(x + dx, y + dy))),
                );
            }
        }
        out
    }

    fn at_mut(&mut self, x: i32, y: i32) -> &mut Tile {
        let i = self.idx(x, y);
        &mut self.tiles[i]
    }

    /// Breaks a window: it becomes an opening with glass on the floor.
    pub fn smash(&mut self, x: i32, y: i32) {
        let t = self.at_mut(x, y);
        t.wall = Wall::None;
        t.deco |= deco::SHARDS;
    }

    pub fn is_wall(&self, x: i32, y: i32) -> bool {
        self.inside(x, y) && self.at(x, y).wall != Wall::None
    }

    /// A leaf per doorway tile (the vault's is its round door), hinged at the
    /// outer ends of a run: the main entrance in glass, the outer wall's in
    /// steel, the rest in wood.
    pub fn doors(&self) -> Vec<Door> {
        let door = |x: i32, y: i32| {
            self.inside(x, y) && self.at(x, y).deco & (deco::DOOR | deco::VAULT_DOOR) == deco::DOOR
        };
        let mut out = Vec::new();
        for y in 0..self.h {
            for x in 0..self.w {
                if !door(x, y) {
                    continue;
                }
                let flat = self.is_wall(x - 1, y)
                    || self.is_wall(x + 1, y)
                    || door(x - 1, y)
                    || door(x + 1, y);
                let (dx, dy) = if flat { (1, 0) } else { (0, 1) };
                let mut i = 0;
                while door(x - dx * (i + 1), y - dy * (i + 1)) {
                    i += 1;
                }
                let mut n = i + 1;
                while door(x + dx * (n - i), y + dy * (n - i)) {
                    n += 1;
                }
                let end = self.at(x - dx * (i + 1), y - dy * (i + 1)).wall;
                let kind = match (n, end) {
                    (3.., _) => DoorKind::Glass,
                    (_, Wall::Brick) => DoorKind::Steel,
                    _ => DoorKind::Wood,
                };
                let along = vec2(dx as f32, dy as f32);
                let low = i < n / 2;
                let reach = TILE * 0.5 - 1.5;
                let centre = tile_centre(x, y);
                out.push(Door {
                    x,
                    y,
                    hinge: centre + along * if low { -reach } else { reach },
                    along: if low { along } else { -along },
                    across: vec2(-along.y, along.x),
                    kind,
                });
            }
        }
        out
    }

    /// Per tile: no wall and no solid prop on it, so a body fits at its centre.
    pub fn free(&self) -> Vec<bool> {
        let used = used_tiles(self);
        self.tiles
            .iter()
            .zip(used)
            .map(|(t, u)| t.wall == Wall::None && !u)
            .collect()
    }

    pub fn bounds(&self) -> Aabb {
        Aabb::new(Vec2::ZERO, vec2(self.w as f32, self.h as f32) * TILE)
    }

    pub fn solids(&self) -> Vec<Aabb> {
        let mut out = Vec::new();
        for y in 0..self.h {
            for x in 0..self.w {
                if self.at(x, y).wall != Wall::None {
                    out.push(tile_box(x, y));
                }
            }
        }
        out.extend(self.props.iter().filter(|p| p.solid).map(|p| p.body));
        out
    }

    /// The walls and tall props, or with `low` the half-height props.
    pub fn occluders(&self, low: bool) -> Occluders {
        let n = self.tiles.len();
        let mut boxes = vec![[0u8; 4]; n];
        let mut objects = vec![0u8; n];
        for (i, t) in self.tiles.iter().enumerate() {
            if !low && !matches!(t.wall, Wall::None | Wall::Glass) {
                boxes[i] = [0, 0, 255, 255];
            }
        }
        let q = |v: f32, up: bool| {
            let s = v * 255.0;
            (if up { s.ceil() } else { s.floor() }).clamp(0.0, 255.0) as u8
        };
        let mut id = 0u8;
        for p in self
            .props
            .iter()
            .filter(|p| if low { p.kind.is_low() } else { p.opaque })
        {
            id = if id >= 254 { 1 } else { id + 1 };
            let b = p.body;
            for (x, y) in self.tiles_under(b) {
                let t = tile_box(x, y);
                let lo = (b.min.max(t.min) - t.min) / TILE;
                let hi = (b.max.min(t.max) - t.min) / TILE;
                if hi.x <= lo.x || hi.y <= lo.y {
                    continue;
                }
                let i = self.idx(x, y);
                let old = boxes[i];
                let new = [q(lo.x, false), q(lo.y, false), q(hi.x, true), q(hi.y, true)];
                boxes[i] = if old[2] > old[0] {
                    [
                        old[0].min(new[0]),
                        old[1].min(new[1]),
                        old[2].max(new[2]),
                        old[3].max(new[3]),
                    ]
                } else {
                    new
                };
                objects[i] = id;
            }
        }
        Occluders { boxes, objects }
    }

    fn tiles_under(&self, b: Aabb) -> impl Iterator<Item = (i32, i32)> {
        let (x0, y0) = tile_of(b.min);
        let (x1, y1) = tile_of(b.max - Vec2::splat(0.01));
        let (x0, x1) = (x0.max(0), x1.min(self.w - 1));
        (y0.max(0)..=y1.min(self.h - 1)).flat_map(move |y| (x0..=x1).map(move |x| (x, y)))
    }

    fn touched(&self, r: &Room, reached: &[bool]) -> bool {
        (r.y0..=r.y1).any(|y| (r.x0..=r.x1).any(|x| reached[self.idx(x, y)]))
    }
}

/// The tiles a solid prop stands on.
fn used_tiles(map: &Map) -> Vec<bool> {
    let mut used = vec![false; map.tiles.len()];
    for p in map.props.iter().filter(|p| p.solid) {
        for (x, y) in map.tiles_under(p.block) {
            used[map.idx(x, y)] = true;
        }
    }
    used
}

/// The tiles just in front of a block facing `face`.
fn front_of(x: i32, y: i32, w: i32, h: i32, face: Face) -> Vec<(i32, i32)> {
    let (fx, fy, fw, fh) = match face {
        NORTH => (x, y + h, w, 1),
        SOUTH => (x, y - 1, w, 1),
        EAST => (x + w, y, 1, h),
        _ => (x - 1, y, 1, h),
    };
    (fy..fy + fh)
        .flat_map(|y| (fx..fx + fw).map(move |x| (x, y)))
        .collect()
}

pub fn tile_box(x: i32, y: i32) -> Aabb {
    let min = vec2(x as f32, y as f32) * TILE;
    Aabb::new(min, min + Vec2::splat(TILE))
}

pub fn tile_of(p: Vec2) -> (i32, i32) {
    ((p.x / TILE).floor() as i32, (p.y / TILE).floor() as i32)
}

pub fn tile_centre(x: i32, y: i32) -> Vec2 {
    (vec2(x as f32, y as f32) + Vec2::splat(0.5)) * TILE
}

const WARM: [f32; 3] = [1.0, 0.82, 0.58];
const FLUORESCENT: [f32; 3] = [0.82, 0.92, 1.0];
const SODIUM: [f32; 3] = [1.0, 0.62, 0.28];
const SERVER_BLUE: [f32; 3] = [0.45, 0.7, 1.0];
const VAULT_RED: [f32; 3] = [1.0, 0.32, 0.22];

struct Gen {
    map: Map,
    rng: Rng,
    used: Vec<bool>,
    clear: Vec<bool>,
}

pub fn generate(seed: u64) -> Map {
    let mut rng = Rng::new(seed);
    let bw = rng.range(46, 61);
    let bh = rng.range(34, 43);
    let side = rng.range(7, 10);
    let road = 6;
    let pavement = 3;
    let park = rng.range(11, 14);
    let w = bw + 2 * side;
    let h = road + pavement + bh + park;
    let (bx0, by0) = (side, road + pavement);
    let n = (w * h) as usize;
    let map = Map {
        w,
        h,
        tiles: vec![
            Tile {
                floor: Floor::Grass,
                wall: Wall::None,
                tint: 0,
                deco: 0,
                room: NO_ROOM,
                indoor: false,
            };
            n
        ],
        rooms: Vec::new(),
        props: Vec::new(),
        lights: Vec::new(),
        spawn: Vec2::ZERO,
        building: (bx0, by0, bx0 + bw - 1, by0 + bh - 1),
    };
    let mut g = Gen {
        map,
        rng,
        used: vec![false; n],
        clear: vec![false; n],
    };
    g.outside(road, pavement);
    let entrance = g.building();
    g.fix_walls();
    g.mark_clear();
    g.furnish();
    g.street(road, entrance);
    g.fix_props();
    for t in &mut g.map.tiles {
        let r = g.rng.u32();
        if t.tint == 0 {
            t.tint = r as u8;
        }
    }
    g.map
}

impl Gen {
    fn set_floor(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, floor: Floor, tint: u8) {
        for y in y0..=y1 {
            for x in x0..=x1 {
                if self.map.inside(x, y) {
                    let t = self.map.at_mut(x, y);
                    t.floor = floor;
                    t.tint = tint;
                }
            }
        }
    }

    fn set_wall(&mut self, x: i32, y: i32, wall: Wall) {
        if self.map.inside(x, y) {
            let t = self.map.at_mut(x, y);
            t.wall = wall;
            t.deco &= !(deco::DOOR | deco::VAULT_DOOR);
        }
    }

    fn door(&mut self, x: i32, y: i32, floor: Floor, tint: u8) {
        let t = self.map.at_mut(x, y);
        t.wall = Wall::None;
        t.floor = floor;
        t.tint = tint;
        t.deco |= deco::DOOR;
    }

    fn add_room(&mut self, kind: RoomKind, x0: i32, y0: i32, x1: i32, y1: i32) -> u16 {
        let id = self.map.rooms.len() as u16;
        self.map.rooms.push(Room {
            kind,
            x0,
            y0,
            x1,
            y1,
        });
        for y in y0..=y1 {
            for x in x0..=x1 {
                self.map.at_mut(x, y).room = id;
            }
        }
        id
    }

    fn light(&mut self, x: f32, y: f32, radius_tiles: f32, color: [f32; 3], intensity: f32) {
        self.map.lights.push(Light {
            pos: vec2(x, y) * TILE,
            radius: radius_tiles * TILE,
            color,
            intensity,
        });
    }

    fn outside(&mut self, road: i32, pavement: i32) {
        let (bx0, _, bx1, by1) = self.map.building;
        let (w, h) = (self.map.w, self.map.h);
        self.set_floor(0, 0, w - 1, road - 1, Floor::Asphalt, 0);
        for x in 0..w {
            self.map.at_mut(x, road / 2).deco |= deco::LANE;
        }
        let zebra = if self.rng.chance(0.5) {
            bx0 - 4
        } else {
            bx1 + 2
        };
        for y in 0..road {
            for x in zebra..zebra + 3 {
                if self.map.inside(x, y) {
                    self.map.at_mut(x, y).deco |= deco::ZEBRA;
                }
            }
        }
        self.set_floor(0, road, w - 1, road + pavement - 1, Floor::Sidewalk, 0);
        for (x0, x1) in [(bx0 - 2, bx0 - 1), (bx1 + 1, bx1 + 2)] {
            self.set_floor(x0, road + pavement, x1, by1 + 2, Floor::Sidewalk, 0);
        }
        self.set_floor(bx0 - 2, by1 + 1, bx1 + 2, by1 + 2, Floor::Sidewalk, 0);
        let p0 = by1 + 3;
        self.set_floor(bx0 - 4, p0, bx1 + 4, h - 2, Floor::Parking, 0);
        for (r0, r1) in bay_rows(p0, h) {
            if r1 - r0 < BAY_DEEP - 1 || r0 < p0 {
                continue;
            }
            for x in (bx0 - 2..=bx1 + 2).step_by(BAY_WIDE as usize) {
                for y in r0..=r1 {
                    self.map.at_mut(x, y).deco |= deco::BAY;
                }
            }
        }
    }

    /// Returns the entrance's middle tile x.
    fn building(&mut self) -> i32 {
        let (bx0, by0, bx1, by1) = self.map.building;
        for y in by0..=by1 {
            for x in bx0..=bx1 {
                let t = self.map.at_mut(x, y);
                t.floor = Floor::Concrete;
                t.indoor = true;
                t.deco = 0;
                if x == bx0 || x == bx1 || y == by0 || y == by1 {
                    t.wall = Wall::Brick;
                }
            }
        }
        let (ix0, iy0, ix1, iy1) = (bx0 + 1, by0 + 1, bx1 - 1, by1 - 1);
        let depth = (((iy1 - iy0 + 1) as f32 * 0.37) as i32).clamp(11, 15);
        let ly = iy0 + depth; // the lobby's back wall
        let (cy0, cy1) = (ly + 1, ly + 3); // the corridor
        let ry = cy1 + 1; // the back rooms' front wall
        for x in ix0..=ix1 {
            self.set_wall(x, ly, Wall::Drywall);
            self.set_wall(x, ry, Wall::Drywall);
        }

        // Each front corner: nothing, an office, or an office and a washroom.
        let corner = |g: &mut Gen, left: bool| -> i32 {
            if !g.rng.chance(0.75) {
                return 0;
            }
            let cw = g.rng.range(7, 11);
            let wx = if left { ix0 + cw } else { ix1 - cw };
            for y in iy0..ly {
                g.set_wall(wx, y, Wall::Drywall);
            }
            let (rx0, rx1) = if left { (ix0, wx - 1) } else { (wx + 1, ix1) };
            let split = depth >= 12 && g.rng.chance(0.6);
            let tint = g.rng.u32() as u8;
            if split {
                let my = iy0 + depth / 2;
                for x in rx0..=rx1 {
                    g.set_wall(x, my, Wall::Drywall);
                }
                g.set_floor(rx0, iy0, rx1, my - 1, Floor::Carpet, tint);
                g.add_room(RoomKind::Office, rx0, iy0, rx1, my - 1);
                g.set_floor(rx0, my + 1, rx1, ly - 1, Floor::WashroomTile, 0);
                g.add_room(RoomKind::Washroom, rx0, my + 1, rx1, ly - 1);
                let d0 = g.rng.range(iy0 + 1, my - 1);
                g.door(wx, d0, Floor::Carpet, tint);
                let d1 = g.rng.range(my + 1, ly - 1);
                g.door(wx, d1, Floor::WashroomTile, 0);
            } else {
                g.set_floor(rx0, iy0, rx1, ly - 1, Floor::Carpet, tint);
                g.add_room(RoomKind::Office, rx0, iy0, rx1, ly - 1);
                let d = g.rng.range(iy0 + 2, ly - 5);
                g.door(wx, d, Floor::Carpet, tint);
            }
            cw + 1
        };
        let sl = corner(self, true);
        let sr = corner(self, false);
        let (lx0, lx1) = (ix0 + sl, ix1 - sr);

        self.set_floor(lx0, iy0, lx1, ly - 1, Floor::Marble, 0);
        let lobby = self.add_room(RoomKind::Lobby, lx0, iy0, lx1, ly - 1);
        let cr = ly - 4; // the counter's row
        self.set_floor(lx0, cr + 1, lx1, ly - 1, Floor::Wood, 0);
        let entrance = (lx0 + lx1) / 2;
        for x in entrance - 1..=entrance + 1 {
            self.door(x, by0, Floor::Marble, 0);
            self.map.at_mut(x, by0).indoor = true;
        }
        for x in bx0 + 2..bx1 - 1 {
            if (x - entrance).abs() > 2
                && (x - bx0) % 4 != 0
                && self.map.at(x, by0).wall == Wall::Brick
            {
                self.set_wall(x, by0, Wall::Glass);
            }
        }
        // A gap at one end to walk round, and at any end a door opens onto.
        let door = |x: i32| self.map.at(x, cr).deco & deco::DOOR != 0;
        let (mut c0, mut c1) = if self.rng.chance(0.5) {
            (lx0 + 2, lx1)
        } else {
            (lx0, lx1 - 2)
        };
        if door(lx0 - 1) {
            c0 = lx0 + 2;
        }
        if door(lx1 + 1) {
            c1 = lx1 - 2;
        }
        // Not `place`: the counter ignores door clearances.
        self.put(PropKind::Counter, c0, cr, c1 - c0 + 1, 1, SOUTH, lobby);
        for x in (c0 + 1..c1).step_by(3) {
            self.place(PropKind::Chair, x, cr + 1, 1, 1, SOUTH, lobby);
            self.keep_free(x, cr + 1, x, cr + 1);
        }
        let n_doors = if lx1 - lx0 > 20 { 2 } else { 1 };
        let span = (lx1 - lx0 - 3) / n_doors;
        for k in 0..n_doors {
            let x = lx0 + 2 + k * span + self.rng.range(0, span.max(1) - 1);
            self.door(x, ly, Floor::Wood, 0);
            self.door(x + 1, ly, Floor::Wood, 0);
        }

        let tint = self.rng.u32() as u8;
        self.set_floor(ix0, cy0, ix1, cy1, Floor::Lino, tint);
        self.add_room(RoomKind::Corridor, ix0, cy0, ix1, cy1);
        let side_x = if self.rng.chance(0.5) { bx0 } else { bx1 };
        self.door(side_x, cy0 + 1, Floor::Lino, tint);
        self.map.at_mut(side_x, cy0 + 1).indoor = true;
        for x in (ix0 + 3..ix1).step_by(7) {
            self.light(x as f32 + 0.5, cy0 as f32 + 1.5, 6.0, FLUORESCENT, 0.9);
        }

        let mut leaves = Vec::new();
        let mut splits = Vec::new();
        self.bsp(ix0, ry + 1, ix1, iy1, &mut leaves, &mut splits);
        self.assign_back_rooms(leaves, splits, ry);
        for x in bx0 + 2..bx1 - 1 {
            let t = self.map.at(x, by1 - 1);
            if t.wall == Wall::None && t.floor != Floor::Steel && (x - bx0) % 5 < 2 {
                self.set_wall(x, by1, Wall::Glass);
            }
        }
        for y in (iy0 + 2..cr).step_by(5) {
            for x in (lx0 + 2..lx1).step_by(6) {
                self.light(x as f32 + 0.5, y as f32 + 0.5, 7.5, WARM, 0.95);
            }
        }
        for x in (lx0 + 3..lx1).step_by(7) {
            self.light(x as f32, (ly - 2) as f32 + 0.5, 5.0, WARM, 0.6);
        }
        entrance
    }

    fn bsp(
        &mut self,
        x0: i32,
        y0: i32,
        x1: i32,
        y1: i32,
        leaves: &mut Vec<(i32, i32, i32, i32)>,
        splits: &mut Vec<(bool, i32, i32, i32)>,
    ) {
        const MIN: i32 = 4;
        let (w, h) = (x1 - x0 + 1, y1 - y0 + 1);
        let wide = w > 13 || (w > 9 && self.rng.chance(0.4));
        let tall = h > 10 || (h > 8 && self.rng.chance(0.3));
        let can_h = h > 2 * MIN;
        if wide && w > 2 * MIN && (!tall || !can_h || w >= h) {
            let cut = x0 + self.rng.range(MIN, w - MIN);
            for y in y0..=y1 {
                self.set_wall(cut, y, Wall::Drywall);
            }
            splits.push((true, cut, y0, y1));
            self.bsp(x0, y0, cut - 1, y1, leaves, splits);
            self.bsp(cut + 1, y0, x1, y1, leaves, splits);
        } else if tall && can_h {
            let cut = y0 + self.rng.range(MIN, h - MIN);
            for x in x0..=x1 {
                self.set_wall(x, cut, Wall::Drywall);
            }
            splits.push((false, cut, x0, x1));
            self.bsp(x0, y0, x1, cut - 1, leaves, splits);
            self.bsp(x0, cut + 1, x1, y1, leaves, splits);
        } else {
            leaves.push((x0, y0, x1, y1));
        }
    }

    fn assign_back_rooms(
        &mut self,
        leaves: Vec<(i32, i32, i32, i32)>,
        splits: Vec<(bool, i32, i32, i32)>,
        ry: i32,
    ) {
        let area = |i: usize| {
            let (x0, y0, x1, y1) = leaves[i];
            (x1 - x0 + 1) * (y1 - y0 + 1)
        };
        // The vault: the biggest leaf that will take it, deepest first.
        let mut order: Vec<usize> = (0..leaves.len()).collect();
        order.sort_by_key(|&i| (-(area(i).min(80)), -leaves[i].1));
        let vault = order
            .iter()
            .copied()
            .find(|&i| {
                let (x0, y0, x1, y1) = leaves[i];
                x1 - x0 + 1 >= 6 && y1 - y0 + 1 >= 5
            })
            .unwrap_or(order[0]);
        let mut rest: Vec<usize> = order.into_iter().filter(|&i| i != vault).collect();
        rest.sort_by_key(|&i| -area(i));
        let mut kinds = vec![RoomKind::Office; rest.len()];
        let big = [RoomKind::OpenOffice, RoomKind::Manager, RoomKind::Break];
        let mut small = [
            RoomKind::Washroom,
            RoomKind::Server,
            RoomKind::Security,
            RoomKind::Archive,
        ];
        self.rng.shuffle(&mut small[1..]);
        let nb = big.len().min(rest.len());
        kinds[..nb].copy_from_slice(&big[..nb]);
        for (k, kind) in (nb..rest.len()).rev().zip(small) {
            kinds[k] = kind;
        }

        for &(vertical, cut, a0, a1) in &splits {
            let (dx, dy) = if vertical { (1, 0) } else { (0, 1) };
            let spots: Vec<(i32, i32)> = (a0 + 1..a1)
                .map(|k| if vertical { (cut, k) } else { (k, cut) })
                .filter(|&(x, y)| {
                    !self.map.is_wall(x - dx, y - dy) && !self.map.is_wall(x + dx, y + dy)
                })
                .collect();
            if !spots.is_empty() && self.rng.chance(0.55) {
                let (x, y) = self.rng.pick(&spots);
                self.door(x, y, Floor::Concrete, 0);
            }
        }

        for (&i, &kind) in rest.iter().zip(&kinds) {
            let (x0, y0, x1, y1) = leaves[i];
            let tint = self.rng.u32() as u8;
            let floor = match kind {
                RoomKind::OpenOffice | RoomKind::Office | RoomKind::Security => Floor::Carpet,
                RoomKind::Manager => Floor::Wood,
                RoomKind::Break => Floor::Lino,
                RoomKind::Washroom => Floor::WashroomTile,
                RoomKind::Server => Floor::Raised,
                _ => Floor::Concrete,
            };
            self.set_floor(x0, y0, x1, y1, floor, tint);
            self.add_room(kind, x0, y0, x1, y1);
            if y0 == ry + 1 {
                let x = self.rng.range(x0 + 1, x1);
                self.door(x, ry, floor, tint);
                if matches!(
                    kind,
                    RoomKind::Office | RoomKind::Manager | RoomKind::OpenOffice
                ) && self.rng.chance(0.5)
                {
                    for gx in x0..=x1 {
                        if (gx - x).abs() > 1 && self.map.at(gx, ry).wall == Wall::Drywall {
                            self.set_wall(gx, ry, Wall::Glass);
                        }
                    }
                }
            }
            // The BSP's doors were laid as concrete: they take the floor of
            // a room they open into.
            for y in y0 - 1..=y1 + 1 {
                for x in x0 - 1..=x1 + 1 {
                    if self.map.inside(x, y)
                        && self.map.at(x, y).deco & deco::DOOR != 0
                        && self.map.at(x, y).floor == Floor::Concrete
                    {
                        let t = self.map.at_mut(x, y);
                        t.floor = floor;
                        t.tint = tint;
                    }
                }
            }
            self.room_lights(kind, x0, y0, x1, y1);
        }

        // The vault's walls replace any door into it; it gets one of its own.
        let (x0, y0, x1, y1) = leaves[vault];
        self.set_floor(x0, y0, x1, y1, Floor::Steel, 0);
        let vault_id = self.add_room(RoomKind::Vault, x0, y0, x1, y1);
        let room = self.map.rooms[vault_id as usize];
        let (bx0, _, bx1, by1) = self.map.building;
        for y in y0 - 1..=y1 + 1 {
            for x in x0 - 1..=x1 + 1 {
                if !room.contains(x, y) {
                    let exterior = x == bx0 || x == bx1 || y == by1;
                    self.set_wall(x, y, if exterior { Wall::Brick } else { Wall::Vault });
                }
            }
        }
        // (door tile, outward step): onto the corridor if it can, else
        // nearest the middle.
        let mut spots = Vec::new();
        for x in x0 + 1..x1 - 1 {
            spots.push((x, y0 - 1, 0, -1));
            spots.push((x, y1 + 1, 0, 1));
        }
        for y in y0 + 1..y1 - 1 {
            spots.push((x0 - 1, y, -1, 0));
            spots.push((x1 + 1, y, 1, 0));
        }
        let open = |m: &Map, x: i32, y: i32| m.inside(x, y) && m.at(x, y).wall == Wall::None;
        spots.retain(|&(x, y, dx, dy)| {
            let (px, py) = (-dy, dx);
            open(&self.map, x + dx, y + dy)
                && open(&self.map, x + px + dx, y + py + dy)
                && self.map.at(x + dx, y + dy).indoor
                && self.map.at(x + px, y + py).wall == Wall::Vault
        });
        let (mx, my) = room.mid();
        spots.sort_by_key(|&(x, y, dx, dy)| {
            let r = self.map.at(x + dx, y + dy).room;
            let onto_corridor =
                r != NO_ROOM && self.map.rooms[r as usize].kind == RoomKind::Corridor;
            (!onto_corridor, (x - mx).abs() + (y - my).abs())
        });
        if let Some(&(x, y, dx, dy)) = spots.first() {
            let (px, py) = (-dy, dx);
            for (tx, ty) in [(x, y), (x + px, y + py)] {
                self.door(tx, ty, Floor::Steel, 0);
                self.map.at_mut(tx, ty).deco |= deco::VAULT_DOOR;
            }
            let face = match (dx, dy) {
                (0, 1) => NORTH,
                (1, 0) => EAST,
                (0, -1) => SOUTH,
                _ => WEST,
            };
            let (ox, oy) = (x.min(x + px), y.min(y + py));
            let (w, h) = if px != 0 { (2, 1) } else { (1, 2) };
            let block = Aabb::new(
                vec2(ox as f32, oy as f32) * TILE,
                vec2((ox + w) as f32, (oy + h) as f32) * TILE,
            );
            self.map.props.push(Prop {
                kind: PropKind::VaultDoor,
                block,
                body: block,
                face,
                seed: 0,
                solid: false,
                opaque: false,
                room: vault_id,
            });
        }
        self.room_lights(RoomKind::Vault, x0, y0, x1, y1);
    }

    fn room_lights(&mut self, kind: RoomKind, x0: i32, y0: i32, x1: i32, y1: i32) {
        let (color, intensity, reach, on) = match kind {
            RoomKind::Vault => (VAULT_RED, 0.75, 6.5, 1.0),
            RoomKind::Server => (SERVER_BLUE, 0.7, 5.5, 1.0),
            RoomKind::Washroom => (FLUORESCENT, 0.8, 5.0, 0.7),
            RoomKind::Manager => (WARM, 0.85, 6.0, 0.6),
            RoomKind::Break => (FLUORESCENT, 0.85, 6.0, 0.6),
            RoomKind::Security => (FLUORESCENT, 0.7, 5.5, 1.0),
            _ => (FLUORESCENT, 0.8, 6.0, 0.45),
        };
        if !self.rng.chance(on) {
            return;
        }
        let (w, h) = (x1 - x0 + 1, y1 - y0 + 1);
        let nx = ((w as f32) / 5.5).ceil().max(1.0) as i32;
        let ny = ((h as f32) / 5.5).ceil().max(1.0) as i32;
        for j in 0..ny {
            for i in 0..nx {
                let x = x0 as f32 + w as f32 * (i as f32 + 0.5) / nx as f32;
                let y = y0 as f32 + h as f32 * (j as f32 + 0.5) / ny as f32;
                self.light(x, y, reach, color, intensity);
            }
        }
    }

    /// Punch a door into a room the street can't reach, until none is left.
    fn fix_walls(&mut self) {
        for _ in 0..64 {
            let reached = self.flood(false);
            // Only a shut room beside a reached one: one shut in behind
            // another opens on a later pass.
            let Some((room, spots)) = self
                .map
                .rooms
                .iter()
                .filter(|r| !self.map.touched(r, &reached))
                .map(|&r| (r, self.door_spots(r, &reached)))
                .find(|(_, spots)| !spots.is_empty())
            else {
                return;
            };
            let (x, y) = self.rng.pick(&spots);
            let inner = self.map.at(room.x0, room.y0);
            self.door(x, y, inner.floor, inner.tint);
        }
    }

    /// Wall tiles between `room` and a reached tile, once per such pair.
    fn door_spots(&self, room: Room, reached: &[bool]) -> Vec<(i32, i32)> {
        let mut spots = Vec::new();
        for y in room.y0 - 1..=room.y1 + 1 {
            for x in room.x0 - 1..=room.x1 + 1 {
                if !self.map.inside(x, y) {
                    continue;
                }
                let wall = self.map.at(x, y).wall;
                if !matches!(wall, Wall::Drywall | Wall::Glass)
                    && !(room.kind == RoomKind::Vault && wall == Wall::Vault)
                {
                    continue;
                }
                for (dx, dy) in DIRS {
                    let (ox, oy) = (x + dx, y + dy);
                    // Not onto furniture (the counter is in already) either side.
                    if room.contains(x - dx, y - dy)
                        && self.map.inside(ox, oy)
                        && reached[self.map.idx(ox, oy)]
                        && !self.used[self.map.idx(ox, oy)]
                        && !self.used[self.map.idx(x - dx, y - dy)]
                    {
                        spots.push((x, y));
                    }
                }
            }
        }
        spots
    }

    /// The tiles reachable from the street; with `props`, round the furniture.
    fn flood(&self, props: bool) -> Vec<bool> {
        let m = &self.map;
        let mut seen = vec![false; m.tiles.len()];
        let mut queue = VecDeque::new();
        // On the pavement until there is a spawn.
        let start = if m.spawn == Vec2::ZERO {
            (1, 7)
        } else {
            tile_of(m.spawn)
        };
        seen[m.idx(start.0, start.1)] = true;
        queue.push_back(start);
        while let Some((x, y)) = queue.pop_front() {
            for (dx, dy) in DIRS {
                let (nx, ny) = (x + dx, y + dy);
                if !m.inside(nx, ny) {
                    continue;
                }
                let i = m.idx(nx, ny);
                if seen[i] || m.at(nx, ny).wall != Wall::None || (props && self.used[i]) {
                    continue;
                }
                seen[i] = true;
                queue.push_back((nx, ny));
            }
        }
        seen
    }

    /// Around every door, and two tiles straight through it.
    fn mark_clear(&mut self) {
        for y in 0..self.map.h {
            for x in 0..self.map.w {
                if self.map.at(x, y).deco & deco::DOOR == 0 {
                    continue;
                }
                let through = if self.map.is_wall(x - 1, y) || self.map.is_wall(x + 1, y) {
                    [(0, 2), (0, -2)]
                } else {
                    [(2, 0), (-2, 0)]
                };
                let around = (-1..=1).flat_map(|dy| (-1..=1).map(move |dx| (dx, dy)));
                for (dx, dy) in around.chain(through) {
                    if self.map.inside(x + dx, y + dy) {
                        let i = self.map.idx(x + dx, y + dy);
                        self.clear[i] = true;
                    }
                }
            }
        }
    }

    /// Take furniture out of any room where it walls a door in.
    fn fix_props(&mut self) {
        for _ in 0..200 {
            let reached = self.flood(true);
            // A door nobody reaches, beside a room partly reached.
            let mut stuck = None;
            'find: for y in 0..self.map.h {
                for x in 0..self.map.w {
                    if self.map.at(x, y).deco & deco::DOOR == 0 || reached[self.map.idx(x, y)] {
                        continue;
                    }
                    for (dx, dy) in DIRS {
                        if self.map.inside(x + dx, y + dy) {
                            let r = self.map.at(x + dx, y + dy).room;
                            if r != NO_ROOM
                                && self.map.touched(&self.map.rooms[r as usize], &reached)
                            {
                                stuck = Some(r);
                                break 'find;
                            }
                        }
                    }
                }
            }
            let Some(room) = stuck else { break };
            let props = &self.map.props;
            let Some(k) = props.iter().rposition(|p| p.room == room && p.solid) else {
                break;
            };
            self.map.props.remove(k);
            self.used = used_tiles(&self.map);
        }
        // A chair whose desk went goes too.
        let props = &self.map.props;
        let keep: Vec<bool> = props
            .iter()
            .map(|c| {
                let front = c.block.centre() + face_dir(c.face) * TILE;
                c.kind != PropKind::Chair
                    || props.iter().any(|p| p.solid && p.block.contains(front))
            })
            .collect();
        let mut keep = keep.into_iter();
        self.map.props.retain(|_| keep.next().unwrap());
    }

    /// Whether a block of tiles is free floor of one room.
    fn free(&self, x: i32, y: i32, w: i32, h: i32, room: u16) -> bool {
        (y..y + h).all(|ty| {
            (x..x + w).all(|tx| {
                if !self.map.inside(tx, ty) {
                    return false;
                }
                let i = self.map.idx(tx, ty);
                let t = self.map.tiles[i];
                t.wall == Wall::None
                    && !self.used[i]
                    && !self.clear[i]
                    && (room == NO_ROOM || t.room == room)
                    && t.deco & deco::DOOR == 0
            })
        })
    }

    /// Nothing else goes on these tiles or the ring round them: a desk keeps
    /// its chair, and no pockets too tight to walk into.
    fn keep_free(&mut self, x0: i32, y0: i32, x1: i32, y1: i32) {
        for y in y0 - 1..=y1 + 1 {
            for x in x0 - 1..=x1 + 1 {
                if self.map.inside(x, y) {
                    let i = self.map.idx(x, y);
                    self.clear[i] = true;
                }
            }
        }
    }

    /// A prop on `w` × `h` tiles from `(x, y)`, if they are free.
    /// Decoration goes anywhere. A piece against a wall keeps the row in
    /// front of it open, or a body could not reach it, and the floor its
    /// shallow block leaves would be a pocket too tight to step into.
    #[allow(clippy::too_many_arguments)]
    fn place(
        &mut self,
        kind: PropKind,
        x: i32,
        y: i32,
        w: i32,
        h: i32,
        face: Face,
        room: u16,
    ) -> bool {
        let s = spec(kind);
        let hideout = kind.is_hideout();
        if (s.solid || hideout) && !self.free(x, y, w, h, room) {
            return false;
        }
        // Walked into, but nothing else goes there.
        if hideout {
            self.keep_free(x, y, x + w - 1, y + h - 1);
        }
        if s.solid && s.backed {
            let front = front_of(x, y, w, h, face);
            let open = |&(x, y): &(i32, i32)| {
                self.map.inside(x, y) && !self.map.is_wall(x, y) && !self.used[self.map.idx(x, y)]
            };
            if !front.iter().all(open) {
                return false;
            }
            for (x, y) in front {
                let i = self.map.idx(x, y);
                self.clear[i] = true;
            }
        }
        self.put(kind, x, y, w, h, face, room);
        true
    }

    #[allow(clippy::too_many_arguments)]
    fn put(&mut self, kind: PropKind, x: i32, y: i32, w: i32, h: i32, face: Face, room: u16) {
        let s = spec(kind);
        let block = Aabb::new(
            vec2(x as f32, y as f32) * TILE,
            vec2((x + w) as f32, (y + h) as f32) * TILE,
        );
        if s.solid {
            for (x, y) in self.map.tiles_under(block) {
                let i = self.map.idx(x, y);
                self.used[i] = true;
            }
        }
        let seed = self.rng.u32();
        self.map.props.push(Prop {
            kind,
            block,
            body: body_in(block, face, &s),
            face,
            seed,
            solid: s.solid,
            opaque: s.opaque,
            room,
        });
    }

    /// Up to `n` of the spots, shuffled, that will take one.
    fn place_some(
        &mut self,
        kind: PropKind,
        room: u16,
        mut spots: Vec<(i32, i32, i32, i32, Face)>,
        n: usize,
    ) {
        self.rng.shuffle(&mut spots);
        let mut placed = 0;
        for (x, y, w, h, face) in spots {
            if placed >= n {
                break;
            }
            if self.place(kind, x, y, w, h, face, room) {
                placed += 1;
            }
        }
    }

    /// Against the room's walls, `along` tiles long, facing in.
    fn along_walls(&mut self, kind: PropKind, room: u16, along: i32, n: usize) {
        let r = self.map.rooms[room as usize];
        let walled = |x: i32, y: i32, dx: i32, dy: i32| {
            (0..along).all(|k| self.map.is_wall(x + k * dx, y + k * dy))
        };
        let mut spots = Vec::new();
        for x in r.x0..=r.x1 - along + 1 {
            if walled(x, r.y1 + 1, 1, 0) {
                spots.push((x, r.y1, along, 1, SOUTH));
            }
            if walled(x, r.y0 - 1, 1, 0) {
                spots.push((x, r.y0, along, 1, NORTH));
            }
        }
        for y in r.y0..=r.y1 - along + 1 {
            if walled(r.x0 - 1, y, 0, 1) {
                spots.push((r.x0, y, 1, along, EAST));
            }
            if walled(r.x1 + 1, y, 0, 1) {
                spots.push((r.x1, y, 1, along, WEST));
            }
        }
        self.place_some(kind, room, spots, n);
    }

    fn in_corners(&mut self, kind: PropKind, room: u16, n: usize) {
        let r = self.map.rooms[room as usize];
        let corners = vec![
            (r.x0, r.y0, 1, 1, NORTH),
            (r.x1, r.y0, 1, 1, NORTH),
            (r.x0, r.y1, 1, 1, SOUTH),
            (r.x1, r.y1, 1, 1, SOUTH),
        ];
        self.place_some(kind, room, corners, n);
    }

    fn furnish(&mut self) {
        use PropKind::*;
        for id in 0..self.map.rooms.len() as u16 {
            let r = self.map.rooms[id as usize];
            match r.kind {
                RoomKind::Lobby => self.furnish_lobby(id),
                RoomKind::Corridor => {
                    self.along_walls(Plant, id, 1, (r.w() / 12) as usize);
                    self.along_walls(Thicket, id, 2, (r.w() / 20) as usize);
                    self.along_walls(Cooler, id, 1, 1);
                    self.along_walls(Bin, id, 1, 1);
                }
                RoomKind::OpenOffice => {
                    for y in (r.y0 + 1..r.y1 - 1).step_by(3) {
                        for x in (r.x0 + 1..r.x1 - 1).step_by(3) {
                            if self.place(Desk, x, y, 2, 1, SOUTH, id) {
                                self.place(Chair, x, y - 1, 1, 1, NORTH, id);
                                self.keep_free(x, y - 1, x + 1, y);
                            }
                        }
                    }
                    self.in_corners(BigPlant, id, 1);
                    self.in_corners(Plant, id, 2);
                    self.along_walls(Thicket, id, 2, 1);
                    self.along_walls(Copier, id, 1, 1);
                    self.along_walls(Cabinet, id, 1, 2);
                }
                RoomKind::Office => {
                    let (cx, cy) = r.mid();
                    if self.place(Desk, cx - 1, cy + 1, 2, 1, NORTH, id) {
                        self.place(Chair, cx - 1, cy + 2, 1, 1, SOUTH, id);
                        self.keep_free(cx - 1, cy + 1, cx, cy + 2);
                    }
                    self.along_walls(Bookshelf, id, 2, 1);
                    self.along_walls(Cabinet, id, 1, 2);
                    self.in_corners(Plant, id, 1);
                }
                RoomKind::Manager => {
                    let (cx, cy) = r.mid();
                    self.place(Rug, cx - 2, cy - 1, 4, 3, NORTH, id);
                    if self.place(BigDesk, cx - 1, cy + 1, 3, 1, NORTH, id) {
                        self.place(Chair, cx, cy + 2, 1, 1, SOUTH, id);
                        self.keep_free(cx - 1, cy + 1, cx + 1, cy + 2);
                    }
                    self.along_walls(Bookshelf, id, 2, 2);
                    self.along_walls(Sofa, id, 2, 1);
                    self.along_walls(Safe, id, 1, 1);
                    self.in_corners(BigPlant, id, 1);
                    self.in_corners(Plant, id, 2);
                }
                RoomKind::Break => {
                    let (cx, cy) = r.mid();
                    if self.place(Table, cx - 1, cy - 1, 2, 2, NORTH, id) {
                        for (x, y, f) in [
                            (cx - 2, cy - 1, EAST),
                            (cx - 2, cy, EAST),
                            (cx + 1, cy - 1, WEST),
                            (cx + 1, cy, WEST),
                        ] {
                            self.place(Chair, x, y, 1, 1, f, id);
                        }
                        self.keep_free(cx - 2, cy - 1, cx + 1, cy);
                    }
                    self.along_walls(SinkCounter, id, 2, 1);
                    self.along_walls(Fridge, id, 1, 1);
                    self.in_corners(BigPlant, id, 1);
                    self.along_walls(Vending, id, 1, 1);
                    self.along_walls(Cooler, id, 1, 1);
                }
                RoomKind::Washroom => {
                    self.along_walls(Toilet, id, 1, 3);
                    self.along_walls(SinkCounter, id, 2, 1);
                    self.along_walls(Bin, id, 1, 1);
                }
                RoomKind::Server => {
                    for x in (r.x0 + 1..r.x1).step_by(2) {
                        for y in (r.y0 + 1..r.y1 - 1).step_by(3) {
                            self.place(Rack, x, y, 1, 2, EAST, id);
                        }
                    }
                }
                RoomKind::Security => {
                    self.along_walls(MonitorDesk, id, 3, 1);
                    self.along_walls(Lockers, id, 2, 2);
                    self.along_walls(Cabinet, id, 1, 1);
                }
                RoomKind::Archive => {
                    self.along_walls(Bookshelf, id, 2, 8);
                    self.along_walls(Cabinet, id, 1, 6);
                }
                RoomKind::Vault => {
                    self.along_walls(Lockers, id, 2, (r.w() + r.h()) as usize);
                    for y in (r.y0 + 2..=r.y1 - 2).step_by(2) {
                        for x in (r.x0 + 2..=r.x1 - 2).step_by(2) {
                            let kind = match self.rng.range(0, 4) {
                                0 => Gold,
                                1 => Safe,
                                _ => Pallet,
                            };
                            self.place(kind, x, y, 1, 1, NORTH, id);
                        }
                    }
                }
            }
        }
    }

    fn furnish_lobby(&mut self, id: u16) {
        let r = self.map.rooms[id as usize];
        let cr = r.y1 - 3; // the counter's row
        let rows = if cr - r.y0 >= 8 {
            vec![r.y0 + 2, cr - 3]
        } else {
            vec![r.y0 + 2]
        };
        for y in rows {
            for x in (r.x0 + 3..=r.x1 - 3).step_by(6) {
                self.place(PropKind::Pillar, x, y, 1, 1, NORTH, id);
            }
        }
        let by0 = self.map.building.1;
        let mid = (r.x0..=r.x1)
            .find(|&x| self.map.at(x, by0).deco & deco::DOOR != 0)
            .unwrap_or(r.x0)
            + 1;
        for y in r.y0..cr {
            for x in mid - 1..=mid + 1 {
                self.map.at_mut(x, y).deco |= deco::RUG;
            }
        }
        let qx = if mid - r.x0 > r.x1 - mid {
            mid - 6
        } else {
            mid + 3
        };
        for k in 0..4 {
            for y in [cr - 1, cr - 2] {
                self.place(PropKind::Stanchion, qx + k, y, 1, 1, EAST, id);
            }
        }
        self.along_walls(PropKind::Atm, id, 1, 2);
        self.along_walls(PropKind::Bench, id, 2, 2);
        self.in_corners(PropKind::BigPlant, id, 2);
        self.in_corners(PropKind::Plant, id, 2);
        self.along_walls(PropKind::Thicket, id, 2, 2);
        let mut x = r.x0 + 1;
        while x < r.x1 {
            let kind = if self.rng.chance(0.15) {
                PropKind::Safe
            } else if self.rng.chance(0.5) {
                PropKind::Cabinet
            } else {
                PropKind::Copier
            };
            self.place(kind, x, r.y1, 1, 1, SOUTH, id);
            x += self.rng.range(2, 5);
        }
    }

    fn street(&mut self, road: i32, entrance: i32) {
        let (bx0, by0, bx1, by1) = self.map.building;
        let (w, h) = (self.map.w, self.map.h);
        for x in (3..w - 2).step_by(10) {
            if self.place(PropKind::StreetLamp, x, road, 1, 1, NORTH, NO_ROOM) {
                self.light(x as f32 + 0.5, road as f32 + 0.6, 8.5, SODIUM, 1.0);
            }
        }
        let y = by1 + 3;
        for x in (bx0 + 2..bx1).step_by(13) {
            if self.place(PropKind::StreetLamp, x, y, 1, 1, NORTH, NO_ROOM) {
                self.light(x as f32 + 0.5, y as f32 + 0.5, 8.0, SODIUM, 0.85);
            }
        }
        for y in by0..=by1 {
            for (x, out) in [(bx0, -0.4), (bx1, 1.4)] {
                if self.map.at(x, y).deco & deco::DOOR != 0 {
                    self.light(x as f32 + out, y as f32 + 0.5, 5.0, WARM, 0.8);
                }
            }
        }
        self.light(entrance as f32 + 0.5, by0 as f32 - 0.6, 6.0, WARM, 0.9);
        for (gx0, gx1) in [(1, bx0 - 4), (bx1 + 4, w - 2)] {
            let mut y = by0 + 1;
            while y < h - 2 {
                let x = self.rng.range(gx0, gx1 + 1);
                if self.map.at(x, y).floor == Floor::Grass {
                    let kind = if self.rng.chance(0.65) {
                        PropKind::Tree
                    } else {
                        PropKind::Bush
                    };
                    self.place(kind, x, y, 1, 1, NORTH, NO_ROOM);
                }
                y += self.rng.range(3, 6);
            }
        }
        for _ in 0..6 {
            let x = self.rng.range(2, w - 3);
            if (x - entrance).abs() < 4 {
                continue;
            }
            let kind = self
                .rng
                .pick(&[PropKind::Bench, PropKind::Bin, PropKind::Hydrant]);
            let len = if kind == PropKind::Bench { 2 } else { 1 };
            self.place(kind, x, road + 2, len, 1, SOUTH, NO_ROOM);
        }
        let vx = entrance + if self.rng.chance(0.5) { 3 } else { -8 };
        self.place(PropKind::Van, vx, road - 2, 5, 2, EAST, NO_ROOM);
        let p0 = by1 + 3;
        for (r0, r1) in bay_rows(p0, h) {
            for x in (bx0 - 2..=bx1 + 2 - BAY_WIDE).step_by(BAY_WIDE as usize) {
                if self.rng.chance(0.45) && r1 - r0 >= BAY_DEEP - 1 {
                    let face = if r0 == p0 + 1 { NORTH } else { SOUTH };
                    self.place(PropKind::Car, x, r0, BAY_WIDE, BAY_DEEP, face, NO_ROOM);
                }
            }
        }
        self.map.spawn = tile_centre(entrance, road + 1);
    }
}

const BAY_WIDE: i32 = 2;
const BAY_DEEP: i32 = 4;

/// The car park's two rows of bays, first and last tile.
fn bay_rows(p0: i32, h: i32) -> [(i32, i32); 2] {
    [(p0 + 1, p0 + BAY_DEEP), (h - 1 - BAY_DEEP, h - 2)]
}

fn body_in(block: Aabb, face: Face, s: &Spec) -> Aabb {
    // Worked out facing north or south; east and west swap the axes.
    let swap = |v: Vec2| {
        if face == NORTH || face == SOUTH {
            v
        } else {
            vec2(v.y, v.x)
        }
    };
    let (lo, hi) = (swap(block.min), swap(block.max));
    let d = s.depth.min(hi.y - lo.y);
    let (y0, y1) = if !s.backed {
        let m = (lo.y + hi.y) * 0.5;
        (m - d * 0.5, m + d * 0.5)
    } else if face == NORTH || face == EAST {
        (lo.y, lo.y + d)
    } else {
        (hi.y - d, hi.y)
    };
    Aabb::new(
        swap(vec2(lo.x + s.inset, y0)),
        swap(vec2(hi.x - s.inset, y1)),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::movement::BODY_MARGIN;

    #[test]
    fn every_room_is_reachable_on_many_seeds() {
        for seed in 0..60 {
            let map = generate(seed);
            let counter = map.props.iter().any(|p| p.kind == PropKind::Counter);
            assert!(
                counter,
                "seed {seed}: the counter was taken out to free a door"
            );
            let g = Gen {
                used: used_tiles(&map),
                clear: Vec::new(),
                rng: Rng::new(0),
                map,
            };
            let reached = g.flood(true);
            for (i, r) in g.map.rooms.iter().enumerate() {
                assert!(
                    g.map.touched(r, &reached),
                    "seed {seed}: room {i} ({:?}) shut off",
                    r.kind
                );
            }
            assert!(
                g.map.rooms.iter().any(|r| r.kind == RoomKind::Vault),
                "seed {seed}: no vault"
            );
        }
    }

    #[test]
    fn the_spawn_is_clear_and_the_same_seed_is_the_same_bank() {
        let (a, b) = (generate(7), generate(7));
        assert_eq!((a.w, a.props.len(), a.spawn), (b.w, b.props.len(), b.spawn));
        for s in a.solids() {
            assert!(!s.expand(BODY_MARGIN - 1.0).contains(a.spawn), "{s:?}");
        }
    }

    #[test]
    fn every_wall_piece_can_be_walked_up_to() {
        for seed in 0..20 {
            let map = generate(seed);
            let free = map.free();
            for p in map.props.iter().filter(|p| p.solid && spec(p.kind).backed) {
                let (x, y) = tile_of(p.block.min);
                let (w, h) = (
                    (p.block.width() / TILE) as i32,
                    (p.block.height() / TILE) as i32,
                );
                for (fx, fy) in front_of(x, y, w, h, p.face) {
                    assert!(
                        free[map.idx(fx, fy)],
                        "seed {seed}: {:?} at {x},{y} blocked",
                        p.kind
                    );
                }
            }
        }
    }

    #[test]
    fn every_opaque_prop_is_solid() {
        for seed in 0..20 {
            for p in generate(seed).props {
                assert!(p.solid || !p.opaque, "{:?}", p.kind);
            }
        }
    }

    #[test]
    fn every_chair_faces_its_desk_and_the_desk_faces_back() {
        use PropKind::*;
        for seed in 0..20 {
            let map = generate(seed);
            for c in map.props.iter().filter(|p| p.kind == Chair) {
                let covered = map
                    .props
                    .iter()
                    .any(|p| p.solid && p.block.contains(c.block.centre()));
                assert!(!covered, "seed {seed}: something stands on a chair");
                let front = c.block.centre() + face_dir(c.face) * TILE;
                let desk = map
                    .props
                    .iter()
                    .find(|p| p.solid && p.block.contains(front));
                let desk = desk.unwrap_or_else(|| panic!("seed {seed}: a chair faces nothing"));
                assert!(
                    matches!(desk.kind, Desk | BigDesk | Counter | Table),
                    "{:?}",
                    desk.kind
                );
                // A counter's front is the customers' side; the teller sits behind it.
                if matches!(desk.kind, Desk | BigDesk) {
                    assert_eq!(
                        face_dir(desk.face),
                        -face_dir(c.face),
                        "seed {seed}: {:?}",
                        desk.kind
                    );
                }
            }
        }
    }
}

#[cfg(test)]
pub fn ascii(map: &Map) -> String {
    let used = used_tiles(map);
    let mut s = String::new();
    for y in (0..map.h).rev() {
        for x in 0..map.w {
            let t = map.at(x, y);
            s.push(match t.wall {
                Wall::Brick => '#',
                Wall::Drywall => '=',
                Wall::Vault => 'V',
                Wall::Glass => ':',
                Wall::None if t.deco & deco::DOOR != 0 => 'D',
                Wall::None if used[map.idx(x, y)] => 'o',
                Wall::None if t.room != NO_ROOM => (b'a' + (t.room % 26) as u8) as char,
                _ => '.',
            });
        }
        s.push('\n');
    }
    s
}

#[cfg(test)]
#[test]
#[ignore]
fn print_map() {
    let seed = std::env::var("SEED")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(6);
    let m = generate(seed);
    println!("{}", ascii(&m));
    for (i, r) in m.rooms.iter().enumerate() {
        let letter = (b'a' + (i % 26) as u8) as char;
        println!("{letter} {:?} {:?}", r.kind, (r.x0, r.y0, r.x1, r.y1));
    }
}
