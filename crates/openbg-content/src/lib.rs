//! Reusable conversion from resolved game resources to canonical runtime content.

use std::error::Error;
use std::fmt;

use openbg_catalog::{CatalogError, ResourceCatalog};
use openbg_domain::{NavigationGrid, ResRef, ResourceId, ResourceKind};
use openbg_formats::{
    apply_palette, compose_base_layer, compose_base_layer_with_pages, Are, Bam, Bcs, Cre,
    CreColors, CreScripts, Dlg, FormatError, Ids, IndexedBitmap, Itm, ResourceData, RgbaBitmap,
    Sto, Tlk, TwoDa, Wed,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ImageData {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AreaContent {
    pub id: ResRef,
    pub base: ImageData,
    pub navigation: NavigationGrid,
    pub actors: Vec<ActorPlacement>,
    pub regions: Vec<RegionPlacement>,
    pub entrances: Vec<EntrancePlacement>,
    pub animations: Vec<AreaAnimationPlacement>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ActorPlacement {
    pub name: String,
    pub position: [u16; 2],
    pub orientation: u16,
    pub animation_id: u32,
    pub creature: Option<ResRef>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RegionPlacement {
    pub name: String,
    pub kind: u16,
    pub bounds: [u16; 4],
    pub destination_area: Option<ResRef>,
    pub destination_entrance: String,
    pub flags: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EntrancePlacement {
    pub name: String,
    pub position: [u16; 2],
    pub orientation: u16,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AreaAnimationPlacement {
    pub name: String,
    pub position: [u16; 2],
    pub schedule: u32,
    pub animation: ResRef,
    pub sequence: u16,
    pub frame: u16,
    pub flags: u32,
    pub height: u16,
    pub transparency: u16,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AnimationFrame {
    pub image: ImageData,
    pub center: [i16; 2],
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AnimationContent {
    pub id: ResRef,
    pub frames: Vec<AnimationFrame>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CreatureAnimationContent {
    pub animation: AnimationContent,
    pub flip_x: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CreatureConversation {
    pub creature: ResRef,
    pub display_name: Option<String>,
    pub animation_id: u32,
    pub colors: CreColors,
    pub scripts: CreScripts,
    pub inventory: Vec<CreatureItemContent>,
    pub dialogue: Option<DialogueContent>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CreatureItemContent {
    pub id: ResRef,
    pub display_name: Option<String>,
    pub item_type: u16,
    pub equipped_appearance: String,
    pub price: u32,
    pub weight: u32,
    pub charges: [u16; 3],
    pub flags: u32,
    pub slot: Option<usize>,
    pub equipped: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StoreItemContent {
    pub id: ResRef,
    pub display_name: Option<String>,
    pub item_type: u16,
    pub base_price: u32,
    pub purchase_price: u32,
    pub weight: u32,
    pub charges: [u16; 3],
    pub flags: u32,
    pub stock: Option<u32>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StoreContent {
    pub id: ResRef,
    pub display_name: Option<String>,
    pub flags: u32,
    pub sell_markup: u32,
    pub buy_markup: u32,
    pub depreciation: u32,
    pub capacity: u16,
    pub purchased_item_types: Vec<u32>,
    pub items: Vec<StoreItemContent>,
}

/// Resolves numeric identifier tables used by compiled scripts and rules.
pub struct IdsLoader<'a, C: ResourceCatalog + ?Sized> {
    catalog: &'a C,
}

impl<'a, C: ResourceCatalog + ?Sized> IdsLoader<'a, C> {
    #[must_use]
    pub const fn new(catalog: &'a C) -> Self {
        Self { catalog }
    }

    /// Loads one IDS resource.
    ///
    /// # Errors
    ///
    /// Returns [`ContentError`] when the resource is absent or malformed.
    pub fn load(&self, id: &ResRef) -> Result<Ids, ContentError> {
        let resource = ResourceId::new(id.clone(), ResourceKind::Ids);
        Ok(Ids::parse(&self.catalog.read_file(&resource)?)?)
    }
}

/// Resolves compiled world or character scripts.
pub struct BcsLoader<'a, C: ResourceCatalog + ?Sized> {
    catalog: &'a C,
}

impl<'a, C: ResourceCatalog + ?Sized> BcsLoader<'a, C> {
    #[must_use]
    pub const fn new(catalog: &'a C) -> Self {
        Self { catalog }
    }

    /// Loads one BCS resource.
    ///
    /// # Errors
    ///
    /// Returns [`ContentError`] when the resource is absent or malformed.
    pub fn load(&self, id: &ResRef) -> Result<Bcs, ContentError> {
        let resource = ResourceId::new(id.clone(), ResourceKind::Bcs);
        Ok(Bcs::parse(&self.catalog.read_file(&resource)?)?)
    }
}

/// Resolves item definitions independently from creature inventory placement.
pub struct ItmLoader<'a, C: ResourceCatalog + ?Sized> {
    catalog: &'a C,
}

impl<'a, C: ResourceCatalog + ?Sized> ItmLoader<'a, C> {
    #[must_use]
    pub const fn new(catalog: &'a C) -> Self {
        Self { catalog }
    }

    /// Loads one ITM resource.
    ///
    /// # Errors
    ///
    /// Returns [`ContentError`] when the resource is absent or malformed.
    pub fn load(&self, id: &ResRef) -> Result<Itm, ContentError> {
        let resource = ResourceId::new(id.clone(), ResourceKind::Itm);
        Ok(Itm::parse(&self.catalog.read_file(&resource)?)?)
    }
}

/// Resolves a store and its item definitions through the installation catalog.
pub struct StoreLoader<'a, C: ResourceCatalog + ?Sized> {
    catalog: &'a C,
    strings: Tlk,
}

impl<'a, C: ResourceCatalog + ?Sized> StoreLoader<'a, C> {
    /// Loads `dialog.tlk` for store and item names.
    ///
    /// # Errors
    ///
    /// Returns [`ContentError`] when the string table is absent or malformed.
    pub fn new(catalog: &'a C) -> Result<Self, ContentError> {
        let id = ResourceId::new(
            ResRef::new("DIALOG").expect("DIALOG is a valid fixed resref"),
            ResourceKind::Tlk,
        );
        let strings = Tlk::parse(&catalog.read_file(&id)?)?;
        Ok(Self { catalog, strings })
    }

    /// Loads one V1 store and resolves its stock item metadata.
    ///
    /// # Errors
    ///
    /// Returns [`ContentError`] when the store, one of its items, or a required
    /// string reference is absent or malformed.
    pub fn load(&self, id: &ResRef) -> Result<StoreContent, ContentError> {
        let resource = ResourceId::new(id.clone(), ResourceKind::Sto);
        let store = Sto::parse(&self.catalog.read_file(&resource)?)?;
        let mut items = Vec::with_capacity(store.items.len());
        for stock in &store.items {
            let item = ItmLoader::new(self.catalog).load(&stock.resource)?;
            let identified = stock.infinite || stock.flags & 1 != 0;
            let name = if identified {
                item.identified_name
            } else {
                item.unidentified_name
            };
            items.push(StoreItemContent {
                id: stock.resource.clone(),
                display_name: optional_tlk_text(&self.strings, name)?,
                item_type: item.item_type,
                base_price: item.price,
                purchase_price: percentage(item.price, store.sell_markup),
                weight: item.weight,
                charges: stock.charges,
                flags: stock.flags,
                stock: (!stock.infinite).then_some(stock.stock),
            });
        }
        Ok(StoreContent {
            id: id.clone(),
            display_name: optional_tlk_text(&self.strings, store.name)?,
            flags: store.flags,
            sell_markup: store.sell_markup,
            buy_markup: store.buy_markup,
            depreciation: store.depreciation,
            capacity: store.capacity,
            purchased_item_types: store.purchased_item_types,
            items,
        })
    }
}

fn percentage(value: u32, percent: u32) -> u32 {
    let scaled = u64::from(value) * u64::from(percent) / 100;
    u32::try_from(scaled).unwrap_or(u32::MAX)
}

fn optional_tlk_text(strings: &Tlk, strref: u32) -> Result<Option<String>, ContentError> {
    if strref == u32::MAX {
        return Ok(None);
    }
    strings
        .text(strref)
        .map(Some)
        .ok_or_else(|| ContentError::Invalid(format!("TLK string reference {strref} is missing")))
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DialogueContent {
    pub id: ResRef,
    pub states: Vec<DialogueStateContent>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DialogueStateContent {
    pub text: String,
    pub trigger: Option<String>,
    pub transitions: Vec<DialogueTransitionContent>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DialogueTransitionContent {
    pub text: Option<String>,
    pub trigger: Option<String>,
    pub action: Option<String>,
    pub terminates: bool,
    pub next_dialogue: Option<ResRef>,
    pub next_state: Option<u32>,
}

/// Resolves typed rules tables without coupling consumers to archive storage.
pub struct TwoDaLoader<'a, C: ResourceCatalog + ?Sized> {
    catalog: &'a C,
}

impl<'a, C: ResourceCatalog + ?Sized> TwoDaLoader<'a, C> {
    #[must_use]
    pub const fn new(catalog: &'a C) -> Self {
        Self { catalog }
    }

    /// Loads and parses one `2DA V1.0` resource.
    ///
    /// # Errors
    ///
    /// Returns [`ContentError`] when the resource is absent or malformed.
    pub fn load(&self, id: &ResRef) -> Result<TwoDa, ContentError> {
        let resource = ResourceId::new(id.clone(), ResourceKind::TwoDa);
        Ok(TwoDa::parse(&self.catalog.read_file(&resource)?)?)
    }
}

/// Builds canonical area content without depending on Bevy or filesystem I/O.
pub struct AreaLoader<'a, C: ResourceCatalog + ?Sized> {
    catalog: &'a C,
}

impl<'a, C: ResourceCatalog + ?Sized> AreaLoader<'a, C> {
    #[must_use]
    pub const fn new(catalog: &'a C) -> Self {
        Self { catalog }
    }

    /// Loads and composes an area's static base layer.
    ///
    /// # Errors
    ///
    /// Returns [`ContentError`] when WED/TIS/PVRZ resolution or decoding fails.
    pub fn load(&self, area: &ResRef) -> Result<AreaContent, ContentError> {
        let are_id = ResourceId::new(area.clone(), ResourceKind::Are);
        let are = Are::parse(&self.catalog.read_file(&are_id)?)?;
        let search_name = ResRef::new(format!("{}SR", area.as_str()))
            .map_err(|error| ContentError::Invalid(error.to_string()))?;
        let search_id = ResourceId::new(search_name, ResourceKind::Bmp);
        let search = IndexedBitmap::parse(&self.catalog.read_file(&search_id)?)?;
        let navigation = NavigationGrid::new(
            u16::try_from(search.width)
                .map_err(|_| ContentError::Invalid("search-map width exceeds u16".into()))?,
            u16::try_from(search.height)
                .map_err(|_| ContentError::Invalid("search-map height exceeds u16".into()))?,
            search.pixels,
        )
        .map_err(|error| ContentError::Invalid(error.to_string()))?;
        let wed_id = ResourceId::new(area.clone(), ResourceKind::Wed);
        let wed_bytes = self.catalog.read_file(&wed_id)?;
        let wed = Wed::parse(&wed_bytes)?;
        let tis_id = ResourceId::new(wed.base.tileset.clone(), ResourceKind::Tis);
        let tis = self.catalog.read(&tis_id)?;
        let image = match tis.as_borrowed() {
            ResourceData::Tileset {
                tile_size: 5120, ..
            } => compose_base_layer(&wed, tis.as_borrowed())?,
            ResourceData::Tileset { tile_size: 12, .. } => {
                compose_base_layer_with_pages(&wed, tis.as_borrowed(), |page| {
                    let id = ResourceId::new(page.clone(), ResourceKind::Pvrz);
                    self.catalog
                        .read_file(&id)
                        .map_err(|error| FormatError::new("PVRZ catalog", error.to_string()))
                })?
            }
            ResourceData::Tileset { tile_size, .. } => {
                return Err(ContentError::Format(FormatError::new(
                    "TIS V1",
                    format!("unsupported tile block size {tile_size}"),
                )));
            }
            ResourceData::File(_) => {
                return Err(ContentError::Format(FormatError::new(
                    "TIS V1",
                    "resource is not a tileset",
                )));
            }
        };
        Ok(AreaContent {
            id: area.clone(),
            base: ImageData {
                width: image.width,
                height: image.height,
                rgba: image.pixels,
            },
            navigation,
            actors: are
                .actors
                .into_iter()
                .map(|actor| ActorPlacement {
                    name: actor.name,
                    position: actor.position,
                    orientation: actor.orientation,
                    animation_id: actor.animation_id,
                    creature: actor.creature,
                })
                .collect(),
            regions: are
                .regions
                .into_iter()
                .map(|region| RegionPlacement {
                    name: region.name,
                    kind: region.kind,
                    bounds: region.bounds,
                    destination_area: region.destination_area,
                    destination_entrance: region.destination_entrance,
                    flags: region.flags,
                })
                .collect(),
            entrances: are
                .entrances
                .into_iter()
                .map(|entrance| EntrancePlacement {
                    name: entrance.name,
                    position: entrance.position,
                    orientation: entrance.orientation,
                })
                .collect(),
            animations: are
                .animations
                .into_iter()
                .map(|animation| AreaAnimationPlacement {
                    name: animation.name,
                    position: animation.position,
                    schedule: animation.schedule,
                    animation: animation.animation,
                    sequence: animation.sequence,
                    frame: animation.frame,
                    flags: animation.flags,
                    height: animation.height,
                    transparency: animation.transparency,
                })
                .collect(),
        })
    }
}

/// Builds renderer-independent animation content from BAM resources.
pub struct AnimationLoader<'a, C: ResourceCatalog + ?Sized> {
    catalog: &'a C,
}

impl<'a, C: ResourceCatalog + ?Sized> AnimationLoader<'a, C> {
    #[must_use]
    pub const fn new(catalog: &'a C) -> Self {
        Self { catalog }
    }

    /// Loads the first non-empty animation cycle from a BAM resource.
    ///
    /// # Errors
    ///
    /// Returns [`ContentError`] when the BAM cannot be resolved/decoded or has
    /// no non-empty animation cycle.
    pub fn load_first_cycle(&self, id: &ResRef) -> Result<AnimationContent, ContentError> {
        let resource_id = ResourceId::new(id.clone(), ResourceKind::Bam);
        let bytes = self.catalog.read_file(&resource_id)?;
        let bam = Bam::parse(&bytes)?;
        let cycle = bam
            .cycles
            .iter()
            .find(|cycle| !cycle.frame_indices.is_empty())
            .ok_or_else(|| ContentError::Invalid(format!("BAM {id} has no animation frames")))?;
        Ok(animation_content(id, &bam, cycle))
    }

    /// Loads one animation cycle from a BAM resource.
    ///
    /// # Errors
    ///
    /// Returns [`ContentError`] when the BAM cannot be decoded or the requested
    /// cycle is absent or empty.
    pub fn load_cycle(
        &self,
        id: &ResRef,
        cycle_index: u16,
    ) -> Result<AnimationContent, ContentError> {
        let resource_id = ResourceId::new(id.clone(), ResourceKind::Bam);
        let bytes = self.catalog.read_file(&resource_id)?;
        let bam = Bam::parse(&bytes)?;
        let cycle = bam
            .cycles
            .get(usize::from(cycle_index))
            .ok_or_else(|| ContentError::Invalid(format!("BAM {id} has no cycle {cycle_index}")))?;
        if cycle.frame_indices.is_empty() {
            return Err(ContentError::Invalid(format!(
                "BAM {id} cycle {cycle_index} has no animation frames"
            )));
        }
        Ok(animation_content(id, &bam, cycle))
    }
}

/// Resolves an ARE/CRE animation ID to its standing/walking BAM body sprite.
pub struct CreatureAnimationLoader<'a, C: ResourceCatalog + ?Sized> {
    catalog: &'a C,
}

impl<'a, C: ResourceCatalog + ?Sized> CreatureAnimationLoader<'a, C> {
    #[must_use]
    pub const fn new(catalog: &'a C) -> Self {
        Self { catalog }
    }

    /// Loads the general animation for a creature facing its ARE orientation.
    ///
    /// Character animation IDs select a direction-specific BAM. Legacy and
    /// ambient animation families keep their directions in one BAM and are
    /// mirrored for east-facing orientations.
    ///
    /// # Errors
    ///
    /// Returns [`ContentError`] when the animation ID is not mapped or its BAM
    /// cannot be resolved/decoded.
    pub fn load(
        &self,
        animation_id: u32,
        orientation: u16,
    ) -> Result<CreatureAnimationContent, ContentError> {
        let (id, flip_x) = creature_animation_resref(animation_id, orientation)?;
        let animation = AnimationLoader::new(self.catalog).load_first_cycle(&id)?;
        Ok(CreatureAnimationContent { animation, flip_x })
    }

    /// Loads an actor animation, applying its `CRE V1.0` avatar color ramps.
    ///
    /// The CRE animation ID is authoritative when present; `fallback_animation_id`
    /// supports embedded or otherwise unavailable creature data.
    ///
    /// # Errors
    ///
    /// Returns [`ContentError`] when the CRE, BAM, or master palette cannot be
    /// resolved or decoded.
    pub fn load_actor(
        &self,
        fallback_animation_id: u32,
        orientation: u16,
        creature: Option<&ResRef>,
    ) -> Result<CreatureAnimationContent, ContentError> {
        let Some(creature) = creature else {
            return self.load(fallback_animation_id, orientation);
        };
        let resource = ResourceId::new(creature.clone(), ResourceKind::Cre);
        let creature = Cre::parse(&self.catalog.read_file(&resource)?)?;
        let animation_id = if creature.animation_id == 0 {
            fallback_animation_id
        } else {
            creature.animation_id
        };
        let (id, flip_x) = creature_animation_resref(animation_id, orientation)?;
        let resource_id = ResourceId::new(id.clone(), ResourceKind::Bam);
        let bam = Bam::parse(&self.catalog.read_file(&resource_id)?)?;
        let cycle = bam
            .cycles
            .iter()
            .find(|cycle| !cycle.frame_indices.is_empty())
            .ok_or_else(|| ContentError::Invalid(format!("BAM {id} has no animation frames")))?;
        let palette = if uses_character_color_ranges(animation_id) {
            Some(self.remapped_palette(&bam, creature.colors)?)
        } else {
            None
        };
        let animation = animation_content_with_palette(&id, &bam, cycle, palette.as_deref());
        Ok(CreatureAnimationContent { animation, flip_x })
    }

    fn remapped_palette(&self, bam: &Bam, colors: CreColors) -> Result<Vec<[u8; 4]>, ContentError> {
        let master_id = ResourceId::new(
            ResRef::new("MPAL256").expect("fixed master palette resref is valid"),
            ResourceKind::Bmp,
        );
        let master = RgbaBitmap::parse(&self.catalog.read_file(&master_id)?)?;
        remap_character_palette(bam, colors, &master)
    }
}

fn uses_character_color_ranges(animation_id: u32) -> bool {
    (0x5000..=0x6315).contains(&animation_id) || animation_id == 0x6402
}

fn creature_animation_resref(
    animation_id: u32,
    orientation: u16,
) -> Result<(ResRef, bool), ContentError> {
    let facing = orientation % 16;
    let flip_x = facing > 8;
    let name = if (0x5000..=0x6315).contains(&animation_id) {
        let family = (animation_id >> 8) & 0xf;
        let race = animation_id & 0xf;
        let gender = (animation_id >> 4) & 0xf;
        let race = match race {
            0 | 5 => 'H',
            1 => 'E',
            2 | 4 => 'D',
            3 => 'I',
            _ => return unsupported_creature_animation(animation_id),
        };
        let gender = match gender {
            0 => 'M',
            1 => 'F',
            _ => return unsupported_creature_animation(animation_id),
        };
        let body = match family {
            0 | 1 | 3 => "B1",
            2 => "W1",
            _ => return unsupported_creature_animation(animation_id),
        };
        let direction = if facing <= 8 { facing } else { 16 - facing } + 1;
        format!("C{race}{gender}{body}G1{direction}")
    } else {
        match animation_id {
            0x6402 => "CMNK1G1".to_owned(),
            0xb000 => "ACOWG1".to_owned(),
            0xc700 => "NBOYLG1".to_owned(),
            0xc710 => "NGRLLG1".to_owned(),
            0xd100 => "AGULG1".to_owned(),
            _ => return unsupported_creature_animation(animation_id),
        }
    };
    let id = ResRef::new(name).map_err(|error| ContentError::Invalid(error.to_string()))?;
    Ok((id, flip_x))
}

fn unsupported_creature_animation<T>(animation_id: u32) -> Result<T, ContentError> {
    Err(ContentError::Invalid(format!(
        "unsupported creature animation ID {animation_id:#06x}"
    )))
}

fn animation_content(id: &ResRef, bam: &Bam, cycle: &openbg_formats::BamCycle) -> AnimationContent {
    animation_content_with_palette(id, bam, cycle, None)
}

fn animation_content_with_palette(
    id: &ResRef,
    bam: &Bam,
    cycle: &openbg_formats::BamCycle,
    palette: Option<&[[u8; 4]]>,
) -> AnimationContent {
    let frames = cycle
        .frame_indices
        .iter()
        .map(|index| {
            let frame = &bam.frames[usize::from(*index)];
            AnimationFrame {
                image: ImageData {
                    width: u32::from(frame.width),
                    height: u32::from(frame.height),
                    rgba: palette.map_or_else(
                        || frame.rgba.clone(),
                        |palette| apply_palette(&frame.indices, palette),
                    ),
                },
                center: [frame.center_x, frame.center_y],
            }
        })
        .collect();
    AnimationContent {
        id: id.clone(),
        frames,
    }
}

fn remap_character_palette(
    bam: &Bam,
    colors: CreColors,
    master: &RgbaBitmap,
) -> Result<Vec<[u8; 4]>, ContentError> {
    const SHADE_COUNT: usize = 12;
    const FIRST_COLOR_INDEX: usize = 0x04;
    let master_width = usize::try_from(master.width)
        .map_err(|_| ContentError::Invalid("MPAL256 width exceeds usize".into()))?;
    if master_width < SHADE_COUNT {
        return Err(ContentError::Invalid(format!(
            "MPAL256 must contain at least 12 shade columns; got {}x{}",
            master.width, master.height
        )));
    }
    let mut palette = bam.palette.clone();
    if palette.len() != 256 {
        return Err(ContentError::Invalid(format!(
            "character BAM palette has {} colors instead of 256",
            palette.len()
        )));
    }
    for (slot, range) in colors.as_array().into_iter().enumerate() {
        let row = usize::from(range);
        if row
            >= usize::try_from(master.height)
                .map_err(|_| ContentError::Invalid("MPAL256 height exceeds usize".into()))?
        {
            return Err(ContentError::Invalid(format!(
                "CRE color range {range} exceeds MPAL256 height {}",
                master.height
            )));
        }
        let source = row * master_width;
        let destination = FIRST_COLOR_INDEX + slot * SHADE_COUNT;
        for shade in 0..SHADE_COUNT {
            palette[destination + shade] = master.pixels[source + shade];
        }
    }
    // Character BAMs reuse abbreviated shade bands after the seven primary
    // ranges. These aliases match the original engine's paperdoll palette.
    palette.copy_within(0x11..0x19, 0x58); // minor
    palette.copy_within(0x1d..0x25, 0x60); // major
    palette.copy_within(0x11..0x19, 0x68); // minor
    palette.copy_within(0x05..0x0d, 0x70); // metal
    palette.copy_within(0x35..0x3d, 0x78); // leather
    palette.copy_within(0x35..0x3d, 0x80); // leather
    palette.copy_within(0x11..0x19, 0x88); // minor
    for destination in (0x90..0xa8).step_by(8) {
        palette.copy_within(0x35..0x3d, destination); // leather
    }
    palette.copy_within(0x29..0x31, 0xb0); // skin
    for destination in (0xb8..0x100).step_by(8) {
        palette.copy_within(0x35..0x3d, destination); // leather
    }
    palette[1] = [0, 0, 0, 255];
    Ok(palette)
}

/// Resolves CRE/DLG string references through the installation's `dialog.tlk`.
pub struct ConversationLoader<'a, C: ResourceCatalog + ?Sized> {
    catalog: &'a C,
    strings: Tlk,
}

impl<'a, C: ResourceCatalog + ?Sized> ConversationLoader<'a, C> {
    /// Loads `dialog.tlk` once for subsequent creature/dialogue resolution.
    ///
    /// # Errors
    ///
    /// Returns [`ContentError`] when the string table is missing or malformed.
    pub fn new(catalog: &'a C) -> Result<Self, ContentError> {
        let id = ResourceId::new(
            ResRef::new("DIALOG").expect("DIALOG is a valid fixed resref"),
            ResourceKind::Tlk,
        );
        let strings = Tlk::parse(&catalog.read_file(&id)?)?;
        Ok(Self { catalog, strings })
    }

    /// Loads a creature's display name and complete dialogue state table.
    ///
    /// # Errors
    ///
    /// Returns [`ContentError`] when the CRE/DLG cannot be resolved or decoded,
    /// or when a referenced TLK string is outside the string table.
    pub fn load_creature(&self, id: &ResRef) -> Result<CreatureConversation, ContentError> {
        let resource = ResourceId::new(id.clone(), ResourceKind::Cre);
        let creature = Cre::parse(&self.catalog.read_file(&resource)?)?;
        let display_name = self
            .optional_text(creature.long_name)?
            .or(self.optional_text(creature.short_name)?);
        let dialogue = creature
            .dialogue
            .as_ref()
            .map(|dialogue| self.load_dialogue(dialogue))
            .transpose()?;
        let inventory = creature
            .inventory
            .as_ref()
            .map(|inventory| {
                inventory
                    .items
                    .iter()
                    .enumerate()
                    .map(|(index, instance)| {
                        let item = ItmLoader::new(self.catalog).load(&instance.resource)?;
                        let identified = instance.flags & 1 != 0;
                        let name = if identified {
                            item.identified_name
                        } else {
                            item.unidentified_name
                        };
                        let slot = inventory
                            .slots
                            .iter()
                            .position(|entry| *entry == Some(index));
                        Ok(CreatureItemContent {
                            id: instance.resource.clone(),
                            display_name: self.optional_text(name)?,
                            item_type: item.item_type,
                            equipped_appearance: item.equipped_appearance,
                            price: item.price,
                            weight: item.weight,
                            charges: instance.charges,
                            flags: instance.flags,
                            slot,
                            equipped: slot.is_some_and(|slot| slot <= 17),
                        })
                    })
                    .collect::<Result<Vec<_>, ContentError>>()
            })
            .transpose()?
            .unwrap_or_default();
        Ok(CreatureConversation {
            creature: id.clone(),
            display_name,
            animation_id: creature.animation_id,
            colors: creature.colors,
            scripts: creature.scripts,
            inventory,
            dialogue,
        })
    }

    /// Loads and localizes one DLG state table.
    ///
    /// # Errors
    ///
    /// Returns [`ContentError`] for missing/malformed DLG data or invalid TLK
    /// references.
    pub fn load_dialogue(&self, id: &ResRef) -> Result<DialogueContent, ContentError> {
        let resource = ResourceId::new(id.clone(), ResourceKind::Dlg);
        let dialogue = Dlg::parse(&self.catalog.read_file(&resource)?)?;
        let mut states = Vec::with_capacity(dialogue.states.len());
        for state in &dialogue.states {
            let start = usize::try_from(state.first_transition)
                .map_err(|_| ContentError::Invalid("DLG transition index exceeds usize".into()))?;
            let count = usize::try_from(state.transition_count)
                .map_err(|_| ContentError::Invalid("DLG transition count exceeds usize".into()))?;
            let state_transitions = dialogue
                .transitions
                .get(start..start + count)
                .ok_or_else(|| ContentError::Invalid("DLG transition range is missing".into()))?;
            let transitions = state_transitions
                .iter()
                .map(|transition| {
                    Ok(DialogueTransitionContent {
                        text: transition
                            .text
                            .map(|strref| self.required_text(strref))
                            .transpose()?,
                        trigger: transition.trigger.clone(),
                        action: transition.action.clone(),
                        terminates: transition.flags & (1 << 3) != 0,
                        next_dialogue: transition.next_dialogue.clone(),
                        next_state: transition.next_state,
                    })
                })
                .collect::<Result<Vec<_>, ContentError>>()?;
            states.push(DialogueStateContent {
                text: self.required_text(state.text)?,
                trigger: state.trigger.clone(),
                transitions,
            });
        }
        Ok(DialogueContent {
            id: id.clone(),
            states,
        })
    }

    fn optional_text(&self, strref: u32) -> Result<Option<String>, ContentError> {
        if strref == u32::MAX {
            return Ok(None);
        }
        self.strings.text(strref).map(Some).ok_or_else(|| {
            ContentError::Invalid(format!("TLK string reference {strref} is missing"))
        })
    }

    fn required_text(&self, strref: u32) -> Result<String, ContentError> {
        self.optional_text(strref)?.ok_or_else(|| {
            ContentError::Invalid(format!("required TLK string reference {strref} is absent"))
        })
    }
}

#[derive(Debug)]
pub enum ContentError {
    Catalog(CatalogError),
    Format(FormatError),
    Invalid(String),
}

impl fmt::Display for ContentError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Catalog(error) => error.fmt(formatter),
            Self::Format(error) => error.fmt(formatter),
            Self::Invalid(error) => formatter.write_str(error),
        }
    }
}

impl Error for ContentError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Catalog(error) => Some(error),
            Self::Format(error) => Some(error),
            Self::Invalid(_) => None,
        }
    }
}

impl From<CatalogError> for ContentError {
    fn from(error: CatalogError) -> Self {
        Self::Catalog(error)
    }
}

impl From<FormatError> for ContentError {
    fn from(error: FormatError) -> Self {
        Self::Format(error)
    }
}

#[cfg(test)]
mod tests {
    use openbg_catalog::{CatalogError, ResourceCatalog};
    use openbg_domain::{ResRef, ResourceId};
    use openbg_formats::{Bam, CreColors, OwnedResourceData, RgbaBitmap};

    use super::{creature_animation_resref, remap_character_palette, AreaLoader, ContentError};

    struct EmptyCatalog;

    impl ResourceCatalog for EmptyCatalog {
        fn contains(&self, _: &ResourceId) -> bool {
            false
        }

        fn read(&self, id: &ResourceId) -> Result<OwnedResourceData, CatalogError> {
            Err(CatalogError::NotFound(id.clone()))
        }
    }

    #[test]
    fn missing_area_preserves_catalog_diagnostics() {
        let area = ResRef::new("ARTEST").expect("valid resref");
        let error = AreaLoader::new(&EmptyCatalog)
            .load(&area)
            .expect_err("empty catalog has no area");
        assert!(matches!(
            error,
            ContentError::Catalog(CatalogError::NotFound(_))
        ));
    }

    #[test]
    fn resolves_character_animation_direction_and_mirroring() {
        let (south, south_flip) = creature_animation_resref(0x6200, 0).expect("Gorion");
        assert_eq!(south.as_str(), "CHMW1G11");
        assert!(!south_flip);

        let (east, east_flip) = creature_animation_resref(0x6210, 12).expect("Phlydia");
        assert_eq!(east.as_str(), "CHFW1G15");
        assert!(east_flip);
    }

    #[test]
    fn resolves_candlekeep_special_animation_families() {
        assert_eq!(
            creature_animation_resref(0xc710, 0)
                .expect("girl")
                .0
                .as_str(),
            "NGRLLG1"
        );
        assert_eq!(
            creature_animation_resref(0xb000, 14)
                .expect("cow")
                .0
                .as_str(),
            "ACOWG1"
        );
    }

    #[test]
    fn remaps_primary_and_alias_character_color_bands() {
        let bam = Bam {
            frames: Vec::new(),
            cycles: Vec::new(),
            palette: vec![[0, 0, 0, 255]; 256],
            transparent_index: 0,
        };
        let mut pixels = Vec::with_capacity(12 * 256);
        for row in 0_u8..=255 {
            for shade in 0_u8..12 {
                pixels.push([row, shade, 0, 255]);
            }
        }
        let master = RgbaBitmap {
            width: 12,
            height: 256,
            pixels,
        };
        let colors = CreColors {
            metal: 1,
            minor: 2,
            major: 3,
            skin: 4,
            leather: 5,
            armor: 6,
            hair: 7,
        };

        let palette = remap_character_palette(&bam, colors, &master).expect("valid ramps");
        assert_eq!(palette[0x04], [1, 0, 0, 255]);
        assert_eq!(palette[0x10], [2, 0, 0, 255]);
        assert_eq!(palette[0x4c + 11], [7, 11, 0, 255]);
        assert_eq!(palette[0x58], [2, 1, 0, 255]);
        assert_eq!(palette[0x70], [1, 1, 0, 255]);
        assert_eq!(palette[0xb0], [4, 1, 0, 255]);
        assert_eq!(palette[0xff], [5, 8, 0, 255]);
    }
}
