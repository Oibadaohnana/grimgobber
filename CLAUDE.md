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

## Multiplayer first

Every feature is built for 2–6 players on a headless server, even when
only tried alone. Before writing one, decide where it runs: the server
(authority: the wraith, doors, sanity, anything that changes the bank or another
player), the owning client (his own feet and aim), or every client (only
drawing and sound). Then:

- State that others must see lives in a replicated component or crosses
  as a message; never only in a client's resource or local entity.
- Simulation must run headless: no input, window, assets or GPU in it,
  and it loops over every robber, never assumes `LocalPlayer`.
- A client asks, the server decides and replicates: no client-side
  change to shared state that the server never hears of.
- Bump `net::PROTOCOL` when what crosses changes, and try it with
  `--server` and two `--connect` clients, not only a game alone.

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
  On the laptop's Radeon 780M it is about 270 fps at 1400×900 and 120 at
  2560×1440 (lamps baked), the same in release: the per-pixel tracing
  bounds it.
- `SEED=n cargo test print_map -- --ignored --nocapture` prints a map as
  ASCII (`#` brick, `=` drywall, `V` vault, `:` glass, `D` door, `o`
  furniture, letters for rooms).

## Where things are

| file | what it does |
| --- | --- |
| `src/movement.rs` | bims' steered walk: `Body`, `Steer`, `Solids`, `Aabb::push_out_round`. Pure arithmetic, stepped at 60 Hz. |
| `src/map.rs` | the generator: tiles, rooms, props, lamps; `solids()` for movement, `occluders()` for the ray tracer |
| `src/paint.rs` | `Painter` (vertex-coloured triangles) and every prop's, the robber's, the wraith's (`wraith(t)`, animated) and its footprints' picture |
| `src/render.rs` | `WorldMaterial`, the grid textures, spawning a map, baking the lamps into the light maps, writing the robber and the flashlights each frame |
| `src/shaders/world.wgsl` | the per-pixel ray tracing (lamp and flashlight shadows), the procedural floors and walls |
| `src/player.rs` | input → `Controls` (PreUpdate), the body's fixed step (FixedUpdate), the picture blended between steps, camera, zoom |
| `src/doors.rs` | door leaves (`Map::doors`): pushed open within reach (and `Reach` for his arm), the glass entrance by sensor; how shut each is in `Shut`, uploaded to the doors texture |
| `src/wraith.rs` | the wraith: its roaming, its secret name (`Mind`, server only), the notes with its letters (`Note`, read in `read`), its name heard (`hear`), seeing all round and hearing (`notice`), the roll against sanity (`haunt_chance`), signs (`Prints`, a haunted door, a flicker), the haunt (shown, flicker, hunt, the kill: `Dead`, the catch in a trap); on clients its picture in a haunt, the footprints, the dead hidden from the living and drawn pale |
| `src/crew.rs` | the crew and the rounds: `Crewmate` (one a player, replicated) with his `Look` (name, colour, `Outfit`), saved in `~/.config/grimgobber/player.txt` (`Settings`); `SetLook`, `Ready`, `Leave` asked of the server; `start` (everyone ready: a new bank) and `finish` (caught, all dead or all left) |
| `src/menu.rs` | the lobby screen (crew, name typed straight in, colour, outfit, the portrait rendered by its own camera to an image, Ready), the Esc menu (`escape` swallows input while open, so it reads its own keys first), both by keys too (`Nav`), names over the robbers he sees |
| `src/talk.rs` | T's box: typing swallows every key (`talk`, before `read_controls` and the inventory's input), Enter sends `Say` |
| `src/trap.rs` | the ghost trap: set with a left click (`SetTrap`), taken back with E (`TakeTrap`), kept by the server (`Trap`), sprung by wraith.rs |
| `src/sanity.rs` | `Sanity` per robber (0–100, replicated), drained by the server in the bank's rooms, dark faster; on the HUD |
| `src/paths.rs` | `Walkable`: paths over `Map::free` tiles (a Dijkstra), for the wraith |
| `src/hearing.rs` | how far the robber's feet carry (`feet`), and the ring showing how far a smash reaches: `Sight::carry` per tile into the `heard` texture (tiles of reach left), which the shader draws a faint ring from where it runs out |
| `src/sight.rs` | the shader's `seen` on the CPU over the same grids and the doors texture's bytes, for the wraith |
| `src/inventory.rs` | the gear: items (a stack of coins is one, `count`) on cells of one grid (`GRID`) or in the hands, `fits`/`put`/`stow`/`add`/`remove`, quick slots |
| `src/inventory_ui.rs` | the Tab screen and the quick-slot bar: one mesh rebuilt every frame (a child of the camera, scaled by its zoom, in logical pixels from the window's middle), UI text over it, drag and drop |
| `src/smash.rs` | the crowbar's left click: the window in front breaks (`Map::smash`), and everything built from the map is rebuilt |
| `src/throw.rs` | coins: the range ring and landing marker in hand, thrown with a left click (`landing` short of solids), flying, plinging (a `Noise`), lying on the floor till the robber picks one up (E) |
| `src/prompt.rs` | the "E ..." prompt: what E would do offers itself each frame (`Prompt`, cleared in `First`), shown where it says |
| `src/guns.rs` | pistols: the robber's aimed by `player::gun_line` (bims' carry onto the pointer), fired and reloaded; hitscan `shoot` (`Sight::stops`), tracers, the reticle (`Gunplay`) |
| `src/sound.rs` | the old game's door and footstep samples (`sounds/*.ogg`, embedded): `Noise` messages and walkers' loops, placed by distance and sight as `audio.js` did |
| `src/door.rs` | the front door (`--serve`, UDP 8794 on grim.buggly.de): lobbies by code, each a child `--server PORT --code CODE` on the ports above; SCARE always (restarted), a player's `HOST` a random code, closed once empty `IDLE`; the client's `Asking`, polled per frame |
| `src/net.rs` | playing together: `Mode` (title, alone, online), connecting at run time (`connect`, after the door's answer in `knock`), going back to the title (`lost`), `--server`/`--connect`, what is replicated (`Seed`, robbers with `Owner`, `Sanity` and `Dead`, door leaves, the `Wraith`), each robber's `Moved` sent up, noises and footprints passed down (`pass_on`) |
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
  furniture, within 6 tiles in a lit room or outside and 3.75 in a dark one
  (`Map::lit`, in `extra.a`). Per room, not per pixel: by the light on each
  spot, far lamp pools showed as islands. Inside the flashlight's cone
  (`beam`) sight is the full 6 however dark it is. The zoom goes out to
  just past it (`player::zoom`). The wraith's own reach (`SEES` 8 lit or
  in a flashlight, 4 dark, all round) is separate. The flashlight's light (`torch_at`) casts its own shadows.
  Crouched (`misc.w`), the first half-height object a sight ray crosses
  hides what lies more than 2 tiles past it
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
  and the wraith and its footprints, which glow dimly on purpose: they must
  be seen in the dark.
  Screens, LEDs and the like are ordinary paint, or they shine in a dark
  room. Glow can go past white for the bloom.
- **Noises are messages** (`sound::Noise`): a sample to play, if there is
  one, and `loud`, how many tiles off the wraith hears it. The sound plugin
  plays them, the wraith listens to the same ones (and to the robbers'
  feet), and hearing.rs shows the glass's. How far one reaches is
  `Sight::carry`, a Dijkstra over tiles (a wall or shut door costs
  `THROUGH_WALL` more), the same for the wraith and for the ring drawn.
- **A left click uses what is in hand**: the crowbar smashes, a coin is
  thrown, the pistol fires. R reloads (only with the pistol in hand); a new bank is F5.
- **A pistol's `count` is the rounds in its magazine**, not a stack.
- **Items are indexed**: quick slots and drags hold indices into
  `Inventory::items`, so take items out only with `Inventory::remove`
  (or `use_one`), which keeps the quick slots right.
- **Sight is written twice**: `seen`/`trace` in world.wgsl and in
  `sight.rs`. Change one, change the other. The wraith's check is traced
  from the robber to it (sight is mutual), so the crouched cover rule
  hides the robber from it exactly as it hides it from him.
- **Fading vertices** (UV y = 1, `Painter::fades`) take how clearly they
  are seen as alpha: people vanish out of sight instead of showing under
  the fog. Only for things on the blended material.
- **Three world materials**: ground (mode 0), things (1) and ghosts (2,
  `render::ghost_material`), which draw any mesh pale and see-through,
  unlit and unfogged: the dead.
- **Lamps are baked**, not traced per pixel: two light maps (16 texels a
  tile, half floats), the ground's and things' (they differ under
  furniture and in doorways), each drawn by a `Bake` camera rendering
  `lamps_at` in modes 3 and 4. `render::bake` redraws them when `Shut` or
  `smash::Bank` changes (one map a frame, taking turns, so a swinging door
  costs half), and both while a pipeline compiles (or the first bake
  draws nothing). Anything new that changes what blocks a lamp
  must trigger a bake too. So there are three cameras: queries for the
  player's view take `With<MainCamera>`.
- **Only a held flashlight shines**: `read_controls` clears `Controls::torch`
  unless the flashlight is in his hands, so everything that reads `torch`
  (the shader, the wraith's sight, sound) follows. The dead's never shines. The robber's tools are
  `Part::Tool`, shown while held.
- **Thickets are hideouts** (`PropKind::is_hideout`): not solid, so he walks
  in, but placed like a solid prop and their tiles kept clear. In one
  (`Level::hideout`) the wraith only sees him if it is hunting him already,
  or is in the same thicket.
- **A robber is an entity**: `Player` (his body) with `Controls`,
  `Inventory`, `Shown`, `Reach`, `Swing`, `Gunplay` and `Sanity` beside it, so there
  can be several. `LocalPlayer` marks the one this machine plays; input,
  the camera, the shader's eye, the HUD and the ear read only him (a
  `Single`, which skips the system until he exists; `Living` also skips
  it once he is `Dead`). Simulation (steps, doors, the wraith's senses,
  sanity) runs over every robber. The others are painted fading
  (`paint::robber(fades)`). `BB_PLAYERS=n` adds a test crew (`dev::Bot`).
- **Every lit flashlight lights the world** (`Params::torches`, up to
  `MAX_TORCHES` = 6, the robber's own first, count in `map.w`), each with
  its own shadows. Only his own (`eye`, `misc.z`, `beam`) stretches his
  sight. Six at once cost about 12% at 2560×1440.
- **Simulation and drawing are split** so `grimgobber --server` runs
  headless (`MinimalPlugins`, 60 Hz, no window or GPU). A plugin adds its
  drawing systems only `if crate::drawn(app)`. `build_map` spawns data only
  (robbers, the wraith, door leaves, `Shut`, `Level`, `Sight`, `Walkable`);
  the client's `draw_map` and the `dress` systems (`player`, `wraith`,
  `doors`) add meshes, `Transform` and marks when they appear
  (`Added<..>`), which is also how replicated entities arrive. Guns, coins
  and smashing are client-only for now.
- **Networking is server-authoritative but your feet are yours.** The
  server (bevy_replicon over renet, UDP) replicates the wraith, door leaves,
  every robber and the `Seed`; a client builds the bank from the seed and
  spawns nobody itself (`net::Joined`). Its own robber (`Owner` = its
  client id, so `LocalPlayer`) is stepped locally and sent up as `Moved`;
  the receive marker `ours` keeps the server's copy from overwriting it.
  Only robbers in `player::Driven` are stepped. Authority systems (the
  wraith's mind, sanity, `swing`, the parked actions) run `in_state(
  ClientState::Disconnected)`, which is the server and a game alone.
  Other players and the wraith ease to where the server says (`show`).
  A changed component is sent to every player, so don't touch one that
  didn't change (see `swing`). Bump `net::PROTOCOL` when what crosses
  changes. Lobbies are processes: one game server is one bank, so the
  door runs one per lobby, and a client knocks (`Asking`), then connects
  to the port it is given. A game alone has no server at all. Test the
  door locally: `grimgobber --serve 9140` in the background (kill its
  `--server` children after, it does not), then `./hidden ...
  --door 127.0.0.1:9140` with `BB_JOIN=SCARE` or `BB_HOST=1` (and
  `BB_IDLE=8` on the door to watch a lobby close). Test one server alone:
  `--server` in the background and two `./hidden
  ... --connect 127.0.0.1` with `BB_KEYS`.
- **Doors' state is `doors::Shut`** (the doors texture's bytes, a resource
  the server has too); sight and hearing read it, and the client uploads it
  to the texture when it changes.
- **Everything spawned for a map carries `WorldPart`.** A round's end
  (`crew::finish`) despawns them all on the server; everywhere, `clear`
  (main.rs) then sees the seed gone and removes the map's resources
  (`Level`, `Sight`, `Shut`, `Bank`, `Walkable`, `Current`), so every system
  that needs one must be guarded (`run_if(resource_exists::<..>)`). A
  chained tuple with such a condition needs `distributive_run_if`: a chain
  flushes commands between its systems, so a resource checked at its
  start can be gone by its end.
- **Rounds start in the lobby.** The game begins with no bank; a robber is
  made per `Crewmate` when every one is ready (`build_map`, in his `Look`,
  `Owner` his id, 0 alone; `net::mine` makes this machine's `LocalPlayer`).
  The lobby shows whenever this machine has no robber. A smoke run readies
  itself, so the commands here go straight into a bank; `BB_LOBBY=1` keeps
  it in the lobby (with `Ready` in `BB_KEYS` to start, `Leave` to go back,
  `Menu` for the Esc menu, `Key:ArrowDown`, `Key:Enter` and the like for
  real key presses, which menus and text boxes hear). Try looks without touching your own settings:
  `XDG_CONFIG_HOME=some/dir` with `grimgobber/player.txt` in it.
- **The doors and tiles textures are rewritten at run time** (doors swing,
  windows get smashed), so they keep their data in the main world
  (`RenderAssetUsages::default()`); the others are render-world only and
  have no data to write. A smash also rebuilds what came from the map: the
  `Level` (solids, cover, thickets), `Walkable` and `Sight`, from
  `smash::Bank`, the map kept as it now stands. A shut door is a thin slab
  across its doorway in `trace`, opaque (glass for the entrance) times how
  shut it is. Doors never block movement: they open before he gets there.
  The wraith passes through them shut; it swings one only as a sign
  (`Leaf::haunt`: open with nobody there, then slammed).
- **The wraith rolls, it does not choose.** Seeing or hearing a living
  robber (`notice`), it rolls against his `Sanity` (`haunt_chance`: 3%
  sane, 53% at none), at most every `ROLL_EACH` for him and `ROLL_GAP` for
  anyone, and no haunt within `RESPITE` of the last. No haunt: a sign he
  can see (footprints, a door, a flicker), which costs whoever sees it
  `SIGN_SHOCK`. A haunt: shown and still for `WARN`, the lights round it
  flickering (`Wraith::flicker`, the shader's `flickered`, on the baked
  lamps and the flashlights there), then it hunts for `HUNT`; within
  `TOUCH` it kills (`Dead`) and the haunt ends. Between haunts it keeps to
  its room (one kind for now). Try haunts with `BB_SANITY=0` and
  `BB_WRAITH` next to `BB_SPAWN`.
- **Catching it is the round's goal.** Its name (`NAMES`, five letters,
  from the seed) never leaves the server; its letters lie on `Note`s, read
  by walking over them into `Wraith::letters`. With all five, T speaks a
  word (`talk::Say`): the name forces a `NAMED_HUNT` haunt at the speaker
  and sets `Wraith::named`; a wrong one costs `WRONG_NAME` sanity. Named and
  shown, within `TRAPS` of an unsprung `Trap` it is caught (checked before
  its touch, and `TRAPS` > `TOUCH`, so a trap under the bait wins). Client
  requests (`Say`, `SetTrap`, `TakeTrap`) are `FromClient` on the server,
  and from `ClientId::Server` alone; `net::sent_by` finds whose. Try the
  whole round: `BB_SEED=6 BB_SPAWN=24,14 BB_NOTES=25,14 BB_AIM=180
  BB_SAY=FELIX BB_KEYS="20:+D,120:-D,130:Slot3,140:SetTrap,150:+A,168:-A,180:Say"`.
- **A `dress` that paints with the world's materials runs in `main.rs`'s
  chain after `draw_map`**: replicated entities can arrive in the same
  frame as the seed, before the client has built the bank's materials.
