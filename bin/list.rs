use anyhow::Result;

use engine::core::asset::Assets;
use engine::core::resource::{ResourceId, ResourceKind};
use engine::io::source::ResourceSource;
use engine::io::fs_source::FsSource;

use engine::adapters::wed::{WedLoader};
use engine::adapters::wed_to_area::WedToArea;

fn main() -> Result<()> {
    // point to "<game>/override" for now
    let src = FsSource::new("./override");

    let list = src.list()?;
    println!("found {} resources (override)", list.len());

    // pick a WED by name
    let wed_id = ResourceId::new("AR0001", ResourceKind::Wed);
    if !src.exists(&wed_id) {
        println!("WED not found: {:?}", wed_id);
        return Ok(());
    }

    let mut r = src.open(&wed_id)?;
    let wed = WedLoader.load(&mut r)?;

    let assets = Assets::new();
    let builder = WedToArea { source: &src, tis_loader: engine::adapters::tis::TisLoader };

    let area = builder.build(&assets, &wed)?;
    let scene = engine::core::render_types::extract_area(&area);

    println!(
        "area: {}x{}, layers={}",
        scene.layers[0].width,
        scene.layers[0].height,
        scene.layers.len()
    );

    Ok(())
}