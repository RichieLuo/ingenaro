# Ingenaro branding

`ingenaro-logo.png` is the Ingenaro application artwork: a lively purple spider with a jaunty wave, a few soft web strands and mint connection nodes on a pale lavender field. The illustration keeps the SQLShop brand family's crisp vector style while giving Ingenaro a more playful mascot. `ingenaro-icon.png` currently uses the same spider artwork for the editor header.

`ingenaro-logo.svg` is an editable vector companion. The Windows build script converts the PNG into a multi-resolution ICO (16, 24, 32, 48, 64, 128 and 256 pixels) in Cargo's build output directory and embeds it into `ingenaro.exe`.

Palette: purple `#6129E8`, deep purple `#5422CF`, lavender `#AD8DF0`, pale plate `#F0EAFD`, mint `#A3F5BF`.

Windows uses a single SQLShop-style title bar and the normalized application icon for its brand mark. The source images remain unchanged; `src/branding_icon.rs` trims faint export noise and generates the same 80% rounded plate and transparent margins as the other Shop applications. Runtime and embedded EXE icons share this renderer. See [the shared branding conventions](../../docs/branding.md).

`ingenaro-spider-wordmark.png` is the wide spider-and-wordmark banner used at the top of the README, centered at 560 pixels to match the other Shop projects. Application icons continue to be generated from `ingenaro-logo.png`; `ingenaro-icon.png` retains its compact artwork role in the existing macOS/Linux editor header.

`naro-weaving.gif` is the transparent animated sprite shown while the editor draws canvas edits. It contains 12 frames at 50 ms each. The editor decodes and caches the GIF textures for playback.

The built-in ImageGen tool extracted the mascot from `ingenaro-icon.png`. The GIF adds a looping bob and tilt to the transparent cutout. ImageGen prompt:

```text
Use case: background-extraction
Asset type: tiny desktop UI mascot sprite, later encoded as an animated GIF.
Input image 1: exact edit target, Ingenaro's purple spider.
Extract only the existing smiling purple spider as a clean cutout with genuine transparent alpha. Keep the original character's silhouette, pose, raised right paw, every leg, big upward-looking eyes, happy open mouth, lavender stripes, and original purple shading and gradients as closely as possible. Remove the entire pale lavender field, the web lines and green nodes behind the character, and all cast/ground shadows. Preserve the three small mint wave marks above the raised paw as part of the cutout. Include the entire original silhouette with a small clear transparent safety margin on every side. Keep original composition and line quality, with clean edges and absolutely no opaque plate, glow, lettering, checkerboard, or added scenery. Output one complete single-frame cutout on transparent RGBA.
```
