import init, { segment_rgba, reconstruct_schem } from "./wasm/voxelens_wasm.js";

const $ = <T extends HTMLElement>(id: string) => document.getElementById(id) as T;
const src = $<HTMLCanvasElement>("src");
const out = $<HTMLCanvasElement>("out");
const status = $<HTMLElement>("status");
const num = (id: string) => Number($<HTMLInputElement>(id).value);

let image: ImageData | null = null;

await init();
status.textContent = "wasm ready — load a screenshot";

$<HTMLInputElement>("file").addEventListener("change", async (e) => {
  const file = (e.target as HTMLInputElement).files?.[0];
  if (!file) return;
  const bmp = await createImageBitmap(file);
  for (const c of [src, out]) {
    c.width = bmp.width;
    c.height = bmp.height;
  }
  const ctx = src.getContext("2d")!;
  ctx.drawImage(bmp, 0, 0);
  image = ctx.getImageData(0, 0, bmp.width, bmp.height);
  status.textContent = `${bmp.width}×${bmp.height} loaded`;
});

$<HTMLButtonElement>("segBtn").addEventListener("click", () => {
  if (!image) return void (status.textContent = "load an image first");
  const t = performance.now();
  const rgba = new Uint8Array(image.data.buffer);
  const overlay = segment_rgba(rgba, image.width, image.height);
  const od = new ImageData(new Uint8ClampedArray(overlay), image.width, image.height);
  out.getContext("2d")!.putImageData(od, 0, 0);
  status.textContent = `segmented in ${(performance.now() - t).toFixed(0)} ms`;
});

$<HTMLButtonElement>("reconBtn").addEventListener("click", () => {
  if (!image) return void (status.textContent = "load an image first");
  try {
    const rgba = new Uint8Array(image.data.buffer);
    const bytes = reconstruct_schem(
      rgba,
      image.width,
      image.height,
      num("fov"),
      num("yaw"),
      num("pitch"),
      num("ex"),
      num("ey"),
      num("ez"),
      num("gy"),
      32,
      4556,
      3,
    );
    const url = URL.createObjectURL(new Blob([bytes], { type: "application/octet-stream" }));
    $<HTMLElement>("dl").innerHTML =
      `<a href="${url}" download="reconstruction.schem">⬇ reconstruction.schem (${bytes.length} bytes)</a>`;
    status.textContent = "reconstructed ✓";
  } catch (err) {
    status.textContent = "error: " + (err as Error).message;
  }
});
