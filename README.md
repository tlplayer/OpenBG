# OpenBG

OpenBG is a clean-room, data-driven reimplementation of the BioWare Infinity
Engine for Baldur's Gate. It uses **Rust** for deterministic game rules and
resource handling, **Lua** for moddable behavior, and **Bevy** for platform,
rendering, audio, input, and tooling.

The long-term target is observable 1:1 compatibility with Baldur's Gate game
play while loading assets from a legally owned game installation. OpenBG does
not ship copyrighted game data.

> Status: architecture and format-spike stage. The Rust files currently in
> `engine/` are prototypes; the workspace described below is the target layout.

## Area viewer

The first vertical slice loads an Enhanced Edition area through KEY/BIFF,
decodes its WED and palette/PVRZ-backed TIS resources, and displays the composed
base layer in a Bevy window:

```bash
cargo run -p openbg-area -- \
  "/path/to/Baldur's Gate Enhanced Edition" AR2600
```

`AR2600` is the default area when the second argument is omitted. Pan with WASD
or the arrow keys and zoom with the mouse wheel. The viewer currently renders
only the static base layer; overlays, animated tiles, doors, walls, actors,
lighting, and fog belong to later slices.

## Goals

- Preserve Infinity Engine behavior where it affects saves, combat, movement,
  dialogue, quests, timing, or presentation.
- Import an original installation into a documented, validated `data/` format.
- Keep game rules independent from Bevy so simulations can run headlessly.
- Make content, AI, and UI behavior extensible through versioned Lua APIs.
- Support layered overrides without mutating the imported base data.
- Prove parity with repeatable fixtures, golden renders, and replay tests.

“1:1 parity” means matching externally observable behavior, not reproducing the
original executable's internal design or every historical bug. Compatibility
quirks that content depends on are explicit, named, tested, and selectable by a
rules profile.

## Architecture

OpenBG is split into three pipelines:

```text
owned game install                 OpenBG runtime
KEY/BIFF + override + TLK          data/ + mods/
          |                              |
          v                              v
   openbg-import --------------> ResourceCatalog
   parse / validate / convert      (layered, read-only)
                                         |
                                         v
                               deterministic Simulation
                                rules / state / commands
                                  |              ^
                            RenderFrame       Lua events
                                  |              |
                                  v              v
                              Bevy shell      ScriptHost
                         render/audio/input/UI
```

The importer knows Infinity Engine file formats. The simulation knows canonical
OpenBG domain types. Bevy knows how to present snapshots and turn player input
into commands. Lua observes events and requests validated commands; it never
receives unrestricted access to Bevy's ECS or the filesystem.

### Dependency rule

Dependencies point inward:

```text
apps -> bevy adapter / importer / scripting -> simulation -> domain
                                                   ^
formats -------------------------------------------|
```

`openbg-domain` and `openbg-sim` must not depend on Bevy, a renderer, a window,
or an original asset format. This makes deterministic tests fast and prevents
engine upgrades from rewriting the rules layer.

## Target workspace

```text
OpenBG/
├── Cargo.toml                     # Cargo workspace and shared dependencies
├── crates/
│   ├── openbg-domain/             # IDs, fixed-point values, canonical data
│   ├── openbg-formats/            # bounded binary readers for Infinity files
│   ├── openbg-import/             # install discovery and canonical conversion
│   ├── openbg-catalog/            # manifests, override layers, typed loading
│   ├── openbg-sim/                # clock, commands, rules, world and save state
│   ├── openbg-script/             # sandboxed Lua VM and versioned host API
│   ├── openbg-bevy/               # presentation ECS, rendering, audio and input
│   ├── openbg-ui/                 # UI models and screen composition
│   └── openbg-testkit/            # fixtures, replays, hashes and golden helpers
├── apps/
│   ├── openbg/                    # game executable
│   ├── openbg-import/             # installation converter CLI
│   ├── openbg-inspect/            # resource/area/dialogue inspection CLI
│   └── openbg-replay/             # headless deterministic replay runner
├── lua/
│   ├── bootstrap.lua
│   ├── ai/
│   ├── rules/
│   └── ui/
├── schemas/                       # canonical format and Lua API versions
├── tests/
│   ├── fixtures/                  # tiny synthetic, redistributable assets
│   ├── format/                    # parser and malformed-input tests
│   ├── parity/                    # expected rules outcomes and replays
│   └── golden/                    # approved images/audio metadata
├── docs/
│   ├── architecture.md
│   ├── formats.md
│   ├── lua-api.md
│   └── parity-matrix.md
└── data/                           # generated; ignored by Git
```

Each crate owns one reason to change. Cross-crate shared types belong in
`openbg-domain` only when they are genuinely part of the stable domain model;
it must not become a miscellaneous utilities crate.

## Core interfaces

These are architectural contracts, not final syntax. They keep format parsing,
resource resolution, simulation, scripting, and presentation replaceable.

```rust
pub struct ResourceId {
    pub resref: ResRef,             // validated, normalized Infinity identifier
    pub kind: ResourceKind,
}

pub trait ResourceLayer: Send + Sync {
    fn metadata(&self, id: &ResourceId) -> Result<Option<ResourceMeta>>;
    fn read(&self, id: &ResourceId) -> Result<Option<bytes::Bytes>>;
}

pub trait ResourceCatalog: Send + Sync {
    fn resolve(&self, id: &ResourceId) -> Result<ResolvedResource>;
    fn dependencies(&self, id: &ResourceId) -> Result<Vec<ResourceId>>;
}

pub trait Decoder: Send + Sync {
    type Output;
    fn decode(&self, input: &[u8], context: &DecodeContext)
        -> Result<Self::Output, FormatError>;
}
```

Layers resolve in a deterministic order: session overrides, user mods, DLC or
expansion, imported game override, then imported base archives. Every resolved
resource records its source layer and content hash for diagnostics and saves.

The simulation accepts commands and emits facts; presentation never mutates
world state directly:

```rust
pub trait GameSimulation {
    fn tick(&mut self, input: &[GameCommand]) -> TickOutput;
    fn snapshot(&self) -> WorldSnapshot;
    fn save(&self) -> SaveGame;
    fn load(&mut self, save: SaveGame) -> Result<(), LoadError>;
}

pub struct TickOutput {
    pub tick: GameTick,
    pub events: Vec<GameEvent>,
    pub checksum: StateChecksum,
}

pub enum GameCommand {
    Move { actors: Vec<EntityId>, destination: Point },
    Attack { actor: EntityId, target: EntityId },
    UseAbility { actor: EntityId, ability: ResRef, target: Target },
    SelectDialogueReply { conversation: ConversationId, reply: ReplyId },
    Pause(bool),
}
```

Use stable game IDs rather than Bevy `Entity` values in saves, scripts, events,
and tests. Use integer or fixed-point quantities and an explicit game tick for
rules-critical time, positions, probabilities, and effect durations. Floating
point is acceptable in presentation interpolation, not authoritative outcomes.

## Import and canonical data

`openbg-import` is a separate executable. It never changes the original install.
It discovers the edition, reads `chitin.key`, BIFF archives, `override/`, TLK
strings, movies, music, and configuration, then writes canonical data plus a
manifest.

```text
data/
├── manifest.json                  # schema, source edition, hashes, provenance
├── catalog.bin                    # ResourceId -> canonical object/blob
├── objects/                       # versioned MessagePack/bincode domain records
│   ├── areas/
│   ├── creatures/
│   ├── items/
│   ├── spells/
│   ├── dialogues/
│   ├── scripts/
│   └── rules/
├── textures/                      # decoded/atlas-ready image data
├── audio/                         # normalized streams and metadata
├── strings/                       # localized string tables
└── raw/                           # optional copied bytes for inspection/debugging
```

Import stages are resumable and content-addressed:

1. Discover and fingerprint the installation.
2. Index KEY/BIFF and loose overrides without decoding them.
3. Parse with strict bounds checks and useful byte-offset errors.
4. Resolve cross-resource references and report missing dependencies.
5. Convert into versioned canonical objects.
6. Validate invariants and emit a machine-readable report.
7. Build texture/audio caches and atomically publish the manifest.

Keeping source bytes is optional. Canonical records always include their schema
version, source `ResourceId`, source hash, and converter version. A changed
source or converter invalidates only affected records.

Initial format priority:

| Capability | Formats |
| --- | --- |
| Resource index and text | KEY, BIFF/BIF, TLK, 2DA, IDS |
| Area rendering | ARE, WED, TIS, MOS, BAM, BMP |
| Actors and inventory | CRE, ITM, SPL, EFF, PRO |
| Rules and behavior | BCS/BS, DLG, GAM, INI |
| Save compatibility | SAV, GAM, ARE, CRE, STO, WMP |
| Audio and movies | WAV/WAVC, ACM, MUS, MVE as edition requires |

Parsers retain unknown fields where round-tripping or compatibility requires
them. Unsupported variants fail explicitly; they do not silently invent values.

## Simulation modules

`openbg-sim` is a deterministic state machine composed from narrow systems:

- `clock`: pause, rounds, turns, animation time, timers, and time-of-day.
- `world`: areas, actors, ownership, transitions, fog and global state.
- `movement`: search maps, pathfinding, collision, formations and doors.
- `combat`: initiative, attack rolls, damage, death and combat log facts.
- `effects`: opcode registry, stacking, dispelling, timing and saving throws.
- `abilities`: memorization, casting, projectiles, item use and targeting.
- `ai`: triggers, object selectors, responses, action queues and interrupts.
- `dialogue`: state selection, conditions, replies, journal and side effects.
- `inventory`: slots, stacking, containers, stores, identification and charges.
- `party`: selection, reputation, experience, rest and character progression.
- `travel`: world map, encounters, transitions and chapter progression.
- `save`: stable serialization, migrations and deterministic restore.

Rules that differ between BG1, Tales of the Sword Coast, and later targets live
behind a `RulesProfile`; scattered edition checks are not allowed.

## Lua boundary

Lua extends policy, not engine invariants. Rust owns resource validation,
simulation state, effect execution, serialization, and determinism. Lua is well
suited to AI strategies, encounter orchestration, mod rules, and UI behavior.

Scripts receive immutable event payloads and return commands:

```lua
function on_event(ctx, event)
  if event.kind == "enemy_seen" and ctx:self():can_act() then
    return { command.attack(event.enemy_id) }
  end
  return {}
end
```

The host API is namespaced and versioned (`openbg.api.v1`). Each invocation has
an instruction budget, deterministic random stream, structured diagnostics, and
declared capabilities. Wall-clock time, ambient randomness, network access,
native libraries, arbitrary files, and direct ECS access are unavailable.

Script state that survives a save must be plain, schema-checked data owned by
the simulation. Lua VM internals are never serialized.

## Bevy integration

Bevy is an adapter around the game, not the source of game truth.

- A fixed schedule advances the simulation only when the game clock allows it.
- Input systems translate keyboard, mouse, and controller gestures into
  `GameCommand` values.
- Extraction systems turn `WorldSnapshot` and `GameEvent` values into a small
  `RenderFrame`/`AudioCue` presentation model.
- Presentation entities may be freely spawned, interpolated, or rebuilt because
  stable simulation IDs remain authoritative.
- Loading and rendering use Bevy asset handles only inside `openbg-bevy`.

The first renderer should favor correctness and inspection tools over effects:
orthographic isometric camera, WED/TIS tile layers, palette/alpha behavior,
doors and wall occlusion, BAM actors, selection circles, fog, cursor, and UI.

## Testing and parity

Parity is an evidence problem. Every implemented behavior needs a fixture or a
captured reference, not just a subjective play test.

- **Parser tests:** valid, truncated, malformed, and fuzz-generated source data.
- **Conversion tests:** canonical output hashes from synthetic fixtures.
- **Rule unit tests:** table-driven attack, effect, trigger, and timing cases.
- **Replay tests:** seed + initial state + commands -> per-tick state checksums.
- **Golden render tests:** fixed camera snapshots with tolerance masks.
- **Integration tests:** load area, pathfind, fight, converse, transition, save,
  reload, and confirm the same checksum.
- **Differential tests:** record the original game's observable outcome and
  compare it to an OpenBG replay where legally and technically practical.

`docs/parity-matrix.md` will track each subsystem as `unknown`, `documented`,
`parsed`, `simulated`, `presented`, and `verified`, including fixture IDs and
known compatibility quirks. CI uses only synthetic or user-supplied data.

## Milestones

Each milestone produces a runnable vertical slice and has an objective exit
test. Later milestones may refine earlier parsers, but may not bypass their
boundaries.

### M0 — Buildable foundation

- Create the Cargo workspace, crate boundaries, error types, logging, CI, and
  synthetic fixture policy.
- Define `ResRef`, `ResourceId`, manifest versions, ticks, stable entity IDs,
  commands, events, and checksums.
- Exit: all crates build; a headless simulation runs 10,000 identical seeded
  ticks twice and produces the same checksum.

### M1 — Installation index and importer

- Discover supported installs; parse KEY/BIFF, TLK, 2DA, IDS, and overrides.
- Generate a deterministic catalog and validation report without altering the
  source install.
- Exit: inspect/list/extract known resources from a user-supplied BG install;
  repeated imports produce byte-identical manifests.

### M2 — Area viewer

- Parse WED/TIS/ARE/MOS/BAM; render base/overlay tiles, actors, doors, walls,
  occlusion, selection circles, and camera controls in Bevy.
- Exit: selected reference areas match approved golden screenshots and can be
  inspected resource-by-resource.

### M3 — Exploration slice

- Add search maps, deterministic pathfinding, collision, fog, doors, regions,
  area transitions, time, party selection, and formations.
- Exit: a replay walks a party through two connected areas, triggers a region,
  saves, reloads, and ends with the same checksum.

### M4 — Actors, inventory, and combat

- Parse CRE/ITM/SPL/EFF/PRO; implement stats, equipment, attack loop, death,
  core effect opcodes, projectiles, spell/item use, and combat feedback.
- Exit: table-driven reference encounters agree on rolls, damage, effects,
  inventory, deaths, experience, and final checksums.

### M5 — AI and Lua

- Implement Infinity triggers, object selectors, response blocks, action queues,
  and interruption semantics; expose the versioned sandboxed Lua command API.
- Exit: original AI fixtures and equivalent Lua behaviors pass deterministic
  traces; budget exhaustion produces a diagnostic without corrupting state.

### M6 — Dialogue, quests, stores, and travel

- Parse DLG/STO/WMP; add dialogue conditions/actions, journals, reputation,
  containers, stores, rest, world travel, encounters, and chapter state.
- Exit: a scripted quest crosses areas, branches dialogue, purchases an item,
  updates the journal, travels, and survives save/reload deterministically.

### M7 — UI, audio, and complete game loop

- Implement character creation, record/inventory/spell screens, options,
  portraits, tooltips, localized text, music, ambience, voices, and movies.
- Exit: start a new game and complete a chosen BG1 vertical chapter using only
  OpenBG, with automated smoke replays for its critical path.

### M8 — BG1 parity and mod platform

- Close the parity matrix, fill required effect/action/trigger coverage, add
  compatibility profiles, layered mods, Lua packaging, diagnostics, and stable
  save migrations.
- Exit: the BG1 campaign is completable; the supported parity matrix has no
  unexplained blockers; long replays and save round-trips pass on CI.

BG2 support begins only after BG1 behavior is measured and stable. It should add
format/rules profiles and features, not fork the engine.

## Engineering rules

- Parse untrusted binary data with explicit endianness, checked offsets, bounded
  allocation, and contextual errors. Fuzz every binary decoder.
- Do not use `unwrap`/`expect` on imported data paths.
- Do not introduce a global service locator. Pass catalog, clock, rules, random
  stream, and command/event interfaces explicitly.
- Keep authoritative iteration order stable; never base outcomes on `HashMap`
  order or frame timing.
- Preserve provenance and diagnostics across import, load, and script layers.
- Prefer a complete vertical slice over many half-parsed formats.
- Pin toolchain and dependency versions; upgrade Bevy behind adapter tests.
- Treat saves, canonical data, Lua APIs, and mod manifests as versioned public
  formats with migrations and compatibility tests.

## Near-term work

The next implementation step is **M0**, followed by a narrow M1 spike:

1. Move the current prototypes into a Cargo workspace without preserving broken
   APIs merely for compatibility.
2. Implement and test `ResRef`, `ResourceId`, `ResourceKind`, checked binary
   reading, and layered catalog resolution.
3. Parse only enough KEY/BIFF to list and extract a resource, then add TLK.
4. Create synthetic mini-archives so CI and contributors need no game data.
5. Build `openbg-inspect list|get|deps` before starting the Bevy viewer.

That sequence gives the renderer trustworthy inputs and gives every later
milestone a deterministic test seam.
