//! A reddish sunset sky with rainbow hues shimmering through the clouds.
//!
//! Tweaks `CloudsConfig` to:
//!   * place the sun near the horizon with a deep red/orange tint,
//!   * paint the cloud bottoms a warm crimson and the tops a soft pink,
//!   * cycle the cloud top ambient color through the visible spectrum over
//!     time so the clouds shimmer with rainbow hues, while the bottom hue
//!     drifts gently around warm reds.
//!
//! Run with:
//! ```sh
//! cargo run --example sunset_rainbow
//! ```
//!
//! Optional features (fly camera + debug UI):
//! ```sh
//! cargo run --example sunset_rainbow --features fly_camera,debug
//! ```
use bevy::diagnostic::{DiagnosticsStore, FrameTimeDiagnosticsPlugin};
use bevy::prelude::*;
use bevy::render::view::Hdr;
#[cfg(feature = "debug")]
use bevy_egui::EguiPlugin;
use bevy_stratus::CloudsPlugin;
use bevy_stratus::config::CloudsConfig;
#[cfg(feature = "fly_camera")]
use bevy_stratus::fly_camera::{FlyCam, FlyCameraPlugin};

fn main() {
    App::new()
        .add_plugins((
            DefaultPlugins,
            FrameTimeDiagnosticsPlugin::default(),
            CloudsPlugin::default(),
            #[cfg(feature = "fly_camera")]
            FlyCameraPlugin,
            #[cfg(feature = "debug")]
            EguiPlugin::default(),
        ))
        .insert_resource(sunset_rainbow_config())
        .add_systems(Startup, setup)
        .add_systems(Update, (close_on_esc, animate_rainbow_clouds, fps_in_title))
        .run();
}

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut std_materials: ResMut<Assets<StandardMaterial>>,
) {
    commands.spawn((
        Camera3d::default(),
        Hdr,
        #[cfg(feature = "fly_camera")]
        FlyCam,
        Transform::from_translation(Vec3::new(0.0, 3.0, 0.0)).looking_to(Vec3::X, Vec3::Y),
    ));

    // A dim, warm ground so the sunset palette stays dominant.
    commands.spawn((
        Mesh3d(meshes.add(Plane3d::new(Vec3::Y, Vec2::splat(1e4)))),
        MeshMaterial3d(std_materials.add(Color::srgb(0.18, 0.07, 0.08))),
    ));
}

/// Build a `CloudsConfig` tuned for a reddish, rainbow-tinted sunset.
fn sunset_rainbow_config() -> CloudsConfig {
    // Low sun, just above the horizon — Y is small/positive so light rakes
    // across the cloud layer.
    let sun_dir = Vec3::new(-0.85, 0.18, 0.5).normalize();

    CloudsConfig {
        // A bit of extra coverage so there are plenty of cloud surfaces to
        // catch the colored ambient.
        clouds_coverage: 0.62,
        // Slightly denser clouds read the rainbow tints more strongly.
        clouds_density: 0.05,
        clouds_detail_strength: 0.32,
        // Push detail scale up a touch so we still see structure.
        clouds_detail_scale: 50.0,

        // Wider scattering lobe asymmetry — strong forward scatter gives the
        // classic glowing-rim look against a low sun.
        forward_scattering_g: 0.86,
        backward_scattering_g: -0.3,
        scattering_lerp: 0.55,

        // Warm, saturated sun. HDR > 1.0 so it actually punches through the
        // clouds after tonemapping.
        sun_color: Vec4::new(1.7, 0.55, 0.35, 1.0),
        sun_dir: Vec4::new(sun_dir.x, sun_dir.y, sun_dir.z, 0.0),

        // Initial ambient colors. `animate_rainbow_clouds` will overwrite the
        // top color every frame; the bottom drifts more subtly.
        clouds_ambient_color_top: Vec4::new(1.0, 0.55, 0.7, 0.0) * 0.95,
        clouds_ambient_color_bottom: Vec4::new(0.85, 0.18, 0.22, 0.0) * 0.95,

        // A breezy sky helps the rainbow tints move across the cloud field.
        // Clouds are kilometers across, so wind needs to be aggressive to read
        // visually within a short demo session.
        wind_velocity: Vec3::new(-60.0, 0.0, 110.0),

        // Default reprojection is 0.95 which heavily smears motion between
        // frames; drop it so the wind drift is actually perceptible.
        reprojection_strength: 0.6,

        ..default()
    }
}

/// Cycle the cloud top ambient through the visible spectrum so the clouds
/// shimmer with rainbow hues, while keeping the bottom anchored in warm reds.
fn animate_rainbow_clouds(time: Res<Time>, mut config: ResMut<CloudsConfig>) {
    let t = time.elapsed_secs();

    // Top: full-spectrum sweep (~12 s per cycle), high saturation, bright value.
    let top_hue = (t / 12.0).fract() * 360.0;
    let top = Color::hsl(top_hue, 0.85, 0.62).to_linear();
    config.clouds_ambient_color_top = Vec4::new(top.red, top.green, top.blue, 0.0) * 1.05;

    // Bottom: gentle drift inside the red/orange band (340° → 25°).
    let bottom_hue = 340.0 + ((t * 0.35).sin() * 0.5 + 0.5) * 45.0;
    let bottom = Color::hsl(bottom_hue % 360.0, 0.9, 0.32).to_linear();
    config.clouds_ambient_color_bottom =
        Vec4::new(bottom.red, bottom.green, bottom.blue, 0.0) * 0.95;
}

fn close_on_esc(
    mut commands: Commands,
    focused_windows: Query<(Entity, &Window)>,
    input: Res<ButtonInput<KeyCode>>,
) {
    for (window, focus) in focused_windows.iter() {
        if focus.focused && input.just_pressed(KeyCode::Escape) {
            commands.entity(window).despawn();
        }
    }
}

/// Display the current FPS in the window title bar so we can confirm
/// the rainbow clouds are animating smoothly.
fn fps_in_title(diagnostics: Res<DiagnosticsStore>, mut windows: Query<&mut Window>) {
    let Some(fps) = diagnostics
        .get(&FrameTimeDiagnosticsPlugin::FPS)
        .and_then(|d| d.smoothed())
    else {
        return;
    };
    let Some(frame_time_ms) = diagnostics
        .get(&FrameTimeDiagnosticsPlugin::FRAME_TIME)
        .and_then(|d| d.smoothed())
    else {
        return;
    };
    for mut window in &mut windows {
        window.title = format!("sunset_rainbow — {fps:.0} FPS ({frame_time_ms:.2} ms)");
    }
}
