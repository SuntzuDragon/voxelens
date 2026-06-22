# voxelens — Roadmap

> Durable record of the project's goal, design decisions, and milestone plan.
> This is the source of truth if other notes are lost.

## Goal & motivation

Reconstruct a Minecraft schematic from a screenshot. The motivation is to help
the **seed reverse-engineering** community: rebuilding structures/terrain from
screenshots and panoramas is one of the last un-automated steps in their
workflow. This frames the eventual product around **multi-view / panorama input,
arbitrary builds (not templates), block-position accuracy, and a scriptable
CLI** — with an accessible browser front-end later.

**Name:** `voxelens` (voxel + lens). Verified free on crates.io, npm, and GitHub.

## Working principles

- **No shortcuts.** Solid, well-organized project; good principles followed
  consistently from line one. Organized layout — nothing loose at the root.
- **Test-first.** Write the failing test, then the code. Bugs get a reproducing
  test before the fix. `cargo clippy -D warnings` + `cargo fmt --check` in CI.
- **Source over web claims.** Engine constants are verified against the
  **decompiled Minecraft source for the exact version** (extract/decompile the
  local jar), not wiki/forum claims. Web sources may surface candidates, but each
  is confirmed in source: FOV-is-vertical + projection matrix
  (`GameRenderer.getBasicProjectionMatrix`), `near=0.05`/far, eye height `1.62`,
  directional-shading multipliers, `options.txt` FOV normalization (`0.0 => 70`),
  and `DataVersion` (jar `version.json`).

## Key decisions

- **Per-block CV reconstruction.** Detect individual block faces, classify each
  face's texture against the known Minecraft texture set, back-project to the
  voxel grid. **No whole-structure templates** — a hand-built tree must never be
  silently "recognized" as vanilla. This also generalizes to multi-view.
- **Rust core + native CLI first; WASM/browser later** (browser is a hard
  requirement, just not the MVP). Develop and TDD natively (fast `cargo test`
  loop, visual per-stage PNG dumps), then wrap the proven core in `wasm-bindgen`.
  The native CLI is itself a deliverable (batch processing for seed-finders).
  Zero wasted work: the native core is exactly what compiles to WASM.
- **Output: Sponge `.schem` v2** (widest WorldEdit/FAWE reach, simplest to emit).
  `.litematic` possible later off the same voxel model.
- **Classifier via a synthetic-augmentation harness** built from real MC texture
  PNGs (model the real degradation; keep a real face-crop holdout).

## Honest feasibility note

Single-image 3D is ill-posed: occluded/back blocks can only be **inferred**
(solidity), never measured. Minecraft makes it tractable — integer grid,
axis-aligned cubes (3 vanishing points), fixed 16x16 textures, known ground plane
(superflat fixes absolute scale), fixed directional face shading. Visible faces
are fully measurable; hidden interior is inferred and reported as such.
**Multi-view/panorama with parallax is the real unlock** — it resolves occlusion
by measurement (space carving) and reuses the entire per-view pipeline.

## Technical foundation

**Camera math.** FOV slider = **vertical** FOV in degrees ->
`perspective(fov_v, aspect = W/H, near = 0.05, far = renderDist*16*4)`.
Eye height **1.62** blocks. Blocks are 1x1x1 m, axis-aligned, +X east / +Z south
/ +Y up. World -> pixel (square pixels):

```
fy = (H/2) / tan(vfov_rad/2)
u  = W/2 + fy * ( x_cam / -z_cam )
v  = H/2 - fy * ( y_cam / -z_cam )     // visible iff -z_cam > 0
```

FOV 70 @ 16:9 => `fy = 0.7141 * H`, horizontal FOV ~= 102.4 deg. Use the
screenshot's true pixel dims. (All to be re-confirmed against decompiled source.)

**Directional face shading.** MC multiplies face brightness by fixed
per-orientation factors (top brightest; the two horizontal side-axes
progressively darker; bottom darkest) + smooth-lighting/AO at edges. So
orientation is partly readable from brightness (a cross-check against vanishing
points) and is a deterministic classifier augmentation.

**`.schem` v2** (NBT, gzip, big-endian). Root: `Version=2`, `DataVersion`,
`Width`/`Height`/`Length` (Short, unsigned — validate so dims don't wrap),
optional `Offset` (IntArray[3]), `Palette` (Compound, e.g.
`"minecraft:oak_log[axis=y]" -> n`), `BlockData` (ByteArray of W*H*L
unsigned-LEB128 varint palette indices, order `x + z*W + y*W*L`), optional
`Metadata`/`PaletteMax`/`BlockEntities`. Palette < 128 => one byte per block.

## Stack

- **`voxelens-core`** — WASM-compatible (no native-only deps; I/O stays in the
  CLI). Planned crates: `imageproc` + `image` (Canny/Hough/template-match/morph),
  `nalgebra` (camera/linear algebra), `fastnbt` (NBT), `flate2` (gzip,
  miniz_oxide backend, wasm-safe).
- **`voxelens-cli`** (`clap`) — load a screenshot, run the pipeline, dump an
  annotated PNG after each stage (edges -> faces -> classified faces -> voxels)
  + write the `.schem`.
- **Later — `voxelens-wasm`** (`wasm-bindgen`) + thin Vite/TS UI (upload ->
  params -> canvas overlay -> download), Web Worker + OffscreenCanvas, deployed
  to Cloudflare Pages (static; single-threaded => no COOP/COEP needed).

**Architecture rule:** all algorithms are pure functions over pixel buffers +
plain structs in `voxelens-core` — identical under `cargo test`, the CLI, and
WASM. CLI and web app are thin I/O shells.

## Milestones (each: tests first, then code)

- **M0 — Scaffold.** Workspace, git, toolchain/fmt/clippy config, CI, repo
  layout, fixture + manifest, README, this roadmap. _(done)_
- **M1 — Voxel model + `.schem` v2 writer.** `VoxelModel`, LEB128 varint,
  `fastnbt`+`flate2` serializer; golden byte-fixture + re-parse round-trip; CLI
  emits a `.schem` loadable in WorldEdit. _(done — 16 tests; golden NBT
  hand-verified against the spec; `voxelens emit-test-schem` writes the column.)_
- **M2 — Camera / projection** (`nalgebra`). `world_to_pixel`, `pixel_to_ray`,
  ground/grid-plane intersection; constants verified against decompiled jar;
  tests assert exact pixels from the fixture pose.
- **M3 — Image load + segmentation.** Decode PNG; segment trunk/wool/ground/sky
  (greens on luma); bbox tests on the fixture; stage-dump PNG.
- **M4 — Block-face detection.** Canny -> Hough -> cluster into 3 axis families
  via expected edge slopes; assemble quads; label top/left/right via VP +
  shading-brightness; synthetic-cube + real-fixture tests; stage-dump overlay.
- **M5 — Texture classifier + augmentation harness.** Augment MC textures
  (shading, tint, warp->rectify, downscale/mip, brightness/gamma, rotations);
  NCC on luma with reject-unknown threshold; resilience-curve/confusion-matrix
  tests + real face-crop holdout.
- **M6 — Reconstruction -> VoxelModel -> `.schem`.** Back-project classified
  faces to grid cells, snap to lattice, place blocks, infer solid interior
  (report inferred vs measured); end-to-end CLI on the fixture; assert positions
  vs ground truth.
- **M7 — WASM + browser (Cloudflare).** `wasm-bindgen` wrap; thin Vite/TS UI;
  Cloudflare Pages deploy; browser-mode smoke test + manual e2e.
- **Future — multi-view / panorama.** Run M3–M5 per view into a shared voxel
  grid; resolve occlusion by space carving across parallax.

## Fixture ground truth

`fixtures/wool_tree_superflat_fov70_2560x1439.png` (see `fixtures/manifest.toml`):

- **Dimensions:** 2560 x 1439, FOV 70 (vertical), gamma 0.5, GUI/hand hidden.
- **Camera (flying, exact):** feet `(6.0, -56.0, 3.0)`, eye `(6.0, -54.38, 3.0)`
  (= feet + 1.62), yaw `129.0`, pitch `14.0`.
- **World:** grass surface at `y = -60`; bottom oak-log block at `(-2, -60, -4)`.
- _Pending:_ final F3 confirmation of no drift / not sneaking; exact rendering
  jar (for `DataVersion`).

## Open risks

- **MC version mismatch** (instance says 26.2; local jar is 1.21.10) — resolve
  the exact rendering jar; read `DataVersion` from `version.json`, never hardcode.
- **Sim-to-real gap** in the classifier — mitigated by the real face-crop holdout.
- **Resolution** ~25–60 px/block; rectify-to-16x16 is lossy but workable.
- **Generalization** — unknown pose/FOV needs vanishing-point self-calibration;
  shaders/resource packs are where NCC eventually yields to a learned classifier
  (ONNX via `tract`/`ort` natively; ONNX Runtime Web in-browser).
- **Texture licensing** — load Mojang textures from a local install at runtime;
  commit only self-made stand-in tiles.
