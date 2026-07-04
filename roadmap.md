# OpenBG feature-first roadmap

OpenBG implements file formats to unlock observable game features, not to grow
a collection of disconnected parsers. A format is **accounted for** when OpenBG
can identify and safely decode the subset currently needed. It is **done** only
when that data drives a visible or audible in-game behavior and that behavior
has a repeatable regression test.

The primary format reference is the
[Infinity Engine Structures Description Project](https://gibberlings3.github.io/iesdp/).
Real-game validation currently targets Baldur's Gate: Enhanced Edition and the
Candlekeep prologue (`AR2600`). Compatibility work must retain explicit edition
and version boundaries rather than silently treating all Infinity Engine games
as identical.

## Definition of done for a format slice

Every format slice should pass through the same vertical seam:

1. **Identify:** map the correct KEY resource type or loose-file extension to a
   `ResourceKind`. Preserve unknown type codes.
2. **Parse:** add a bounded parser with synthetic valid, truncated, malformed,
   excessive-count, and unsupported-version fixtures. Retain source identifiers
   in diagnostics.
3. **Inspect:** resolve a real resource through `GameInstall` and expose enough
   information through `openbg-inspect` to diagnose it without the renderer.
4. **Convert:** produce renderer- and storage-independent content or simulation
   data. Format structs must not leak into authoritative game rules.
5. **Consume:** connect that data to one narrowly defined playable feature.
6. **See it in game:** exercise a named scenario in `openbg-area` or the future
   game executable and record the expected visible/audible/state change.
7. **Regress:** add the cheapest stable automated check: unit test, simulation
   checksum, CLI snapshot, golden image, or save/load round trip.

CLI output alone proves parsing and catalog integration, but it does **not**
mark a format done. A screenshot alone is also insufficient without a
repeatable scenario and an automated lower-level test.

## Currently accounted-for formats

These formats already have a code path. “Accounted for” does not imply complete
coverage of every version or field.

| Format | Current slice | Important remaining work | In-game evidence |
| --- | --- | --- | --- |
| KEY | Resource index and BIF locator mapping | Add remaining runtime `ResourceKind` mappings and override precedence | AR2600 resources resolve from an installed game |
| BIF/BIFF | File and tileset extraction | Additional archive variants and catalog diagnostics | AR2600 art and actors load from archives |
| TLK | String lookup | Alternate language/encoding coverage and external string patches | NPC names and dialogue text appear |
| ARE | Actors, regions, named entrances, travel destinations, and area animations | Doors, containers, polygonal triggers, traps, embedded CREs, scripts, rest/spawn data, seamless streaming | Candlekeep travel regions launch the destination area and place the player at the named entrance without bouncing back |
| WED | Base overlay and navigation-related geometry subset | Doors, wall polygons, occlusion, secondary overlays | Candlekeep base layout renders |
| TIS | Palette tiles and PVRZ-backed EE tile composition | Animated/secondary tiles and broader edition coverage | Candlekeep ground art renders |
| PVRZ | Decoding through the TIS composition path | General standalone page service, cache, and diagnostics | EE Candlekeep tiles render |
| BAM | V1/BAMC frames, cycles, indexed palettes, character recoloring | BAM V2, more animation families, equipment layers, timing metadata | Actors and area animations render; CRE colors are visible |
| BMP | 4/8-bit indexed and 24-bit images | Other required BMP variants only when encountered | Search map drives movement; `MPAL256` drives avatar colors |
| CRE | Names, dialogue, animation ID, colors, five script slots, items, and inventory slots | Stats, full equipment behavior, spellbook, effects, death variables, version variants | The prototype player uses a remapped humanoid CRE and remains idle; player inventory is visible with `I`, NPC inventory with `O` |
| DLG | States, transitions, source triggers/actions, reaction-gated reply selection | Full trigger/action execution, journal semantics, cross-dialogue behavior | Winthrop's visible responses and store route change according to computed player reaction |
| 2DA | Plain-text V1.0 tables, defaults, lookup, catalog loader, CLI inspection, reaction modifiers | Encrypted legacy tables, additional typed rule consumers, override layering | `STARTARE.2DA` places the actor; `RMODREP.2DA` and `RMODCHR.2DA` select reputation/charisma dialogue branches |
| IDS | Headered/count-prefixed tables, decimal/hex values, aliases, signatures, reverse lookup | Broader table validation and use throughout the script/effect/rule systems | `TRIGGER.IDS` and `ACTION.IDS` symbolically select the supported live script action |
| BCS | Compiled envelope, condition/response blocks, trigger/action call indexing | Typed parameters, objects, response weights, full VM, BS catalog loader | `WDASIGHT.BCS` executes its unconditional `RandomWalk()` on Candlekeep watchers |
| ITM | V1 gameplay header, icons, appearance, price/weight, ability combat subset | Effects, usability, all versions, equipment overlays and full inventory rules | A persistent player inventory screen appears with `I`; `O`/`E` inspect and exercise NPC equipment transitions |
| STO | V1 store header, flags, markups, accepted categories, finite/infinite stock, ITM/TLK resolution | Depreciation, reputation/charisma pricing, identification, stealing, drinks, rooms, cures, persistence, other versions | Winthrop's real `StartStore("INN2616")` action is labeled and opens a fixed buy/sell overlay linked to player inventory, mutable gold, and stock |

## Unaccounted runtime formats: recommended implementation order

This order is feature-first. Each row should be implemented only far enough to
complete its named in-game slice, then expanded as later features demand it.

| Order | Format | Why it is interesting | General implementation | Required in-game acceptance test |
| ---: | --- | --- | --- | --- |
| 1 | GAM | Supplies party composition, globals, game time, current area, reputation, journal state, and startup/save state. | Add `ResourceKind::Gam`; parse versioned headers and offset tables; convert party members and global variables into canonical simulation state. | Start Candlekeep from the game's GAM state and visibly reflect the correct party, time/global state, and current area. |
| 2 | SPL | Defines innate, wizard, and priest abilities and their effect lists. | Parse spell metadata, levels/schools, icons, casting data, abilities, and embedded effects; reuse the ITM ability/effect model where layouts overlap. | Cast one stock Candlekeep-available spell/ability and see casting, resource consumption, targeting, and its first gameplay effect. |
| 3 | EFF | Provides reusable and persistent effects used by items, spells, creatures, and saves. | Parse EFF V1/V2 with an opcode registry; normalize embedded and standalone effects; implement timing, stacking, source attribution, resistance, and deterministic expiration incrementally. | Apply a stock effect, see its stat/status presentation change, wait or dispel it, and see the effect end correctly. |
| 4 | PRO | Controls projectile travel, impact, area effects, and visual/audio references. | Parse projectile headers and extension records; separate deterministic trajectory/impact rules from rendering; resolve BAM/VVC/SPL references. | Fire a ranged weapon or spell projectile and see it travel, hit the selected target, and apply the effect at impact rather than cast time. |
| 5 | MOS | Supplies UI panels, minimaps, loading art, and other large static presentation assets. | Parse palette MOS V1/MOSC first, then PVRZ-backed MOS V2; output canonical RGBA images through the existing image boundary. | Replace one generated/debug panel with the original game MOS and verify correct layout, transparency, and scaling in game. |
| 6 | SAV | Packages mutable GAM/ARE/CRE/STO state for continued play. | Parse the SAV archive container with strict path and size limits; load members through an overlay catalog; write atomically only after canonical save schemas stabilize. | Save after changing Candlekeep state, restart, load, and visibly recover position, inventory, globals, conversations, and store state. |
| 7 | WMP | Enables world-map discovery and travel after leaving an area. | Correctly map KEY type `0x03f7`; parse maps, area nodes, links, travel times, icons, and encounter references; convert to a travel graph. | Leave Candlekeep, open the world map, select a reachable destination, advance time, and arrive in the expected area. |
| 8 | CHR | Supports exported/imported player characters and character creation handoff. | Parse the CHR wrapper and embedded CRE; preserve portrait, soundset, and export metadata; reuse CRE conversion. | Import a stock/exported character, start a game, and see its identity, stats, inventory, colors, and portrait. |

The minimum path toward a playable Candlekeep remains:

```text
2DA -> IDS -> BCS/BS -> ITM -> STO -> GAM -> SPL -> EFF -> PRO -> MOS
```

The first narrow slices for 2DA, IDS/BCS, and ITM/CRE are now game-visible.
They remain intentionally incomplete formats; later features expand their
typed fields and supported behavior without bypassing these boundaries.

## Unaccounted presentation and engine-support formats

These matter, but they should follow the gameplay slice that consumes them.

| Format | Interest / feature | General implementation and in-game proof |
| --- | --- | --- |
| CHU | Classic UI window/control layouts | Parse windows and controls, map BAM/MOS/font references into Bevy UI, then reproduce one interactive inventory or store screen. |
| MENU / GUI | Enhanced Edition UI layouts and behavior | Treat as a versioned EE UI frontend, not authoritative rules; load only the subset required for one screen and compare interaction/layout in game. |
| PLT | Paperdolls and recolorable character UI art | Decode indexed color planes with CRE/equipment ramps; show the correctly colored paperdoll beside the same actor in inventory. |
| VVC | Persistent visual spell effects | Parse timing, blend flags, orientation, and BAM references; show one stock aura/effect attached to the correct actor or location. |
| VEF | Sequences/composites of visual effects | Parse timed VVC/BAM/VEF references with recursion limits; reproduce one multi-part stock spell effect. |
| WFX | Randomized sound selection/variation | Parse variation parameters and feed the audio service; hear repeatable seeded variations for one event. |
| WAV / WAVC | Voice, ambience, UI, and effect audio | Decode RIFF, Ogg-in-WAV EE resources, and legacy WAVC/ACM payloads behind one PCM interface; hear a Candlekeep voice line and positional ambience. |
| ACM | Legacy compressed audio | Add a bounded decoder or audited dependency only when a target edition needs it; compare decoded duration/hash and hear the asset in game. |
| MUS | Music playlists and transitions | Parse playlist commands and ACM/Ogg references; enter/leave combat or an area and hear the correct transition. |
| MVE | Classic-edition movies | Decode through an isolated media adapter; play one intro movie with synchronized audio and skippable input. |
| WBM | Enhanced Edition WebM movies | Use a media adapter rather than game-rule code; play and skip one EE movie from the normal game flow. |
| FNT / TTF | UI fonts | Parse FNT metrics or load TTF through the presentation layer; render a dialogue/UI screen with correct wrapping and baseline behavior. |
| PNG | EE portraits and UI images | Decode through the image layer with size limits; display a stock portrait in the party/inventory UI. |
| TOH / TOT | Externalized or patched strings | Layer patched offsets/text above TLK with deterministic precedence; show a patched string in dialogue or UI. |

## Existing formats that need completion slices

Some high-value work is extension of an existing parser rather than a new file
extension. Track these as feature slices so they do not disappear behind the
“format exists” label.

| Existing format | Completion slice | In-game proof |
| --- | --- | --- |
| CRE + ITM | Full usability, effects, equipped armor/weapon/offhand/helmet overlays | Imoen/Gorion displays correct body, clothing, weapon, shield, and equipment changes without diagnostic tinting. |
| IDS + BCS/BS | Typed objects/parameters, response selection, action queue, broader trigger/action VM | A second stock behavior with stateful conditions executes deterministically and BS scripts load through the catalog. |
| STO + GAM | Replace prototype gold/inventory with party state; add pricing modifiers, services, and persistence | Buy and sell after loading a real game state, then save/reload with identical gold, inventory, and store stock. |
| BAM | Equipment overlays and remaining animation families/stances | Walk, idle, attack, hit, die, and equipped overlays stay aligned in all facings. |
| BAM V2 + PVRZ | EE animation pages | Render one stock BAM V2 animation through the normal sprite path. |
| ARE + WED | Doors, walls, containers, traps, occlusion, entrances | Open a Candlekeep door, walk behind a wall with correct occlusion, and use a container/transition. |
| DLG + IDS/BCS | Full trigger/action evaluation | A stock dialogue branch appears only when its original condition is true and its action changes game state. |
| 2DA | Encrypted input, overrides, typed rule consumers | Change an override table and see the intended rule change without recompiling OpenBG. |
| TLK | Locales and patched strings | Switch supported language or string patch and see the correct text in game. |

## Loose configuration and Enhanced Edition support

These are not all KEY-indexed game resources. Add them only behind a named
feature and keep presentation/configuration separate from authoritative rules.

| Format | Likely use | Trigger for implementation |
| --- | --- | --- |
| INI | Edition configuration, animation/resource metadata, key maps | A required animation family, install profile, or user setting cannot be represented safely without it. |
| LUA | EE UI/configuration glue | An EE UI screen requires data or behavior not sensibly reproduced through canonical OpenBG UI code. Sandbox it; never let arbitrary game Lua mutate authoritative state directly. |
| SQL | EE metadata databases | A selected EE feature demonstrably reads required content unavailable through indexed resources. Prefer read-only access through an isolated adapter. |
| GLSL | Original shader assets | Normally replace with Bevy-native shaders; parse/load only if exact presentation parity requires a shipped shader. |
| SRC / VAR / MAZE | Edition- or feature-specific data | Implement only when a real resource is encountered in a chosen playable scenario and its consumer is understood. |
| BIO / RES | Biography or edition-specific text | Character creation/import visibly requires the shipped text. |

## Tool and mod-authoring formats: not runtime priorities

These formats may eventually belong in separate tooling crates, but they must
not displace runtime work for Candlekeep:

- BAF script source, D dialogue source, and WeiDU/mod metadata.
- CBF, CFB, DAT, IAP, KFU, TBG/TBGN, and other packaging/conversion formats.
- Developer source assets that are compiled into BCS, DLG, BAM, or other
  runtime resources before the game consumes them.

Implement one only when OpenBG has a concrete authoring/import/export workflow,
with a round-trip fixture and a tool-level acceptance test. It does not require
an in-game proof unless the runtime also consumes that format directly.

## Working cadence

For each slice, add a short entry below before coding:

```text
Feature:
Formats:
Named real resources:
Parser/loader work:
CLI proof:
In-game scenario:
Automated regression:
Known unsupported variants:
```

Keep at most one new-format slice and one completion slice active at a time.
The next recommended slice is **GAM-backed startup state**: replace prototype
gold/inventory and the debug selected actor with the real party, globals, time,
current area, and economy state used by the new store loop.
