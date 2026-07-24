use bevy::{
    asset::load_embedded_asset,
    ecs::system::ResMut,
    prelude::*,
    render::{
        Extract, Render, RenderApp, RenderStartup, RenderSystems,
        extract_resource::ExtractResourcePlugin,
        render_asset::RenderAssets,
        render_resource::{
            AsBindGroup, BindGroup, BindGroupEntries, BindGroupLayoutDescriptor,
            BindGroupLayoutEntries, CachedComputePipelineId, CachedPipelineState,
            ComputePassDescriptor, ComputePipelineDescriptor, PipelineCache, ShaderStages,
            binding_types::uniform_buffer,
        },
        renderer::{RenderContext, RenderDevice, RenderGraph, RenderGraphSystems, RenderQueue},
        texture::GpuImage,
    },
};
/// Controls the compute shader which renders the volumetric clouds.
use std::borrow::Cow;

use crate::config::CloudsConfig;

use super::uniforms::{CloudsImage, CloudsUniform, CloudsUniformBuffer};

const WORKGROUP_SIZE: u32 = 8;

fn workgroups_for_extent(width: u32, height: u32) -> (u32, u32) {
    (
        width.div_ceil(WORKGROUP_SIZE).max(1),
        height.div_ceil(WORKGROUP_SIZE).max(1),
    )
}

fn config_render_resolution(config: &CloudsConfig) -> UVec2 {
    UVec2::new(
        config.render_resolution.x.ceil().max(1.0) as u32,
        config.render_resolution.y.ceil().max(1.0) as u32,
    )
}

#[derive(Resource, Clone, Copy)]
pub(crate) struct CameraMatrices {
    pub translation: Vec3,
    pub inverse_camera_view: Mat4,
    pub inverse_camera_projection: Mat4,
}

#[derive(Resource)]
struct CloudsUniformBindGroup(BindGroup);

#[derive(Resource)]
struct CloudsImageBindGroup(BindGroup);

#[expect(clippy::too_many_arguments)]
fn prepare_uniforms_bind_group(
    mut commands: Commands,
    pipeline: Res<CloudsPipeline>,
    pipeline_cache: Res<PipelineCache>,
    render_queue: Res<RenderQueue>,
    mut clouds_uniform_buffer: ResMut<CloudsUniformBuffer>,
    camera: ResMut<CameraMatrices>,
    clouds_config: Res<CloudsConfig>,
    render_device: Res<RenderDevice>,
    time: Res<Time>,
) {
    let buffer = clouds_uniform_buffer.buffer.get_mut();

    buffer.clouds_raymarch_steps_count = clouds_config.clouds_raymarch_steps_count;
    buffer.planet_radius = clouds_config.planet_radius;
    buffer.clouds_bottom_height = clouds_config.clouds_bottom_height;
    buffer.clouds_top_height = clouds_config.clouds_top_height;
    buffer.clouds_coverage = clouds_config.clouds_coverage;
    buffer.clouds_detail_strength = clouds_config.clouds_detail_strength;
    buffer.clouds_base_edge_softness = clouds_config.clouds_base_edge_softness;
    buffer.clouds_bottom_softness = clouds_config.clouds_bottom_softness;
    buffer.clouds_density = clouds_config.clouds_density;
    buffer.clouds_shadow_raymarch_steps_count = clouds_config.clouds_shadow_raymarch_steps_count;
    buffer.clouds_shadow_raymarch_step_size = clouds_config.clouds_shadow_raymarch_step_size;
    buffer.clouds_shadow_raymarch_step_multiply =
        clouds_config.clouds_shadow_raymarch_step_multiply;
    buffer.forward_scattering_g = clouds_config.forward_scattering_g;
    buffer.backward_scattering_g = clouds_config.backward_scattering_g;
    buffer.scattering_lerp = clouds_config.scattering_lerp;
    buffer.clouds_ambient_color_top = clouds_config.clouds_ambient_color_top;
    buffer.clouds_ambient_color_bottom = clouds_config.clouds_ambient_color_bottom;
    buffer.clouds_min_transmittance = clouds_config.clouds_min_transmittance;
    buffer.clouds_base_scale = clouds_config.clouds_base_scale;
    buffer.clouds_detail_scale = clouds_config.clouds_detail_scale;
    buffer.sun_dir = clouds_config.sun_dir;
    buffer.sun_color = clouds_config.sun_color;
    buffer.camera_translation = camera.translation;
    buffer.time = time.elapsed_secs_wrapped();
    buffer.reprojection_strength = clouds_config.reprojection_strength;
    // Carry last frame's camera forward before overwriting it, so the shader can
    // compare the two and reproject history only while the camera is static.
    // (The uniform buffer persists across frames, so `buffer.inverse_camera_view`
    // still holds the previous frame's value at this point.)
    buffer.prev_inverse_camera_view = buffer.inverse_camera_view;
    buffer.inverse_camera_view = camera.inverse_camera_view;
    buffer.inverse_camera_projection = camera.inverse_camera_projection;
    buffer.wind_displacement += time.delta_secs() * clouds_config.wind_velocity;

    clouds_uniform_buffer
        .buffer
        .write_buffer(&render_device, &render_queue);

    let bind_group_uniforms = render_device.create_bind_group(
        None,
        &pipeline_cache.get_bind_group_layout(&pipeline.uniform_bind_group_layout),
        &BindGroupEntries::single(clouds_uniform_buffer.buffer.binding().unwrap().clone()),
    );
    commands.insert_resource(CloudsUniformBindGroup(bind_group_uniforms));
}

fn prepare_textures_bind_group(
    mut commands: Commands,
    pipeline: Res<CloudsPipeline>,
    pipeline_cache: Res<PipelineCache>,
    gpu_images: Res<RenderAssets<GpuImage>>,
    clouds_image: Res<CloudsImage>,
    render_device: Res<RenderDevice>,
) {
    let Some(cloud_render_view) = gpu_images.get(&clouds_image.cloud_render_image) else {
        commands.remove_resource::<CloudsImageBindGroup>();
        return;
    };
    let Some(cloud_atlas_view) = gpu_images.get(&clouds_image.cloud_atlas_image) else {
        commands.remove_resource::<CloudsImageBindGroup>();
        return;
    };
    let Some(cloud_worley_view) = gpu_images.get(&clouds_image.cloud_worley_image) else {
        commands.remove_resource::<CloudsImageBindGroup>();
        return;
    };
    let Some(sky_view) = gpu_images.get(&clouds_image.sky_image) else {
        commands.remove_resource::<CloudsImageBindGroup>();
        return;
    };

    let bind_group = render_device.create_bind_group(
        None,
        &pipeline_cache.get_bind_group_layout(&pipeline.texture_bind_group_layout),
        &BindGroupEntries::sequential((
            &cloud_render_view.texture_view,
            &cloud_atlas_view.texture_view,
            &cloud_worley_view.texture_view,
            &sky_view.texture_view,
        )),
    );
    commands.insert_resource(CloudsImageBindGroup(bind_group));
}

/// The compute shading pipeline
///
/// Note that the compute shader is loaded in [`CloudsShaderPlugin`] so this resource depends on
/// that plugin.
#[derive(Resource)]
struct CloudsPipeline {
    texture_bind_group_layout: BindGroupLayoutDescriptor,
    uniform_bind_group_layout: BindGroupLayoutDescriptor,
    init_pipeline: CachedComputePipelineId,
    update_pipeline: CachedComputePipelineId,
}

/// The cloud storage textures are `Rgba16Float` with read-write access, which
/// wgpu only permits when the adapter exposes
/// `TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES`. Emit one clear error at startup
/// if it is missing, instead of letting the compute pipeline fail cryptically.
fn warn_if_missing_format_features(render_device: Res<RenderDevice>) {
    use bevy::render::settings::WgpuFeatures;
    if !render_device
        .features()
        .contains(WgpuFeatures::TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES)
    {
        error!(
            "bevy_stratus: render adapter is missing \
             TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES, required by the f16 \
             read-write cloud storage textures. Enable it via \
             `RenderPlugin {{ render_creation: WgpuSettings {{ features: \
             WgpuFeatures::TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES, .. }} }}` \
             or the cloud compute pass will fail to create."
        );
    }
}

fn init_clouds_pipeline(
    mut commands: Commands,
    render_device: Res<RenderDevice>,
    pipeline_cache: Res<PipelineCache>,
    asset_server: Res<AssetServer>,
) {
    {
        let texture_bind_group_layout = CloudsImage::bind_group_layout_descriptor(&render_device);
        let shader = load_embedded_asset!(asset_server.as_ref(), "shaders/clouds_compute.wgsl");

        let entries = BindGroupLayoutEntries::sequential(
            ShaderStages::COMPUTE,
            (uniform_buffer::<CloudsUniform>(false),),
        );

        let uniform_bind_group_layout =
            BindGroupLayoutDescriptor::new("uniform_bind_group_layout", &entries);

        let init_pipeline = pipeline_cache.queue_compute_pipeline(ComputePipelineDescriptor {
            zero_initialize_workgroup_memory: false,
            immediate_size: 0,
            label: None,
            layout: vec![
                uniform_bind_group_layout.clone(),
                texture_bind_group_layout.clone(),
            ],
            shader: shader.clone(),
            shader_defs: vec![],
            entry_point: Some(Cow::from("init")),
        });
        let update_pipeline = pipeline_cache.queue_compute_pipeline(ComputePipelineDescriptor {
            zero_initialize_workgroup_memory: false,
            immediate_size: 0,
            label: None,
            layout: vec![
                uniform_bind_group_layout.clone(),
                texture_bind_group_layout.clone(),
            ],
            shader,
            shader_defs: vec![],
            entry_point: Some(Cow::from("update")),
        });

        commands.insert_resource(CloudsPipeline {
            texture_bind_group_layout,
            uniform_bind_group_layout,
            init_pipeline,
            update_pipeline,
        });
    }
}

#[derive(Default)]
enum CloudsState {
    #[default]
    Loading,
    Init,
    Update,
}

/// Render-world state for the clouds compute pass, formerly held by the
/// render-graph node.
#[derive(Resource, Default)]
struct CloudsPassState {
    state: CloudsState,
    initialized_resolution: Option<UVec2>,
}

fn run_clouds_compute(
    mut pass_state: ResMut<CloudsPassState>,
    pipeline: Res<CloudsPipeline>,
    pipeline_cache: Res<PipelineCache>,
    config: Option<Res<CloudsConfig>>,
    texture_bind_group: Option<Res<CloudsImageBindGroup>>,
    uniform_bind_group: Option<Res<CloudsUniformBindGroup>>,
    mut ctx: RenderContext,
) {
    let current_resolution = config
        .as_deref()
        .map(config_render_resolution)
        .unwrap_or(UVec2::new(1920, 1080));
    let has_bind_groups = texture_bind_group.is_some() && uniform_bind_group.is_some();

    // if the corresponding pipeline has loaded, transition to the next stage
    match pass_state.state {
        CloudsState::Loading => {
            if let CachedPipelineState::Ok(_) =
                pipeline_cache.get_compute_pipeline_state(pipeline.init_pipeline)
            {
                if has_bind_groups {
                    pass_state.initialized_resolution = Some(current_resolution);
                }
                pass_state.state = CloudsState::Init;
            }
        }
        CloudsState::Init => {
            if pass_state.initialized_resolution == Some(current_resolution)
                && let CachedPipelineState::Ok(_) =
                    pipeline_cache.get_compute_pipeline_state(pipeline.update_pipeline)
            {
                pass_state.state = CloudsState::Update;
            } else if has_bind_groups {
                pass_state.initialized_resolution = Some(current_resolution);
            }
        }
        CloudsState::Update => {
            if pass_state.initialized_resolution != Some(current_resolution) {
                if has_bind_groups {
                    pass_state.initialized_resolution = Some(current_resolution);
                } else {
                    pass_state.initialized_resolution = None;
                }
                pass_state.state = CloudsState::Init;
            }
        }
    }

    // Fast path: clouds disabled — skip the per-frame Update dispatch
    // once the one-shot Init has populated the atlas/worley LUTs.
    //
    // CRITICAL: we must NOT skip the Init dispatch even when disabled,
    // otherwise the worley/atlas textures stay zeroed and toggling
    // clouds on later produces no density anywhere along the rays
    // (only the sky/sun is visible).
    if matches!(pass_state.state, CloudsState::Update)
        && let Some(config) = config.as_deref()
        && !config.enabled
    {
        return;
    }
    let (Some(texture_bind_group), Some(uniform_bind_group)) =
        (texture_bind_group, uniform_bind_group)
    else {
        return;
    };

    let mut pass = ctx
        .command_encoder()
        .begin_compute_pass(&ComputePassDescriptor::default());

    pass.set_bind_group(0, &uniform_bind_group.0, &[]);
    pass.set_bind_group(1, &texture_bind_group.0, &[]);

    match pass_state.state {
        CloudsState::Loading => {}
        CloudsState::Init => {
            let init_pipeline = pipeline_cache
                .get_compute_pipeline(pipeline.init_pipeline)
                .unwrap();
            pass.set_pipeline(init_pipeline);
            let (groups_x, groups_y) =
                workgroups_for_extent(current_resolution.x, current_resolution.y);
            pass.dispatch_workgroups(groups_x, groups_y, 1);
        }
        CloudsState::Update => {
            let update_pipeline = pipeline_cache
                .get_compute_pipeline(pipeline.update_pipeline)
                .unwrap();
            pass.set_pipeline(update_pipeline);
            let (groups_x, groups_y) =
                workgroups_for_extent(current_resolution.x, current_resolution.y);
            pass.dispatch_workgroups(groups_x, groups_y, 1);
        }
    }
}

/// A plugin for the compute shader which renders clouds.
pub(crate) struct CloudsComputePlugin;

impl Plugin for CloudsComputePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(ExtractResourcePlugin::<CloudsImage>::default());
        app.add_plugins(ExtractResourcePlugin::<CloudsUniform>::default());

        let render_app = app.sub_app_mut(RenderApp);
        render_app.init_resource::<CloudsPassState>();
        render_app.init_resource::<CloudsUniformBuffer>();
        render_app.add_systems(RenderStartup, (warn_if_missing_format_features, init_clouds_pipeline));
        render_app.add_systems(
            Render,
            prepare_textures_bind_group.in_set(RenderSystems::PrepareResources),
        );
        render_app.add_systems(
            Render,
            prepare_uniforms_bind_group.in_set(RenderSystems::PrepareResources),
        );

        // The clouds image must be written before any camera samples it, so
        // encode the dispatch in the per-frame setup phase of the root render
        // schedule (the 0.19 equivalent of the old node edge to
        // `CameraDriverLabel`).
        render_app.add_systems(
            RenderGraph,
            run_clouds_compute.in_set(RenderGraphSystems::Begin),
        );

        render_app.add_systems(
            ExtractSchedule,
            (extract_clouds_config, extract_time, extract_camera_matrices),
        );
    }
}

fn extract_clouds_config(mut commands: Commands, config: Extract<Res<CloudsConfig>>) {
    commands.insert_resource(**config);
}

fn extract_time(mut commands: Commands, time: Extract<Res<Time>>) {
    commands.insert_resource(**time);
}

fn extract_camera_matrices(mut commands: Commands, camera: Extract<Res<CameraMatrices>>) {
    commands.insert_resource(**camera);
}
