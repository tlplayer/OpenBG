use anyhow::Result;

use crate::core::area::{Area, TileMap, Tileset};
use crate::core::asset::{Assets, Handle};
use crate::core::resource::{ResourceId, ResourceKind};

use super::wed::Wed;
use super::tis::{Tis, TisLoader};
use crate::io::source::ResourceSource;
use crate::io::loader::AssetLoader;

pub struct WedToArea<'a, S: ResourceSource> {
    pub source: &'a S,
    pub tis_loader: TisLoader,
}

impl<'a, S: ResourceSource> WedToArea<'a, S> {
    pub fn build(&self, assets: &Assets, wed: &Wed) -> Result<Area> {
        let tis_id = ResourceId::new(&wed.tileset_resref, ResourceKind::Tis);
        let mut reader = self.source.open(&tis_id)?;
        let tis: Tis = self.tis_loader.load(&mut reader)?;

        let tileset_h: Handle<Tileset> = assets.insert(Tileset {
            tile_size: tis.tile_size,
            tile_count: tis.tile_count,
        });

        // Placeholder indices; real mapping comes from WED layers
        let count = (wed.width * wed.height) as usize;
        let indices = vec![0u32; count];

        let base = TileMap {
            width: wed.width,
            height: wed.height,
            tile_size: tis.tile_size,
            tileset: tileset_h,
            indices,
        };

        Ok(Area { base })
    }
}