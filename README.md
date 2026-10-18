# Bank Bobbery (Rust + Bevy)

A top-down heist on foot, at night, in a bank nobody has built before. This
is the start of a Rust/Bevy remake of `../bankbobbery`. So far it is only
movement, light and one guard to hide from.

- **Movement** is bims_bevy's steered walk, ported number for number
  (`src/movement.rs`). WASD walk, the mouse aims (the body faces the pointer
  at once, and walking away from it strides backwards), Shift sprints
  (1.8×, eased in over 0.1 s), and Space or Alt dodge-rolls (195 units in
  0.38 s, 1.1 s cooldown). C crouches and stands: a little over half speed
  and no sprint, folded forward with his knees out in front. A roll goes
  head over heels. F switches the flashlight on and off. A hand with
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
- **Light is ray traced per pixel** (`src/shaders/world.wgsl`). Every
  pixel traces rays through the tile grid to each lamp in reach, and the
  walls and tall furniture (cabinets, shelves, racks, pillars, the van,
  trees) cast exact shadows away from that lamp. The robber's
  **flashlight** is a light like any other: a cone from his hand, 15 tiles
  long, with a soft edge, throwing shadows away from him. With it off he
  throws none. Glass lets light through.
- **Sight is ray traced too.** You see only what the robber could see:
  walls and tall furniture block it, glass doesn't. You see 12 tiles into a
  room with its lights on (and outside), 5 into a dark one. Inside the
  flashlight's beam you see the full 12 tiles, as in a lit room. Anything
  out of sight is drawn under a grey-blue fog.
- **Doors** open as you reach them, half a tile off: the robber puts his
  arm out and pushes, the door swings away from him, and it swings shut
  behind him with a flick of his hand. The glass entrance opens on a sensor
  instead, from further off. Shut, they stop sight and lamplight like a wall (the
  glass entrance only like glass). Inside doors are wood, the side doors
  steel, double doors hinge at their outer edges. Out of your sight a door
  stays drawn as you last saw it: you don't see a guard open one in the fog.
- **Half-height cover.** Desks, tables, the counter, sofas and parked cars are half
  height. Crouched behind one you are in cover (the HUD says so), and you
  see only 2 tiles past it, though walls and tall things beyond still show
  over it, and a guard can't see you there.
- **A guard** stands in the security room (or the lobby). You don't play
  him: press T, then left-click, and he walks there by the shortest way,
  opening doors as he goes. He sees only ahead, in a 90° cone the way he
  last walked, 2 tiles less far than the robber where it is lit (10) and
  as far in the dark (5), so you can spot him first. Where your flashlight
  shines counts as lit for him, and so do you while it is on. His cone is washed
  light red wherever you can see it, shadowed by whatever blocks his view;
  while you crouch, the floor where you would be hidden behind cover is
  left clear.
  Sight works both ways: crouched in cover, you are hidden from him exactly
  when he is hidden from you. While he sees you, a red **!** hangs over the
  robber, he points at you, and he gives chase at a walk, stopping when he is on you. Out of
  his sight, he still knows where you are for a second and keeps coming,
  round corners and through doors. After that he heads for where you
  were, which a pale ghost of the robber marks, and on up to 2 tiles the way you were going; there he
  searches, sweeping 90° either way of it once, so rolling past him only
  buys a moment. Otherwise, standing, he looks 10° left and right of the
  way he stopped, and back, over 5 seconds. Out of your sight he is gone,
  not drawn under the fog like the furniture.
- **An inventory**, DayZ style (Tab): one 8x5 grid, each item taking a
  block of cells (crowbar 4x1, flashlight 2x1, lockpicks 1x1). Drag items
  about with the mouse, R turning the one you
  hold; drop one on the hands to hold it. Quick slots 1-4 along the bottom
  take their item into his hands or put it away (drop an item on a slot to
  bind it, right-click to clear it). What he holds is what he carries in his
  hands: the flashlight shines only in hand, the crowbar shows only when
  held. The game runs on while it is open; WASD still walks.
- **Coins.** He carries three (one stack, on quick slot 4). In hand, a
  left click throws one at the pointer, up to 8 tiles, short of anything
  solid in the way: a gold ring shows how far you can throw and a marker
  where it would land. It lands with a pling a guard hears 7 tiles off (the
  same spread as any noise, shown by a faint ring), and he goes to look and
  pockets it. Walk over a coin on the floor and E picks it up again.
- **Going through a guard's pockets.** Open the inventory (Tab) next to a
  guard you knocked out and a VICINITY panel shows what he has, such as the
  coins he picked up: drag a thing into your grid or hands (coins join your
  stack), or right-click or ctrl-click it to take it; drag one of yours
  onto the panel, or ctrl-click it, to leave it with him.
- **Guns.** The thug carries a pistol (8 in the magazine, on quick slot 3)
  and 16 rounds. In hand it aims at the pointer as in bims, the arm
  swinging the barrel onto it, a reticle on it (grey past its 20 tiles); a
  left click fires (not while sprinting or rolling), R reloads from the
  rounds. A shot stops at a wall, a window, a tall thing or a shut door, or
  at the first body in its way, and is heard 26 tiles off. Three hits put a
  guard down for good (his pockets can still be gone through). A guard
  draws his own only once you are armed (your pistol in hand, or a shot
  fired in the last 20 seconds) and he sees you; then he fires every 0.8
  seconds, not quite straight. You have 3 health (the HUD shows it, the
  screen flashes red when you are hit); at 0 nothing ends yet.
- **Smashing windows.** With the crowbar in hand, a left click swings it;
  a window right in front of you breaks into an opening you can climb
  through, glass strewn on the floor and teeth of it left in the frame. You
  and the guard both see and walk through it.
- **Hearing.** The guard hears what you do: a smashed window carries 14
  tiles, your feet sprinting 6, rolling 3, walking 2.5, a door you open or
  shut 4; crouched, you make no sound at all, at doors neither. A noise spreads across the floor, round
  furniture, down corridors and through open doors, and loses 4 tiles of
  its reach to every wall or shut door it passes. A smashed window shows
  how far (and so does a coin landing): a faint ring sweeps out to exactly
  where a guard would hear it.
  Unless he is after you already, a guard who hears it walks over to look
  and searches there; a yellow **?** marks what he heard, and the HUD says so.
- **Knocking a guard out.** Right behind him (within about a tile, in the
  back half of where he faces) and unseen, E knocks him out: he drops and
  lies on the floor, out cold, for 30 seconds, a pale "Zz" over him. Out,
  he sees and hears nothing and forgets whom he was after; then he gets up
  and carries on. A prompt over him ("E Knock out") says when you can, and
  it lands with a punch and the thud of him hitting the floor.
- **Plants to hide in.** Big potted palms stand in the lobby and offices,
  their fronds over you as you pass, and planters of tall, dense plants
  line some walls (the lobby, the corridor, the open office). Walk into one,
  at half pace through the leaves, and you are hidden (the HUD says so): a guard does not see you there
  unless he was watching you as you went in, or is standing in it with you.
- **Sound**, from `../bankbobbery`: doors opening and shutting, and
  footsteps for the robber (soft crouched) and the guard; and a window
  smashing, synthesised (`sounds/glass.py`); and a knockout, from bims2's
  punch; a coin's pling, synthesised (`sounds/coin.py`); and the old game's
  pistol shot, reload, holster and unholster. Each is quieter
  with distance and quieter still through a wall, so you hear a guard
  before you see him.
- **All the graphics are its own.** Floors and walls are painted
  procedurally in the shader. The furniture and the robber (striped shirt,
  beanie, mask, swag bag, crowbar) and the guard (peaked cap, reflective
  band, radio, holster) are vertex-coloured meshes built in
  code (`src/paint.rs`). There are no image assets; the sounds
  (`sounds/`, Ogg, cut from the old game's samples or synthesised) are
  built into the binary.

## Run it

```bash
nix-shell            # rust, cargo, the Vulkan/Wayland/X11 runtime libs, weston
cargo run            # a random bank
BB_SEED=1234 cargo run
```

### Together

Up to 6 play one bank. One machine runs the server, headless; everyone
else connects to it (UDP, port 8794 unless given):

```bash
cargo run -- --server              # or --server 9000
cargo run -- --connect buggly.de   # or --connect 192.168.1.5:9000
```

The server runs the guard and the doors and says who is where; your own
robber moves on your machine at once. Online, the crowbar, coins, the
pistol and the knockout do nothing yet, and F5 is the server's.

Inside the game: **Tab** opens the inventory, **1-4** are the quick slots,
**E** knocks out a guard you are right behind,
**F** switches the flashlight (in hand) on and off, **T** then a
left click sends the guard somewhere, **R** reloads the pistol in hand, **F5** builds a
new bank, the **mouse wheel** zooms (it starts out as far as it goes,
12.5 tiles above and below the robber), and **F11** toggles full screen.

`cargo test` checks the movement's feel against bims' numbers. It also
checks that on 60 seeds every room is reachable, that the same seed always
gives the same bank, that no opaque box sits where a body can stand, that
every shelf, cabinet and the like against a wall has open floor in front of
it, that sight stops at walls and at its range, and that a guard can walk
from the street into the vault.

## Looking at it without a window

```bash
BB_SEED=6 BB_SMOKE_FRAMES=150 BB_SCREENSHOT=shot.png ./hidden target/debug/grimgobber
```

`./hidden` runs the game on a headless weston. The `BB_*` variables
(scripted keys, a fixed aim, the spawn tile, the zoom, an unpaced frame-rate
run) are documented at the top of `src/dev.rs`.
