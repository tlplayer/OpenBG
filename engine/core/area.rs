use crate::core::asset::Handle;

#[derive(Clone)]
pub struct TileMap {
    pub width: u32,
    pub height: u32,
    pub tile_size: u32,
    pub tileset: Handle<Tileset>,
    // indices into tileset; row-major
    pub indices: Vec<u32>,
}

#[derive(Clone)]
pub struct Tileset {
    pub tile_size: u32,
    pub tile_count: u32,
    // GPU upload handled in render layer
}

#[derive(Clone)]
pub struct Area {
    pub base: TileMap,
    // overlays, walkmesh, regions added later
}