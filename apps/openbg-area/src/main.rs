use std::error::Error;
use std::fmt;
use std::path::PathBuf;

use bevy::asset::RenderAssetUsages;
use bevy::input::mouse::MouseWheel;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::window::{PresentMode, PrimaryWindow, WindowPlugin};
use openbg_catalog::GameInstall;
use openbg_content::{AnimationContent, AnimationLoader, AreaContent, AreaLoader, ImageData};
use openbg_domain::ResRef;

const DEFAULT_AREA: &str = "AR2600";
const XVART_SPEED: f32 = 180.0;
const ARRIVAL_DISTANCE: f32 = 2.0;

fn main() -> Result<(), Box<dyn Error>> {
    let (game_root, area) = arguments()?;
    let install = GameInstall::open(&game_root)?;
    let content = AreaLoader::new(&install).load(&area)?;
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
        "loaded {area}: {}x{} pixels, {} ARE actors from {}",
        content.base.width,
        content.base.height,
        content.actors.len(),
        game_root.display()
    );

    App::new()
        .insert_resource(ClearColor(Color::srgb(0.025, 0.025, 0.035)))
        .insert_resource(LoadedArea { content, xvart })
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
                click_to_move,
                choose_wander_target,
                move_xvart,
                animate_sprites,
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
}

#[derive(Component)]
struct AreaCamera;

#[derive(Component)]
struct Xvart;

#[derive(Component)]
struct DestinationMarker;

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

#[derive(Component)]
struct WanderRoute {
    points: [Vec2; 4],
    next: usize,
}

#[derive(Component)]
struct FrameAnimation {
    frames: Vec<Handle<Image>>,
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

    for actor in &area.content.actors {
        let position = area_to_world(
            actor.position,
            area.content.base.width,
            area.content.base.height,
        );
        commands.spawn((
            Sprite::from_color(Color::srgba(1.0, 0.72, 0.15, 0.72), Vec2::new(10.0, 16.0)),
            Transform::from_translation(position.extend(4.0)),
            Name::new(format!("ARE actor: {}", actor.name)),
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
    let mut xvart = commands.spawn((
        Sprite::from_image(xvart_handle),
        Transform::from_xyz(-300.0, -180.0, 10.0).with_scale(Vec3::splat(scale)),
        Xvart,
        MovementIntent {
            target: None,
            mode: MovementMode::Wander,
        },
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
            frames,
            current: 0,
            timer: Timer::from_seconds(0.12, TimerMode::Repeating),
        });
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

#[allow(clippy::cast_precision_loss)] // Infinity coordinates and images are bounded to u16 scale.
fn area_to_world(position: [u16; 2], width: u32, height: u32) -> Vec2 {
    Vec2::new(
        f32::from(position[0]) - width as f32 * 0.5,
        height as f32 * 0.5 - f32::from(position[1]),
    )
}

#[allow(clippy::needless_pass_by_value)] // Bevy system parameters are value wrappers.
fn animate_sprites(time: Res<Time>, mut animations: Query<(&mut Sprite, &mut FrameAnimation)>) {
    for (mut sprite, mut animation) in &mut animations {
        animation.timer.tick(time.delta());
        if animation.timer.just_finished() {
            animation.current = (animation.current + 1) % animation.frames.len();
            sprite.image = animation.frames[animation.current].clone();
        }
    }
}

#[allow(clippy::needless_pass_by_value)] // Bevy system parameters are value wrappers.
#[allow(clippy::cast_precision_loss)] // Image dimensions are bounded far below f32 integer precision.
fn click_to_move(
    buttons: Res<ButtonInput<MouseButton>>,
    window: Single<&Window, With<PrimaryWindow>>,
    camera: Single<(&Camera, &GlobalTransform), With<AreaCamera>>,
    area: Res<LoadedArea>,
    mut xvart: Single<&mut MovementIntent, With<Xvart>>,
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
    let target = Vec2::new(
        world.x.clamp(-half_width, half_width),
        world.y.clamp(-half_height, half_height),
    );
    xvart.target = Some(target);
    xvart.mode = MovementMode::Player;

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

fn choose_wander_target(mut xvart: Single<(&mut MovementIntent, &mut WanderRoute), With<Xvart>>) {
    let (intent, route) = &mut *xvart;
    if intent.mode == MovementMode::Wander && intent.target.is_none() {
        intent.target = Some(route.points[route.next]);
        route.next = (route.next + 1) % route.points.len();
    }
}

#[allow(clippy::needless_pass_by_value)] // Bevy system parameters are value wrappers.
fn move_xvart(
    time: Res<Time>,
    mut xvart: Single<(&mut Transform, &mut Sprite, &mut MovementIntent), With<Xvart>>,
    markers: Query<Entity, With<DestinationMarker>>,
    mut commands: Commands,
) {
    let (transform, sprite, intent) = &mut *xvart;
    let Some(target) = intent.target else {
        return;
    };
    let current = transform.translation.truncate();
    let (next, arrived) = advance_position(current, target, XVART_SPEED * time.delta_secs());
    sprite.flip_x = target.x < current.x;
    transform.translation.x = next.x;
    transform.translation.y = next.y;

    if arrived || next.distance_squared(target) <= ARRIVAL_DISTANCE * ARRIVAL_DISTANCE {
        let was_player_order = intent.mode == MovementMode::Player;
        intent.target = None;
        intent.mode = MovementMode::Wander;
        if was_player_order {
            for marker in &markers {
                commands.entity(marker).despawn();
            }
        }
    }
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

    use super::advance_position;

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
}
