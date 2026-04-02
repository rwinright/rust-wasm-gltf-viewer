declare module "./wasm/pkg/gltf_editor_rs_backend" {
  export default function init(input?: RequestInfo | URL | Response | BufferSource | WebAssembly.Module): Promise<void>;

  export class Viewer {
    constructor(canvasId: string);
    resize(): void;
    set_camera(yaw: number, pitch: number, distance: number): void;
    get_camera(): Float32Array;
    set_background(r: number, g: number, b: number): void;
    begin_orbit(x: number, y: number): void;
    drag_orbit(x: number, y: number): void;
    end_orbit(): void;
    begin_pan(x: number, y: number): void;
    drag_pan(x: number, y: number): void;
    end_pan(): void;
    zoom_by(delta: number): void;
    get_scene_stats(): Float64Array;
    get_mesh_names(): string[];
    get_selected_mesh(): number;
    select_mesh(meshIndex: number): boolean;
    clear_selection(): void;
    reset_camera_to_scene(): void;
    pick_mesh(x: number, y: number): number;
    load_gltf_from_bytes(bytes: Uint8Array): void;
    render_frame(): void;
  }
}
