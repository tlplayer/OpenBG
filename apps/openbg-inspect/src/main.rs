use std::error::Error;
use std::path::PathBuf;

use openbg_catalog::GameInstall;
use openbg_content::{
    AnimationLoader, AreaLoader, BcsLoader, ConversationLoader, CreatureAnimationLoader, IdsLoader,
    ItmLoader, TwoDaLoader,
};
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
        "table" => inspect_table(&install, &resource)?,
        "ids" => inspect_ids(&install, &resource)?,
        "script" => inspect_script(&install, &resource)?,
        "item" => inspect_item(&install, &resource)?,
        _ => return Err(USAGE.into()),
    }
    Ok(())
}

fn inspect_item(install: &GameInstall, id: &ResRef) -> Result<(), Box<dyn Error>> {
    let item = ItmLoader::new(install).load(id)?;
    println!(
        "item {id}: names={}/{}, type={}, appearance={:?}, price={}, weight={}, abilities={}",
        item.unidentified_name,
        item.identified_name,
        item.item_type,
        item.equipped_appearance,
        item.price,
        item.weight,
        item.abilities.len()
    );
    for (index, ability) in item.abilities.iter().enumerate() {
        println!(
            "  ability {index}: attack={}, location={}, speed={}, thac0={:+}, damage={}d{}{:+}",
            ability.attack_type,
            ability.location,
            ability.speed_factor,
            ability.thac0_bonus,
            ability.damage_dice_count,
            ability.damage_dice_sides,
            ability.damage_bonus
        );
    }
    Ok(())
}

fn inspect_ids(install: &GameInstall, id: &ResRef) -> Result<(), Box<dyn Error>> {
    let ids = IdsLoader::new(install).load(id)?;
    println!("ids {id}: {} entries", ids.entries.len());
    for entry in ids.entries {
        println!("  {:#010x}\t{}", entry.value, entry.symbol);
    }
    Ok(())
}

fn inspect_script(install: &GameInstall, id: &ResRef) -> Result<(), Box<dyn Error>> {
    let script = BcsLoader::new(install).load(id)?;
    let triggers = IdsLoader::new(install).load(&ResRef::new("TRIGGER")?)?;
    let actions = IdsLoader::new(install).load(&ResRef::new("ACTION")?)?;
    println!(
        "script {id}: {} triggers, {} actions",
        script.trigger_ids.len(),
        script.action_ids.len()
    );
    if std::env::var_os("OPENBG_RAW").is_some() {
        println!("{}", script.source);
        return Ok(());
    }
    for value in script.trigger_ids {
        println!(
            "  trigger {value}: {}",
            triggers
                .symbol(value)
                .map_or("<unknown>", |entry| entry.name())
        );
    }
    for value in script.action_ids {
        println!(
            "  action {value}: {}",
            actions
                .symbol(value)
                .map_or("<unknown>", |entry| entry.name())
        );
    }
    Ok(())
}

fn inspect_table(install: &GameInstall, id: &ResRef) -> Result<(), Box<dyn Error>> {
    let table = TwoDaLoader::new(install).load(id)?;
    println!(
        "table {id}: {} columns, {} rows, default={:?}",
        table.columns.len(),
        table.rows.len(),
        table.default
    );
    print!("ROW");
    for column in &table.columns {
        print!("\t{column}");
    }
    println!();
    for row in table.rows {
        print!("{}", row.label);
        for value in row.values {
            print!("\t{value}");
        }
        println!();
    }
    Ok(())
}

fn inspect_creature(install: &GameInstall, creature: &ResRef) -> Result<(), Box<dyn Error>> {
    let content = ConversationLoader::new(install)?.load_creature(creature)?;
    let avatar = CreatureAnimationLoader::new(install).load_actor(
        content.animation_id,
        0,
        Some(creature),
    )?;
    println!(
        "creature {}: name={:?}, animation={:#06x} ({}), colors={{metal={}, minor={}, major={}, skin={}, leather={}, armor={}, hair={}}}, dialogue={}",
        content.creature,
        content.display_name,
        content.animation_id,
        avatar.animation.id,
        content.colors.metal,
        content.colors.minor,
        content.colors.major,
        content.colors.skin,
        content.colors.leather,
        content.colors.armor,
        content.colors.hair,
        content
            .dialogue
            .as_ref()
            .map_or("<none>", |dialogue| dialogue.id.as_str())
    );
    println!("  scripts: {:?}", content.scripts);
    for item in &content.inventory {
        println!(
            "  item {} name={:?} slot={:?} equipped={} charges={:?} type={} appearance={:?} weight={} price={}",
            item.id,
            item.display_name,
            item.slot,
            item.equipped,
            item.charges,
            item.item_type,
            item.equipped_appearance,
            item.weight,
            item.price
        );
    }
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

const USAGE: &str = "usage: openbg-inspect <game-directory> area <resref>\n       openbg-inspect <game-directory> animation <resref>\n       openbg-inspect <game-directory> creature <resref>\n       openbg-inspect <game-directory> table <resref>\n       openbg-inspect <game-directory> ids <resref>\n       openbg-inspect <game-directory> script <resref>\n       openbg-inspect <game-directory> item <resref>";
