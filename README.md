# GLTF-Editor-RS

React controls + Rust rendering for browser GLTF viewing.

## Stack

- Frontend: React + Vite + TypeScript
- Renderer: Rust -> WebAssembly (`wasm-bindgen`) + WebGL2
- GLTF parsing: Rust `gltf` crate
- Shading: Textured PBR-style lighting (base color texture + metallic/roughness factors)

## Project Layout

- `frontend/`: React app, control panel, file upload, camera sliders, pointer event wiring
- `backend/`: Rust renderer compiled to WASM

## Prerequisites

- Node.js 20+
- Rust stable (`rustup`)
- `wasm-pack`

Install `wasm-pack` if needed:

```bash
cargo install wasm-pack
```

## Run Locally

From project root:

```bash
npm run install:frontend
npm run dev
```

Or run explicit build from root:

```bash
npm run build
```

Manual frontend-only flow:

```bash
cd frontend
npm install
npm run wasm:build
npm run dev
```

Then open `http://localhost:5173`.

## Notes

- The viewport rendering path is Rust-only.
- React only drives UI state and sends camera/file commands into Rust.
- Mouse controls are handled in Rust: left drag orbits, middle/right drag pans, wheel zooms.
- React sliders still override camera state any time you move them.
- `.glb` works best. Self-contained `.gltf` files may work when buffers are embedded.
