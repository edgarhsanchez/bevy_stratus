#![doc = include_str!("../README.md")]

mod compute;
/// Controls the compute shader which renders the volumetric clouds.
pub mod config;
/// A utility plugin to control the camera using keyboard and mouse.
#[cfg(feature = "fly_camera")]
pub mod fly_camera;
mod images;
mod render;
mod skybox;
#[cfg(feature = "debug")]
mod ui;
mod uniforms;
use bevy::prelude::*;

#[cfg(feature = "debug")]
use self::ui::ui_system;
#[cfg(feature = "debug")]
use bevy_egui::EguiPrimaryContextPass;

use crate::{
    compute::CameraMatrices,
    images::{build_images, cloud_output_size, upsert_cloud_output_image},
    render::{CloudsMaterial, CloudsShaderPlugin},
    skybox::{SkyboxMaterials, init_skybox_mesh, setup_daylight, update_skybox_transform},
    uniforms::CloudsImage,
};

pub use crate::config::{CloudQuality, CloudQualityPreset, CloudsConfig};
pub use crate::skybox::SkyboxPlane as CloudsSkyboxPlane;

/// Marker component the host inserts on the single 3D camera that should
/// receive the volumetric cloud skybox. When no entity has this marker, the
/// systems fall back to the first 3D camera they find.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct CloudsCamera;

#[derive(Resource, Clone, Copy, Debug, PartialEq, Eq)]
struct CloudsOutputSize(UVec2);

use self::compute::CloudsComputePlugin;

/// A plugin for rendering clouds.
///
/// The configuration of the clouds can be changed using the [`CloudsConfig`] resource.
pub struct CloudsPlugin {
    /// When `true` (default), the plugin spawns a default `DirectionalLight`
    /// representing the sun. Set to `false` when the host already provides its
    /// own lighting.
    pub spawn_default_sun: bool,
}

impl Default for CloudsPlugin {
    fn default() -> Self {
        Self {
            spawn_default_sun: true,
        }
    }
}

impl Plugin for CloudsPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(CloudsConfig::default())
            .add_plugins((CloudsComputePlugin, CloudsShaderPlugin))
            .add_systems(Startup, clouds_setup)
            .add_systems(PostUpdate, sync_cloud_output_images)
            .add_systems(
                PostUpdate,
                (update_skybox_transform, update_camera_matrices)
                    .after(TransformSystems::Propagate),
            );
        if self.spawn_default_sun {
            app.add_systems(Startup, setup_daylight);
        }
        #[cfg(feature = "debug")]
        app.add_systems(EguiPrimaryContextPass, ui_system);
    }
}

fn clouds_setup(
    mut commands: Commands,
    config: Res<CloudsConfig>,
    images: ResMut<Assets<Image>>,
    meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<CloudsMaterial>>,
) {
    let output_size = cloud_output_size(&config);
    let (cloud_render_image, cloud_atlas_image, cloud_worley_image, sky_image) =
        build_images(images, output_size);

    let material = materials.add(CloudsMaterial {
        cloud_render_image: cloud_render_image.clone(),
        cloud_atlas_image: cloud_atlas_image.clone(),
        cloud_worley_image: cloud_worley_image.clone(),
        sky_image: sky_image.clone(),
    });
    init_skybox_mesh(
        &mut commands,
        meshes,
        SkyboxMaterials::from_one_material(MeshMaterial3d(material.clone())),
    );
    commands.insert_resource(CloudsImage {
        cloud_render_image,
        cloud_atlas_image,
        cloud_worley_image,
        sky_image,
    });
    commands.insert_resource(CloudsOutputSize(output_size));
    commands.insert_resource(CameraMatrices {
        translation: Vec3::ZERO,
        inverse_camera_projection: Mat4::IDENTITY,
        inverse_camera_view: Mat4::IDENTITY,
    });
}

fn sync_cloud_output_images(
    config: Res<CloudsConfig>,
    mut output_size: ResMut<CloudsOutputSize>,
    mut images: ResMut<Assets<Image>>,
    mut clouds_image: ResMut<CloudsImage>,
    mut materials: ResMut<Assets<CloudsMaterial>>,
) {
    let desired_size = cloud_output_size(&config);
    if desired_size == output_size.0 {
        return;
    }

    upsert_cloud_output_image(
        &mut images,
        &mut clouds_image.cloud_render_image,
        desired_size,
    );
    // The atlas is `render_resolution`-sized (only sampled `% render_resolution`),
    // so it must be resized alongside the render buffer or the base noise would
    // be sampled out of bounds after a quality change.
    upsert_cloud_output_image(
        &mut images,
        &mut clouds_image.cloud_atlas_image,
        desired_size,
    );
    upsert_cloud_output_image(&mut images, &mut clouds_image.sky_image, desired_size);

    for (_, material) in materials.iter_mut() {
        material.cloud_render_image = clouds_image.cloud_render_image.clone();
        material.cloud_atlas_image = clouds_image.cloud_atlas_image.clone();
        material.sky_image = clouds_image.sky_image.clone();
    }

    output_size.0 = desired_size;
    info!(
        "volumetric_clouds: resized output textures to {}x{}",
        desired_size.x, desired_size.y
    );
}

fn update_camera_matrices(
    config: Res<CloudsConfig>,
    marked: Query<(&GlobalTransform, &Camera), With<CloudsCamera>>,
    fallback: Query<(&GlobalTransform, &Camera), (With<Camera3d>, Without<CloudsCamera>)>,
    mut matrices: ResMut<CameraMatrices>,
) {
    if !config.enabled {
        return;
    }
    let Some((camera_transform, camera)) = marked.iter().next().or_else(|| fallback.iter().next())
    else {
        return;
    };
    matrices.translation = camera_transform.translation();
    matrices.inverse_camera_view = camera_transform.to_matrix();
    matrices.inverse_camera_projection = camera.computed.clip_from_view.inverse();
}
