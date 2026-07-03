use std::error::Error;
use std::path::PathBuf;

use openbg_catalog::{GameInstall, ResourceCatalog};
use openbg_content::{AnimationLoader, AreaLoader, ConversationLoader};
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
        "creature" => inspect_creature(&install, &resource)?,
        "ids" => print!("{}", String::from_utf8(install.read_file(&openbg_domain::ResourceId::new(resource, openbg_domain::ResourceKind::Ids))?)?),
        "2da" => print!("{}", String::from_utf8(install.read_file(&openbg_domain::ResourceId::new(resource, openbg_domain::ResourceKind::TwoDa))?)?),
        _ => return Err(USAGE.into()),
    }
    Ok(())
}

fn inspect_creature(install: &GameInstall, creature: &ResRef) -> Result<(), Box<dyn Error>> {
    let content = ConversationLoader::new(install)?.load_creature(creature)?;
    println!(
        "creature {}: name={:?}, dialogue={}",
        content.creature,
        content.display_name,
        content
            .dialogue
            .as_ref()
            .map_or("<none>", |dialogue| dialogue.id.as_str())
    );
    if let Some(dialogue) = content.dialogue {
        for (index, state) in dialogue.states.iter().enumerate() {
            println!(
                "  state {index}: {:?} trigger={:?} replies={}",
                state.text,
                state.trigger,
                state.transitions.len()
            );
            for transition in &state.transitions {
                println!(
                    "    reply={:?} trigger={:?} next={:?}:{:?} terminates={}",
                    transition.text,
                    transition.trigger,
                    transition.next_dialogue,
                    transition.next_state,
                    transition.terminates
                );
            }
        }
    }
    Ok(())
}

fn inspect_area(install: &GameInstall, area: &ResRef) -> Result<(), Box<dyn Error>> {
    let content = AreaLoader::new(install).load(area)?;
    println!(
        "area {}: {}x{} RGBA, {}x{} navigation, {} actors, {} regions, {} animations",
        content.id,
        content.base.width,
        content.base.height,
        content.navigation.width(),
        content.navigation.height(),
        content.actors.len(),
        content.regions.len(),
        content.animations.len()
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
    for region in &content.regions {
        println!(
            "  region {:<24} kind={} bounds={:?} destination={}",
            region.name,
            region.kind,
            region.bounds,
            region.destination_area.as_ref().map_or("-", ResRef::as_str)
        );
    }
    for animation in &content.animations {
        println!(
            "  animation {:<21} at ({:>5}, {:>5}) bam={} cycle={} flags={:#x}",
            animation.name,
            animation.position[0],
            animation.position[1],
            animation.animation,
            animation.sequence,
            animation.flags
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

const USAGE: &str = "usage: openbg-inspect <game-directory> area <resref>\n       openbg-inspect <game-directory> animation <resref>\n       openbg-inspect <game-directory> creature <resref>\n       openbg-inspect <game-directory> ids <resref>\n       openbg-inspect <game-directory> 2da <resref>";
