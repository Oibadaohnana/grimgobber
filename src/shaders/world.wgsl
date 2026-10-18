// The ground (mode 0), every mesh over it (mode 1) and ghosts (mode 2), lit per pixel by lamps
// and the flashlight, each shadowed by rays traced through the tile grid. The lamps' light is
// traced only into the light maps (mode 3 the ground's, 4 things', render.rs) and read from them.
// Sight is a range, not a ray: no rays are traced from the eye.

#import bevy_sprite::mesh2d_vertex_output::VertexOutput

struct Params {
    eye: vec4<f32>, // the robber xy, the way he faces (cos, sin)
    map: vec4<f32>, // the map's size in tiles xy, a tile in world units z, torch count w
    misc: vec4<f32>, // mode, lamp count, flashlight on (1/0), crouched (1/0)
    guard: vec4<f32>, // the guard xy, the way he faces (cos, sin); zero for none
    torches: array<vec4<f32>, 6>, // every flashlight on, as `eye`
    lights: array<vec4<f32>, 256>, // two a lamp: (x, y, reach, power), (r, g, b, 0)
};

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> P: Params;
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var tiles: texture_2d<f32>; // floor, wall, tint, deco
@group(#{MATERIAL_BIND_GROUP}) @binding(2) var occ: texture_2d<f32>; // the opaque box, 0..1 of the tile
// The opaque box's object (0: a wall), indoor, the half-height box's object, in a lit room.
@group(#{MATERIAL_BIND_GROUP}) @binding(3) var extra: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(4) var low_boxes: texture_2d<f32>; // the half-height box
@group(#{MATERIAL_BIND_GROUP}) @binding(5) var doors: texture_2d<f32>; // how shut, lying along x, glass
// A smash's reach: sixteenths of a tile of it left, how strongly shown (hearing.rs).
@group(#{MATERIAL_BIND_GROUP}) @binding(6) var heard: texture_2d<f32>;
// The lamps' light, ground's or things' by the mode; a light map is not bound while drawn into.
@group(#{MATERIAL_BIND_GROUP}) @binding(7) var baked: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(8) var baked_sampler: sampler;

const FLOOR_GRASS: u32 = 0u;
const FLOOR_ASPHALT: u32 = 1u;
const FLOOR_SIDEWALK: u32 = 2u;
const FLOOR_PARKING: u32 = 3u;
const FLOOR_MARBLE: u32 = 4u;
const FLOOR_CARPET: u32 = 5u;
const FLOOR_WOOD: u32 = 6u;
const FLOOR_WASHROOM: u32 = 7u;
const FLOOR_CONCRETE: u32 = 8u;
const FLOOR_VAULT: u32 = 9u;
const FLOOR_LINO: u32 = 10u;
const FLOOR_RAISED: u32 = 11u;

const WALL_NONE: u32 = 0u;
const WALL_BRICK: u32 = 1u;
const WALL_DRYWALL: u32 = 2u;
const WALL_VAULT: u32 = 3u;
const WALL_GLASS: u32 = 4u;

const DECO_DOOR: u32 = 1u;
const DECO_LANE: u32 = 2u;
const DECO_BAY: u32 = 4u;
const DECO_ZEBRA: u32 = 8u;
const DECO_VAULT_DOOR: u32 = 16u;
const DECO_RUG: u32 = 32u;
const DECO_SHARDS: u32 = 64u;

const EYE_SPREAD: f32 = 7.0;
const LENS_SPREAD: f32 = 5.0;
const LENS_AHEAD: f32 = 16.0;
const CONE_IN: f32 = 0.30;
const CONE_OUT: f32 = 0.52;
const TORCH_COLOUR: vec3<f32> = vec3<f32>(1.0, 0.96, 0.86);
const TORCH_POWER: f32 = 1.5;
// In tiles: the flashlight's reach, and sight in the dark and at most.
const TORCH_REACH: f32 = 15.0;
const DARK: f32 = 3.75;
const VIEW: f32 = 6.0;
// The guard's sight in tiles where lit and in the dark, and half his cone.
const GUARD_VIEW: f32 = 10.0;
const GUARD_DARK: f32 = 5.0;
const GUARD_CONE: f32 = 0.7853982;
const WATCHED: vec3<f32> = vec3<f32>(1.0, 0.42, 0.38);
const HEARD: vec3<f32> = vec3<f32>(0.70, 0.85, 1.0);
// The run through an opaque box, in world units, that stops a ray fully.
const STOP_SOFT: f32 = 3.0;

fn in_map(c: vec2<i32>) -> bool {
    return c.x >= 0 && c.y >= 0 && f32(c.x) < P.map.x && f32(c.y) < P.map.y;
}

fn tile_at(c: vec2<i32>) -> vec4<u32> {
    if !in_map(c) {
        return vec4<u32>(FLOOR_GRASS, WALL_NONE, 0u, 0u);
    }
    return vec4<u32>(round(textureLoad(tiles, c, 0) * 255.0));
}

fn object_at(c: vec2<i32>) -> u32 {
    if !in_map(c) {
        return 0u;
    }
    return u32(round(textureLoad(extra, c, 0).r * 255.0));
}

fn low_at(c: vec2<i32>) -> u32 {
    if !in_map(c) {
        return 0u;
    }
    return u32(round(textureLoad(extra, c, 0).b * 255.0));
}

fn is_wall(c: vec2<i32>) -> bool {
    return tile_at(c).y != WALL_NONE;
}

struct Spot {
    cell: vec2<i32>,
    in_box: bool,
    object: u32,
    // The half-height object the point is on, 0 for none.
    low: u32,
    // A wall's top: only walls stand between it and anything, furniture is lower.
    top: bool,
    // A door leaf: its own doorway's slab is not in its way.
    door: bool,
};

fn spot_of(w: vec2<f32>) -> Spot {
    let cell = vec2<i32>(floor(w / P.map.z));
    let b = select(vec4<f32>(0.0), textureLoad(occ, cell, 0), in_map(cell));
    let f = w / P.map.z - vec2<f32>(cell);
    let in_box = b.z > b.x && f.x >= b.x && f.x <= b.z && f.y >= b.y && f.y <= b.w;
    let h = select(vec4<f32>(0.0), textureLoad(low_boxes, cell, 0), in_map(cell));
    let on_low = h.z > h.x && f.x >= h.x && f.x <= h.z && f.y >= h.y && f.y <= h.w;
    return Spot(cell, in_box, select(0u, object_at(cell), in_box), select(0u, low_at(cell), on_low), false, false);
}

// How far a → b runs inside opaque boxes (x) and through glass (y), in world
// units. It skips the box the target pixel is on and every box of its object
// (a cabinet's whole top is seen once its front is), and likewise the box
// and object its source stands in (a lamp over furniture).
// Where a ray enters and leaves a box in its cell, as shares of the ray.
fn span(b: vec4<f32>, cell: vec2<i32>, p: vec2<f32>, inv: vec2<f32>) -> vec2<f32> {
    let s0 = (vec2<f32>(cell) + b.xy - p) * inv;
    let s1 = (vec2<f32>(cell) + b.zw - p) * inv;
    return vec2<f32>(max(min(s0.x, s1.x), min(s0.y, s1.y)), min(max(s0.x, s1.x), max(s0.y, s1.y)));
}

// With `low`, z is how far b lies past the first half-height object crossed
// (-1 for none), not counting one that either end is on.
fn trace(a: vec2<f32>, b: vec2<f32>, src: Spot, dst: Spot, low: bool) -> vec3<f32> {
    let tile = P.map.z;
    let p = a / tile;
    let q = b / tile;
    let d = q - p;
    let len = length(d);
    if len < 1e-5 {
        return vec3<f32>(0.0, 0.0, -1.0);
    }
    var cell = vec2<i32>(floor(p));
    let dir = vec2<i32>(sign(d));
    let big = 1e30;
    let inv = vec2<f32>(
        select(big, 1.0 / d.x, abs(d.x) > 1e-9),
        select(big, 1.0 / d.y, abs(d.y) > 1e-9),
    );
    var tmax = vec2<f32>(
        select(big, (f32(cell.x) + select(0.0, 1.0, d.x > 0.0) - p.x) * inv.x, abs(d.x) > 1e-9),
        select(big, (f32(cell.y) + select(0.0, 1.0, d.y > 0.0) - p.y) * inv.y, abs(d.y) > 1e-9),
    );
    let delta = abs(inv);
    var t = 0.0;
    var opaque = 0.0;
    var glass = 0.0;
    var cover_id = 0u;
    var cover_exit = -1.0;
    for (var i = 0; i < 128; i++) {
        let t1 = min(min(tmax.x, tmax.y), 1.0);
        if in_map(cell) {
            let o = textureLoad(occ, cell, 0);
            if o.z > o.x {
                let obj = object_at(cell);
                let skip = (dst.top && obj != 0u)
                    || (all(cell == dst.cell) && dst.in_box)
                    || (dst.object != 0u && obj == dst.object)
                    || (all(cell == src.cell) && src.in_box)
                    || (src.object != 0u && obj == src.object);
                if !skip {
                    let r = span(o, cell, p, inv);
                    opaque += max(0.0, min(r.y, t1) - max(r.x, t));
                }
            }
            let k = tile_at(cell);
            if k.y == WALL_GLASS && !all(cell == dst.cell) {
                glass += t1 - t;
            }
            // A door: a thin slab across the doorway, as solid as it is shut.
            if (k.w & DECO_DOOR) != 0u && !(dst.door && all(cell == dst.cell)) {
                let shut = textureLoad(doors, cell, 0);
                if shut.r > 0.0 {
                    let slab = select(vec4<f32>(0.45, 0.0, 0.55, 1.0), vec4<f32>(0.0, 0.45, 1.0, 0.55), shut.g > 0.5);
                    let r = span(slab, cell, p, inv);
                    let through = max(0.0, min(r.y, t1) - max(r.x, t)) * shut.r;
                    if shut.b > 0.5 {
                        glass += through;
                    } else {
                        opaque += through;
                    }
                }
            }
            if low {
                let h = textureLoad(low_boxes, cell, 0);
                let id = low_at(cell);
                if h.z > h.x && id != dst.low && id != src.low && (cover_exit < 0.0 || id == cover_id) {
                    let r = span(h, cell, p, inv);
                    if min(r.y, t1) > max(r.x, t) {
                        cover_id = id;
                        cover_exit = max(cover_exit, min(r.y, 1.0));
                    }
                }
            }
        }
        // Fully blocked: the rest cannot change `clear_share`. Low cover needs the whole ray.
        if t1 >= 1.0 || (!low && opaque * len * tile >= STOP_SOFT) {
            break;
        }
        if tmax.x < tmax.y {
            cell.x += dir.x;
            t = tmax.x;
            tmax.x += delta.x;
        } else {
            cell.y += dir.y;
            t = tmax.y;
            tmax.y += delta.y;
        }
    }
    let past = select(-1.0, (1.0 - cover_exit) * len * tile, cover_exit >= 0.0);
    return vec3<f32>(vec2<f32>(opaque, glass) * len * tile, past);
}

fn clear_share(run: f32) -> f32 {
    return 1.0 - smoothstep(0.0, STOP_SOFT, run);
}

fn ambient(dst: Spot) -> vec3<f32> {
    let indoor = in_map(dst.cell) && textureLoad(extra, dst.cell, 0).g > 0.5;
    return select(vec3<f32>(0.050, 0.062, 0.105), vec3<f32>(0.018, 0.020, 0.030), indoor);
}

fn lamps_at(w: vec2<f32>, dst: Spot) -> vec3<f32> {
    var light = vec3<f32>(0.0);
    for (var i = 0; i < i32(P.misc.y); i++) {
        let lp = P.lights[2 * i];
        let d = distance(lp.xy, w);
        if d >= lp.z {
            continue;
        }
        let x = d / lp.z;
        let fall = (1.0 - x * x) * (1.0 - x * x) / (1.0 + 6.0 * x * x);
        let run = trace(lp.xy, w, spot_of(lp.xy), dst, false);
        light += P.lights[2 * i + 1].rgb * (lp.w * fall * clear_share(run.x));
    }
    return light;
}

fn baked_at(at: vec2<f32>) -> vec3<f32> {
    let size = P.map.xy * P.map.z;
    let uv = vec2<f32>(at.x / size.x, 1.0 - at.y / size.y);
    return textureSampleLevel(baked, baked_sampler, uv, 0.0).rgb;
}

// How far inside a flashlight's cone a point is, 0 to 1.
fn cone_of(t: vec4<f32>, w: vec2<f32>) -> f32 {
    let off = w - (t.xy + t.zw * LENS_AHEAD);
    return smoothstep(cos(CONE_OUT), cos(CONE_IN), dot(off, t.zw) / max(length(off), 1e-3));
}

// The robber's own, 0 with it off.
fn beam(w: vec2<f32>) -> f32 {
    return select(0.0, cone_of(P.eye, w), P.misc.z > 0.5);
}

fn torch_at(w: vec2<f32>, dst: Spot) -> vec3<f32> {
    var light = vec3<f32>(0.0);
    for (var i = 0; i < i32(P.map.w); i++) {
        light += one_torch(P.torches[i], w, dst);
    }
    return light;
}

fn one_torch(t: vec4<f32>, w: vec2<f32>, dst: Spot) -> vec3<f32> {
    let ahead = t.zw;
    let lens = t.xy + ahead * LENS_AHEAD;
    let off = w - lens;
    let d = length(off);
    let reach = TORCH_REACH * P.map.z;
    if d >= reach {
        return vec3<f32>(0.0);
    }
    let x = d / reach;
    let fall = (1.0 - x * x) * (1.0 - x * x) / (1.0 + 4.0 * x * x);
    let cone = cone_of(t, w);
    let spill = 0.22 * (1.0 - smoothstep(0.4 * P.map.z, 2.2 * P.map.z, distance(w, t.xy)));
    let power = TORCH_POWER * cone * fall + spill;
    if power < 0.002 {
        return vec3<f32>(0.0);
    }
    let side = vec2<f32>(-ahead.y, ahead.x) * LENS_SPREAD;
    let src = spot_of(lens);
    var clear = 0.0;
    for (var k = -1; k <= 1; k++) {
        let run = trace(lens + side * f32(k), w, src, dst, false);
        clear += clear_share(run.x) * select(1.0, 0.85, run.y > 1.0);
    }
    return TORCH_COLOUR * (power * clear / 3.0);
}

// A wall's top is lit as the floor just past its nearest open face: a ray to
// the top itself would graze along the wall's other tiles and leave saw teeth
// of shadow on it.
fn wall_face(w: vec2<f32>, cell: vec2<i32>) -> vec2<f32> {
    let tile = P.map.z;
    let f = w / tile - vec2<f32>(cell);
    let lo = vec2<f32>(cell) * tile;
    let hi = lo + vec2<f32>(tile);
    var best = w;
    var near = 1e9;
    if !is_wall(cell - vec2<i32>(1, 0)) && f.x < near {
        near = f.x;
        best = vec2<f32>(lo.x - 1.0, w.y);
    }
    if !is_wall(cell + vec2<i32>(1, 0)) && 1.0 - f.x < near {
        near = 1.0 - f.x;
        best = vec2<f32>(hi.x + 1.0, w.y);
    }
    if !is_wall(cell - vec2<i32>(0, 1)) && f.y < near {
        near = f.y;
        best = vec2<f32>(w.x, lo.y - 1.0);
    }
    if !is_wall(cell + vec2<i32>(0, 1)) && 1.0 - f.y < near {
        near = 1.0 - f.y;
        best = vec2<f32>(w.x, hi.y + 1.0);
    }
    // Open diagonals too, by chessboard distance, so where walls meet each
    // side's half-tile band carries on square instead of one side taking the
    // whole tile.
    for (var dy = -1; dy <= 1; dy += 2) {
        for (var dx = -1; dx <= 1; dx += 2) {
            let corner = vec2<f32>(select(lo.x, hi.x, dx > 0), select(lo.y, hi.y, dy > 0));
            let d = max(abs(w.x - corner.x), abs(w.y - corner.y)) / tile;
            if !is_wall(cell + vec2<i32>(dx, dy)) && d < near {
                near = d;
                best = corner + vec2<f32>(f32(dx), f32(dy));
            }
        }
    }
    return best;
}

fn luma(c: vec3<f32>) -> f32 {
    return dot(c, vec3<f32>(0.2126, 0.7152, 0.0722));
}

// Within reach (by the room, not the spot: per spot, far lamp pools show as
// islands), then three rays from the eyes, a little apart for a soft edge.
fn seen(w: vec2<f32>, dst: Spot) -> f32 {
    let eye = P.eye.xy;
    let off = w - eye;
    let dist = length(off);
    if dist < 20.0 {
        return 1.0;
    }
    let tile = P.map.z;
    let lit = in_map(dst.cell) && textureLoad(extra, dst.cell, 0).a > 0.5;
    // In the beam, as far as in a lit room.
    let reach = mix(select(DARK, VIEW, lit), VIEW, beam(w)) * tile;
    let ranged = 1.0 - smoothstep(reach - 0.8 * tile, reach, dist);
    if ranged <= 0.0 {
        return 0.0;
    }
    let side = vec2<f32>(-off.y, off.x) / dist * EYE_SPREAD;
    let src = spot_of(eye);
    // Crouched, half-height things hide what is more than 2 tiles past them;
    // wall tops and tall things stand above them.
    let crouched = P.misc.w > 0.5 && !dst.top && dst.object == 0u;
    var clear = 0.0;
    for (var k = -1; k <= 1; k++) {
        let run = trace(eye + side * f32(k), w, src, dst, crouched);
        let over = select(1.0, 1.0 - smoothstep(1.5 * tile, 2.0 * tile, run.z), run.z >= 0.0);
        clear += clear_share(run.x) * select(1.0, 0.85, run.y > 1.0) * over;
    }
    return clear / 3.0 * ranged;
}

// The colour, and how clearly it is seen in alpha.
// How clearly the guard sees a point, as guard.rs reckons it: in his cone,
// within reach, by one ray. Where the flashlight shines (`torch`, its light)
// he sees as far as in a lit room; that takes in the robber himself.
fn watched(w: vec2<f32>, dst: Spot, torch: vec3<f32>) -> f32 {
    let ahead = P.guard.zw;
    if dot(ahead, ahead) < 0.5 {
        return 0.0;
    }
    let off = w - P.guard.xy;
    let dist = length(off);
    let tile = P.map.z;
    let cone = smoothstep(cos(GUARD_CONE + 0.05), cos(GUARD_CONE - 0.05), dot(off, ahead) / max(dist, 1e-3));
    let lit = in_map(dst.cell) && textureLoad(extra, dst.cell, 0).a > 0.5;
    let reach = mix(select(GUARD_DARK, GUARD_VIEW, lit), GUARD_VIEW, smoothstep(0.02, 0.06, luma(torch))) * tile;
    let near = cone * (1.0 - smoothstep(reach - 0.8 * tile, reach, dist));
    if near <= 0.0 {
        return 0.0;
    }
    var clear = clear_share(trace(P.guard.xy, w, spot_of(P.guard.xy), dst, false).x);
    // The robber crouched, a spot is hidden from him as guard.rs hides the
    // robber: traced from the spot, a half-height thing within 2 tiles.
    if P.misc.w > 0.5 && !dst.top && dst.object == 0u {
        let past = trace(w, P.guard.xy, dst, spot_of(P.guard.xy), true).z;
        clear *= select(1.0, 1.0 - smoothstep(1.5 * tile, 2.0 * tile, past), past >= 0.0);
    }
    return near * clear;
}

fn heard_texel(c: vec2<i32>) -> vec2<f32> {
    return select(vec2<f32>(0.0), textureLoad(heard, c, 0).rg, in_map(c));
}

// The noises' reach at a point, read between tile centres so its edge is smooth.
fn heard_at(w: vec2<f32>) -> vec2<f32> {
    let p = w / P.map.z - 0.5;
    let c = vec2<i32>(floor(p));
    let f = fract(p);
    let lo = mix(heard_texel(c), heard_texel(c + vec2<i32>(1, 0)), f.x);
    let hi = mix(heard_texel(c + vec2<i32>(0, 1)), heard_texel(c + vec2<i32>(1, 1)), f.x);
    return mix(lo, hi, f.y);
}

// What a pixel at `at` stands for: a wall's top is moved to its face.
struct Point {
    w: vec2<f32>,
    dst: Spot,
};

fn point_of(at: vec2<f32>, ground: bool) -> Point {
    var w = at;
    var dst = spot_of(w);
    if dst.in_box && dst.object == 0u {
        w = wall_face(w, dst.cell);
        dst = spot_of(w);
        dst.top = true;
    } else if ground {
        // The floor under a thing is not the thing.
        dst.in_box = false;
        dst.object = 0u;
        dst.low = 0u;
    }
    dst.door = !ground;
    return Point(w, dst);
}

fn shade(albedo: vec3<f32>, glow: bool, ground: bool, at: vec2<f32>) -> vec4<f32> {
    let p = point_of(at, ground);
    let w = p.w;
    let dst = p.dst;
    let s = seen(w, dst);
    if glow {
        return vec4<f32>(albedo * mix(0.35, 1.0, s), s);
    }
    let torch = torch_at(w, dst);
    let light = ambient(dst) + baked_at(at) + torch;
    let lit = albedo * (light + vec3<f32>(0.075, 0.08, 0.11));
    let dim = albedo * light;
    let fog = mix(dim, vec3<f32>(luma(dim)), 0.85) * 0.2 + albedo * vec3<f32>(0.004, 0.006, 0.014);
    var c = mix(fog, lit, s);
    // His cone, a light red wash, only where the robber sees.
    if s > 0.0 {
        c = mix(c, srgb(WATCHED), 0.2 * s * watched(w, dst, torch));
    }
    // How far a smash carries: a faint ring where its reach runs out, in
    // the fog too (it is his own noise).
    let h = heard_at(w);
    let left = h.x * 255.0 / 16.0;
    let ring = smoothstep(0.0, 0.05, left) * (1.0 - smoothstep(0.08, 0.3, left));
    c = mix(c, srgb(HEARD), h.y * 0.22 * ring);
    return vec4<f32>(c, s);
}

fn hash_u(x: u32) -> u32 {
    var h = x * 747796405u + 2891336453u;
    h = ((h >> ((h >> 28u) + 4u)) ^ h) * 277803737u;
    return (h >> 22u) ^ h;
}

fn hash2(c: vec2<i32>) -> f32 {
    let h = hash_u(u32(c.x) * 1597334677u ^ hash_u(u32(c.y)));
    return f32(h & 0xffffffu) / 16777215.0;
}

fn noise(p: vec2<f32>) -> f32 {
    let i = vec2<i32>(floor(p));
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    let a = hash2(i);
    let b = hash2(i + vec2<i32>(1, 0));
    let c = hash2(i + vec2<i32>(0, 1));
    let d = hash2(i + vec2<i32>(1, 1));
    return mix(mix(a, b, u.x), mix(c, d, u.x), u.y);
}

fn fbm(p: vec2<f32>) -> f32 {
    return noise(p) * 0.55 + noise(p * 2.03 + 7.1) * 0.3 + noise(p * 4.1 + 3.3) * 0.15;
}

fn srgb(c: vec3<f32>) -> vec3<f32> {
    return pow(c, vec3<f32>(2.2));
}

fn grid_line(p: vec2<f32>, size: f32) -> f32 {
    let g = abs(fract(p / size + 0.5) - 0.5) * size;
    return min(g.x, g.y);
}

const CARPETS = array<vec3<f32>, 6>(
    vec3<f32>(0.24, 0.28, 0.36),
    vec3<f32>(0.22, 0.32, 0.26),
    vec3<f32>(0.38, 0.20, 0.22),
    vec3<f32>(0.42, 0.37, 0.30),
    vec3<f32>(0.28, 0.29, 0.31),
    vec3<f32>(0.18, 0.32, 0.34),
);

fn floor_colour(kind: u32, tint: u32, deco: u32, cell: vec2<i32>, f: vec2<f32>, w: vec2<f32>) -> vec3<f32> {
    let tile = P.map.z;
    let edge = min(min(f.x, 1.0 - f.x), min(f.y, 1.0 - f.y));
    var c = vec3<f32>(0.5);
    switch kind {
        case FLOOR_GRASS: {
            let n = fbm(w / 40.0);
            let blades = noise(w / 2.2);
            c = mix(vec3<f32>(0.13, 0.24, 0.10), vec3<f32>(0.24, 0.36, 0.15), n) * (0.85 + 0.3 * blades);
        }
        case FLOOR_ASPHALT, FLOOR_PARKING: {
            let base = select(0.16, 0.19, kind == FLOOR_PARKING);
            c = vec3<f32>(base, base, base * 1.04) * (0.8 + 0.35 * fbm(w / 6.0)) + vec3<f32>(0.06) * step(0.93, noise(w / 1.3));
            if (deco & DECO_LANE) != 0u && f.y < 0.07 && fract(w.x / (tile * 3.0)) < 0.55 {
                c = vec3<f32>(0.80, 0.68, 0.26);
            }
            if (deco & DECO_ZEBRA) != 0u && fract(w.y / 26.0) < 0.5 {
                let wear = 0.75 + 0.25 * fbm(w / 9.0);
                c = mix(c, vec3<f32>(0.74, 0.74, 0.70), wear);
            }
            if (deco & DECO_BAY) != 0u && f.x < 0.05 {
                c = vec3<f32>(0.72, 0.72, 0.68);
            }
            let up = tile_at(cell + vec2<i32>(0, 1));
            if kind == FLOOR_ASPHALT && up.x == FLOOR_SIDEWALK && f.y > 0.88 {
                c = vec3<f32>(0.55, 0.55, 0.53) * (0.9 + 0.2 * noise(w / 3.0));
            }
        }
        case FLOOR_SIDEWALK: {
            c = vec3<f32>(0.46, 0.46, 0.44) * (0.9 + 0.15 * hash2(cell)) * (0.85 + 0.25 * fbm(w / 9.0));
            if edge < 0.025 {
                c *= 0.6;
            }
        }
        case FLOOR_MARBLE: {
            let check = (cell.x + cell.y) % 2 == 0;
            let vein = abs(sin(w.x * 0.015 + w.y * 0.01 + fbm(w / 45.0) * 7.0));
            if check {
                c = vec3<f32>(0.80, 0.77, 0.71) * (0.95 + 0.08 * fbm(w / 20.0));
                c = mix(c * 0.84, c, smoothstep(0.0, 0.08, vein));
            } else {
                c = vec3<f32>(0.50, 0.50, 0.47) * (0.92 + 0.12 * fbm(w / 20.0));
                c = mix(vec3<f32>(0.64, 0.63, 0.60), c, smoothstep(0.0, 0.05, vein));
            }
            if edge < 0.012 {
                c *= 0.7;
            }
            if (deco & DECO_RUG) != 0u {
                let l = (tile_at(cell - vec2<i32>(1, 0)).w & DECO_RUG) != 0u;
                let r = (tile_at(cell + vec2<i32>(1, 0)).w & DECO_RUG) != 0u;
                c = vec3<f32>(0.42, 0.08, 0.10) * (0.85 + 0.2 * noise(w / 1.5));
                let pattern = abs(fract((w.y + abs(fract(w.x / 26.0) - 0.5) * 26.0) / 26.0) - 0.5);
                c = mix(c, vec3<f32>(0.52, 0.14, 0.12), step(pattern, 0.08));
                if (!l && f.x < 0.14) || (!r && f.x > 0.86) {
                    c = vec3<f32>(0.62, 0.48, 0.20);
                }
                if (!l && f.x < 0.06) || (!r && f.x > 0.94) {
                    c = vec3<f32>(0.30, 0.05, 0.06);
                }
            }
        }
        case FLOOR_CARPET: {
            var carpets = CARPETS;
            c = carpets[tint % 6u];
            let check = (cell.x + cell.y) % 2 == 0;
            let grain = select(sin(w.x * 1.1), sin(w.y * 1.1), check);
            c *= 0.88 + 0.16 * noise(w / 1.2) + 0.03 * grain;
            if edge < 0.01 {
                c *= 0.85;
            }
        }
        case FLOOR_WOOD: {
            let h = tile / 4.0;
            let row = i32(floor(w.y / h));
            let off = hash2(vec2<i32>(row, 7)) * tile * 2.0;
            let plank = i32(floor((w.x + off) / (tile * 2.0)));
            let tone = hash2(vec2<i32>(plank, row));
            c = vec3<f32>(0.46, 0.29, 0.16) * (0.8 + 0.35 * tone);
            c *= 0.9 + 0.12 * sin(w.y * 0.9 + noise(w / vec2<f32>(40.0, 3.0)) * 5.0);
            let along = fract((w.x + off) / (tile * 2.0));
            if fract(w.y / h) < 0.05 || along < 0.01 {
                c *= 0.55;
            }
        }
        case FLOOR_WASHROOM: {
            c = vec3<f32>(0.80, 0.82, 0.83) * (0.95 + 0.06 * hash2(vec2<i32>(floor(w / (tile / 4.0)))));
            if grid_line(w, tile / 4.0) < 0.8 {
                c = vec3<f32>(0.55, 0.56, 0.56);
            }
        }
        case FLOOR_CONCRETE: {
            c = vec3<f32>(0.40, 0.40, 0.39) * (0.8 + 0.3 * fbm(w / 14.0));
            if grid_line(w, tile * 3.0) < 0.8 {
                c *= 0.75;
            }
        }
        case FLOOR_VAULT: {
            c = vec3<f32>(0.36, 0.39, 0.42) * (0.92 + 0.1 * fbm(w / 10.0));
            let s = 9.0;
            let g = w / s;
            let k = vec2<i32>(floor(g));
            let fp = fract(g) - 0.5;
            let turn = select(1.0, -1.0, (k.x + k.y) % 2 == 0);
            let r = vec2<f32>(fp.x + fp.y * turn, fp.y - fp.x * turn) * 0.7071;
            let bar = length(vec2<f32>(r.x * 0.45, r.y * 2.2));
            c *= 1.0 + 0.25 * (1.0 - smoothstep(0.12, 0.2, bar));
            if edge < 0.02 {
                c *= 0.6;
            }
            let corner = length(abs(f - 0.5) - vec2<f32>(0.42));
            if corner < 0.035 {
                c = vec3<f32>(0.55, 0.58, 0.62);
            }
        }
        case FLOOR_LINO: {
            let q = vec2<i32>(floor(w / (tile * 0.5)));
            let check = (q.x + q.y) % 2 == 0;
            let other = select(vec3<f32>(0.22, 0.38, 0.40), vec3<f32>(0.45, 0.18, 0.16), tint % 2u == 0u);
            c = select(other, vec3<f32>(0.78, 0.76, 0.70), check) * (0.92 + 0.1 * noise(w / 5.0));
        }
        case FLOOR_RAISED: {
            c = vec3<f32>(0.56, 0.58, 0.60);
            let holes = grid_line(w + tile * 0.0625, tile / 8.0);
            if holes > 2.4 && edge > 0.08 {
                c *= 0.8;
            }
            if edge < 0.02 {
                c *= 0.55;
            }
        }
        default: {}
    }
    if (deco & DECO_DOOR) != 0u {
        let across_x = is_wall(cell - vec2<i32>(1, 0)) || is_wall(cell + vec2<i32>(1, 0));
        let m = select(f.x, f.y, across_x);
        if abs(m - 0.5) < 0.1 {
            c = vec3<f32>(0.45, 0.45, 0.46) * (0.9 + 0.2 * noise(w / 2.0));
            if (deco & DECO_VAULT_DOOR) != 0u {
                let stripe = fract((w.x + w.y) / 12.0);
                c = select(vec3<f32>(0.08), vec3<f32>(0.80, 0.62, 0.10), stripe < 0.5);
            }
        }
    }
    if (deco & DECO_SHARDS) != 0u {
        // A smashed window: the frame's posts at its ends, glass strewn about.
        let across_x = is_wall(cell - vec2<i32>(1, 0)) || is_wall(cell + vec2<i32>(1, 0));
        let along = select(f.y, f.x, across_x);
        let across = select(f.x, f.y, across_x);
        let glass = vec3<f32>(0.72, 0.86, 0.92);
        // Slivers, one in some 5-unit cells, each a thin diamond at its own angle.
        let g = w / 5.0;
        let k = vec2<i32>(floor(g));
        if hash2(k) > 0.6 && abs(across - 0.5) < 0.46 {
            let turn = hash2(k + vec2<i32>(5, 9)) * 3.14159;
            let q = fract(g) - 0.5 + (vec2<f32>(hash2(k + vec2<i32>(3, 1)), hash2(k + vec2<i32>(1, 11))) - 0.5) * 0.4;
            let r = vec2<f32>(q.x * cos(turn) + q.y * sin(turn), q.y * cos(turn) - q.x * sin(turn));
            let len = 0.15 + 0.25 * hash2(k + vec2<i32>(17, 2));
            if abs(r.y) < (len - abs(r.x)) * 0.45 {
                c = mix(c, glass, 0.85);
            }
        }
        // Jagged teeth of the pane still in the frame.
        let tooth = 0.06 + 0.16 * hash2(vec2<i32>(i32(floor(across * 9.0)), cell.x * 7 + cell.y));
        if along < tooth || along > 1.0 - tooth {
            c = mix(c, vec3<f32>(0.55, 0.74, 0.82), 0.5);
        }
        if along < 0.05 || along > 0.95 {
            c = vec3<f32>(0.26, 0.28, 0.30);
        }
    }
    return c;
}

fn wall_colour(kind: u32, cell: vec2<i32>, f: vec2<f32>, w: vec2<f32>, under: vec3<f32>) -> vec3<f32> {
    let tile = P.map.z;
    let wl = is_wall(cell - vec2<i32>(1, 0));
    let wr = is_wall(cell + vec2<i32>(1, 0));
    let wd = is_wall(cell - vec2<i32>(0, 1));
    let wu = is_wall(cell + vec2<i32>(0, 1));
    var e = 1.0;
    if !wl { e = min(e, f.x); }
    if !wr { e = min(e, 1.0 - f.x); }
    if !wd { e = min(e, f.y); }
    if !wu { e = min(e, 1.0 - f.y); }
    var c = vec3<f32>(0.5);
    switch kind {
        case WALL_BRICK: {
            let row = floor(w.y / 9.0);
            let bx = (w.x + row * 9.0) / 18.0;
            let brick = vec2<i32>(i32(floor(bx)), i32(row));
            c = vec3<f32>(0.36, 0.17, 0.12) * (0.8 + 0.35 * hash2(brick));
            if fract(w.y / 9.0) < 0.12 || fract(bx) < 0.06 {
                c = vec3<f32>(0.22, 0.20, 0.18);
            }
            if e < 0.18 {
                c = vec3<f32>(0.42, 0.40, 0.37) * (0.9 + 0.15 * noise(w / 3.0));
            }
        }
        case WALL_DRYWALL: {
            c = vec3<f32>(0.50, 0.48, 0.45) * (0.95 + 0.08 * noise(w / 6.0));
        }
        case WALL_VAULT: {
            c = vec3<f32>(0.32, 0.36, 0.40) * (0.9 + 0.12 * fbm(w / 12.0));
            let stripe = abs(fract((w.x + w.y) / 30.0) - 0.5);
            c *= 0.92 + 0.12 * step(stripe, 0.25);
            let rivet = length(fract(w / (tile / 3.0)) - 0.5);
            if rivet < 0.08 {
                c = vec3<f32>(0.58, 0.62, 0.66);
            }
        }
        case WALL_GLASS: {
            let horizontal = wl || wr;
            let m = select(f.x, f.y, horizontal);
            let along = select(f.y, f.x, horizontal);
            c = mix(under, vec3<f32>(0.55, 0.74, 0.82), 0.45);
            c += vec3<f32>(0.08) * step(0.85, fract((along + m) * 2.0));
            if abs(m - 0.5) > 0.36 {
                c = vec3<f32>(0.30, 0.32, 0.34);
            }
            if along < 0.04 || along > 0.96 {
                c = vec3<f32>(0.26, 0.28, 0.30);
            }
            return c;
        }
        default: {}
    }
    if e < 0.05 {
        c *= 0.35;
    } else if e < 0.14 {
        c *= 0.78;
    }
    return c;
}

@fragment
fn fragment(mesh: VertexOutput) -> @location(0) vec4<f32> {
    let w = mesh.world_position.xy;
    if P.misc.x > 2.5 {
        let p = point_of(w, P.misc.x < 3.5);
        return vec4<f32>(lamps_at(p.w, p.dst), 1.0);
    }
    if P.misc.x < 0.5 {
        let tile = P.map.z;
        let cell = vec2<i32>(floor(w / tile));
        let f = w / tile - vec2<f32>(cell);
        let t = tile_at(cell);
        var c = floor_colour(t.x, t.z, t.w, cell, f, w);
        if t.y != WALL_NONE {
            c = wall_colour(t.y, cell, f, w, c);
        }
        return vec4<f32>(shade(srgb(c), false, true, w).rgb, 1.0);
    }
#ifdef VERTEX_COLORS
    let colour = mesh.color;
#else
    let colour = vec4<f32>(1.0, 0.0, 1.0, 1.0);
#endif
    if P.misc.x > 1.5 {
        // Where a guard last saw the robber: pale and see-through, in any light.
        return vec4<f32>(srgb(vec3<f32>(0.72, 0.84, 1.0)) * (0.3 + luma(colour.rgb)), colour.a * 0.45);
    }
    let c = shade(colour.rgb, mesh.uv.x > 0.5, false, w);
    // A fading vertex (UV y = 1) is someone, gone out of sight.
    return vec4<f32>(c.rgb, colour.a * select(1.0, c.a, mesh.uv.y > 0.5));
}
