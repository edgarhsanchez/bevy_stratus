use bevy::prelude::*;

#[derive(Resource, Clone, Copy)]
/// The configuration that gets passed to the compute shader that renders the clouds.
///
/// The resource gets added automatically by `CloudsPlugin`. However, you can overwrite it
/// by inserting a new instance of it.
///
/// # Example
///
/// ```rust ignore
/// App::new()
///     .add_plugins((DefaultPlugins, CloudsPlugin))
///     .insert_resource(CloudsConfig {clouds_coverage: 0.6, ..default()})
///     .run();
/// ```
pub struct CloudsConfig {
    /// Number of raymarching steps. More steps reduces noise but requires more computational power
    pub clouds_raymarch_steps_count: u32,
    /// Number of raymarching steps for shadowing.
    /// More steps reduces noise but requires more computational power
    pub clouds_shadow_raymarch_steps_count: u32,
    /// Radius of the planet the clouds encompass. Determines the curvature of the cloud layer near
    /// the horizon.
    pub planet_radius: f32,
    /// Height of the `clouds_bottom_height` of the cloud layer.
    pub clouds_bottom_height: f32,
    /// Height of the `clouds_top_height` of the cloud layer.
    pub clouds_top_height: f32,
    /// `clouds_coverage` of 0.0 means no clouds (fair weather), 1.0 means full overcast
    pub clouds_coverage: f32,
    /// Determines how much the base cloud structure is eroded by higher-frequency,
    /// lower-amplitude detail noise.
    pub clouds_detail_strength: f32,
    /// Softness of the clouds
    pub clouds_base_edge_softness: f32,
    /// Softness of the `clouds_bottom_height` of the clouds
    pub clouds_bottom_softness: f32,
    /// `clouds_density` of the clouds between 0.0 and 1.0
    pub clouds_density: f32,
    /// Step size of raymarching steps for calculating the shadow inside clouds
    pub clouds_shadow_raymarch_step_size: f32,
    /// Step size exponential multiplication factor of raymarching steps for calculating the
    /// shadow inside clouds
    pub clouds_shadow_raymarch_step_multiply: f32,
    /// Scattering factor for forward scattering lobe. See Frostbite paper in README for details.
    pub forward_scattering_g: f32,
    /// Scattering factor for backward scattering lobe. See Frostbite paper in README for details.
    pub backward_scattering_g: f32,
    /// Factor between 0.0 and 1.0 for mixing forward and backward scattering.
    pub scattering_lerp: f32,
    /// The color of ambient lighting at the `clouds_top_height` of the clouds.
    pub clouds_ambient_color_top: Vec4,
    /// The color of ambient lighting at the `clouds_bottom_height` of the clouds.
    pub clouds_ambient_color_bottom: Vec4,
    /// Minimal transmittance in a ray, if transmittance is too low the ray is discarded.
    pub clouds_min_transmittance: f32,
    /// Determines the overall scale of the clouds
    pub clouds_base_scale: f32,
    ///Determines the scale of the details inside the clouds
    pub clouds_detail_scale: f32,
    /// Direction towards the sun.
    pub sun_dir: Vec4,
    /// Color of the sun (HDR, RGBA).
    pub sun_color: Vec4,
    /// Strength of reprojection. 0.0 means we don't mix the current frame with the last frame.
    /// 0.95 means we take 5% of the current frame and 95% of last frame and combine those two to
    /// reduce noise.
    /// Automatically updates each frame.
    pub reprojection_strength: f32,
    /// Determines whether the egui UI is visible or not. Requires the `debug` feature.
    pub ui_visible: bool,
    /// Resolution of the image we're writing to.
    pub render_resolution: Vec2,
    /// Velocity of the wind.
    pub wind_velocity: Vec3,
    /// When `false`, the cloud compute pass and skybox updates are skipped
    /// so the plugin imposes near-zero per-frame cost. The skybox meshes are
    /// also expected to be hidden by the host. Defaults to `true`.
    pub enabled: bool,
}

/// Ready-made quality presets trading visual fidelity against GPU cost.
///
/// Each preset only drives the two performance-critical knobs — the render
/// resolution and the raymarch/shadow step counts. Art-direction fields
/// (coverage, colours, wind, scattering, …) are left untouched so you can pick
/// a look and a performance tier independently:
///
/// ```rust ignore
/// // Cinematic look, but scaled down for a mid-range GPU:
/// let config = CloudsConfig { clouds_coverage: 0.7, ..default() }
///     .with_quality(CloudQuality::Medium);
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CloudQuality {
    /// 960x540, 8 raymarch / 3 shadow steps. Cheapest; good for low-end GPUs.
    Low,
    /// 1280x720, 10 raymarch / 4 shadow steps.
    Medium,
    /// 1920x1080, 12 raymarch / 6 shadow steps. The default look.
    High,
    /// 2560x1440, 16 raymarch / 8 shadow steps. For high-end GPUs.
    Ultra,
}

/// The concrete performance settings a [`CloudQuality`] expands to.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CloudQualityPreset {
    /// Resolution the cloud compute pass renders at (may be below the window
    /// resolution — the result is upscaled by the skybox).
    pub render_resolution: Vec2,
    /// Number of primary raymarch steps.
    pub raymarch_steps: u32,
    /// Number of shadow raymarch steps taken per lit sample.
    pub shadow_raymarch_steps: u32,
}

impl CloudQuality {
    /// Expands the preset into concrete performance settings.
    pub const fn preset(self) -> CloudQualityPreset {
        let (w, h, raymarch_steps, shadow_raymarch_steps) = match self {
            Self::Low => (960.0, 540.0, 8, 3),
            Self::Medium => (1280.0, 720.0, 10, 4),
            Self::High => (1920.0, 1080.0, 12, 6),
            Self::Ultra => (2560.0, 1440.0, 16, 8),
        };
        CloudQualityPreset {
            render_resolution: Vec2::new(w, h),
            raymarch_steps,
            shadow_raymarch_steps,
        }
    }
}

impl CloudsConfig {
    /// Applies a [`CloudQuality`] preset, overwriting only the render resolution
    /// and step counts and leaving every art-direction field untouched.
    #[must_use]
    pub fn with_quality(mut self, quality: CloudQuality) -> Self {
        let preset = quality.preset();
        self.render_resolution = preset.render_resolution;
        self.clouds_raymarch_steps_count = preset.raymarch_steps;
        self.clouds_shadow_raymarch_steps_count = preset.shadow_raymarch_steps;
        self
    }

    /// Overrides the render resolution, clamping to at least `256x144` so the
    /// worley detail LUT stays fully populated by the one-shot `init` pass.
    #[must_use]
    pub fn with_render_resolution(mut self, width: u32, height: u32) -> Self {
        self.render_resolution = Vec2::new(width.max(256) as f32, height.max(144) as f32);
        self
    }
}

impl Default for CloudsConfig {
    fn default() -> Self {
        let sun_dir = Vec3::new(-0.7, 0.5, 0.75).normalize();
        Self {
            clouds_raymarch_steps_count: 12,
            clouds_shadow_raymarch_steps_count: 6,
            planet_radius: 6_371_000.0,
            clouds_bottom_height: 1250.0,
            clouds_top_height: 2400.0,
            clouds_coverage: 0.5,
            clouds_detail_strength: 0.27,
            clouds_base_edge_softness: 0.1,
            clouds_bottom_softness: 0.25,
            clouds_density: 0.03,
            clouds_shadow_raymarch_step_size: 10.0,
            clouds_shadow_raymarch_step_multiply: 1.3,
            forward_scattering_g: 0.8,
            backward_scattering_g: -0.2,
            scattering_lerp: 0.5,
            clouds_ambient_color_top: Vec4::new(149.0, 167.0, 200.0, 0.0) * (1.5 / 225.0),
            clouds_ambient_color_bottom: Vec4::new(39.0, 67.0, 87.0, 0.0) * (1.5 / 225.0),
            clouds_min_transmittance: 0.1,
            clouds_base_scale: 1.5,
            clouds_detail_scale: 42.0,
            sun_dir: Vec4::new(sun_dir.x, sun_dir.y, sun_dir.z, 0.0),
            sun_color: Vec4::new(1.0, 0.9, 0.85, 1.0) * 1.4,
            reprojection_strength: 0.95,
            ui_visible: true,
            render_resolution: Vec2::new(1920.0, 1080.0),
            wind_velocity: Vec3::new(-1.1, 0.0, 2.3),
            enabled: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quality_presets_are_monotonic() {
        let tiers = [
            CloudQuality::Low,
            CloudQuality::Medium,
            CloudQuality::High,
            CloudQuality::Ultra,
        ];
        for pair in tiers.windows(2) {
            let lo = pair[0].preset();
            let hi = pair[1].preset();
            assert!(hi.render_resolution.x >= lo.render_resolution.x);
            assert!(hi.render_resolution.y >= lo.render_resolution.y);
            assert!(hi.raymarch_steps >= lo.raymarch_steps);
            assert!(hi.shadow_raymarch_steps >= lo.shadow_raymarch_steps);
        }
    }

    #[test]
    fn every_preset_meets_the_worley_population_floor() {
        // The one-shot `init` pass populates the 32^3 worley LUT by mapping
        // render-resolution invocations onto 8x4 depth slices, so it needs at
        // least 256x128 invocations to fill every slice.
        for quality in [
            CloudQuality::Low,
            CloudQuality::Medium,
            CloudQuality::High,
            CloudQuality::Ultra,
        ] {
            let res = quality.preset().render_resolution;
            assert!(res.x >= 256.0, "{quality:?} width {} < 256", res.x);
            assert!(res.y >= 128.0, "{quality:?} height {} < 128", res.y);
        }
    }

    #[test]
    fn with_quality_changes_only_performance_fields() {
        let base = CloudsConfig {
            clouds_coverage: 0.73,
            clouds_density: 0.09,
            ..Default::default()
        };
        let tuned = base.with_quality(CloudQuality::Low);

        // Performance fields follow the preset...
        assert_eq!(tuned.render_resolution, Vec2::new(960.0, 540.0));
        assert_eq!(tuned.clouds_raymarch_steps_count, 8);
        assert_eq!(tuned.clouds_shadow_raymarch_steps_count, 3);
        // ...art direction is preserved.
        assert_eq!(tuned.clouds_coverage, 0.73);
        assert_eq!(tuned.clouds_density, 0.09);
    }

    #[test]
    fn with_render_resolution_clamps_to_the_worley_floor() {
        let cfg = CloudsConfig::default().with_render_resolution(64, 32);
        assert_eq!(cfg.render_resolution, Vec2::new(256.0, 144.0));
    }
}
