<h1 align="center">bevy_stratus</h1>

<p align="center">
  <strong>Fast half-precision volumetric raymarched clouds for <a href="https://github.com/bevyengine/bevy">Bevy</a>.</strong>
</p>

`bevy_stratus` renders animated volumetric clouds with a compute-shader raymarch
(the Horizon Zero Dawn / Guerrilla Games method) and composites them onto a
skybox. It is tuned for **real-time use in games**: every intermediate buffer is
half-precision, the temporal-reprojection camera state lives in a uniform rather
than in-band in a render target, and the noise atlas is sized to the render
resolution instead of a fixed oversized texture.

> **Lineage.** `bevy_stratus` began as a fork of
> [`bevy-volumetric-clouds`](https://github.com/evroon/bevy-volumetric-clouds)
> by **evroon**, whose implementation of the raymarch and cloud noise is the
> foundation this builds on. Huge thanks to evroon — see [Credits](#credits).

## Highlights

- **All-`f16` buffers.** The render, atlas, worley and sky storage textures are
  `Rgba16Float` instead of `Rgba32Float` — roughly halving both VRAM and the
  per-frame read/write bandwidth of the raymarch (the atlas + worley LUTs are
  sampled hundreds of times per pixel, so this is where the time goes).
- **Camera state out of the render target.** Temporal reprojection compares the
  current and previous camera matrices, which are now passed through the uniform
  buffer. The original stashed the matrix inside the render texture's bottom row,
  which pinned that texture to `f32`; moving it out frees the render buffer to go
  `f16` and keeps full precision for the comparison.
- **Right-sized noise atlas.** The base-noise atlas is allocated at the render
  resolution (it is only ever sampled modulo that size) rather than a fixed
  1920×1920, saving tens of MB.
- **Quality presets.** [`CloudQuality`](#quality) expands to sensible
  resolution + step-count tiers so you don't have to hand-tune them.
- **Zero-cost when disabled.** With `CloudsConfig::enabled = false` the compute
  dispatch is skipped after a one-shot LUT init, so the plugin costs almost
  nothing when clouds are off.

## Requirements

- **Bevy 0.19**
- A render adapter exposing the `TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES` wgpu
  feature (all modern desktop GPUs on Vulkan/Metal/DX12). The `f16` cloud
  textures use read-write storage, which requires it. Enable it when building
  your app:

  ```rust ignore
  use bevy::prelude::*;
  use bevy::render::{RenderPlugin, settings::{WgpuSettings, WgpuFeatures}};

  App::new().add_plugins(DefaultPlugins.set(RenderPlugin {
      render_creation: WgpuSettings {
          features: WgpuFeatures::TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES,
          ..default()
      }
      .into(),
      ..default()
  }));
  ```

  If it's missing, `bevy_stratus` logs a single clear error at startup instead of
  failing cryptically.

## Usage

Add the dependency and the plugin:

```rust ignore
use bevy::prelude::*;
use bevy_stratus::{CloudsPlugin, CloudsConfig, CloudQuality};

fn main() {
    App::new()
        .add_plugins((DefaultPlugins, CloudsPlugin::default()))
        // Pick a look, then scale it to a performance tier:
        .insert_resource(
            CloudsConfig { clouds_coverage: 0.6, ..default() }
                .with_quality(CloudQuality::High),
        )
        .run();
}
```

Attach the [`CloudsCamera`] marker to the 3D camera that should receive the
skybox (otherwise the first `Camera3d` found is used). `CloudsPlugin` spawns a
default sun; set `CloudsPlugin { spawn_default_sun: false }` if your app already
provides its own `DirectionalLight`.

### Quality

`CloudQuality` drives only the performance-critical knobs (render resolution and
raymarch/shadow step counts), leaving your art-direction fields untouched:

| Preset   | Resolution | Raymarch steps | Shadow steps |
| -------- | ---------- | -------------- | ------------ |
| `Low`    | 960×540    | 8              | 3            |
| `Medium` | 1280×720   | 10             | 4            |
| `High`   | 1920×1080  | 12             | 6            |
| `Ultra`  | 2560×1440  | 16             | 8            |

```rust ignore
let config = CloudsConfig { clouds_coverage: 0.7, ..default() }
    .with_quality(CloudQuality::Medium);
```

The compute pass renders at `render_resolution` (which may be below the window
resolution — the skybox upscales the result), so lowering it is the biggest
single performance lever.

## Examples

```sh
cargo run --example minimal --features fly_camera
cargo run --example sunset_rainbow --features fly_camera
cargo run --example demo --features "fly_camera debug"
```

## Roadmap

- **Drop the adapter-feature requirement.** Convert the LUTs to write-once /
  sampled reads and ping-pong the render history so no read-write storage is
  needed — making the crate portable to any wgpu backend (including WebGPU) and
  enabling hardware bilinear filtering of the noise.
- **Proper temporal reprojection.** Reproject history using the previous camera
  matrix (already in the uniform) instead of only blending while the camera is
  static, so accumulation survives motion.

## Credits

- **evroon** — the original
  [`bevy-volumetric-clouds`](https://github.com/evroon/bevy-volumetric-clouds),
  which this crate is derived from and could not exist without.
- The raymarching and cloud-noise approach follows the *Nubis* system from
  **Horizon Zero Dawn** by **Guerrilla Games**, and the energy-conserving
  integration from **Frostbite** (see the original repository for the papers).

## License

MIT — see [LICENSE](LICENSE). Derived from evroon's `bevy-volumetric-clouds`
(also MIT); both copyright notices are retained.
