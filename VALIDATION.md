# Incremental validation

This file records a runnable check for each visible project increment. These are
manual acceptance checks rather than the full automated test suite: run the
command, inspect the named behavior, and record bugs against the increment.

OpenBG reads assets from a legally owned game installation. Either pass its path
to the script or set `OPENBG_GAME` once in your shell.

## M2/M3 slice 1 — Candlekeep animation and exploration

Run from the repository root:

```bash
./scripts/validate-area-viewer.sh \
  "/path/to/Baldur's Gate Enhanced Edition" AR2600
```

Or, with `OPENBG_GAME` set:

```bash
./scripts/validate-area-viewer.sh
```

The script launches only `openbg-area` in offline mode. Cargo may perform an
incremental compile when source files changed; it does not run the workspace
test suite.

Check the following:

- Candlekeep's base image fills the area without corrupt or missing tiles.
- Six fountains near the central keep animate continuously.
- Chimney smoke, butterflies, fish, and blue torches animate where visible.
- Fountain and smoke BAMs blend over the area without black rectangles.
- The selected Xvart has a green selection ellipse.
- ARE actors appear as color-coded humanoid/animal markers. Right-clicking one
  makes the Xvart approach it and opens its localized CRE/DLG/TLK conversation
  once in range. Number keys `1`–`9` choose visible replies; `Esc` closes it.
- Keeper of the Portal displays his canonical line beginning “I apologize” and
  an end/continue option rather than generated prototype text.
- Left-clicking a reachable point routes the Xvart around blocked search-map
  cells rather than moving in a straight line through scenery.
- Clicks on castle walls and other blocked terrain are rejected or snapped to
  nearby walkable ground; the Xvart never walks through terrain class 10 walls.
- Black fog initially covers unexplored space and is permanently cleared around
  the Xvart as it moves.
- `F` hides and restores the fog display without erasing explored state.
- `R` shows and hides parsed ARE regions: blue rectangles are travel regions;
  red rectangles are information/trigger regions.
- WASD or arrow keys pan the camera, and the mouse wheel zooms.

Expected limitations for this increment:

- ARE actors use generated markers rather than decoded creature animations.
- Dialogue currently evaluates unconditional, `True()`, and
  `NumTimesTalkedTo` conditions. Replies guarded by other script conditions are
  hidden; dialogue actions and cross-DLG transitions are retained but not yet
  executed.
- Doors, wall occlusion, WED overlay animation, region activation, area
  transitions, party formations, and save/replay are not implemented yet.

Resource-level inspection is available separately:

```bash
cargo run --offline -p openbg-inspect -- \
  "/path/to/Baldur's Gate Enhanced Edition" area AR2600

cargo run --offline -p openbg-inspect -- \
  "/path/to/Baldur's Gate Enhanced Edition" animation FOUNTN

cargo run --offline -p openbg-inspect -- \
  "/path/to/Baldur's Gate Enhanced Edition" creature KEEPER
```
