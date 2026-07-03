use std::error::Error;
use std::path::PathBuf;

use openbg_catalog::GameInstall;
use openbg_content::{AnimationLoader, AreaLoader};
use openbg_domain::ResRef;

fn main() -> Result<(), Box<dyn Error>> {
    let mut arguments = std::env::args_os().skip(1);
    let game_root = arguments.next().map(PathBuf::from).ok_or(USAGE)?;
    let command = arguments
        .next()
        .and_then(|value| value.into_string().ok())
        .ok_or(USAGE)?;
    let resource = arguments
        .next()
        .and_then(|value| value.into_string().ok())
        .ok_or(USAGE)?;
    let resource = ResRef::new(resource)?;
    let install = GameInstall::open(game_root)?;

    match command.as_str() {
        "area" => inspect_area(&install, &resource)?,
        "animation" => inspect_animation(&install, &resource)?,
        _ => return Err(USAGE.into()),
    }
    Ok(())
}

fn inspect_area(install: &GameInstall, area: &ResRef) -> Result<(), Box<dyn Error>> {
    let content = AreaLoader::new(install).load(area)?;
    println!(
        "area {}: {}x{} RGBA, {} actors",
        content.id,
        content.base.width,
        content.base.height,
        content.actors.len()
    );
    for actor in &content.actors {
        let creature = actor.creature.as_ref().map_or("<embedded>", ResRef::as_str);
        println!(
            "  {:<32} at ({:>5}, {:>5}) orientation={:>2} animation={:#06x} creature={}",
            actor.name,
            actor.position[0],
            actor.position[1],
            actor.orientation,
            actor.animation_id,
            creature
        );
    }
    Ok(())
}

fn inspect_animation(install: &GameInstall, animation: &ResRef) -> Result<(), Box<dyn Error>> {
    let content = AnimationLoader::new(install).load_first_cycle(animation)?;
    println!("animation {}: {} frames", content.id, content.frames.len());
    for (index, frame) in content.frames.iter().enumerate() {
        println!(
            "  frame {index:>3}: {}x{}, center=({}, {})",
            frame.image.width, frame.image.height, frame.center[0], frame.center[1]
        );
    }
    Ok(())
}

const USAGE: &str = "usage: openbg-inspect <game-directory> area <resref>\n       openbg-inspect <game-directory> animation <resref>";
