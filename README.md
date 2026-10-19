# Grimgobber (Rust + Bevy)

A top-down co-op ghost hunt for up to 6 players, at night, in a haunted
bank nobody has built before. Read its signs to learn what kind of wraith
it is, find the notes that spell its name, and catch it the one way its
kind can be caught, before the dark wears your sanity down and it comes
for you.

Grimgobber grew out of the Bank Bobbery remake (`../bankbobbery2`, itself
a remake of the JS heist `../bankbobbery`) and kept its walk, its
generated bank and its ray-traced light and sight. It is its own game now:
there is nothing to rob. The ghost hunters still wear the old robbers'
clothes for now, and the code still calls a player a robber.

## A round

- **The lobby.** Your name (just type), colour and outfit (stripes, hoodie,
  suit or tracksuit), with your hunter as they will look; kept in
  `~/.config/grimgobber/player.txt`. When everyone is ready (alone, Start),
  a new bank. A round ends when the wraith is caught, when everyone in it is
  dead, or when everyone has gone back to the lobby.
- **What is it?** The wraith is a **poltergeist**, a **shade** or a
  **banshee**, and each shows three signs of six: footprints, a door
  slammed, flickering lights, things thrown, frost on the floor, a wail.
  Tick what you see on the **phone (P)**; it says which kinds are left.
- **Its name** has five letters, each written on a note lying somewhere in
  the bank (bring a light: out of sight they are not drawn). Walk over a
  note to read it; the crew's letters show on the HUD, each in its place.
  With all five, **T** opens a box with the name already in it (change it if
  you like), Enter speaks: its name brings it, a long haunt straight at
  you; a wrong word costs you 5% sanity.
- **Catching it.** Each kind is caught its own way, with tools from the
  **van** on the street (E by one):
  - a **poltergeist**: shoot it three times while it shows itself, and
    while it lies still, E seals it in a **coffer**;
  - a **shade**: hold a **UV lantern**'s beam (the left button held) on it
    for 3 seconds while it shows itself;
  - a **banshee**: net it with the **ghost trap**. A left click sets the
    trap and leaves its **trigger** in your hand; a left click on the
    trigger fires a blue net 1.5 tiles round every set trap, which takes a
    banshee that comes into it while it is out. E takes the trap back. A
    banshee goes to noises, so a thrown coin lures it.

  The wrong tool enrages any of them: a hunt straight at you.
- **The end.** Catching it shows how long the round took and a heatmap of
  the bank: where it went (violet) and where the crew went (orange).
  Enter, Esc or a click closes it.

## The wraith

It haunts the bank, unseen, keeping to its room and now and then wandering
out of it. It sees all round (8 tiles where you are lit, by a lamp or your
flashlight, 4 in the dark) and hears what you do. Each time it notices a
hunter (at most every 10 seconds for them, every 4 for anyone), it rolls
against their sanity: a haunt at 3% sane, 53% at none.

- **No haunt: a sign** they can see. Pale footprints walk towards them one
  by one; or a door near them swings open with nobody there and slams; or
  the lights round them flicker.
- **A haunt**: it shows itself, a crawling thing of black hair and
  long-fingered arms, its face thrown back to stare up at you, and the
  lamps and flashlights within 8 tiles of it flicker. After 3 seconds it
  hunts, for 20: it goes for whoever it sees or hears, at a walk, so you
  outrun it sprinting; hide in the plants, crouch out of sight, keep quiet.
  While it haunts, the doors to the outside slam and lock (a red lock on
  them): there is no running out. Its touch kills, and ends the haunt;
  another may not come for 30 seconds.
- **Dead**, you walk on unseen by the living, pale and see-through, and
  touch nothing.
- **Sanity.** Each hunter starts at 100% (a bar beside the quick slots
  shows it). It drains in the bank's rooms, 0.25% a second in a dark one and
  0.05% in a lit one (outside, not at all); a sign of the wraith you see
  costs 5%, being near as a haunt starts 15%.

## The bank

- **It is generated** from a seed (`src/map.rs`). It sits on a city block:
  a road with the crew's van at the kerb, a pavement, a car park and trees.
  Inside are a marble lobby with pillars, a queue and the teller counter, a
  corridor with a side door, and back rooms split by a BSP: open-plan
  office, manager, break room, washrooms, server room, security, archive
  and a steel vault with one door. A flood fill makes sure every room can
  be reached.
- **Light is ray traced per pixel** (`src/shaders/world.wgsl`). Walls and
  tall furniture (cabinets, shelves, racks, pillars, the van, trees) cast
  exact shadows away from each lamp. The lamps stand still, so their light
  is traced once into a light map, again only when a door moves. Your
  **flashlight** is a light like any other: a cone from your shoulder, 15
  tiles long, with a soft edge, throwing shadows away from you. Glass lets
  light through.
- **Sight is ray traced too.** You see only what your hunter could see:
  walls and tall furniture block it, glass doesn't. You see 6 tiles into a
  room with its lights on (and outside), 3.75 into a dark one, and the full
  6 inside the flashlight's beam. You can zoom out only as far as you can
  see. Anything out of sight is drawn under a grey-blue fog.
- **Doors** open as you reach them, half a tile off: you put an arm out and
  push, the door swings away, and it swings shut behind you with a flick of
  the hand. The glass entrance opens on a sensor instead, from further off.
  Shut, they stop sight and lamplight like a wall (the glass entrance only
  like glass). Inside doors are wood, the side doors steel, double doors
  hinge at their outer edges. Out of your sight a door stays drawn as you
  last saw it.
- **Lights.** A switch by a door of each lit room turns its lamps on and
  off (E). Every lamp on drains the bank's generator, which slowly
  recovers: leave them all on and in about three minutes the breaker trips
  and every lamp inside goes out. Reset it at the fuse box (in a different
  room each bank) once the generator has recovered a little.
- **Half-height cover.** Desks, tables, the counter, sofas and parked cars
  are half height. Crouched behind one you are in cover (the HUD says so),
  you see only 2 tiles past it, though walls and tall things beyond still
  show over it, and the wraith can't see you there.
- **Plants to hide in.** Big potted palms stand in the lobby and offices,
  planters of tall, dense plants line some walls, and bushes grow outside.
  Walk into one, at half pace through the leaves, and you are hidden (the
  HUD says so): the wraith sees nobody through one, and someone in one only
  from right beside it. If it saw you go in, a hunt follows you there.

## Moving and gear

- **Movement** is bims_bevy's steered walk, ported number for number
  (`src/movement.rs`). WASD walk, the mouse aims (the body faces the pointer
  at once, and walking away from it strides backwards), Shift sprints
  (1.8×, eased in over 0.1 s), and Space or Alt dodge-rolls (195 units in
  0.38 s, 1.1 s cooldown). C crouches and stands: a little over half speed,
  no sprint and no sound. F switches the flashlight strapped to your
  backpack on and off, whatever you hold. Letting go stops you dead. Lone
  corners are rounded, so you slide round door frames instead of snagging
  on them.
- **An inventory**, DayZ style (Tab): one 8x5 grid, each item taking a
  block of cells (crowbar 4x1, pistol 2x1, ghost trap and UV lantern 2x2,
  coffer 3x2). Drag items about with the mouse, R turning the one you hold;
  drop one on the hands to hold it. Quick slots 1-4 along the bottom take
  their item into your hands or put it away (drop an item on a slot to bind
  it, right-click to clear it). The game runs on while it is open; WASD
  still walks.
- **Coins.** You carry three. In hand, a left click throws one at the
  pointer, up to 8 tiles, short of anything solid in the way: a gold ring
  shows how far you can throw and a marker where it would land. It lands
  with a pling heard 7 tiles off (shown by a faint ring). Walk over a coin
  on the floor and E picks it up again.
- **The pistol** (8 in the magazine, 16 rounds besides) aims at the pointer
  as in bims, the arm swinging the barrel onto it, a reticle on it (grey
  past its 20 tiles); a left click fires (not while sprinting or rolling),
  R reloads. A shot stops at a wall, a window, a tall thing, a shut door or
  the first body in its way, and is heard 26 tiles off.
- **The crowbar.** In hand, a left click swings it; a window right in front
  of you breaks into an opening you can climb through.
- **Hearing.** The wraith hears what you do: a smashed window carries 14
  tiles, your feet sprinting 6, rolling 3, walking 2.5, a door you open or
  shut 4; crouched, you make no sound at all. A noise spreads across the
  floor, round furniture, down corridors and through open doors, and loses
  4 tiles of its reach to every wall or shut door it passes.
- **Sound**: doors and footsteps from the old Bank Bobbery, a window
  smashing and a coin's pling synthesised (`sounds/*.py`), the old game's
  pistol sounds, and the air: a temperate night outside, a deep hum inside.
  Each is quieter with distance and quieter still through a wall. The
  wraith has no sounds of its own yet.
- **All the graphics are its own.** Floors and walls are painted
  procedurally in the shader; the furniture, the hunters and the wraith are
  vertex-coloured meshes built in code (`src/paint.rs`). There are no image
  assets; the sounds (`sounds/`, Ogg) are built into the binary.

## Run it

```bash
nix-shell            # rust, cargo, the Vulkan/Wayland/X11 runtime libs, weston
cargo run            # the title: join, host or play alone
BB_SEED=1234 cargo run
```

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
is where; your own hunter moves on your machine at once. Online, a
smashed window and a coin on the floor are seen only by whoever smashed
or threw it.

### Keys

**Tab** the inventory, **1-4** the quick slots, **E** picks up a coin,
takes back a trap (its trigger with it), takes a tool at the van, flips a
light switch or seals a downed poltergeist, **P** the phone, **T** speaks
to the wraith (once its five letters are found), **F** the flashlight,
**R** reloads the pistol in hand, **F5** a new bank (alone), the **mouse
wheel** zooms (it starts out as far as it goes), **F11** full screen.
Over the other hunters you see their names.

**Esc** opens a menu (up and down, Enter or Space): Resume, Volume (left
and right in 10% steps, a click a step up; 80% at first, kept with your
look), Back to the lobby (you leave the round; the others play on) and
Quit.

### The simulation

```bash
cargo run -- --sim                 # a bank to try the wraith in
BB_SEED=6 BB_SANITY=0 cargo run -- --sim
```

No lobby and nobody in the bank: a free camera that sees everything (WASD
pans, Shift faster, the wheel zooms), and the wraith always drawn, pale
while it does not show itself; a box at the bottom right says what it is
up to. **H** puts a hunter at the pointer, carrying every tool, and gives
them to you (the one before stays where they stood, as bait), **V**
switches between the free camera and following them, **F5** builds a new
bank. The round never ends: dead, spawn another.

## Tests and looking without a window

`cargo test` checks the movement's feel against bims' numbers. It also
checks that on 60 seeds every room is reachable, that the same seed always
gives the same bank, that no opaque box sits where a body can stand, that
every shelf, cabinet and the like against a wall has open floor in front of
it, that sight stops at walls and at its range, that the wraith can walk
from the street into the vault, and that three signs always name its kind.

```bash
BB_SEED=6 BB_SMOKE_FRAMES=150 BB_SCREENSHOT=shot.png ./hidden target/debug/grimgobber
```

`./hidden` runs the game on a headless weston. The `BB_*` variables
(scripted keys, a fixed aim, the spawn tile, the zoom, an unpaced frame-rate
run) are documented at the top of `src/dev.rs`.
