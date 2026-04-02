import { ChangeEvent, MouseEvent, useEffect, useMemo, useRef, useState } from "react";
import init, { Viewer } from "./wasm/pkg/gltf_editor_rs_backend";

type ViewerStatus = "initializing" | "ready" | "error";

type PerfStats = {
  fps: number;
  frameMs: number;
  renderMs: number;
  worstFrameMs: number;
  drawCalls: number;
  triangles: number;
  textures: number;
  geometryMB: number;
  textureMB: number;
  renderLoadPct: number;
};

function formatMeshDisplayName(name: string): string {
  const [base] = name.split("-");
  const trimmed = base.trim();
  return trimmed.length > 0 ? trimmed : name;
}

export default function App() {
  const viewerRef = useRef<Viewer | null>(null);
  const rafRef = useRef<number | null>(null);
  const pointerModeRef = useRef<"orbit" | "pan" | null>(null);
  const dragStartRef = useRef<[number, number] | null>(null);
  const dragMovedRef = useRef(false);
  const canvasRef = useRef<HTMLCanvasElement | null>(null);
  const meshItemRefs = useRef<Array<HTMLButtonElement | null>>([]);
  const perfRef = useRef({
    lastFrameTime: 0,
    frameCount: 0,
    elapsedMs: 0,
    frameTotalMs: 0,
    renderTotalMs: 0,
    worstFrameMs: 0,
    lastCommitMs: 0
  });

  const [status, setStatus] = useState<ViewerStatus>("initializing");
  const [message, setMessage] = useState("Compiling Rust viewer...");

  const [yaw, setYaw] = useState(0.8);
  const [pitch, setPitch] = useState(0.3);
  const [distance, setDistance] = useState(4.0);
  const [perfStats, setPerfStats] = useState<PerfStats>({
    fps: 0,
    frameMs: 0,
    renderMs: 0,
    worstFrameMs: 0,
    drawCalls: 0,
    triangles: 0,
    textures: 0,
    geometryMB: 0,
    textureMB: 0,
    renderLoadPct: 0
  });
  const [meshNames, setMeshNames] = useState<string[]>([]);
  const [selectedMesh, setSelectedMesh] = useState<number>(-1);

  const displayMeshNames = useMemo(() => {
    const baseNames = meshNames.map(formatMeshDisplayName);
    const totals = new Map<string, number>();
    for (const base of baseNames) {
      totals.set(base, (totals.get(base) ?? 0) + 1);
    }

    const seen = new Map<string, number>();
    return baseNames.map((base) => {
      const total = totals.get(base) ?? 0;
      if (total <= 1) {
        return base;
      }

      const index = (seen.get(base) ?? 0) + 1;
      seen.set(base, index);
      return `${base}-${index}`;
    });
  }, [meshNames]);

  const syncCameraFromViewer = () => {
    const viewer = viewerRef.current;
    if (!viewer) return;

    const [nextYaw, nextPitch, nextDistance] = viewer.get_camera();
    setYaw(nextYaw);
    setPitch(nextPitch);
    setDistance(nextDistance);
  };

  useEffect(() => {
    let disposed = false;
    let resizeHandler: (() => void) | null = null;

    const boot = async () => {
      try {
        await init();
        if (disposed) return;

        const viewer = new Viewer("viewer-canvas");
        viewerRef.current = viewer;
        setStatus("ready");
        setMessage("Viewer ready. Load a .glb or self-contained .gltf file.");

        resizeHandler = () => viewer.resize();
        window.addEventListener("resize", resizeHandler);
        viewer.resize();
        viewer.set_camera(yaw, pitch, distance);

        const frame = (timeMs: number) => {
          const perf = perfRef.current;

          if (perf.lastFrameTime !== 0) {
            const deltaMs = timeMs - perf.lastFrameTime;
            perf.frameCount += 1;
            perf.elapsedMs += deltaMs;
            perf.frameTotalMs += deltaMs;
            perf.worstFrameMs = Math.max(perf.worstFrameMs, deltaMs);
          }

          perf.lastFrameTime = timeMs;

          const renderStart = performance.now();
          viewer.render_frame();
          perf.renderTotalMs += performance.now() - renderStart;

          if (perf.elapsedMs >= 300 && perf.frameCount > 0) {
            const fps = (perf.frameCount * 1000) / perf.elapsedMs;
            const frameMs = perf.frameTotalMs / perf.frameCount;
            const renderMs = perf.renderTotalMs / perf.frameCount;
            const [drawCalls, triangles, textures, geometryBytes, textureBytes] =
              viewer.get_scene_stats();

            setPerfStats({
              fps,
              frameMs,
              renderMs,
              worstFrameMs: perf.worstFrameMs,
              drawCalls,
              triangles,
              textures,
              geometryMB: geometryBytes / (1024 * 1024),
              textureMB: textureBytes / (1024 * 1024),
              renderLoadPct: frameMs > 0 ? (renderMs / frameMs) * 100 : 0
            });

            perf.frameCount = 0;
            perf.elapsedMs = 0;
            perf.frameTotalMs = 0;
            perf.renderTotalMs = 0;
            perf.worstFrameMs = 0;
            perf.lastCommitMs = timeMs;
          }

          rafRef.current = requestAnimationFrame(frame);
        };

        rafRef.current = requestAnimationFrame(frame);
      } catch (error) {
        setStatus("error");
        setMessage(`Failed to initialize viewer: ${String(error)}`);
      }
    };

    void boot();

    return () => {
      disposed = true;

      if (resizeHandler) {
        window.removeEventListener("resize", resizeHandler);
      }

      if (rafRef.current !== null) {
        cancelAnimationFrame(rafRef.current);
      }

      perfRef.current.lastFrameTime = 0;

      viewerRef.current = null;
    };
  }, []);

  useEffect(() => {
    if (!viewerRef.current || status !== "ready") return;
    viewerRef.current.set_camera(yaw, pitch, distance);
  }, [yaw, pitch, distance, status]);

  useEffect(() => {
    if (selectedMesh < 0) return;
    meshItemRefs.current[selectedMesh]?.scrollIntoView({
      behavior: "smooth",
      block: "nearest"
    });
  }, [selectedMesh]);

  const handleFileChange = async (event: ChangeEvent<HTMLInputElement>) => {
    const file = event.target.files?.[0];
    if (!file || !viewerRef.current) {
      return;
    }

    try {
      const bytes = new Uint8Array(await file.arrayBuffer());
      viewerRef.current.load_gltf_from_bytes(bytes);
      setMeshNames(Array.from(viewerRef.current.get_mesh_names()));
      setSelectedMesh(viewerRef.current.get_selected_mesh());
      syncCameraFromViewer();
      setMessage(`Loaded ${file.name}`);
    } catch (error) {
      setMessage(`Could not load file: ${String(error)}`);
    }
  };

  const resetCamera = () => {
    const viewer = viewerRef.current;
    if (viewer) {
      viewer.reset_camera_to_scene();
      setSelectedMesh(-1);
      syncCameraFromViewer();
    }
  };

  const handleMouseDown = (event: MouseEvent<HTMLCanvasElement>) => {
    const viewer = viewerRef.current;
    if (!viewer) return;

    dragStartRef.current = [event.clientX, event.clientY];
    dragMovedRef.current = false;

    if (event.button === 0) {
      pointerModeRef.current = "orbit";
      viewer.begin_orbit(event.clientX, event.clientY);
    } else if (event.button === 1 || event.button === 2) {
      pointerModeRef.current = "pan";
      viewer.begin_pan(event.clientX, event.clientY);
    }
  };

  const handleMouseMove = (event: MouseEvent<HTMLCanvasElement>) => {
    const viewer = viewerRef.current;
    if (!viewer) return;

    if (dragStartRef.current) {
      const [sx, sy] = dragStartRef.current;
      if (Math.abs(event.clientX - sx) > 3 || Math.abs(event.clientY - sy) > 3) {
        dragMovedRef.current = true;
      }
    }

    if (pointerModeRef.current === "orbit") {
      viewer.drag_orbit(event.clientX, event.clientY);
      syncCameraFromViewer();
    } else if (pointerModeRef.current === "pan") {
      viewer.drag_pan(event.clientX, event.clientY);
      syncCameraFromViewer();
    }
  };

  const handlePointerEnd = () => {
    const viewer = viewerRef.current;
    if (!viewer) return;

    viewer.end_orbit();
    viewer.end_pan();
    pointerModeRef.current = null;
    dragStartRef.current = null;
  };

  const handleCanvasClick = (event: MouseEvent<HTMLCanvasElement>) => {
    const viewer = viewerRef.current;
    if (!viewer || dragMovedRef.current) {
      return;
    }

    const picked = viewer.pick_mesh(event.nativeEvent.offsetX, event.nativeEvent.offsetY);
    if (picked >= 0) {
      if (picked === selectedMesh) {
        viewer.clear_selection();
        setSelectedMesh(-1);
      } else {
        viewer.select_mesh(picked);
        setSelectedMesh(picked);
      }
      syncCameraFromViewer();
    } else {
      viewer.clear_selection();
      setSelectedMesh(-1);
      syncCameraFromViewer();
    }
  };

  const handleSelectMesh = (meshIndex: number) => {
    const viewer = viewerRef.current;
    if (!viewer) return;

    if (selectedMesh === meshIndex) {
      viewer.clear_selection();
      setSelectedMesh(-1);
      syncCameraFromViewer();
    } else if (viewer.select_mesh(meshIndex)) {
      setSelectedMesh(meshIndex);
      syncCameraFromViewer();
    }
  };

  const handleClearSelection = () => {
    const viewer = viewerRef.current;
    if (!viewer) return;

    viewer.clear_selection();
    setSelectedMesh(-1);
    syncCameraFromViewer();
  };

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;

    const onWheel = (event: globalThis.WheelEvent) => {
      event.preventDefault();
      const viewer = viewerRef.current;
      if (!viewer) return;
      viewer.zoom_by(event.deltaY);
      syncCameraFromViewer();
    };

    canvas.addEventListener("wheel", onWheel, { passive: false });
    return () => canvas.removeEventListener("wheel", onWheel);
  }, []);


  return (
    <div className="page">
      <aside className="panel">
        <h1>GLTF Viewer RS</h1>
        <p>
          React owns UI state and controls. Rust + WebAssembly owns the rendering pipeline in the
          viewport.
        </p>

        <label className="file-picker">
          <span>Model file</span>
          <input
            disabled={status !== "ready"}
            type="file"
            accept=".glb,.gltf"
            onChange={handleFileChange}
          />
        </label>

        <div className="controls">
          <Control
            label="Yaw"
            value={yaw}
            min={-3.14}
            max={3.14}
            step={0.01}
            onChange={setYaw}
          />
          <Control
            label="Pitch"
            value={pitch}
            min={-1.4}
            max={1.4}
            step={0.01}
            onChange={setPitch}
          />
          <Control
            label="Distance"
            value={distance}
            min={0.5}
            max={40}
            step={0.1}
            onChange={setDistance}
          />
        </div>

        <button type="button" onClick={resetCamera} disabled={status !== "ready"}>
          Reset camera
        </button>

        <div className="mesh-list-wrap">
          <div className="mesh-list-head">
            <h2>Meshes</h2>
            <button type="button" onClick={handleClearSelection} disabled={selectedMesh < 0}>
              Clear
            </button>
          </div>
          <div className="mesh-list">
            {meshNames.length === 0 ? (
              <div className="mesh-empty">Load a model to list meshes</div>
            ) : (
              meshNames.map((name, index) => (
                <button
                  key={`${name}-${index}`}
                  type="button"
                  className={`mesh-item ${selectedMesh === index ? "is-selected" : ""}`}
                  onClick={() => handleSelectMesh(index)}
                  ref={(el) => {
                    meshItemRefs.current[index] = el;
                  }}
                >
                  {displayMeshNames[index]}
                </button>
              ))
            )}
          </div>
        </div>

        <div className={`status status-${status}`}>{message}</div>
      </aside>

      <main className="viewport-wrap">
        <div className="perf-viewer" aria-live="polite">
          <div>FPS: {perfStats.fps.toFixed(1)}</div>
          <div>Frame: {perfStats.frameMs.toFixed(2)} ms</div>
          <div>Render: {perfStats.renderMs.toFixed(2)} ms</div>
          <div>Worst: {perfStats.worstFrameMs.toFixed(2)} ms</div>
          <div>Draw calls: {Math.round(perfStats.drawCalls)}</div>
          <div>Triangles: {Math.round(perfStats.triangles).toLocaleString()}</div>
          <div>Textures: {Math.round(perfStats.textures)}</div>
          <div>Geometry: {perfStats.geometryMB.toFixed(2)} MB</div>
          <div>Textures mem: {perfStats.textureMB.toFixed(2)} MB</div>
          <div>Render load: {perfStats.renderLoadPct.toFixed(1)}%</div>
        </div>
        <canvas
          id="viewer-canvas"
          className="viewport"
          ref={canvasRef}
          onMouseDown={handleMouseDown}
          onMouseMove={handleMouseMove}
          onMouseUp={handlePointerEnd}
          onMouseLeave={handlePointerEnd}
          onClick={handleCanvasClick}
          onContextMenu={(event) => event.preventDefault()}
        />
      </main>
    </div>
  );
}

type ControlProps = {
  label: string;
  min: number;
  max: number;
  value: number;
  step: number;
  onChange: (v: number) => void;
};

function Control({ label, min, max, value, step, onChange }: ControlProps) {
  return (
    <label className="control">
      <div>
        <span>{label}</span>
        <strong>{value.toFixed(2)}</strong>
      </div>
      <input
        type="range"
        min={min}
        max={max}
        step={step}
        value={value}
        onChange={(event) => onChange(Number(event.target.value))}
      />
    </label>
  );
}
