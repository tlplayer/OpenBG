//! Reusable conversion from resolved game resources to canonical runtime content.

use std::error::Error;
use std::fmt;

use openbg_catalog::{CatalogError, ResourceCatalog};
use openbg_domain::{NavigationGrid, ResRef, ResourceId, ResourceKind};
use openbg_formats::{
    compose_base_layer, compose_base_layer_with_pages, Are, Bam, Cre, Dlg, FormatError,
    IndexedBitmap, ResourceData, Tlk, Wed,
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
    pub dialogue: Option<DialogueContent>,
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
    let frames = cycle
        .frame_indices
        .iter()
        .map(|index| {
            let frame = &bam.frames[usize::from(*index)];
            AnimationFrame {
                image: ImageData {
                    width: u32::from(frame.width),
                    height: u32::from(frame.height),
                    rgba: frame.rgba.clone(),
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
        Ok(CreatureConversation {
            creature: id.clone(),
            display_name,
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
    use openbg_formats::OwnedResourceData;

    use super::{creature_animation_resref, AreaLoader, ContentError};

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
}
