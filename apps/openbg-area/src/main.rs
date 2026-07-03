use std::collections::{BTreeMap, HashMap};
use std::error::Error;
use std::fmt;
use std::path::PathBuf;

use bevy::asset::RenderAssetUsages;
use bevy::input::mouse::MouseWheel;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::text::TextBounds;
use bevy::window::{PresentMode, PrimaryWindow, WindowPlugin};
use openbg_catalog::GameInstall;
use openbg_content::{
    AnimationContent, AnimationLoader, AreaAnimationPlacement, AreaContent, AreaLoader, BcsLoader,
    ConversationLoader, CreatureAnimationContent, CreatureAnimationLoader, CreatureConversation,
    CreatureItemContent, DialogueStateContent, DialogueTransitionContent, IdsLoader, ImageData,
    TwoDaLoader,
};
use openbg_domain::{GridPoint, ResRef};
use openbg_formats::TwoDa;
use openbg_sim::find_path;

const DEFAULT_AREA: &str = "AR2600";
const XVART_SPEED: f32 = 180.0;
const ARRIVAL_DISTANCE: f32 = 2.0;
const NPC_CLICK_RADIUS: f32 = 36.0;
const TALK_DISTANCE: f32 = 84.0;

fn main() -> Result<(), Box<dyn Error>> {
    let (game_root, area) = arguments()?;
    let install = GameInstall::open(&game_root)?;
    let content = AreaLoader::new(&install).load(&area)?;
    let start_table = ResRef::new("STARTARE")?;
    let selected_start = TwoDaLoader::new(&install)
        .load(&start_table)
        .ok()
        .and_then(|table| start_position(&table, &area));
    if let Some(position) = selected_start {
        println!(
            "STARTARE places selected actor in {area} at ({}, {})",
            position[0], position[1]
        );
    }
    let conversation_loader = ConversationLoader::new(&install)?;
    let mut conversations = BTreeMap::new();
    for creature in content
        .actors
        .iter()
        .filter_map(|actor| actor.creature.as_ref())
    {
        if conversations.contains_key(creature) {
            continue;
        }
        match conversation_loader.load_creature(creature) {
            Ok(conversation) => {
                conversations.insert(creature.clone(), conversation);
            }
            Err(error) => eprintln!("warning: could not load creature {creature}: {error}"),
        }
    }
    let creature_animation_loader = CreatureAnimationLoader::new(&install);
    let creature_animations = content
        .actors
        .iter()
        .map(|actor| {
            creature_animation_loader
                .load_actor(
                    actor.animation_id,
                    actor.orientation,
                    actor.creature.as_ref(),
                )
                .map_err(|error| {
                    eprintln!(
                        "warning: could not load sprite for {} ({:#06x}): {error}",
                        actor.name, actor.animation_id
                    );
                })
                .ok()
        })
        .collect::<Vec<_>>();
    let scripted_behaviors = load_scripted_behaviors(&install, &content, &conversations);
    let area_animations = content
        .animations
        .iter()
        .filter(|placement| placement.flags & 1 != 0)
        .filter_map(|placement| {
            AnimationLoader::new(&install)
                .load_cycle(&placement.animation, placement.sequence)
                .map(|content| LoadedAreaAnimation {
                    placement: placement.clone(),
                    content,
                })
                .map_err(|error| {
                    eprintln!(
                        "warning: could not load area animation {} ({}): {error}",
                        placement.name, placement.animation
                    );
                })
                .ok()
        })
        .collect::<Vec<_>>();
    let xvart_id = ResRef::new("MXVTG1")?;
    let xvart = match AnimationLoader::new(&install).load_first_cycle(&xvart_id) {
        Ok(animation) => {
            println!(
                "loaded {} BAM animation: {} frames",
                animation.id,
                animation.frames.len()
            );
            Some(animation)
        }
        Err(error) => {
            eprintln!("warning: {error}; using generated Xvart marker");
            None
        }
    };
    println!(
        "loaded {area}: {}x{} pixels, {} actors, {} regions, {}/{} area animations from {}",
        content.base.width,
        content.base.height,
        content.actors.len(),
        content.regions.len(),
        area_animations.len(),
        content.animations.len(),
        game_root.display()
    );

    App::new()
        .insert_resource(ClearColor(Color::srgb(0.025, 0.025, 0.035)))
        .insert_resource(LoadedArea {
            content,
            xvart,
            area_animations,
            creature_animations,
            conversations,
            selected_start,
            scripted_behaviors,
        })
        .insert_resource(ConversationState::default())
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "OpenBG Area Viewer".into(),
                resolution: (1280, 720).into(),
                present_mode: PresentMode::AutoVsync,
                ..default()
            }),
            ..default()
        }))
        .add_systems(Startup, setup)
        .add_systems(
            Update,
            (
                camera_controls,
                right_click_npc,
                click_to_move,
                choose_wander_target,
                move_xvart,
                finish_npc_approach,
                choose_dialogue_reply,
                inventory_controls,
                animate_sprites,
                run_scripted_wander,
                reveal_fog,
                toggle_diagnostics,
                dismiss_conversation,
            )
                .chain(),
        )
        .run();
    Ok(())
}

#[derive(Resource)]
struct LoadedArea {
    content: AreaContent,
    xvart: Option<AnimationContent>,
    area_animations: Vec<LoadedAreaAnimation>,
    creature_animations: Vec<Option<CreatureAnimationContent>>,
    conversations: BTreeMap<ResRef, CreatureConversation>,
    selected_start: Option<[u16; 2]>,
    scripted_behaviors: Vec<Option<ScriptedBehavior>>,
}

struct LoadedAreaAnimation {
    placement: AreaAnimationPlacement,
    content: AnimationContent,
}

#[derive(Component)]
struct AreaCamera;

#[derive(Component)]
struct Xvart;

#[derive(Component)]
struct DestinationMarker;

#[derive(Component)]
struct Npc {
    name: String,
    creature: Option<ResRef>,
}

#[derive(Component)]
struct NpcInventory {
    items: Vec<CreatureItemContent>,
}

#[derive(Clone)]
struct ScriptedBehavior {
    script: ResRef,
    action: String,
}

#[derive(Component)]
struct ScriptedWander {
    points: [Vec2; 4],
    next: usize,
}

#[derive(Component)]
struct ConversationDisplay;

#[derive(Resource, Default)]
struct ConversationState {
    pending: Option<Entity>,
    active: Option<ActiveConversation>,
    times_talked: HashMap<Entity, u32>,
}

#[derive(Clone, Copy)]
struct ActiveConversation {
    npc: Entity,
    state: usize,
}

#[derive(Component)]
struct SelectionCircle;

#[derive(Component)]
struct FogLayer;

#[derive(Component)]
struct RegionMarker;

#[derive(Resource)]
struct FogOfWar {
    image: Handle<Image>,
    explored: Vec<bool>,
    last_center: Option<GridPoint>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum MovementMode {
    Wander,
    Player,
}

#[derive(Component)]
struct MovementIntent {
    target: Option<Vec2>,
    mode: MovementMode,
}

#[derive(Component, Default)]
struct NavigationPath {
    waypoints: Vec<Vec2>,
    next: usize,
}

#[derive(Component)]
struct WanderRoute {
    points: [Vec2; 4],
    next: usize,
}

#[derive(Component)]
struct FrameAnimation {
    frames: Vec<Handle<Image>>,
    offsets: Vec<Vec2>,
    current: usize,
    timer: Timer,
}

#[allow(clippy::needless_pass_by_value)] // Bevy system parameters are value wrappers.
fn setup(mut commands: Commands, mut images: ResMut<Assets<Image>>, area: Res<LoadedArea>) {
    let handle = images.add(bevy_image(&area.content.base));
    commands.spawn((
        Sprite::from_image(handle),
        Transform::from_xyz(0.0, 0.0, 0.0),
        Name::new(area.content.id.to_string()),
    ));
    commands.spawn((Camera2d, AreaCamera, Name::new("Area camera")));

    for animation in &area.area_animations {
        let frames = animation
            .content
            .frames
            .iter()
            .map(|frame| {
                images.add(bevy_area_animation_image(
                    &frame.image,
                    animation.placement.flags,
                ))
            })
            .collect::<Vec<_>>();
        if frames.is_empty() {
            continue;
        }
        let position = area_to_world(
            animation.placement.position,
            area.content.base.width,
            area.content.base.height,
        );
        let alpha = 1.0 - f32::from(animation.placement.transparency.min(255)) / 255.0;
        let offsets = animation
            .content
            .frames
            .iter()
            .map(animation_frame_offset)
            .collect::<Vec<_>>();
        let current = usize::from(animation.placement.frame) % frames.len();
        let mut sprite = Sprite::from_image(frames[current].clone());
        sprite.color = Color::srgba(1.0, 1.0, 1.0, alpha);
        sprite.flip_x = animation.placement.flags & (1 << 11) != 0;
        let mut entity = commands.spawn((
            sprite,
            Transform::from_translation((position + offsets[current]).extend(6.0)),
            Name::new(format!("ARE animation: {}", animation.placement.name)),
        ));
        if frames.len() > 1 {
            entity.insert(FrameAnimation {
                frames,
                offsets,
                current,
                timer: Timer::from_seconds(0.10, TimerMode::Repeating),
            });
        }
    }

    let npc_data = ImageData {
        width: 32,
        height: 48,
        rgba: make_npc_pixels(),
    };
    let npc_image = images.add(bevy_image(&npc_data));
    for (index, (actor, animation)) in area
        .content
        .actors
        .iter()
        .zip(&area.creature_animations)
        .enumerate()
    {
        let conversation = actor
            .creature
            .as_ref()
            .and_then(|creature| area.conversations.get(creature));
        let display_name = conversation
            .and_then(|conversation| conversation.display_name.as_deref())
            .unwrap_or(&actor.name);
        let position = area_to_world(
            actor.position,
            area.content.base.width,
            area.content.base.height,
        );
        let loaded_frames = animation.as_ref().map(|loaded| {
            loaded
                .animation
                .frames
                .iter()
                .map(|frame| images.add(bevy_image(&frame.image)))
                .collect::<Vec<_>>()
        });
        let (sprite, transform, frame_animation) = loaded_frames
            .filter(|frames| !frames.is_empty())
            .map(|frames| {
                let loaded = animation.as_ref().expect("frames came from this animation");
                let offsets = loaded
                    .animation
                    .frames
                    .iter()
                    .map(animation_frame_offset)
                    .collect::<Vec<_>>();
                let mut sprite = Sprite::from_image(frames[0].clone());
                sprite.flip_x = loaded.flip_x;
                let transform = Transform::from_translation((position + offsets[0]).extend(8.0));
                let frame_animation = (frames.len() > 1).then(|| FrameAnimation {
                    frames,
                    offsets,
                    current: 0,
                    timer: Timer::from_seconds(0.10, TimerMode::Repeating),
                });
                (sprite, transform, frame_animation)
            })
            .unwrap_or_else(|| {
                let mut sprite = Sprite::from_image(npc_image.clone());
                sprite.color = npc_color(display_name);
                (
                    sprite,
                    Transform::from_translation(position.extend(8.0)),
                    None,
                )
            });
        let mut entity = commands.spawn((
            sprite,
            transform,
            Npc {
                name: display_name.to_owned(),
                creature: actor.creature.clone(),
            },
            Name::new(format!("NPC: {display_name}")),
        ));
        if let Some(frame_animation) = frame_animation {
            entity.insert(frame_animation);
        }
        if let Some(inventory) = conversation
            .filter(|creature| !creature.inventory.is_empty())
            .map(|creature| creature.inventory.clone())
        {
            entity.insert(NpcInventory { items: inventory });
        }
        if let Some(behavior) = area.scripted_behaviors[index].as_ref() {
            println!(
                "script {}: {} executes {}",
                actor.name, behavior.script, behavior.action
            );
            entity.insert((
                ScriptedWander {
                    points: [
                        position + Vec2::new(30.0, 0.0),
                        position + Vec2::new(30.0, 30.0),
                        position + Vec2::new(-30.0, 30.0),
                        position,
                    ],
                    next: 0,
                },
                Name::new(format!(
                    "NPC: {display_name} [{}:{}]",
                    behavior.script, behavior.action
                )),
            ));
        }
    }

    for region in &area.content.regions {
        let [left, top, right, bottom] = region.bounds;
        let center = area_to_world(
            [
                left.saturating_add(right) / 2,
                top.saturating_add(bottom) / 2,
            ],
            area.content.base.width,
            area.content.base.height,
        );
        let size = Vec2::new(
            f32::from(right.abs_diff(left)).max(2.0),
            f32::from(bottom.abs_diff(top)).max(2.0),
        );
        let color = if region.kind == 2 {
            Color::srgba(0.15, 0.55, 1.0, 0.35)
        } else {
            Color::srgba(1.0, 0.25, 0.1, 0.28)
        };
        commands.spawn((
            Sprite::from_color(color, size),
            Transform::from_translation(center.extend(18.0)),
            Visibility::Hidden,
            RegionMarker,
            Name::new(format!("ARE region: {}", region.name)),
        ));
    }

    let animation_handles = area.xvart.as_ref().map(|animation| {
        animation
            .frames
            .iter()
            .map(|frame| images.add(bevy_image(&frame.image)))
            .collect::<Vec<_>>()
    });
    let (xvart_handle, scale) = animation_handles
        .as_ref()
        .and_then(|frames| frames.first().cloned().map(|frame| (frame, 1.0)))
        .unwrap_or_else(|| {
            let fallback = ImageData {
                width: 64,
                height: 64,
                rgba: make_xvart_pixels(),
            };
            (images.add(bevy_image(&fallback)), 1.5)
        });
    let requested_start = area
        .selected_start
        .map_or(Vec2::new(-300.0, -180.0), |position| {
            area_to_world(position, area.content.base.width, area.content.base.height)
        });
    let start = snap_to_walkable(requested_start, &area.content).unwrap_or(requested_start);
    let selection_data = ImageData {
        width: 80,
        height: 40,
        rgba: make_selection_pixels(80, 40),
    };
    let selection_image = images.add(bevy_image(&selection_data));
    commands.spawn((
        Sprite::from_image(selection_image),
        Transform::from_translation((start + Vec2::new(0.0, -18.0)).extend(9.0)),
        SelectionCircle,
        Name::new("Selected party member"),
    ));
    let mut xvart = commands.spawn((
        Sprite::from_image(xvart_handle),
        Transform::from_translation(start.extend(10.0)).with_scale(Vec3::splat(scale)),
        Xvart,
        MovementIntent {
            target: None,
            mode: MovementMode::Wander,
        },
        NavigationPath::default(),
        WanderRoute {
            points: [
                Vec2::new(300.0, -180.0),
                Vec2::new(300.0, 220.0),
                Vec2::new(-300.0, 220.0),
                Vec2::new(-300.0, -180.0),
            ],
            next: 0,
        },
        Name::new("Xvart"),
    ));
    if let Some(frames) = animation_handles.filter(|frames| frames.len() > 1) {
        xvart.insert(FrameAnimation {
            offsets: vec![Vec2::ZERO; frames.len()],
            frames,
            current: 0,
            timer: Timer::from_seconds(0.12, TimerMode::Repeating),
        });
    }

    let fog_width = u32::from(area.content.navigation.width());
    let fog_height = u32::from(area.content.navigation.height());
    let fog_pixels = vec![0_u8, 0, 0, 218]
        .into_iter()
        .cycle()
        .take(fog_width as usize * fog_height as usize * 4)
        .collect();
    let fog = Image::new(
        Extent3d {
            width: fog_width,
            height: fog_height,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        fog_pixels,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    );
    let fog_handle = images.add(fog);
    let mut fog_sprite = Sprite::from_image(fog_handle.clone());
    fog_sprite.custom_size = Some(Vec2::new(
        area.content.base.width as f32,
        area.content.base.height as f32,
    ));
    commands.spawn((
        fog_sprite,
        Transform::from_xyz(0.0, 0.0, 20.0),
        FogLayer,
        Name::new("Fog of war"),
    ));
    commands.insert_resource(FogOfWar {
        image: fog_handle,
        explored: vec![false; fog_width as usize * fog_height as usize],
        last_center: None,
    });
}

#[allow(clippy::needless_pass_by_value)] // Bevy system parameters are value wrappers.
fn reveal_fog(
    area: Res<LoadedArea>,
    xvart: Single<&Transform, With<Xvart>>,
    mut circle: Single<&mut Transform, (With<SelectionCircle>, Without<Xvart>)>,
    mut fog: ResMut<FogOfWar>,
    mut images: ResMut<Assets<Image>>,
) {
    let world = xvart.translation.truncate();
    circle.translation.x = world.x;
    circle.translation.y = world.y - 18.0;
    let center = world_to_grid(world, &area.content);
    if fog.last_center == Some(center) {
        return;
    }
    fog.last_center = Some(center);

    let width = usize::from(area.content.navigation.width());
    let height = usize::from(area.content.navigation.height());
    let radius_x = 20_i32;
    let radius_y = 27_i32;
    let mut changed = false;
    for y in 0..height {
        for x in 0..width {
            let dx = i32::try_from(x).unwrap_or(i32::MAX) - i32::from(center.x);
            let dy = i32::try_from(y).unwrap_or(i32::MAX) - i32::from(center.y);
            if dx * dx * radius_y * radius_y + dy * dy * radius_x * radius_x
                > radius_x * radius_x * radius_y * radius_y
            {
                continue;
            }
            let index = y * width + x;
            if !fog.explored[index] {
                fog.explored[index] = true;
                changed = true;
            }
        }
    }
    if !changed {
        return;
    }
    let explored = fog.explored.clone();
    if let Some(mut image) = images.get_mut(&fog.image) {
        if let Some(pixels) = image.data.as_mut() {
            for (index, is_explored) in explored.into_iter().enumerate() {
                if is_explored {
                    pixels[index * 4 + 3] = 0;
                }
            }
        }
    }
}

#[allow(clippy::needless_pass_by_value)] // Bevy system parameters are value wrappers.
fn toggle_diagnostics(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut fog: Query<&mut Visibility, (With<FogLayer>, Without<RegionMarker>)>,
    mut regions: Query<&mut Visibility, (With<RegionMarker>, Without<FogLayer>)>,
) {
    if keyboard.just_pressed(KeyCode::KeyF) {
        for mut visibility in &mut fog {
            *visibility = match *visibility {
                Visibility::Hidden => Visibility::Visible,
                _ => Visibility::Hidden,
            };
        }
    }
    if keyboard.just_pressed(KeyCode::KeyR) {
        for mut visibility in &mut regions {
            *visibility = match *visibility {
                Visibility::Hidden => Visibility::Visible,
                _ => Visibility::Hidden,
            };
        }
    }
}

fn bevy_image(image: &ImageData) -> Image {
    Image::new(
        Extent3d {
            width: image.width,
            height: image.height,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        image.rgba.clone(),
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    )
}

fn bevy_area_animation_image(image: &ImageData, flags: u32) -> Image {
    const BLACK_IS_TRANSPARENT: u32 = 1 << 1;
    if flags & BLACK_IS_TRANSPARENT == 0 {
        return bevy_image(image);
    }

    let mut keyed = image.clone();
    for pixel in keyed.rgba.chunks_exact_mut(4) {
        if pixel[0] == 0 && pixel[1] == 0 && pixel[2] == 0 {
            pixel[3] = 0;
        }
    }
    bevy_image(&keyed)
}

#[allow(clippy::cast_precision_loss)] // Infinity coordinates and images are bounded to u16 scale.
fn area_to_world(position: [u16; 2], width: u32, height: u32) -> Vec2 {
    Vec2::new(
        f32::from(position[0]) - width as f32 * 0.5,
        height as f32 * 0.5 - f32::from(position[1]),
    )
}

#[allow(clippy::needless_pass_by_value)] // Bevy system parameters are value wrappers.
fn animate_sprites(
    time: Res<Time>,
    mut animations: Query<(&mut Sprite, &mut Transform, &mut FrameAnimation)>,
) {
    for (mut sprite, mut transform, mut animation) in &mut animations {
        animation.timer.tick(time.delta());
        if animation.timer.just_finished() {
            let old_offset = animation.offsets[animation.current];
            animation.current = (animation.current + 1) % animation.frames.len();
            sprite.image = animation.frames[animation.current].clone();
            let offset = animation.offsets[animation.current] - old_offset;
            transform.translation.x += offset.x;
            transform.translation.y += offset.y;
        }
    }
}

#[allow(clippy::needless_pass_by_value)] // Bevy system parameters are value wrappers.
fn run_scripted_wander(
    time: Res<Time>,
    mut actors: Query<(&mut Transform, &mut Sprite, &mut ScriptedWander), Without<Xvart>>,
) {
    for (mut transform, mut sprite, mut wander) in &mut actors {
        let current = transform.translation.truncate();
        let target = wander.points[wander.next];
        let (next, arrived) = advance_position(current, target, 45.0 * time.delta_secs());
        sprite.flip_x = target.x < current.x;
        transform.translation.x = next.x;
        transform.translation.y = next.y;
        if arrived {
            wander.next = (wander.next + 1) % wander.points.len();
        }
    }
}

fn load_scripted_behaviors(
    install: &GameInstall,
    area: &AreaContent,
    conversations: &BTreeMap<ResRef, CreatureConversation>,
) -> Vec<Option<ScriptedBehavior>> {
    let ids = IdsLoader::new(install);
    let Ok(triggers) = ids.load(&ResRef::new("TRIGGER").expect("fixed resref")) else {
        return vec![None; area.actors.len()];
    };
    let Ok(actions) = ids.load(&ResRef::new("ACTION").expect("fixed resref")) else {
        return vec![None; area.actors.len()];
    };
    let Some(true_id) = triggers.value("True") else {
        return vec![None; area.actors.len()];
    };
    let supported = ["RandomWalk", "RandomWalkContinuous"]
        .into_iter()
        .filter_map(|name| actions.value(name).map(|id| (id, name)))
        .collect::<Vec<_>>();
    let loader = BcsLoader::new(install);
    area.actors
        .iter()
        .map(|actor| {
            let scripts = actor
                .creature
                .as_ref()
                .and_then(|creature| conversations.get(creature))?
                .scripts
                .clone();
            [
                scripts.override_script,
                scripts.class,
                scripts.race,
                scripts.general,
                scripts.default,
            ]
            .into_iter()
            .flatten()
            .filter(|script| script.as_str() != "NONE")
            .find_map(|script| {
                let compiled = loader.load(&script).ok()?;
                compiled.blocks.iter().find_map(|block| {
                    (block.trigger_ids == [true_id]).then_some(())?;
                    let (_, action) = supported
                        .iter()
                        .find(|(id, _)| block.action_ids.contains(id))?;
                    Some(ScriptedBehavior {
                        script: script.clone(),
                        action: (*action).to_owned(),
                    })
                })
            })
        })
        .collect()
}

#[allow(clippy::cast_precision_loss)] // BAM frame dimensions are u16-sized.
fn animation_frame_offset(frame: &openbg_content::AnimationFrame) -> Vec2 {
    Vec2::new(
        frame.image.width as f32 * 0.5 - f32::from(frame.center[0]),
        f32::from(frame.center[1]) - frame.image.height as f32 * 0.5,
    )
}

#[allow(clippy::needless_pass_by_value)] // Bevy system parameters are value wrappers.
#[allow(clippy::too_many_arguments)]
fn right_click_npc(
    buttons: Res<ButtonInput<MouseButton>>,
    window: Single<&Window, With<PrimaryWindow>>,
    camera: Single<(&Camera, &GlobalTransform), With<AreaCamera>>,
    area: Res<LoadedArea>,
    npcs: Query<(Entity, &Transform, &Npc), Without<Xvart>>,
    mut xvart: Single<
        (&Transform, &mut MovementIntent, &mut NavigationPath),
        (With<Xvart>, Without<Npc>),
    >,
    displays: Query<Entity, With<ConversationDisplay>>,
    mut conversation: ResMut<ConversationState>,
    mut commands: Commands,
) {
    if !buttons.just_pressed(MouseButton::Right) {
        return;
    }
    let Some(cursor) = window.cursor_position() else {
        return;
    };
    let (camera, camera_transform) = *camera;
    let Ok(world) = camera.viewport_to_world_2d(camera_transform, cursor) else {
        return;
    };
    let selected = npcs
        .iter()
        .map(|(entity, transform, _)| {
            (
                entity,
                transform.translation.truncate().distance_squared(world),
            )
        })
        .filter(|(_, distance)| *distance <= NPC_CLICK_RADIUS * NPC_CLICK_RADIUS)
        .min_by(|left, right| left.1.total_cmp(&right.1))
        .map(|(entity, _)| entity);

    let Some(entity) = selected else {
        clear_conversation(&mut commands, &displays, &mut conversation);
        return;
    };
    let Ok((_, npc_transform, npc)) = npcs.get(entity) else {
        return;
    };
    let npc_position = npc_transform.translation.truncate();
    let (xvart_transform, intent, path) = &mut *xvart;
    let xvart_position = xvart_transform.translation.truncate();
    if xvart_position.distance(npc_position) <= TALK_DISTANCE {
        intent.target = None;
        path.waypoints.clear();
        path.next = 0;
        conversation.pending = None;
        start_conversation(
            &mut commands,
            &displays,
            &area,
            &mut conversation,
            entity,
            npc_position,
            npc,
        );
    } else if assign_path(xvart_position, npc_position, &area.content, intent, path).is_some() {
        intent.mode = MovementMode::Player;
        conversation.pending = Some(entity);
        conversation.active = None;
        despawn_conversation(&mut commands, &displays);
    }
}

#[allow(clippy::needless_pass_by_value)] // Bevy system parameters are value wrappers.
fn finish_npc_approach(
    area: Res<LoadedArea>,
    npcs: Query<(&Transform, &Npc), Without<Xvart>>,
    mut xvart: Single<
        (&Transform, &mut MovementIntent, &mut NavigationPath),
        (With<Xvart>, Without<Npc>),
    >,
    displays: Query<Entity, With<ConversationDisplay>>,
    mut conversation: ResMut<ConversationState>,
    mut commands: Commands,
) {
    let Some(entity) = conversation.pending else {
        return;
    };
    let Ok((npc_transform, npc)) = npcs.get(entity) else {
        conversation.pending = None;
        return;
    };
    let npc_position = npc_transform.translation.truncate();
    let (xvart_transform, intent, path) = &mut *xvart;
    if xvart_transform
        .translation
        .truncate()
        .distance(npc_position)
        > TALK_DISTANCE
    {
        return;
    }
    intent.target = None;
    path.waypoints.clear();
    path.next = 0;
    conversation.pending = None;
    start_conversation(
        &mut commands,
        &displays,
        &area,
        &mut conversation,
        entity,
        npc_position,
        npc,
    );
}

fn start_conversation(
    commands: &mut Commands,
    displays: &Query<Entity, With<ConversationDisplay>>,
    area: &LoadedArea,
    conversation: &mut ConversationState,
    entity: Entity,
    npc_position: Vec2,
    npc: &Npc,
) {
    let talked = *conversation.times_talked.get(&entity).unwrap_or(&0);
    let Some(dialogue) = npc
        .creature
        .as_ref()
        .and_then(|creature| area.conversations.get(creature))
        .and_then(|creature| creature.dialogue.as_ref())
    else {
        conversation.active = None;
        show_conversation_text(
            commands,
            displays,
            npc_position,
            npc,
            "This creature has no assigned dialogue.",
        );
        return;
    };
    if dialogue.states.is_empty() {
        conversation.active = None;
        show_conversation_text(
            commands,
            displays,
            npc_position,
            npc,
            "The assigned dialogue contains no states.",
        );
        return;
    }
    let state = dialogue
        .states
        .iter()
        .position(|state| trigger_matches(state.trigger.as_deref(), talked))
        .unwrap_or(0);
    conversation.times_talked.insert(entity, talked + 1);
    conversation.active = Some(ActiveConversation { npc: entity, state });
    show_dialogue_state(
        commands,
        displays,
        npc_position,
        npc,
        &dialogue.states[state],
        talked,
    );
}

fn show_dialogue_state(
    commands: &mut Commands,
    displays: &Query<Entity, With<ConversationDisplay>>,
    npc_position: Vec2,
    npc: &Npc,
    state: &DialogueStateContent,
    talked: u32,
) {
    let mut text = format!("{}\n{}", npc.name, state.text);
    let replies = visible_transitions(state, talked);
    for (number, transition) in replies.iter().take(9).enumerate() {
        let reply = transition
            .text
            .as_deref()
            .unwrap_or(if transition.terminates {
                "[End conversation]"
            } else {
                "[Continue]"
            });
        text.push_str(&format!("\n{}. {reply}", number + 1));
    }
    if replies.is_empty() {
        text.push_str("\n[No currently valid replies — Esc closes]");
    }
    show_conversation_text(commands, displays, npc_position, npc, &text);
}

fn show_conversation_text(
    commands: &mut Commands,
    displays: &Query<Entity, With<ConversationDisplay>>,
    npc_position: Vec2,
    npc: &Npc,
    text: &str,
) {
    despawn_conversation(commands, displays);
    let panel_position = npc_position + Vec2::new(0.0, 145.0);
    commands.spawn((
        Sprite::from_color(
            Color::srgba(0.035, 0.025, 0.02, 0.94),
            Vec2::new(580.0, 240.0),
        ),
        Transform::from_translation(panel_position.extend(40.0)),
        ConversationDisplay,
        Name::new("Conversation background"),
    ));
    commands.spawn((
        Text2d::new(text),
        TextLayout::justify(Justify::Center),
        TextFont::from_font_size(16.0),
        TextBounds::new(540.0, 220.0),
        TextColor(Color::srgb(0.96, 0.86, 0.68)),
        Transform::from_translation(panel_position.extend(41.0)),
        ConversationDisplay,
        Name::new(format!("Conversation with {}", npc.name)),
    ));
}

fn visible_transitions(
    state: &DialogueStateContent,
    talked: u32,
) -> Vec<&DialogueTransitionContent> {
    state
        .transitions
        .iter()
        .filter(|transition| trigger_matches(transition.trigger.as_deref(), talked))
        .collect()
}

#[allow(clippy::needless_pass_by_value)] // Bevy system parameters are value wrappers.
fn inventory_controls(
    keyboard: Res<ButtonInput<KeyCode>>,
    conversation: Res<ConversationState>,
    mut npcs: Query<(&Transform, &Npc, &mut NpcInventory, &mut Sprite)>,
    displays: Query<Entity, With<ConversationDisplay>>,
    mut commands: Commands,
) {
    if !keyboard.just_pressed(KeyCode::KeyI) && !keyboard.just_pressed(KeyCode::KeyE) {
        return;
    }
    let Some(active) = conversation.active else {
        return;
    };
    let Ok((transform, npc, mut inventory, mut sprite)) = npcs.get_mut(active.npc) else {
        return;
    };
    let changed = keyboard
        .just_pressed(KeyCode::KeyE)
        .then(|| equip_next_item(&mut inventory.items))
        .flatten();
    if let Some(index) = changed {
        sprite.color = equipment_debug_color(inventory.items[index].item_type);
    }
    show_inventory_panel(
        &mut commands,
        &displays,
        transform.translation.truncate(),
        npc,
        &inventory.items,
        changed,
    );
}

fn equip_next_item(items: &mut [CreatureItemContent]) -> Option<usize> {
    let candidate = items
        .iter()
        .enumerate()
        .find(|(_, item)| !item.equipped && equipment_slot(item.item_type).is_some())?
        .0;
    let slot = equipment_slot(items[candidate].item_type)?;
    for item in items.iter_mut() {
        if item.equipped && item.slot == Some(slot) {
            item.equipped = false;
            item.slot = None;
        }
    }
    items[candidate].equipped = true;
    items[candidate].slot = Some(slot);
    Some(candidate)
}

fn equipment_slot(item_type: u16) -> Option<usize> {
    match item_type {
        7 => Some(0),  // helmet
        2 => Some(1),  // armor/robe
        12 => Some(2), // shield
        3 => Some(7),  // belt
        4 => Some(8),  // boots
        15..=31 => Some(9),
        32 => Some(17), // cloak
        _ => None,
    }
}

fn equipment_debug_color(item_type: u16) -> Color {
    match equipment_slot(item_type) {
        Some(0 | 1 | 2 | 7 | 8 | 17) => Color::srgb(0.72, 0.88, 1.0),
        Some(_) => Color::srgb(1.0, 0.82, 0.62),
        None => Color::WHITE,
    }
}

fn show_inventory_panel(
    commands: &mut Commands,
    displays: &Query<Entity, With<ConversationDisplay>>,
    npc_position: Vec2,
    npc: &Npc,
    items: &[CreatureItemContent],
    changed: Option<usize>,
) {
    let mut text = format!(
        "{} inventory — I refreshes, E equips next supported item",
        npc.name
    );
    for (index, item) in items.iter().take(12).enumerate() {
        let marker = if item.equipped { "[equipped]" } else { "" };
        let changed = if changed == Some(index) {
            " <- equipped"
        } else {
            ""
        };
        let name = item.display_name.as_deref().unwrap_or(item.id.as_str());
        text.push_str(&format!(
            "\n{marker:10} {name} ({}, slot {:?}, {} lb){changed}",
            item.id, item.slot, item.weight
        ));
    }
    show_conversation_text(commands, displays, npc_position, npc, &text);
}

fn trigger_matches(trigger: Option<&str>, talked: u32) -> bool {
    let Some(trigger) = trigger else {
        return true;
    };
    let compact = trigger
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect::<String>()
        .to_ascii_uppercase();
    if compact.is_empty() || compact == "TRUE()" {
        return true;
    }
    if let Some(argument) = compact
        .strip_prefix("NUMTIMESTALKEDTO(")
        .and_then(|value| value.strip_suffix(')'))
        .and_then(|value| value.parse::<u32>().ok())
    {
        return talked == argument;
    }
    if let Some(argument) = compact
        .strip_prefix("NUMTIMESTALKEDTOGT(")
        .and_then(|value| value.strip_suffix(')'))
        .and_then(|value| value.parse::<u32>().ok())
    {
        return talked > argument;
    }
    false
}

#[allow(clippy::needless_pass_by_value)] // Bevy system parameters are value wrappers.
#[allow(clippy::too_many_arguments)]
fn choose_dialogue_reply(
    keyboard: Res<ButtonInput<KeyCode>>,
    area: Res<LoadedArea>,
    npcs: Query<(&Transform, &Npc), Without<Xvart>>,
    displays: Query<Entity, With<ConversationDisplay>>,
    mut conversation: ResMut<ConversationState>,
    mut xvart: Single<&mut MovementIntent, With<Xvart>>,
    mut commands: Commands,
) {
    let selected = [
        KeyCode::Digit1,
        KeyCode::Digit2,
        KeyCode::Digit3,
        KeyCode::Digit4,
        KeyCode::Digit5,
        KeyCode::Digit6,
        KeyCode::Digit7,
        KeyCode::Digit8,
        KeyCode::Digit9,
    ]
    .iter()
    .position(|key| keyboard.just_pressed(*key));
    let (Some(selected), Some(active)) = (selected, conversation.active) else {
        return;
    };
    let Ok((transform, npc)) = npcs.get(active.npc) else {
        clear_conversation(&mut commands, &displays, &mut conversation);
        return;
    };
    let Some(dialogue) = npc
        .creature
        .as_ref()
        .and_then(|creature| area.conversations.get(creature))
        .and_then(|creature| creature.dialogue.as_ref())
    else {
        return;
    };
    let Some(state) = dialogue.states.get(active.state) else {
        return;
    };
    let talked = conversation
        .times_talked
        .get(&active.npc)
        .copied()
        .unwrap_or(1)
        .saturating_sub(1);
    let replies = visible_transitions(state, talked);
    let Some(reply) = replies.get(selected).copied() else {
        return;
    };
    if let Some(action) = &reply.action {
        eprintln!("dialogue action retained but not executed yet: {action:?}");
    }
    if reply.terminates {
        clear_conversation(&mut commands, &displays, &mut conversation);
        xvart.mode = MovementMode::Wander;
        return;
    }
    if reply
        .next_dialogue
        .as_ref()
        .is_some_and(|next| next != &dialogue.id)
    {
        show_conversation_text(
            &mut commands,
            &displays,
            transform.translation.truncate(),
            npc,
            "This reply continues in another DLG resource; cross-dialogue loading is next.",
        );
        return;
    }
    let Some(next_state) = reply
        .next_state
        .and_then(|state| usize::try_from(state).ok())
    else {
        return;
    };
    let Some(next) = dialogue.states.get(next_state) else {
        return;
    };
    conversation.active = Some(ActiveConversation {
        npc: active.npc,
        state: next_state,
    });
    show_dialogue_state(
        &mut commands,
        &displays,
        transform.translation.truncate(),
        npc,
        next,
        talked,
    );
}

fn despawn_conversation(
    commands: &mut Commands,
    displays: &Query<Entity, With<ConversationDisplay>>,
) {
    for entity in displays {
        commands.entity(entity).despawn();
    }
}

fn clear_conversation(
    commands: &mut Commands,
    displays: &Query<Entity, With<ConversationDisplay>>,
    conversation: &mut ConversationState,
) {
    conversation.pending = None;
    conversation.active = None;
    despawn_conversation(commands, displays);
}

#[allow(clippy::needless_pass_by_value)] // Bevy system parameters are value wrappers.
fn dismiss_conversation(
    keyboard: Res<ButtonInput<KeyCode>>,
    displays: Query<Entity, With<ConversationDisplay>>,
    mut conversation: ResMut<ConversationState>,
    mut xvart: Single<&mut MovementIntent, With<Xvart>>,
    mut commands: Commands,
) {
    if keyboard.just_pressed(KeyCode::Escape) {
        clear_conversation(&mut commands, &displays, &mut conversation);
        xvart.mode = MovementMode::Wander;
    }
}

#[allow(clippy::needless_pass_by_value)] // Bevy system parameters are value wrappers.
#[allow(clippy::cast_precision_loss)] // Image dimensions are bounded far below f32 integer precision.
fn click_to_move(
    buttons: Res<ButtonInput<MouseButton>>,
    window: Single<&Window, With<PrimaryWindow>>,
    camera: Single<(&Camera, &GlobalTransform), With<AreaCamera>>,
    area: Res<LoadedArea>,
    mut xvart: Single<(&Transform, &mut MovementIntent, &mut NavigationPath), With<Xvart>>,
    markers: Query<Entity, With<DestinationMarker>>,
    mut commands: Commands,
) {
    if !buttons.just_pressed(MouseButton::Left) {
        return;
    }
    let Some(cursor) = window.cursor_position() else {
        return;
    };
    let (camera, camera_transform) = *camera;
    let Ok(world) = camera.viewport_to_world_2d(camera_transform, cursor) else {
        return;
    };
    let half_width = area.content.base.width as f32 * 0.5;
    let half_height = area.content.base.height as f32 * 0.5;
    let requested_target = Vec2::new(
        world.x.clamp(-half_width, half_width),
        world.y.clamp(-half_height, half_height),
    );
    let (transform, intent, path) = &mut *xvart;
    let current = transform.translation.truncate();
    let Some(target) = assign_path(current, requested_target, &area.content, intent, path) else {
        eprintln!(
            "no walkable path to click at ({:.0}, {:.0})",
            world.x, world.y
        );
        return;
    };
    intent.mode = MovementMode::Player;

    for marker in &markers {
        commands.entity(marker).despawn();
    }
    commands.spawn((
        Sprite::from_color(Color::srgba(0.2, 1.0, 0.25, 0.8), Vec2::splat(14.0)),
        Transform::from_translation(target.extend(5.0)).with_rotation(Quat::from_rotation_z(0.785)),
        DestinationMarker,
        Name::new("Move destination"),
    ));
}

#[allow(clippy::needless_pass_by_value)] // Bevy system parameters are value wrappers.
fn choose_wander_target(
    area: Res<LoadedArea>,
    mut xvart: Single<
        (
            &Transform,
            &mut MovementIntent,
            &mut NavigationPath,
            &mut WanderRoute,
        ),
        With<Xvart>,
    >,
) {
    let (transform, intent, path, route) = &mut *xvart;
    if intent.mode == MovementMode::Wander && intent.target.is_none() {
        let requested = route.points[route.next];
        route.next = (route.next + 1) % route.points.len();
        assign_path(
            transform.translation.truncate(),
            requested,
            &area.content,
            intent,
            path,
        );
    }
}

#[allow(clippy::needless_pass_by_value)] // Bevy system parameters are value wrappers.
fn move_xvart(
    time: Res<Time>,
    mut xvart: Single<
        (
            &mut Transform,
            &mut Sprite,
            &mut MovementIntent,
            &mut NavigationPath,
        ),
        With<Xvart>,
    >,
    markers: Query<Entity, With<DestinationMarker>>,
    mut commands: Commands,
) {
    let (transform, sprite, intent, path) = &mut *xvart;
    let Some(target) = intent.target else {
        return;
    };
    let current = transform.translation.truncate();
    let (next, arrived) = advance_position(current, target, XVART_SPEED * time.delta_secs());
    sprite.flip_x = target.x < current.x;
    transform.translation.x = next.x;
    transform.translation.y = next.y;

    if arrived || next.distance_squared(target) <= ARRIVAL_DISTANCE * ARRIVAL_DISTANCE {
        path.next += 1;
        if let Some(next_target) = path.waypoints.get(path.next).copied() {
            intent.target = Some(next_target);
        } else {
            let was_player_order = intent.mode == MovementMode::Player;
            intent.target = None;
            intent.mode = MovementMode::Wander;
            path.waypoints.clear();
            path.next = 0;
            if was_player_order {
                for marker in &markers {
                    commands.entity(marker).despawn();
                }
            }
        }
    }
}

fn assign_path(
    current: Vec2,
    requested_target: Vec2,
    area: &AreaContent,
    intent: &mut MovementIntent,
    path: &mut NavigationPath,
) -> Option<Vec2> {
    let start = world_to_grid(current, area);
    let goal = world_to_grid(requested_target, area);
    let start = area.navigation.nearest_walkable(start, 12)?;
    let goal = area.navigation.nearest_walkable(goal, 12)?;
    let cells = find_path(&area.navigation, start, goal).ok()?;
    let waypoints = cells
        .into_iter()
        .skip(1)
        .map(|point| grid_to_world(point, area))
        .collect::<Vec<_>>();
    let first = waypoints
        .first()
        .copied()
        .unwrap_or_else(|| grid_to_world(goal, area));
    path.waypoints = waypoints;
    path.next = 0;
    intent.target = Some(first);
    Some(grid_to_world(goal, area))
}

fn snap_to_walkable(world: Vec2, area: &AreaContent) -> Option<Vec2> {
    area.navigation
        .nearest_walkable(world_to_grid(world, area), 24)
        .map(|point| grid_to_world(point, area))
}

#[allow(clippy::cast_precision_loss)] // Area/search dimensions are bounded to u16 scale.
fn world_to_grid(world: Vec2, area: &AreaContent) -> GridPoint {
    let width = area.base.width as f32;
    let height = area.base.height as f32;
    let grid_width = f32::from(area.navigation.width());
    let grid_height = f32::from(area.navigation.height());
    let area_x = (world.x + width * 0.5).clamp(0.0, width - f32::EPSILON);
    let area_y = (height * 0.5 - world.y).clamp(0.0, height - f32::EPSILON);
    GridPoint::new(
        u16::try_from((area_x * grid_width / width).floor() as u32)
            .expect("clamped grid X fits u16"),
        u16::try_from((area_y * grid_height / height).floor() as u32)
            .expect("clamped grid Y fits u16"),
    )
}

#[allow(clippy::cast_precision_loss)] // Area/search dimensions are bounded to u16 scale.
fn grid_to_world(point: GridPoint, area: &AreaContent) -> Vec2 {
    let width = area.base.width as f32;
    let height = area.base.height as f32;
    let x = (f32::from(point.x) + 0.5) * width / f32::from(area.navigation.width());
    let y = (f32::from(point.y) + 0.5) * height / f32::from(area.navigation.height());
    Vec2::new(x - width * 0.5, height * 0.5 - y)
}

fn advance_position(current: Vec2, target: Vec2, maximum_distance: f32) -> (Vec2, bool) {
    let offset = target - current;
    let distance = offset.length();
    if distance <= maximum_distance || distance <= f32::EPSILON {
        (target, true)
    } else {
        (current + offset / distance * maximum_distance, false)
    }
}

fn make_xvart_pixels() -> Vec<u8> {
    let mut pixels = vec![0_u8; 64 * 64 * 4];
    for pixel_y in 0..64_usize {
        for pixel_x in 0..64_usize {
            let x = i32::try_from(pixel_x).expect("sprite X coordinate fits i32");
            let y = i32::try_from(pixel_y).expect("sprite Y coordinate fits i32");
            let selection = {
                let dx = x - 32;
                let dy = (y - 56) * 3;
                let radius = dx * dx + dy * dy;
                (650..=900).contains(&radius)
            };
            let legs = ((23..=29).contains(&x) || (35..=41).contains(&x)) && (43..=55).contains(&y);
            let body = ellipse(x, y, 32, 39, 12, 17);
            let ears = ((10..=20).contains(&x) || (44..=54).contains(&x)) && (13..=25).contains(&y);
            let head = ellipse(x, y, 32, 23, 15, 13);
            let eye = ellipse(x, y, 27, 21, 3, 4) || ellipse(x, y, 37, 21, 3, 4);
            let pupil = (x == 28 || x == 38) && (20..=22).contains(&y);
            let mouth = (27..=37).contains(&x) && y == 29;

            let color = if pupil || mouth {
                Some([20, 18, 28, 255])
            } else if eye {
                Some([240, 238, 210, 255])
            } else if head || ears || body || legs {
                Some([45, 105, 205, 255])
            } else if selection {
                Some([60, 255, 80, 210])
            } else {
                None
            };
            if let Some(color) = color {
                let index = (pixel_y * 64 + pixel_x) * 4;
                pixels[index..index + 4].copy_from_slice(&color);
            }
        }
    }
    pixels
}

fn make_selection_pixels(width: usize, height: usize) -> Vec<u8> {
    let mut pixels = vec![0_u8; width * height * 4];
    let center_x = width as f32 * 0.5;
    let center_y = height as f32 * 0.5;
    let outer_x = center_x - 2.0;
    let outer_y = center_y - 2.0;
    for y in 0..height {
        for x in 0..width {
            let dx = (x as f32 + 0.5 - center_x) / outer_x;
            let dy = (y as f32 + 0.5 - center_y) / outer_y;
            let distance = dx * dx + dy * dy;
            if (0.72..=1.0).contains(&distance) {
                let offset = (y * width + x) * 4;
                pixels[offset..offset + 4].copy_from_slice(&[45, 255, 75, 220]);
            }
        }
    }
    pixels
}

fn make_npc_pixels() -> Vec<u8> {
    let mut pixels = vec![0_u8; 32 * 48 * 4];
    for y in 0..48_usize {
        for x in 0..32_usize {
            let x_i = i32::try_from(x).expect("NPC sprite X fits i32");
            let y_i = i32::try_from(y).expect("NPC sprite Y fits i32");
            let head = ellipse(x_i, y_i, 16, 10, 7, 8);
            let body = (9..=23).contains(&x) && (17..=34).contains(&y);
            let legs = ((10..=14).contains(&x) || (18..=22).contains(&x)) && (35..=46).contains(&y);
            if head || body || legs {
                let offset = (y * 32 + x) * 4;
                pixels[offset..offset + 4].copy_from_slice(&[255, 255, 255, 235]);
            }
        }
    }
    pixels
}

fn npc_color(name: &str) -> Color {
    if name.eq_ignore_ascii_case("cow") {
        Color::srgb(0.72, 0.52, 0.30)
    } else if name.eq_ignore_ascii_case("seagul") {
        Color::srgb(0.92, 0.95, 1.0)
    } else if name.eq_ignore_ascii_case("watcher") || name.eq_ignore_ascii_case("gatewarden") {
        Color::srgb(0.55, 0.72, 0.92)
    } else if name.eq_ignore_ascii_case("tutor") {
        Color::srgb(0.82, 0.68, 0.32)
    } else {
        Color::srgb(0.92, 0.58, 0.22)
    }
}

fn ellipse(x: i32, y: i32, center_x: i32, center_y: i32, rx: i32, ry: i32) -> bool {
    let dx = x - center_x;
    let dy = y - center_y;
    dx * dx * ry * ry + dy * dy * rx * rx <= rx * rx * ry * ry
}

#[allow(clippy::needless_pass_by_value)] // Bevy system parameters are value wrappers.
fn camera_controls(
    time: Res<Time>,
    keyboard: Res<ButtonInput<KeyCode>>,
    mut wheel: MessageReader<MouseWheel>,
    mut camera: Single<(&mut Transform, &mut Projection), With<AreaCamera>>,
) {
    let (transform, projection) = &mut *camera;
    let mut direction = Vec2::ZERO;
    if keyboard.pressed(KeyCode::KeyA) || keyboard.pressed(KeyCode::ArrowLeft) {
        direction.x -= 1.0;
    }
    if keyboard.pressed(KeyCode::KeyD) || keyboard.pressed(KeyCode::ArrowRight) {
        direction.x += 1.0;
    }
    if keyboard.pressed(KeyCode::KeyW) || keyboard.pressed(KeyCode::ArrowUp) {
        direction.y += 1.0;
    }
    if keyboard.pressed(KeyCode::KeyS) || keyboard.pressed(KeyCode::ArrowDown) {
        direction.y -= 1.0;
    }
    if direction != Vec2::ZERO {
        let speed = 700.0 * time.delta_secs();
        transform.translation += (direction.normalize() * speed).extend(0.0);
    }

    let scroll = wheel.read().map(|event| event.y).sum::<f32>();
    if scroll != 0.0 {
        if let Projection::Orthographic(orthographic) = &mut **projection {
            orthographic.scale = (orthographic.scale * 0.85_f32.powf(scroll)).clamp(0.1, 8.0);
        }
    }
}

fn arguments() -> Result<(PathBuf, ResRef), ViewerError> {
    let mut arguments = std::env::args_os().skip(1);
    let game_root = arguments
        .next()
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("OPENBG_GAME").map(PathBuf::from))
        .ok_or(ViewerError::Usage)?;
    let area = arguments
        .next()
        .and_then(|value| value.into_string().ok())
        .unwrap_or_else(|| DEFAULT_AREA.to_owned());
    let area = ResRef::new(area).map_err(|error| ViewerError::Data(error.to_string()))?;
    Ok((game_root, area))
}

fn start_position(table: &TwoDa, area: &ResRef) -> Option<[u16; 2]> {
    if !table
        .get("START_AREA", "VALUE")?
        .eq_ignore_ascii_case(area.as_str())
    {
        return None;
    }
    let x = table.get("START_XPOS", "VALUE")?.parse().ok()?;
    let y = table.get("START_YPOS", "VALUE")?.parse().ok()?;
    Some([x, y])
}

#[derive(Debug)]
enum ViewerError {
    Usage,
    Data(String),
}

impl fmt::Display for ViewerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Usage => formatter.write_str(
                "usage: openbg-area <game-directory> [area-resref]\n       or set OPENBG_GAME",
            ),
            Self::Data(message) => formatter.write_str(message),
        }
    }
}

impl Error for ViewerError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Usage | Self::Data(_) => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use bevy::prelude::Vec2;
    use openbg_content::CreatureItemContent;
    use openbg_domain::ResRef;
    use openbg_formats::TwoDa;

    use super::{advance_position, equip_next_item, start_position, trigger_matches};

    #[test]
    fn movement_advances_without_overshooting() {
        let (position, arrived) = advance_position(Vec2::ZERO, Vec2::new(10.0, 0.0), 4.0);
        assert_eq!(position, Vec2::new(4.0, 0.0));
        assert!(!arrived);
    }

    #[test]
    fn movement_snaps_to_a_reachable_target() {
        let target = Vec2::new(3.0, 4.0);
        let (position, arrived) = advance_position(Vec2::ZERO, target, 5.0);
        assert_eq!(position, target);
        assert!(arrived);
    }

    #[test]
    fn evaluates_only_the_supported_dialogue_trigger_subset() {
        assert!(trigger_matches(None, 0));
        assert!(trigger_matches(Some(" True()\r\n"), 0));
        assert!(trigger_matches(Some("NumTimesTalkedTo(0)"), 0));
        assert!(trigger_matches(Some("NumTimesTalkedToGT(2)"), 3));
        assert!(!trigger_matches(Some("NumTimesTalkedTo(0)"), 1));
        assert!(!trigger_matches(
            Some("Global(\"Chapter\",\"GLOBAL\",1)"),
            0
        ));
    }

    #[test]
    fn reads_selected_actor_start_from_rules_table() {
        let table = TwoDa::parse(
            b"2DA V1.0\nBADVAL\nVALUE\nSTART_AREA AR2600\nSTART_XPOS 1080\nSTART_YPOS 530\n",
        )
        .expect("valid start table");
        let area = ResRef::new("AR2600").expect("valid area");
        assert_eq!(start_position(&table, &area), Some([1080, 530]));
        assert_eq!(
            start_position(&table, &ResRef::new("AR0100").expect("valid area")),
            None
        );
    }

    #[test]
    fn equips_supported_inventory_item_into_canonical_slot() {
        let mut items = vec![
            CreatureItemContent {
                id: ResRef::new("LEAT01").expect("valid item"),
                display_name: Some("Leather Armor".into()),
                item_type: 2,
                equipped_appearance: "2A".into(),
                price: 1,
                weight: 15,
                charges: [0; 3],
                flags: 1,
                slot: Some(1),
                equipped: true,
            },
            CreatureItemContent {
                id: ResRef::new("PLAT01").expect("valid item"),
                display_name: Some("Plate Mail".into()),
                item_type: 2,
                equipped_appearance: "4A".into(),
                price: 1,
                weight: 50,
                charges: [0; 3],
                flags: 1,
                slot: None,
                equipped: false,
            },
        ];

        assert_eq!(equip_next_item(&mut items), Some(1));
        assert!(!items[0].equipped);
        assert_eq!(items[0].slot, None);
        assert!(items[1].equipped);
        assert_eq!(items[1].slot, Some(1));
    }
}
