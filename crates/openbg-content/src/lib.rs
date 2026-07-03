//! Reusable conversion from resolved game resources to canonical runtime content.

use std::error::Error;
use std::fmt;

use openbg_catalog::{CatalogError, ResourceCatalog};
use openbg_domain::{ResRef, ResourceId, ResourceKind};
use openbg_formats::{
    compose_base_layer, compose_base_layer_with_pages, Are, Bam, FormatError, ResourceData, Wed,
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
    pub actors: Vec<ActorPlacement>,
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
pub struct AnimationFrame {
    pub image: ImageData,
    pub center: [i16; 2],
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AnimationContent {
    pub id: ResRef,
    pub frames: Vec<AnimationFrame>,
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
        Ok(AnimationContent {
            id: id.clone(),
            frames,
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

    use super::{AreaLoader, ContentError};

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
}
