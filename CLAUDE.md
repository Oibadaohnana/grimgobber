# Working on Bank Bobbery (Rust)

A single-crate Bevy 0.19 game (same Bevy as `../bims2_bevy`, so builds can
run `--offline` from the shared cargo cache). `README.md` covers what the
game is. This file covers how to work on it.

## Code style

- Keep the code clean and short: the fewest lines that do the job clearly.
  No speculative helpers, layers or options.
- Minimal comments: only what a future reader (you) could not get from the
  code itself, such as a non-obvious reason or a trap. No narration, no
  restating the code, no long doc headers.
- Lightweight tests: a few fast ones that pin what matters (the movement's
  numbers, the map's reachability). No broad or slow test suites.

## Build, test, look

- Work inside `nix-shell shell.nix`, which provides cargo, the runtime libs,
  ALSA (linked by Bevy's audio) and weston. Smoke runs are silent; a sound
  is only heard in a normal run. `cargo build --offline`, `cargo test --offline`,
  `cargo clippy --offline --all-targets` (keep it warning-free), and
  `rustfmt --edition 2024 src/*.rs`.
- `cargo build` proves nothing about the picture, so look at a screenshot:
  `BB_SEED=6 BB_SMOKE_FRAMES=150 BB_SCREENSHOT=shot.png ./hidden target/debug/grimgobber`.
  Smoke frames are paced at 60 fps. The shader compiles during the first
  second or so, so a screenshot taken before about frame 90 may be missing
  layers.
- To check the frame rate, add `BB_UNPACED=1` and read the "smoke run: …
  fps" line. On the RX 7800 XT, in a dev build, it is about 800 fps at
  1400×900 (720 with the flashlight off, 760 with it on in a dark room).
- `SEED=n cargo test print_map -- --ignored --nocapture` prints a map as
  ASCII (`#` brick, `=` drywall, `V` vault, `:` glass, `D` door, `o`
  furniture, letters for rooms).

## Where things are

| file | what it does |
| --- | --- |
| `src/movement.rs` | bims' steered walk: `Body`, `Steer`, `Solids`, `Aabb::push_out_round`. Pure arithmetic, stepped at 60 Hz. |
| `src/map.rs` | the generator: tiles, rooms, props, lamps; `solids()` for movement, `occluders()` for the ray tracer |
| `src/paint.rs` | `Painter` (vertex-coloured triangles) and every prop's, the robber's and the guard's picture |
| `src/render.rs` | `WorldMaterial`, the three grid textures, spawning a map, writing the robber, the flashlight and the lamps each frame |
| `src/shaders/world.wgsl` | the per-pixel ray tracing (lamp and flashlight shadows), the procedural floors and walls |
| `src/player.rs` | input → `Controls` (PreUpdate), the body's fixed step (FixedUpdate), the picture blended between steps, camera, zoom |
| `src/doors.rs` | door leaves (`Map::doors`): pushed open within reach (and `Reach` for his arm), the glass entrance by sensor; how shut each is in `Shut`, uploaded to the doors texture |
| `src/guard.rs` | the guard: T then a click gives him a path over `Map::free` tiles, his fixed step along it, his picture; seeing the robber (`detect`) he chases, losing him he runs to `last_seen`, marked by the robber's ghost; hearing a `Noise` or the robber's feet (`Sight::carry` reaching his tile) he goes to look, marked by a "?"; the "!"; E from behind knocks him out (`out`, `fall`: lying, blind and deaf, a "Zz") |
| `src/hearing.rs` | how far the robber's feet carry (`feet`), and the ring showing how far a smash reaches: `Sight::carry` per tile into the `heard` texture (tiles of reach left), which the shader draws a faint ring from where it runs out |
| `src/sight.rs` | the shader's `seen` on the CPU over the same grids and the doors texture's bytes, for the guard |
| `src/inventory.rs` | the gear: items (a stack of coins is one, `count`) on cells of one grid (`GRID`) or in the hands, `fits`/`put`/`stow`/`add`/`remove`, quick slots |
| `src/inventory_ui.rs` | the Tab screen and the quick-slot bar (and the VICINITY panel of a knocked-out guard's pockets close by): one mesh rebuilt every frame (a child of the camera, scaled by its zoom, in logical pixels from the window's middle), UI text over it, drag and drop |
| `src/smash.rs` | the crowbar's left click: the window in front breaks (`Map::smash`), and everything built from the map is rebuilt |
| `src/throw.rs` | coins: the range ring and landing marker in hand, thrown with a left click (`landing` short of solids), flying, plinging (a `Noise`), lying on the floor till a guard pockets them (`Guard::pockets`) or the robber picks one up (E) |
| `src/prompt.rs` | the "E ..." prompt: what E would do offers itself each frame (`Prompt`, cleared in `First`; the knockout first), shown where it says |
| `src/guns.rs` | pistols: the robber's aimed by `player::gun_line` (bims' carry onto the pointer), fired and reloaded, the guard's drawn when the robber is armed and seen; hitscan `shoot` (`Sight::stops`), tracers, the reticle, health (`Gunplay`) |
| `src/sound.rs` | the old game's door and footstep samples (`sounds/*.ogg`, embedded): `Noise` messages and walkers' loops, placed by distance and sight as `audio.js` did |
| `src/dev.rs` | the `BB_*` switches for headless runs |

## Things easy to get wrong

- **Movement numbers are bims'.** `MARCH_SPEED` 216, `ACCEL` 330 × 4
  under the keys, instant stop, `SPRINT` 1.8, the roll 195 / 0.38 s / 1.1 s,
  `BODY_MARGIN` 23, `CORNER_ROUND` 1.5 × margin, `TILE` 52. The tests in
  `movement.rs` pin the feel. Change them only on purpose. `THICK` (half
  pace in a thicket) is ours, not bims'.
- **An opaque box must lie inside a solid one.** The flashlight's rays start
  at the robber's hand, so a body inside an opaque box would light nothing.
  Make every opaque prop solid; a test enforces this.
- **Sight** (`seen`): three rays from the eyes, blocked by walls and tall
  furniture, within 12 tiles in a lit room or outside and 5 in a dark one
  (`Map::lit`, in `extra.a`). Per room, not per pixel: by the light on each
  spot, far lamp pools showed as islands. Inside the flashlight's cone (`beam`) sight
  is the full 12 however dark it is. The flashlight's light (`torch_at`)
  casts its own shadows. Crouched (`misc.w`), the first half-height object
  a sight ray crosses hides what lies more than 2 tiles past it
  (`trace(.., low)`, the `low_boxes` texture, ids in `extra.b`). Which
  props are half height is `PropKind::is_low`.
- **What skips what in a trace.** A ray ignores the box of the object its
  target pixel is on (a cabinet's whole top is seen once its front is) and
  the box its source stands in (a lamp over furniture). For a pixel on a
  wall top, the trace is redone from the floor just past the wall's
  nearest open face or open diagonal (`wall_face`, chessboard distance, so
  each side's half-tile band meets the next square). Without that, rays
  graze along neighbouring wall tiles and draw saw-tooth shadows on the
  walls. Wall-top traces ignore furniture (`Spot.top`): it is lower than
  the wall, and that point often lies inside a shelf against it. Floor
  pixels under an object never skip it (`ground`), which is why a round
  pillar's square box darkens its plinth's corners.
- **Reserved WGSL words.** `from`, `target`, `filter`, `set`, `type` and
  `meta` are reserved, so naga refuses them as identifiers.
- **The tile textures are `Rgba8Unorm`, not sRGB.** They hold bytes (kinds,
  flags, box bounds) and are read with `textureLoad` × 255. Colours in the
  shader are written in sRGB and converted with `srgb()`. Vertex colours
  are linear (`paint::Hex::paint`).
- **Glowing vertices** (UV x = 1) ignore light altogether, so keep them for
  things that are lights: the street lamps' heads, the flashlight's lens,
  and the guard's "!", which must read in the dark.
  Screens, LEDs and the like are ordinary paint, or they shine in a dark
  room. Glow can go past white for the bloom.
- **Noises are messages** (`sound::Noise`): a sample to play, if there is
  one, and `loud`, how many tiles off a guard hears it. The sound plugin
  plays them, the guard listens to the same ones, and hearing.rs shows
  the glass's. How far one reaches is `Sight::carry`, a Dijkstra over tiles (a
  wall or shut door costs `THROUGH_WALL` more), the same for the guard and
  for the ring drawn, so what you see is what he hears.
- **A left click while T is armed is the guard's order**: `order` clears it
  so the crowbar does not swing (nor a coin fly) too. Otherwise a left click
  uses what is in hand: the crowbar smashes, a coin is thrown, the pistol
  fires. R reloads (only with the pistol in hand); a new bank is F5.
- **A pistol's `count` is the rounds in its magazine**, not a stack.
- **Items are indexed**: quick slots and drags hold indices into
  `Inventory::items`, so take items out only with `Inventory::remove`
  (or `use_one`), which keeps the quick slots right.
- **Sight is written twice**: `seen`/`trace` in world.wgsl and in
  `sight.rs`. Change one, change the other. The guard's check is traced
  from the robber to the guard (sight is mutual), so the crouched cover
  rule hides the robber from him exactly as it hides him from the robber.
  His cone (`CONE`, `GUARD_VIEW` in guard.rs) is drawn by `watched` in
  world.wgsl, from `Params::guard` (the first guard only); keep the two
  in step too.
- **Fading vertices** (UV y = 1, `Painter::fades`) take how clearly they
  are seen as alpha: people vanish out of sight instead of showing under
  the fog. Only for things on the blended material.
- **Three world materials**: ground (mode 0), things (1) and ghosts (2,
  `render::ghost_material`), which draw any mesh pale and see-through,
  unlit and unfogged.
- **Only a held flashlight shines**: `read_controls` clears `Controls::torch`
  unless the flashlight is in his hands, so everything that reads `torch`
  (the shader, the guard, sight and sound) follows. The robber's tools are
  `Part::Tool`, shown while held.
- **Thickets are hideouts** (`PropKind::is_hideout`): not solid, so he walks
  in, but placed like a solid prop and their tiles kept clear. In one
  (`Level::hideout`) a guard only goes on seeing him if he already did, or
  if he is in the same thicket.
- **A robber is an entity**: `Player` (his body) with `Controls`,
  `Inventory`, `Shown`, `Reach`, `Swing` and `Gunplay` beside it, so there
  can be several. `LocalPlayer` marks the one this machine plays; input,
  the camera, the shader's eye, the HUD and the ear read only him (a
  `Single`, which skips the system until he exists). Simulation (steps,
  doors, the guard's sight and hearing) runs over every robber. A guard's
  `target` is the robber he is after. The others are painted fading
  (`paint::robber(fades)`). `BB_PLAYERS=n` adds a test crew (`dev::Bot`).
- **Every lit flashlight lights the world** (`Params::torches`, up to
  `MAX_TORCHES` = 6, the robber's own first, count in `map.w`), each with
  its own shadows. Only his own (`eye`, `misc.z`, `beam`) stretches his
  sight. Six at once cost about 12% at 2560×1440.
- **Simulation and drawing are split** so `grimgobber --server` runs
  headless (`MinimalPlugins`, 60 Hz, no window or GPU). A plugin adds its
  drawing systems only `if crate::drawn(app)`. `build_map` spawns data only
  (robbers, the guard, door leaves, `Shut`, `Level`, `Sight`); the client's
  `draw_map` and the `dress` systems (`player`, `guard`, `doors`) add
  meshes, `Transform` and marks when they appear (`Added<..>`), which is
  also how replicated entities will arrive. Guns, coins, smashing, the
  knockout and the guard's T order are client-only for now.
- **Doors' state is `doors::Shut`** (the doors texture's bytes, a resource
  the server has too); sight and hearing read it, and the client uploads it
  to the texture when it changes.
- **Everything spawned for a map carries `WorldPart`.** F5 despawns all of
  them and builds the next map.
- **The doors and tiles textures are rewritten at run time** (doors swing,
  windows get smashed), so they keep their data in the main world
  (`RenderAssetUsages::default()`); the others are render-world only and
  have no data to write. A smash also rebuilds what came from the map: the
  `Level` (solids, cover, thickets), the guard's `Walkable` and `Sight`, from `smash::Bank`,
  the map kept as it now stands. A shut door is a thin slab
  across its doorway in `trace`, opaque (glass for the entrance) times how
  shut it is. Doors never block movement: they open before he gets there.
