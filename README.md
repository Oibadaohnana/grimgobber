# Bank Bobbery (Rust + Bevy)

A top-down heist on foot, at night, in a bank nobody has built before. This
is the start of a Rust/Bevy remake of `../bankbobbery`, turning into a
Phasmophobia-like heist: the bank is haunted, and the dark and what you
see in it wear down your sanity.

- **Movement** is bims_bevy's steered walk, ported number for number
  (`src/movement.rs`). WASD walk, the mouse aims (the body faces the pointer
  at once, and walking away from it strides backwards), Shift sprints
  (1.8×, eased in over 0.1 s), and Space or Alt dodge-rolls (195 units in
  0.38 s, 1.1 s cooldown). C crouches and stands: a little over half speed
  and no sprint, folded forward with his knees out in front. A roll goes
  head over heels. F switches the flashlight strapped to his backpack on and off, whatever he holds. A hand with
  nothing in it hangs at his side and swings as he walks (on his knee
  crouched). Letting go stops you
  dead. Lone corners are rounded, so you slide round door frames instead of
  snagging on them.
- **The bank is generated** from a seed (`src/map.rs`). It sits on a city
  block: a road with the getaway van at the kerb, a pavement, a car park
  and trees. Inside are a marble lobby with pillars, a queue and the teller
  counter, a corridor with a side door, and back rooms split by a BSP:
  open-plan office, manager, break room, washrooms, server room, security,
  archive and a steel **vault** with one door. A flood fill makes sure
  every room can be reached.
- **Light is ray traced per pixel** (`src/shaders/world.wgsl`). Rays go
  through the tile grid to each lamp in reach, and the walls and tall
  furniture (cabinets, shelves, racks, pillars, the van, trees) cast exact
  shadows away from that lamp. The lamps stand still, so their light is
  traced once into a light map, again only when a door moves. The robber's
  **flashlight** is a light like any other: a cone from his shoulder, 15 tiles
  long, with a soft edge, throwing shadows away from him. With it off he
  throws none. Glass lets light through.
- **Sight is ray traced too.** You see only what the robber could see:
  walls and tall furniture block it, glass doesn't. You see 6 tiles into a
  room with its lights on (and outside), 3.75 into a dark one. Inside the
  flashlight's beam you see the full 6 tiles, as in a lit room. You can
  zoom out only as far as you can see. Anything
  out of sight is drawn under a grey-blue fog.
- **Doors** open as you reach them, half a tile off: the robber puts his
  arm out and pushes, the door swings away from him, and it swings shut
  behind him with a flick of his hand. The glass entrance opens on a sensor
  instead, from further off. Shut, they stop sight and lamplight like a wall (the
  glass entrance only like glass). Inside doors are wood, the side doors
  steel, double doors hinge at their outer edges. Out of your sight a door
  stays drawn as you last saw it: you don't see one open in the fog.
- **Half-height cover.** Desks, tables, the counter, sofas and parked cars are half
  height. Crouched behind one you are in cover (the HUD says so), and you
  see only 2 tiles past it, though walls and tall things beyond still show
  over it, and the wraith can't see you there.
- **Lights.** A switch by a door of each lit room turns its lamps on and
  off (E). Every lamp on drains the bank's generator, which slowly
  recovers: leave them all on and in about three minutes the breaker trips
  and every lamp inside goes out. Reset it at the fuse box (somewhere in
  the building, a different room each bank) once the generator has
  recovered a little.
- **Hiding.** Walk into a bush outside or a thicket inside: the wraith
  sees nobody through one or in one, and your feet are silent there. Only
  a hunt that had you as you went in follows you in; a noise you make
  (a shot) still gives you away.
- **Sanity.** Each robber starts at 100% (a bar beside the quick slots shows it). It drains in
  the bank's rooms, 0.25% a second in a dark one and 0.05% in a lit one
  (outside, not at all), and a sign of the wraith you see costs 5%, being
  near as a haunt starts 15%.
- **The goal: catch the wraith.** Its name has five letters, each written on
  a note lying somewhere in the bank (bring a light: out of sight they are
  not drawn). Walk over a note to read it; the crew's letters show on the
  HUD, each in its place in the name. With all five, **T** opens a box with
  its name already in it (change it if you like), Enter speaks: its name
  brings it, a long haunt straight at you; a wrong word costs you 5% sanity.
- **The end.** Catching it shows how long the round took and a heatmap of
  the bank: where it went (violet) and where the crew went (orange).
  Enter, Esc or a click closes it.
- **What is it?** The wraith is a **poltergeist**, a **shade** or a
  **banshee**, and each shows three signs of six: footprints, a door
  slammed, flickering lights, things thrown, frost on the floor, a wail.
  Tick what you see on the **phone (P)**; it says which kinds are left.
  Each is caught its own way, with tools from the **van** on the street (E
  by one): shoot a poltergeist three times while it shows itself, and while
  it lies still, E seals it in a **coffer**; hold a **UV lantern**'s beam
  (the left button held) on a shade for 3 seconds while it shows itself; net a banshee with the
  **ghost trap** (set with a left click, its **trigger** left in your hand,
  a left click fires a blue net 1.5 tiles round every set trap, which
  takes a banshee that comes into it while it is out; E takes the trap back). A banshee goes to noises, so a thrown coin lures it. The
  wrong tool enrages any of them: a hunt straight at you.
- **The wraith** haunts the bank, unseen, keeping to its room and now and
  then wandering out of it. It sees all round (8 tiles where you are lit, by a lamp or your
  flashlight, 4 in the dark) and hears what the robber does. Each time it
  notices a robber (at most every 10 seconds for him, every 4 for anyone),
  it rolls against his sanity: a haunt at 3% sane, 53% at none.
  - **No haunt: a sign** he can see. Pale footprints walk towards him one
    by one; or a door near him swings open with nobody there and slams; or
    the lights round him flicker.
  - **A haunt**: it shows itself, a crawling thing of black hair and
    long-fingered arms, its face thrown back to stare up at you, and the
    lamps and flashlights within 8 tiles of it flicker. After 3 seconds it
    hunts, for 20: it goes for whoever it sees or hears, at a walk, so you
    outrun it sprinting; hide in the plants, crouch out of sight, keep
    quiet. While it haunts, the doors to the outside slam and lock (a red
    lock on them): there is no running out. Its touch kills, and ends the haunt; another may not come for 30
    seconds.
  - **Dead**, you walk on unseen by the living, pale and see-through, and
    touch nothing.
- **An inventory**, DayZ style (Tab): one 8x5 grid, each item taking a
  block of cells (crowbar 4x1, pistol 2x1, ghost trap 2x2, lockpicks
  1x1). Drag items
  about with the mouse, R turning the one you
  hold; drop one on the hands to hold it. Quick slots 1-4 along the bottom
  take their item into his hands or put it away (drop an item on a slot to
  bind it, right-click to clear it). What he holds is what he carries in his
  hands: the crowbar shows only when held, the UV lantern shines only in
  hand (with the left button held), the backpack's flashlight always can. The game runs on while it is open; WASD still walks.
- **Coins.** He carries three (one stack, on quick slot 4). In hand, a
  left click throws one at the pointer, up to 8 tiles, short of anything
  solid in the way: a gold ring shows how far you can throw and a marker
  where it would land. It lands with a pling heard 7 tiles off (the same
  spread as any noise, shown by a faint ring). Walk over a coin on the
  floor and E picks it up again.
- **Guns.** The thug carries a pistol (8 in the magazine, in the inventory: drag it to a slot or the hands)
  and 16 rounds. In hand it aims at the pointer as in bims, the arm
  swinging the barrel onto it, a reticle on it (grey past its 20 tiles); a
  left click fires (not while sprinting or rolling), R reloads from the
  rounds. A shot stops at a wall, a window, a tall thing or a shut door, or
  at the first body in its way, and is heard 26 tiles off.
- **Smashing windows.** With the crowbar in hand, a left click swings it;
  a window right in front of you breaks into an opening you can climb
  through, glass strewn on the floor and teeth of it left in the frame.
- **Hearing.** The wraith hears what you do: a smashed window carries 14
  tiles, your feet sprinting 6, rolling 3, walking 2.5, a door you open or
  shut 4; crouched, you make no sound at all, at doors neither. A noise spreads across the floor, round
  furniture, down corridors and through open doors, and loses 4 tiles of
  its reach to every wall or shut door it passes. A smashed window shows
  how far (and so does a coin landing): a faint ring sweeps out to exactly
  where it would be heard.
- **Plants to hide in.** Big potted palms stand in the lobby and offices,
  their fronds over you as you pass, and planters of tall, dense plants
  line some walls (the lobby, the corridor, the open office). Walk into one,
  at half pace through the leaves, and you are hidden (the HUD says so): the
  wraith does not see you there unless it is hunting you already, or is in
  it with you.
- **Sound**, from `../bankbobbery`: doors opening and shutting, and
  footsteps (soft crouched); and a window smashing, synthesised
  (`sounds/glass.py`); a coin's pling, synthesised (`sounds/coin.py`); and
  the old game's pistol shot, reload, holster and unholster. Each is
  quieter with distance and quieter still through a wall. The wraith has
  no sounds of its own yet.
- **All the graphics are its own.** Floors and walls are painted
  procedurally in the shader. The furniture and the robber (striped shirt,
  beanie, mask, swag bag, crowbar) and the wraith are vertex-coloured
  meshes built in
  code (`src/paint.rs`). There are no image assets; the sounds
  (`sounds/`, Ogg, cut from the old game's samples or synthesised) are
  built into the binary.

## Run it

```bash
nix-shell            # rust, cargo, the Vulkan/Wayland/X11 runtime libs, weston
cargo run            # a random bank
BB_SEED=1234 cargo run
```

### The simulation

```bash
cargo run -- --sim                 # a bank to try the wraith in
BB_SEED=6 BB_SANITY=0 cargo run -- --sim
```

No lobby and nobody in the bank: a free camera that sees everything, from
close in out to the whole bank (**WASD** pans, **Shift** faster, the
**wheel** zooms), and the wraith always drawn, pale while it does not show
itself; a box at the bottom right says what it is up to. **H** puts a ghost
hunter at the pointer, carrying every tool, and gives him to you (the one before stays where he
stood, as bait), **V** switches between the free camera and following him,
**F5** builds a new bank. The round never ends: dead, spawn another.

### Together

Up to 6 play one bank. The game opens at a title: type a lobby code and
Join, Host a new lobby, or Play alone. Lobbies live at `grim.buggly.de`:
the first, always open, is **SCARE**; a new one gets a random code to give
your crew, and closes after ten minutes with nobody in it.

To run the server yourself (UDP 8794 for the door, 8795-8802 for the
lobbies), and to point the game at it:

```bash
cargo run -- --serve                          # the door and its lobbies
cargo run -- --door 192.168.1.5               # the title asks this door
cargo run -- --server 9000                    # one lobby, no door
cargo run -- --connect 192.168.1.5:9000       # straight into it
```

The server runs the wraith, everyone's sanity and the doors, and says who
is where; your own robber moves on your machine at once. Online, the
crowbar, coins and the pistol do nothing yet.

### The lobby

The game opens in a lobby: the crew, and your name (just type), colour and
outfit (stripes, hoodie, suit or tracksuit), with your robber as he will
look; they are kept in `~/.config/grimgobber/player.txt`. When everyone is
ready (alone, Start), a new bank. Up and down (or Tab) move between the
rows, left and right change the colour or outfit, Enter starts. **Esc** in
the bank opens a menu (up and down, Enter or Space): Resume, Volume
(left and right turn it in 10% steps, a click a step up; 80% at first,
kept with your look),
Back to the lobby (you leave the round; the others play on) and Quit. A
round ends when the wraith is caught, when everyone in it is dead, or when
everyone has gone back to the lobby; then it's the lobby again, and a new
bank when all are ready. Over the other robbers you see their names.

Inside the game: **Tab** opens the inventory, **1-4** are the quick slots,
**E** picks up a coin, takes back a trap (its trigger with it), takes a tool at the van or seals a downed poltergeist, **P** opens the phone, **T** speaks to the wraith
(once its five letters are found), **F** switches the backpack's flashlight on and off,
**R** reloads the pistol in hand, **Esc** opens the menu, **F5** builds a
new bank (alone), the **mouse wheel** zooms (it starts out as far as it goes,
6.5 tiles above and below the robber), and **F11** toggles full screen.

`cargo test` checks the movement's feel against bims' numbers. It also
checks that on 60 seeds every room is reachable, that the same seed always
gives the same bank, that no opaque box sits where a body can stand, that
every shelf, cabinet and the like against a wall has open floor in front of
it, that sight stops at walls and at its range, and that the wraith can
walk from the street into the vault.

## Looking at it without a window

```bash
BB_SEED=6 BB_SMOKE_FRAMES=150 BB_SCREENSHOT=shot.png ./hidden target/debug/grimgobber
```

`./hidden` runs the game on a headless weston. The `BB_*` variables
(scripted keys, a fixed aim, the spawn tile, the zoom, an unpaced frame-rate
run) are documented at the top of `src/dev.rs`.
