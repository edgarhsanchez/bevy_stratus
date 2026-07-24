# Changelog

All notable changes to `bevy_stratus` are documented here.

## 0.1.0 — initial release

`bevy_stratus` is a performance-focused fork of
[`bevy-volumetric-clouds`](https://github.com/evroon/bevy-volumetric-clouds)
by evroon (MIT). Changes relative to that base:

### Performance

- **Half-precision buffers.** The render, atlas, worley, and sky storage
  textures are now `Rgba16Float` instead of `Rgba32Float`, roughly halving VRAM
  and the per-frame read/write bandwidth of the raymarch.
- **Camera state moved to the uniform.** The temporal-reprojection camera
  matrix is passed via the uniform buffer (as `prev_inverse_camera_view`)
  instead of being packed into the render texture's bottom row. This removes the
  in-band storage hack, keeps full `f32` precision for the motion comparison,
  and is what frees the render texture to use `f16`.
- **Right-sized noise atlas.** The base-noise atlas is allocated at the render
  resolution (the only region ever sampled) rather than a fixed 1920×1920,
  saving ~26–51 MB depending on quality.

### API

- Added [`CloudQuality`] (`Low`/`Medium`/`High`/`Ultra`) and
  `CloudQualityPreset`, plus `CloudsConfig::with_quality` and
  `CloudsConfig::with_render_resolution` builder helpers.
- Re-exported `CloudsConfig`, `CloudQuality`, and `CloudQualityPreset` at the
  crate root.
- Added a startup diagnostic that logs a clear, actionable error when the render
  adapter is missing `TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES`.

### Tests

- Added unit tests covering the quality presets, the worley-population
  resolution floor, and the config builder helpers.

### Notes

- The `f16` cloud storage textures use read-write access and therefore require
  the `TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES` wgpu feature (the same feature
  the original `f32` read-write path relied on). Removing this requirement is on
  the roadmap.
