use gltf::image::Format as ImageFormat;
use gltf::mesh::Mode;
use serde_json::Value;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{
    HtmlCanvasElement, WebGl2RenderingContext as GL, WebGlBuffer, WebGlProgram, WebGlShader,
    WebGlTexture, WebGlUniformLocation, WebGlVertexArrayObject,
};

#[wasm_bindgen]
pub struct Viewer {
    canvas: HtmlCanvasElement,
    gl: GL,
    program: WebGlProgram,
    uniforms: Uniforms,
    scene: Option<SceneGpu>,
    yaw: f32,
    pitch: f32,
    distance: f32,
    center: [f32; 3],
    target_distance: f32,
    target_center: [f32; 3],
    pre_select_distance: f32,
    pre_select_center: [f32; 3],
    bg: [f32; 4],
    orbiting: bool,
    panning: bool,
    last_pointer: [f32; 2],
}

struct Uniforms {
    u_view: WebGlUniformLocation,
    u_proj: WebGlUniformLocation,
    u_camera_pos: WebGlUniformLocation,
    u_base_color_factor: WebGlUniformLocation,
    u_metallic_factor: WebGlUniformLocation,
    u_roughness_factor: WebGlUniformLocation,
    u_has_base_color_tex: WebGlUniformLocation,
    u_base_color_tex: WebGlUniformLocation,
    u_highlight: WebGlUniformLocation,
}

struct SceneGpu {
    primitives: Vec<PrimitiveGpu>,
    meshes: Vec<MeshGpu>,
    textures: Vec<WebGlTexture>,
    default_texture: WebGlTexture,
    selected_mesh: Option<usize>,
    scene_center: [f32; 3],
    scene_radius: f32,
    stats: SceneStats,
}

struct MeshGpu {
    name: String,
    parent_mesh: Option<usize>,
    depth: usize,
    volume: f32,
    center: [f32; 3],
    radius: f32,
    aabb_min: [f32; 3],
    aabb_max: [f32; 3],
}

struct SceneStats {
    draw_calls: usize,
    triangles: usize,
    texture_count: usize,
    geometry_bytes: usize,
    texture_bytes: usize,
}

struct PrimitiveGpu {
    vao: WebGlVertexArrayObject,
    _vbo: WebGlBuffer,
    _ebo: WebGlBuffer,
    index_count: i32,
    base_color_factor: [f32; 4],
    metallic_factor: f32,
    roughness_factor: f32,
    base_color_tex_index: Option<usize>,
    mesh_index: usize,
}

struct PrimitiveCpu {
    vertices: Vec<f32>,
    indices: Vec<u32>,
    base_color_factor: [f32; 4],
    metallic_factor: f32,
    roughness_factor: f32,
    base_color_tex_index: Option<usize>,
    mesh_index: usize,
}

struct MeshCpu {
    name: String,
    parent_mesh: Option<usize>,
    depth: usize,
    signed_volume: f32,
    positions: Vec<[f32; 3]>,
}

#[wasm_bindgen]
impl Viewer {
    #[wasm_bindgen(constructor)]
    pub fn new(canvas_id: &str) -> Result<Viewer, JsValue> {
        console_error_panic_hook::set_once();

        let window = web_sys::window().ok_or_else(|| JsValue::from_str("No window"))?;
        let document = window
            .document()
            .ok_or_else(|| JsValue::from_str("No document"))?;

        let canvas = document
            .get_element_by_id(canvas_id)
            .ok_or_else(|| JsValue::from_str("Canvas not found"))?
            .dyn_into::<HtmlCanvasElement>()?;

        let gl = canvas
            .get_context("webgl2")?
            .ok_or_else(|| JsValue::from_str("WebGL2 context not available"))?
            .dyn_into::<GL>()?;

        let program = create_program(&gl, VERT_SHADER_SOURCE, FRAG_SHADER_SOURCE)?;
        let uniforms = Uniforms {
            u_view: get_uniform(&gl, &program, "u_view")?,
            u_proj: get_uniform(&gl, &program, "u_proj")?,
            u_camera_pos: get_uniform(&gl, &program, "u_camera_pos")?,
            u_base_color_factor: get_uniform(&gl, &program, "u_base_color_factor")?,
            u_metallic_factor: get_uniform(&gl, &program, "u_metallic_factor")?,
            u_roughness_factor: get_uniform(&gl, &program, "u_roughness_factor")?,
            u_has_base_color_tex: get_uniform(&gl, &program, "u_has_base_color_tex")?,
            u_base_color_tex: get_uniform(&gl, &program, "u_base_color_tex")?,
            u_highlight: get_uniform(&gl, &program, "u_highlight")?,
        };

        gl.enable(GL::DEPTH_TEST);
        gl.enable(GL::CULL_FACE);

        let mut viewer = Viewer {
            canvas,
            gl,
            program,
            uniforms,
            scene: None,
            yaw: 0.8,
            pitch: 0.3,
            distance: 4.0,
            center: [0.0, 0.0, 0.0],
            target_distance: 4.0,
            target_center: [0.0, 0.0, 0.0],
            pre_select_distance: 4.0,
            pre_select_center: [0.0, 0.0, 0.0],
            bg: [0.15, 0.16, 0.19, 1.0],
            orbiting: false,
            panning: false,
            last_pointer: [0.0, 0.0],
        };

        viewer.resize();
        Ok(viewer)
    }

    pub fn resize(&mut self) {
        let width = self.canvas.client_width().max(1) as u32;
        let height = self.canvas.client_height().max(1) as u32;

        if self.canvas.width() != width || self.canvas.height() != height {
            self.canvas.set_width(width);
            self.canvas.set_height(height);
        }

        self.gl.viewport(0, 0, width as i32, height as i32);
    }

    pub fn set_camera(&mut self, yaw: f32, pitch: f32, distance: f32) {
        self.yaw = yaw;
        self.pitch = pitch.clamp(-1.5, 1.5);
        self.distance = distance.max(0.1);
        self.target_distance = self.distance;
        self.target_center = self.center;
    }

    pub fn get_camera(&self) -> js_sys::Float32Array {
        js_sys::Float32Array::from(&[self.yaw, self.pitch, self.distance][..])
    }

    pub fn set_background(&mut self, r: f32, g: f32, b: f32) {
        self.bg = [r.clamp(0.0, 1.0), g.clamp(0.0, 1.0), b.clamp(0.0, 1.0), 1.0];
    }

    pub fn begin_orbit(&mut self, x: f32, y: f32) {
        self.orbiting = true;
        self.last_pointer = [x, y];
    }

    pub fn drag_orbit(&mut self, x: f32, y: f32) {
        if !self.orbiting {
            return;
        }

        let dx = x - self.last_pointer[0];
        let dy = y - self.last_pointer[1];
        self.last_pointer = [x, y];

        self.yaw += dx * 0.01;
        self.pitch = (self.pitch + dy * 0.01).clamp(-1.5, 1.5);
    }

    pub fn end_orbit(&mut self) {
        self.orbiting = false;
    }

    pub fn begin_pan(&mut self, x: f32, y: f32) {
        self.panning = true;
        self.last_pointer = [x, y];
    }

    pub fn drag_pan(&mut self, x: f32, y: f32) {
        if !self.panning {
            return;
        }

        let dx = x - self.last_pointer[0];
        let dy = y - self.last_pointer[1];
        self.last_pointer = [x, y];

        let eye = self.camera_eye();
        let forward = normalize([
            self.center[0] - eye[0],
            self.center[1] - eye[1],
            self.center[2] - eye[2],
        ]);
        let right = normalize(cross(forward, [0.0, 1.0, 0.0]));
        let up = normalize(cross(right, forward));

        let pan_scale = self.distance * 0.002;
        let tx = -dx * pan_scale;
        let ty = dy * pan_scale;

        self.center = add3(self.center, scale3(right, tx));
        self.center = add3(self.center, scale3(up, ty));
        self.target_center = self.center;
    }

    pub fn end_pan(&mut self) {
        self.panning = false;
    }

    pub fn zoom_by(&mut self, delta: f32) {
        let zoom = (1.0 + delta * 0.001).clamp(0.2, 5.0);
        self.distance = (self.distance * zoom).clamp(0.1, 5000.0);
        self.target_distance = self.distance;
    }

    pub fn get_scene_stats(&self) -> js_sys::Float64Array {
        let Some(scene) = &self.scene else {
            return js_sys::Float64Array::from(&[0.0, 0.0, 0.0, 0.0, 0.0][..]);
        };

        js_sys::Float64Array::from(
            &[
                scene.stats.draw_calls as f64,
                scene.stats.triangles as f64,
                scene.stats.texture_count as f64,
                scene.stats.geometry_bytes as f64,
                scene.stats.texture_bytes as f64,
            ][..],
        )
    }

    pub fn get_mesh_names(&self) -> js_sys::Array {
        let names = js_sys::Array::new();
        if let Some(scene) = &self.scene {
            for mesh in &scene.meshes {
                names.push(&JsValue::from_str(&mesh.name));
            }
        }
        names
    }

    pub fn get_selected_mesh(&self) -> i32 {
        self.scene
            .as_ref()
            .and_then(|scene| scene.selected_mesh)
            .map(|idx| idx as i32)
            .unwrap_or(-1)
    }

    pub fn select_mesh(&mut self, mesh_index: i32) -> bool {
        let Some(scene) = &mut self.scene else {
            return false;
        };

        if mesh_index < 0 {
            return false;
        }

        let idx = mesh_index as usize;
        let Some(mesh) = scene.meshes.get(idx) else {
            return false;
        };

        // Save camera state before the first selection so we can restore it on deselect
        if scene.selected_mesh.is_none() {
            self.pre_select_center = self.target_center;
            self.pre_select_distance = self.target_distance;
        }

        scene.selected_mesh = Some(idx);
        self.target_center = mesh.center;
        self.target_distance = (mesh.radius * 2.5).max(0.6);
        true
    }

    pub fn clear_selection(&mut self) {
        if let Some(scene) = &mut self.scene {
            scene.selected_mesh = None;
            self.target_center = self.pre_select_center;
            self.target_distance = self.pre_select_distance;
        }
    }

    pub fn pick_mesh(&self, x: f32, y: f32) -> i32 {
        let Some(scene) = &self.scene else {
            return -1;
        };

        let width = self.canvas.width().max(1) as f32;
        let height = self.canvas.height().max(1) as f32;
        if x < 0.0 || y < 0.0 || x > width || y > height {
            return -1;
        }

        let aspect = width / height;
        let fovy = 45.0_f32.to_radians();
        let tan_half = (fovy * 0.5).tan();

        let ndc_x = (2.0 * x / width) - 1.0;
        let ndc_y = 1.0 - (2.0 * y / height);

        let eye = self.camera_eye();
        let forward = normalize([
            self.center[0] - eye[0],
            self.center[1] - eye[1],
            self.center[2] - eye[2],
        ]);
        let right = normalize(cross(forward, [0.0, 1.0, 0.0]));
        let up = normalize(cross(right, forward));

        let ray_camera = normalize([ndc_x * aspect * tan_half, ndc_y * tan_half, -1.0]);
        let ray_world = normalize(add3(
            scale3(right, ray_camera[0]),
            add3(scale3(up, ray_camera[1]), scale3(forward, -ray_camera[2])),
        ));

        let mut best_idx: i32 = -1;
        let mut best_t = f32::INFINITY;
        for (idx, mesh) in scene.meshes.iter().enumerate() {
            if let Some(t) = ray_aabb_hit(eye, ray_world, mesh.aabb_min, mesh.aabb_max) {
                if t < best_t {
                    best_t = t;
                    best_idx = idx as i32;
                }
            }
        }

        best_idx
    }

    pub fn load_gltf_from_bytes(&mut self, bytes: Vec<u8>) -> Result<(), JsValue> {
        let (document, buffers, images) = match gltf::import_slice(&bytes) {
            Ok(parsed) => parsed,
            Err(first_error) => {
                if is_probably_json_gltf(&bytes) {
                    if let Ok(sanitized) = sanitize_gltf_json_null_numbers(&bytes) {
                        if let Ok(parsed) = gltf::import_slice(&sanitized) {
                            parsed
                        } else {
                            return Err(JsValue::from_str(&format!(
                                "GLTF parse error: {first_error}"
                            )));
                        }
                    } else {
                        return Err(JsValue::from_str(&format!(
                            "GLTF parse error: {first_error}"
                        )));
                    }
                } else {
                    return Err(JsValue::from_str(&format!("GLTF parse error: {first_error}")));
                }
            }
        };

        let scene = document
            .default_scene()
            .or_else(|| document.scenes().next())
            .ok_or_else(|| JsValue::from_str("No scene found in GLTF"))?;

        let mut cpu_primitives: Vec<PrimitiveCpu> = Vec::new();
        let mut cpu_meshes: Vec<MeshCpu> = Vec::new();
        let mut world_positions: Vec<[f32; 3]> = Vec::new();

        // Keep import transform neutral by default; source tools should define orientation.
        let root_correction = [
            1.0, 0.0, 0.0, 0.0,
            0.0, 1.0, 0.0, 0.0,
            0.0, 0.0, 1.0, 0.0,
            0.0, 0.0, 0.0, 1.0,
        ];

        for node in scene.nodes() {
            collect_node_meshes(
                &node,
                root_correction,
                &buffers,
                &mut cpu_primitives,
                &mut cpu_meshes,
                &mut world_positions,
            )?;
        }

        if cpu_primitives.is_empty() {
            return Err(JsValue::from_str("No triangle mesh geometry found"));
        }

        let (center, radius) = compute_bounds(&world_positions);
        self.center = center;
        self.distance = (radius * 2.5).max(1.0);
        self.target_center = self.center;
        self.target_distance = self.distance;
        self.pre_select_center = self.center;
        self.pre_select_distance = self.distance;

        let (textures, texture_bytes) = upload_gltf_textures(&self.gl, &images)?;
        let default_texture = create_default_white_texture(&self.gl)?;

        let mut primitives_gpu: Vec<PrimitiveGpu> = Vec::with_capacity(cpu_primitives.len());
        let mut meshes_gpu: Vec<MeshGpu> = Vec::with_capacity(cpu_meshes.len());
        let mut triangles: usize = 0;
        let mut geometry_bytes: usize = 0;

        for mesh in cpu_meshes {
            let (aabb_min, aabb_max, center, radius) = if mesh.positions.is_empty() {
                ([0.0, 0.0, 0.0], [0.0, 0.0, 0.0], [0.0, 0.0, 0.0], 0.001)
            } else {
                compute_bounds_ext(&mesh.positions)
            };
            meshes_gpu.push(MeshGpu {
                name: mesh.name,
                parent_mesh: mesh.parent_mesh,
                depth: mesh.depth,
                volume: mesh.signed_volume.abs(),
                center,
                radius,
                aabb_min,
                aabb_max,
            });
        }

        for primitive in cpu_primitives {
            triangles += primitive.indices.len() / 3;
            geometry_bytes += primitive.vertices.len() * std::mem::size_of::<f32>();
            geometry_bytes += primitive.indices.len() * std::mem::size_of::<u32>();

            let vao = self
                .gl
                .create_vertex_array()
                .ok_or_else(|| JsValue::from_str("Failed to create VAO"))?;
            let vbo = self
                .gl
                .create_buffer()
                .ok_or_else(|| JsValue::from_str("Failed to create VBO"))?;
            let ebo = self
                .gl
                .create_buffer()
                .ok_or_else(|| JsValue::from_str("Failed to create EBO"))?;

            self.gl.bind_vertex_array(Some(&vao));

            self.gl.bind_buffer(GL::ARRAY_BUFFER, Some(&vbo));
            let vertex_array = js_sys::Float32Array::from(primitive.vertices.as_slice());
            self.gl.buffer_data_with_array_buffer_view(
                GL::ARRAY_BUFFER,
                &vertex_array,
                GL::STATIC_DRAW,
            );

            self.gl.bind_buffer(GL::ELEMENT_ARRAY_BUFFER, Some(&ebo));
            let index_array = js_sys::Uint32Array::from(primitive.indices.as_slice());
            self.gl.buffer_data_with_array_buffer_view(
                GL::ELEMENT_ARRAY_BUFFER,
                &index_array,
                GL::STATIC_DRAW,
            );

            let stride = (8 * std::mem::size_of::<f32>()) as i32;

            self.gl.enable_vertex_attrib_array(0);
            self.gl
                .vertex_attrib_pointer_with_i32(0, 3, GL::FLOAT, false, stride, 0);

            self.gl.enable_vertex_attrib_array(1);
            self.gl.vertex_attrib_pointer_with_i32(
                1,
                3,
                GL::FLOAT,
                false,
                stride,
                (3 * std::mem::size_of::<f32>()) as i32,
            );

            self.gl.enable_vertex_attrib_array(2);
            self.gl.vertex_attrib_pointer_with_i32(
                2,
                2,
                GL::FLOAT,
                false,
                stride,
                (6 * std::mem::size_of::<f32>()) as i32,
            );

            self.gl.bind_vertex_array(None);

            primitives_gpu.push(PrimitiveGpu {
                vao,
                _vbo: vbo,
                _ebo: ebo,
                index_count: primitive.indices.len() as i32,
                base_color_factor: primitive.base_color_factor,
                metallic_factor: primitive.metallic_factor,
                roughness_factor: primitive.roughness_factor,
                base_color_tex_index: primitive.base_color_tex_index,
                mesh_index: primitive.mesh_index,
            });
        }

        self.scene = Some(SceneGpu {
            meshes: meshes_gpu,
            stats: SceneStats {
                draw_calls: primitives_gpu.len(),
                triangles,
                texture_count: textures.len(),
                geometry_bytes,
                texture_bytes,
            },
            primitives: primitives_gpu,
            textures,
            default_texture,
            selected_mesh: None,
            scene_center: center,
            scene_radius: radius,
        });

        Ok(())
    }

    pub fn render_frame(&mut self) {
        // Smoothly approach target focus to avoid abrupt camera snapping.
        self.center = lerp3(self.center, self.target_center, 0.14);
        self.distance = lerp(self.distance, self.target_distance, 0.14);

        self.gl.clear_color(self.bg[0], self.bg[1], self.bg[2], self.bg[3]);
        self.gl.clear(GL::COLOR_BUFFER_BIT | GL::DEPTH_BUFFER_BIT);

        let Some(scene) = &self.scene else {
            return;
        };

        let aspect = (self.canvas.width().max(1) as f32) / (self.canvas.height().max(1) as f32);
        let proj = mat4_perspective(45.0_f32.to_radians(), aspect, 0.01, 2000.0);

        let eye = self.camera_eye();
        let view = mat4_look_at(eye, self.center, [0.0, 1.0, 0.0]);

        self.gl.use_program(Some(&self.program));
        self.gl
            .uniform_matrix4fv_with_f32_array(Some(&self.uniforms.u_view), false, &view);
        self.gl
            .uniform_matrix4fv_with_f32_array(Some(&self.uniforms.u_proj), false, &proj);
        self.gl
            .uniform3f(Some(&self.uniforms.u_camera_pos), eye[0], eye[1], eye[2]);
        self.gl.uniform1i(Some(&self.uniforms.u_base_color_tex), 0);

        for primitive in &scene.primitives {
            let is_highlighted = scene
                .selected_mesh
                .is_some_and(|idx| idx == primitive.mesh_index);
            self.gl
                .uniform1i(Some(&self.uniforms.u_highlight), if is_highlighted { 1 } else { 0 });

            self.gl.uniform4f(
                Some(&self.uniforms.u_base_color_factor),
                primitive.base_color_factor[0],
                primitive.base_color_factor[1],
                primitive.base_color_factor[2],
                primitive.base_color_factor[3],
            );
            self.gl.uniform1f(
                Some(&self.uniforms.u_metallic_factor),
                primitive.metallic_factor.clamp(0.0, 1.0),
            );
            self.gl.uniform1f(
                Some(&self.uniforms.u_roughness_factor),
                primitive.roughness_factor.clamp(0.04, 1.0),
            );

            self.gl.active_texture(GL::TEXTURE0);
            match primitive.base_color_tex_index {
                Some(index) if index < scene.textures.len() => {
                    self.gl.uniform1i(Some(&self.uniforms.u_has_base_color_tex), 1);
                    self.gl
                        .bind_texture(GL::TEXTURE_2D, Some(&scene.textures[index]));
                }
                _ => {
                    self.gl.uniform1i(Some(&self.uniforms.u_has_base_color_tex), 0);
                    self.gl
                        .bind_texture(GL::TEXTURE_2D, Some(&scene.default_texture));
                }
            }

            self.gl.bind_vertex_array(Some(&primitive.vao));
            self.gl.draw_elements_with_i32(
                GL::TRIANGLES,
                primitive.index_count,
                GL::UNSIGNED_INT,
                0,
            );
        }

        self.gl.bind_vertex_array(None);
    }

    fn camera_eye(&self) -> [f32; 3] {
        [
            self.center[0] + self.distance * self.pitch.cos() * self.yaw.cos(),
            self.center[1] + self.distance * self.pitch.sin(),
            self.center[2] + self.distance * self.pitch.cos() * self.yaw.sin(),
        ]
    }
}

fn collect_node_meshes(
    node: &gltf::Node,
    parent_matrix: [f32; 16],
    buffers: &[gltf::buffer::Data],
    out_primitives: &mut Vec<PrimitiveCpu>,
    out_meshes: &mut Vec<MeshCpu>,
    out_world_positions: &mut Vec<[f32; 3]>,
) -> Result<(), JsValue> {
    let local = mat4_from_gltf(node.transform().matrix());
    let world = mat4_mul(&parent_matrix, &local);

    if let Some(mesh) = node.mesh() {
        let mesh_name = node
            .name()
            .map(|s| s.to_string())
            .or_else(|| mesh.name().map(|s| s.to_string()))
            .unwrap_or_else(|| format!("Mesh {}", out_meshes.len() + 1));
        let mesh_index = out_meshes.len();
        out_meshes.push(MeshCpu {
            name: mesh_name,
            parent_mesh: None,
            depth: 0,
            signed_volume: 0.0,
            positions: Vec::new(),
        });

        for primitive in mesh.primitives() {
            if primitive.mode() != Mode::Triangles {
                continue;
            }

            let reader = primitive.reader(|buffer| Some(&buffers[buffer.index()]));

            let positions: Vec<[f32; 3]> = reader
                .read_positions()
                .ok_or_else(|| JsValue::from_str("Primitive missing POSITION"))?
                .collect();

            let mut normals: Vec<[f32; 3]> = if let Some(n) = reader.read_normals() {
                n.collect()
            } else {
                vec![[0.0, 0.0, 1.0]; positions.len()]
            };

            let texcoords: Vec<[f32; 2]> = if let Some(tc) = reader.read_tex_coords(0) {
                tc.into_f32().collect()
            } else {
                vec![[0.0, 0.0]; positions.len()]
            };

            let raw_indices: Vec<u32> = if let Some(indices) = reader.read_indices() {
                indices.into_u32().collect()
            } else {
                (0..positions.len() as u32).collect()
            };

            // Some exporters emit invalid index streams. Keep only triangles that reference
            // valid vertices to avoid WASM panics from out-of-bounds indexing.
            let mut local_indices: Vec<u32> = Vec::with_capacity(raw_indices.len());
            for tri in raw_indices.chunks_exact(3) {
                let i0 = tri[0] as usize;
                let i1 = tri[1] as usize;
                let i2 = tri[2] as usize;
                if i0 < positions.len() && i1 < positions.len() && i2 < positions.len() {
                    local_indices.extend_from_slice(tri);
                }
            }

            if local_indices.is_empty() {
                continue;
            }

            if reader.read_normals().is_none() {
                compute_flat_normals(&positions, &local_indices, &mut normals);
            }

            let mut vertices: Vec<f32> = Vec::with_capacity(positions.len() * 8);
            for i in 0..positions.len() {
                let p = transform_point(&world, positions[i]);
                let n = normalize(transform_vector(
                    &world,
                    normals.get(i).copied().unwrap_or([0.0, 1.0, 0.0]),
                ));
                let uv = texcoords.get(i).copied().unwrap_or([0.0, 0.0]);

                out_world_positions.push(p);
                out_meshes[mesh_index].positions.push(p);

                vertices.extend_from_slice(&[p[0], p[1], p[2], n[0], n[1], n[2], uv[0], uv[1]]);
            }

            let pbr = primitive.material().pbr_metallic_roughness();
            let base_color = pbr.base_color_factor();
            let base_color_tex_index = pbr.base_color_texture().map(|info| info.texture().index());

            out_primitives.push(PrimitiveCpu {
                vertices,
                indices: local_indices,
                base_color_factor: [base_color[0], base_color[1], base_color[2], base_color[3]],
                metallic_factor: pbr.metallic_factor(),
                roughness_factor: pbr.roughness_factor(),
                base_color_tex_index,
                mesh_index,
            });
        }
    }

    for child in node.children() {
        collect_node_meshes(
            &child,
            world,
            buffers,
            out_primitives,
            out_meshes,
            out_world_positions,
        )?;
    }

    Ok(())
}

fn upload_gltf_textures(
    gl: &GL,
    images: &[gltf::image::Data],
) -> Result<(Vec<WebGlTexture>, usize), JsValue> {
    let mut textures = Vec::with_capacity(images.len());
    let mut texture_bytes: usize = 0;

    for image in images {
        let texture = gl
            .create_texture()
            .ok_or_else(|| JsValue::from_str("Failed to create texture"))?;
        gl.bind_texture(GL::TEXTURE_2D, Some(&texture));

        let rgba = image_to_rgba8(image);
        texture_bytes += rgba.len();
        gl.tex_image_2d_with_i32_and_i32_and_i32_and_format_and_type_and_opt_u8_array(
            GL::TEXTURE_2D,
            0,
            GL::RGBA as i32,
            image.width as i32,
            image.height as i32,
            0,
            GL::RGBA,
            GL::UNSIGNED_BYTE,
            Some(&rgba),
        )?;

        gl.tex_parameteri(GL::TEXTURE_2D, GL::TEXTURE_WRAP_S, GL::REPEAT as i32);
        gl.tex_parameteri(GL::TEXTURE_2D, GL::TEXTURE_WRAP_T, GL::REPEAT as i32);
        gl.tex_parameteri(
            GL::TEXTURE_2D,
            GL::TEXTURE_MIN_FILTER,
            GL::LINEAR_MIPMAP_LINEAR as i32,
        );
        gl.tex_parameteri(GL::TEXTURE_2D, GL::TEXTURE_MAG_FILTER, GL::LINEAR as i32);
        gl.generate_mipmap(GL::TEXTURE_2D);

        textures.push(texture);
    }

    Ok((textures, texture_bytes))
}

fn create_default_white_texture(gl: &GL) -> Result<WebGlTexture, JsValue> {
    let texture = gl
        .create_texture()
        .ok_or_else(|| JsValue::from_str("Failed to create default texture"))?;
    gl.bind_texture(GL::TEXTURE_2D, Some(&texture));

    let pixels = [255_u8, 255_u8, 255_u8, 255_u8];
    gl.tex_image_2d_with_i32_and_i32_and_i32_and_format_and_type_and_opt_u8_array(
        GL::TEXTURE_2D,
        0,
        GL::RGBA as i32,
        1,
        1,
        0,
        GL::RGBA,
        GL::UNSIGNED_BYTE,
        Some(&pixels),
    )?;

    gl.tex_parameteri(GL::TEXTURE_2D, GL::TEXTURE_WRAP_S, GL::CLAMP_TO_EDGE as i32);
    gl.tex_parameteri(GL::TEXTURE_2D, GL::TEXTURE_WRAP_T, GL::CLAMP_TO_EDGE as i32);
    gl.tex_parameteri(GL::TEXTURE_2D, GL::TEXTURE_MIN_FILTER, GL::LINEAR as i32);
    gl.tex_parameteri(GL::TEXTURE_2D, GL::TEXTURE_MAG_FILTER, GL::LINEAR as i32);

    Ok(texture)
}

fn image_to_rgba8(image: &gltf::image::Data) -> Vec<u8> {
    match image.format {
        ImageFormat::R8G8B8A8 => image.pixels.clone(),
        ImageFormat::R8G8B8 => {
            let mut out = Vec::with_capacity((image.width * image.height * 4) as usize);
            for rgb in image.pixels.chunks_exact(3) {
                out.extend_from_slice(&[rgb[0], rgb[1], rgb[2], 255]);
            }
            out
        }
        ImageFormat::R8G8 => {
            let mut out = Vec::with_capacity((image.width * image.height * 4) as usize);
            for rg in image.pixels.chunks_exact(2) {
                out.extend_from_slice(&[rg[0], rg[1], 0, 255]);
            }
            out
        }
        ImageFormat::R8 => {
            let mut out = Vec::with_capacity((image.width * image.height * 4) as usize);
            for r in &image.pixels {
                out.extend_from_slice(&[*r, *r, *r, 255]);
            }
            out
        }
        _ => {
            // Fallback for uncommon formats (e.g. 16-bit channels): keep model visible.
            let pixel_count = (image.width * image.height) as usize;
            let mut out = Vec::with_capacity(pixel_count * 4);
            for _ in 0..pixel_count {
                out.extend_from_slice(&[255, 255, 255, 255]);
            }
            out
        }
    }
}

fn is_probably_json_gltf(bytes: &[u8]) -> bool {
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b' ' | b'\n' | b'\r' | b'\t' => i += 1,
            b'{' | b'[' => return true,
            _ => return false,
        }
    }
    false
}

fn sanitize_gltf_json_null_numbers(bytes: &[u8]) -> Result<Vec<u8>, serde_json::Error> {
    let mut json: Value = serde_json::from_slice(bytes)?;
    sanitize_json_value(&mut json, None);
    serde_json::to_vec(&json)
}

fn sanitize_json_value(value: &mut Value, parent_key: Option<&str>) {
    const NUMERIC_SCALAR_KEYS: &[&str] = &[
        "metallicFactor",
        "roughnessFactor",
        "scale",
        "strength",
        "alphaCutoff",
    ];

    match value {
        Value::Object(map) => {
            for (key, child) in map.iter_mut() {
                if child.is_null() && NUMERIC_SCALAR_KEYS.contains(&key.as_str()) {
                    *child = Value::from(0.0_f64);
                    continue;
                }
                sanitize_json_value(child, Some(key));
            }
        }
        Value::Array(array) => {
            let numeric_array_context = parent_key.is_some_and(|k| {
                matches!(
                    k,
                    "translation"
                        | "rotation"
                        | "scale"
                        | "matrix"
                        | "weights"
                        | "min"
                        | "max"
                        | "baseColorFactor"
                        | "emissiveFactor"
                        | "attenuationColor"
                )
            });

            let contains_numbers = array.iter().any(|v| matches!(v, Value::Number(_)));
            if numeric_array_context || contains_numbers {
                for child in array.iter_mut() {
                    if child.is_null() {
                        *child = Value::from(0.0_f64);
                    } else {
                        sanitize_json_value(child, parent_key);
                    }
                }
            } else {
                for child in array.iter_mut() {
                    sanitize_json_value(child, parent_key);
                }
            }
        }
        _ => {}
    }
}

fn compute_flat_normals(positions: &[[f32; 3]], indices: &[u32], normals: &mut [[f32; 3]]) {
    for tri in indices.chunks_exact(3) {
        let i0 = tri[0] as usize;
        let i1 = tri[1] as usize;
        let i2 = tri[2] as usize;

        let p0 = positions[i0];
        let p1 = positions[i1];
        let p2 = positions[i2];

        let e1 = [p1[0] - p0[0], p1[1] - p0[1], p1[2] - p0[2]];
        let e2 = [p2[0] - p0[0], p2[1] - p0[1], p2[2] - p0[2]];
        let n = normalize(cross(e1, e2));

        for &idx in tri {
            let v = &mut normals[idx as usize];
            v[0] += n[0];
            v[1] += n[1];
            v[2] += n[2];
        }
    }

    for n in normals.iter_mut() {
        *n = normalize(*n);
    }
}

fn compute_bounds(positions: &[[f32; 3]]) -> ([f32; 3], f32) {
    let mut min = [f32::INFINITY; 3];
    let mut max = [f32::NEG_INFINITY; 3];

    for p in positions {
        for i in 0..3 {
            min[i] = min[i].min(p[i]);
            max[i] = max[i].max(p[i]);
        }
    }

    let center = [
        (min[0] + max[0]) * 0.5,
        (min[1] + max[1]) * 0.5,
        (min[2] + max[2]) * 0.5,
    ];

    let mut radius = 0.001_f32;
    for p in positions {
        let dx = p[0] - center[0];
        let dy = p[1] - center[1];
        let dz = p[2] - center[2];
        radius = radius.max((dx * dx + dy * dy + dz * dz).sqrt());
    }

    (center, radius)
}

fn compute_bounds_ext(positions: &[[f32; 3]]) -> ([f32; 3], [f32; 3], [f32; 3], f32) {
    let mut min = [f32::INFINITY; 3];
    let mut max = [f32::NEG_INFINITY; 3];

    for p in positions {
        for i in 0..3 {
            min[i] = min[i].min(p[i]);
            max[i] = max[i].max(p[i]);
        }
    }

    let center = [
        (min[0] + max[0]) * 0.5,
        (min[1] + max[1]) * 0.5,
        (min[2] + max[2]) * 0.5,
    ];

    let mut radius = 0.001_f32;
    for p in positions {
        let dx = p[0] - center[0];
        let dy = p[1] - center[1];
        let dz = p[2] - center[2];
        radius = radius.max((dx * dx + dy * dy + dz * dz).sqrt());
    }

    (min, max, center, radius)
}

fn get_uniform(gl: &GL, program: &WebGlProgram, name: &str) -> Result<WebGlUniformLocation, JsValue> {
    gl.get_uniform_location(program, name)
        .ok_or_else(|| JsValue::from_str(&format!("Uniform {name} not found")))
}

fn create_shader(gl: &GL, shader_type: u32, source: &str) -> Result<WebGlShader, JsValue> {
    let shader = gl
        .create_shader(shader_type)
        .ok_or_else(|| JsValue::from_str("Failed to create shader"))?;

    gl.shader_source(&shader, source);
    gl.compile_shader(&shader);

    let compiled = gl
        .get_shader_parameter(&shader, GL::COMPILE_STATUS)
        .as_bool()
        .unwrap_or(false);

    if compiled {
        Ok(shader)
    } else {
        Err(JsValue::from_str(
            &gl.get_shader_info_log(&shader)
                .unwrap_or_else(|| "Unknown shader compile error".to_string()),
        ))
    }
}

fn create_program(gl: &GL, vert: &str, frag: &str) -> Result<WebGlProgram, JsValue> {
    let program = gl
        .create_program()
        .ok_or_else(|| JsValue::from_str("Failed to create program"))?;

    let vert_shader = create_shader(gl, GL::VERTEX_SHADER, vert)?;
    let frag_shader = create_shader(gl, GL::FRAGMENT_SHADER, frag)?;

    gl.attach_shader(&program, &vert_shader);
    gl.attach_shader(&program, &frag_shader);
    gl.link_program(&program);

    let linked = gl
        .get_program_parameter(&program, GL::LINK_STATUS)
        .as_bool()
        .unwrap_or(false);

    if linked {
        Ok(program)
    } else {
        Err(JsValue::from_str(
            &gl.get_program_info_log(&program)
                .unwrap_or_else(|| "Unknown program link error".to_string()),
        ))
    }
}

fn mat4_from_gltf(m: [[f32; 4]; 4]) -> [f32; 16] {
    [
        m[0][0], m[1][0], m[2][0], m[3][0], m[0][1], m[1][1], m[2][1], m[3][1], m[0][2],
        m[1][2], m[2][2], m[3][2], m[0][3], m[1][3], m[2][3], m[3][3],
    ]
}

fn mat4_mul(a: &[f32; 16], b: &[f32; 16]) -> [f32; 16] {
    let mut out = [0.0_f32; 16];

    for col in 0..4 {
        for row in 0..4 {
            out[col * 4 + row] = a[row] * b[col * 4]
                + a[4 + row] * b[col * 4 + 1]
                + a[8 + row] * b[col * 4 + 2]
                + a[12 + row] * b[col * 4 + 3];
        }
    }

    out
}

fn transform_point(m: &[f32; 16], p: [f32; 3]) -> [f32; 3] {
    [
        m[0] * p[0] + m[4] * p[1] + m[8] * p[2] + m[12],
        m[1] * p[0] + m[5] * p[1] + m[9] * p[2] + m[13],
        m[2] * p[0] + m[6] * p[1] + m[10] * p[2] + m[14],
    ]
}

fn transform_vector(m: &[f32; 16], v: [f32; 3]) -> [f32; 3] {
    [
        m[0] * v[0] + m[4] * v[1] + m[8] * v[2],
        m[1] * v[0] + m[5] * v[1] + m[9] * v[2],
        m[2] * v[0] + m[6] * v[1] + m[10] * v[2],
    ]
}

fn mat4_perspective(fovy: f32, aspect: f32, near: f32, far: f32) -> [f32; 16] {
    let f = 1.0 / (fovy * 0.5).tan();
    let nf = 1.0 / (near - far);

    [
        f / aspect,
        0.0,
        0.0,
        0.0,
        0.0,
        f,
        0.0,
        0.0,
        0.0,
        0.0,
        (far + near) * nf,
        -1.0,
        0.0,
        0.0,
        (2.0 * far * near) * nf,
        0.0,
    ]
}

fn mat4_look_at(eye: [f32; 3], center: [f32; 3], up: [f32; 3]) -> [f32; 16] {
    let f = normalize([
        center[0] - eye[0],
        center[1] - eye[1],
        center[2] - eye[2],
    ]);
    let s = normalize(cross(f, up));
    let u = cross(s, f);

    [
        s[0],
        u[0],
        -f[0],
        0.0,
        s[1],
        u[1],
        -f[1],
        0.0,
        s[2],
        u[2],
        -f[2],
        0.0,
        -dot(s, eye),
        -dot(u, eye),
        dot(f, eye),
        1.0,
    ]
}

fn add3(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

fn scale3(v: [f32; 3], s: f32) -> [f32; 3] {
    [v[0] * s, v[1] * s, v[2] * s]
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn normalize(v: [f32; 3]) -> [f32; 3] {
    let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    if len < 1e-8 {
        [0.0, 1.0, 0.0]
    } else {
        [v[0] / len, v[1] / len, v[2] / len]
    }
}

fn ray_aabb_hit(origin: [f32; 3], dir: [f32; 3], min: [f32; 3], max: [f32; 3]) -> Option<f32> {
    let mut tmin = -f32::INFINITY;
    let mut tmax = f32::INFINITY;

    for axis in 0..3 {
        let o = origin[axis];
        let d = dir[axis];

        if d.abs() < 1e-7 {
            if o < min[axis] || o > max[axis] {
                return None;
            }
            continue;
        }

        let inv = 1.0 / d;
        let mut t0 = (min[axis] - o) * inv;
        let mut t1 = (max[axis] - o) * inv;
        if t0 > t1 {
            std::mem::swap(&mut t0, &mut t1);
        }

        tmin = tmin.max(t0);
        tmax = tmax.min(t1);
        if tmax < tmin {
            return None;
        }
    }

    if tmax < 0.0 {
        None
    } else {
        Some(if tmin >= 0.0 { tmin } else { tmax })
    }
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

fn lerp3(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    [lerp(a[0], b[0], t), lerp(a[1], b[1], t), lerp(a[2], b[2], t)]
}

const VERT_SHADER_SOURCE: &str = r#"#version 300 es
precision highp float;
layout(location = 0) in vec3 a_position;
layout(location = 1) in vec3 a_normal;
layout(location = 2) in vec2 a_uv;

uniform mat4 u_view;
uniform mat4 u_proj;

out vec3 v_world_pos;
out vec3 v_normal;
out vec2 v_uv;

void main() {
  v_world_pos = a_position;
  v_normal = normalize(a_normal);
  v_uv = a_uv;
  gl_Position = u_proj * u_view * vec4(a_position, 1.0);
}
"#;

const FRAG_SHADER_SOURCE: &str = r#"#version 300 es
precision highp float;

in vec3 v_world_pos;
in vec3 v_normal;
in vec2 v_uv;
out vec4 out_color;

uniform vec3 u_camera_pos;
uniform vec4 u_base_color_factor;
uniform float u_metallic_factor;
uniform float u_roughness_factor;
uniform int u_has_base_color_tex;
uniform sampler2D u_base_color_tex;
uniform int u_highlight;

const float PI = 3.14159265359;

vec3 fresnel_schlick(float cosTheta, vec3 F0) {
  return F0 + (1.0 - F0) * pow(1.0 - cosTheta, 5.0);
}

float distribution_ggx(vec3 N, vec3 H, float roughness) {
  float a = roughness * roughness;
  float a2 = a * a;
  float NdotH = max(dot(N, H), 0.0);
  float NdotH2 = NdotH * NdotH;

  float num = a2;
  float denom = (NdotH2 * (a2 - 1.0) + 1.0);
  denom = PI * denom * denom;
  return num / max(denom, 0.000001);
}

float geometry_schlick_ggx(float NdotV, float roughness) {
  float r = roughness + 1.0;
  float k = (r * r) / 8.0;
  float num = NdotV;
  float denom = NdotV * (1.0 - k) + k;
  return num / max(denom, 0.000001);
}

float geometry_smith(vec3 N, vec3 V, vec3 L, float roughness) {
  float NdotV = max(dot(N, V), 0.0);
  float NdotL = max(dot(N, L), 0.0);
  float ggx2 = geometry_schlick_ggx(NdotV, roughness);
  float ggx1 = geometry_schlick_ggx(NdotL, roughness);
  return ggx1 * ggx2;
}

void main() {
    if (u_highlight == 1) {
        out_color = vec4(0.10, 0.36, 1.00, 1.0);
        return;
    }

  vec4 baseColor = u_base_color_factor;
  if (u_has_base_color_tex == 1) {
    baseColor *= texture(u_base_color_tex, v_uv);
  }

  vec3 albedo = pow(baseColor.rgb, vec3(2.2));
  float metallic = clamp(u_metallic_factor, 0.0, 1.0);
  float roughness = clamp(u_roughness_factor, 0.04, 1.0);

  vec3 N = normalize(v_normal);
  vec3 V = normalize(u_camera_pos - v_world_pos);
    vec3 L = normalize(vec3(0.45, 1.0, 0.35));
  vec3 H = normalize(V + L);

  vec3 F0 = mix(vec3(0.04), albedo, metallic);
  vec3 F = fresnel_schlick(max(dot(H, V), 0.0), F0);
  float D = distribution_ggx(N, H, roughness);
  float G = geometry_smith(N, V, L, roughness);

  vec3 numerator = D * G * F;
  float denominator = 4.0 * max(dot(N, V), 0.0) * max(dot(N, L), 0.0) + 0.0001;
  vec3 specular = numerator / denominator;

  vec3 kS = F;
  vec3 kD = (vec3(1.0) - kS) * (1.0 - metallic);

  float NdotL = max(dot(N, L), 0.0);
    vec3 radiance = vec3(6.2);
  vec3 Lo = (kD * albedo / PI + specular) * radiance * NdotL;

    // Approximate GI: sky/ground hemispherical ambient + soft bounce fill.
    float hemi = N.y * 0.5 + 0.5;
    vec3 sky = vec3(0.34, 0.42, 0.56);
    vec3 ground = vec3(0.18, 0.17, 0.15);
    vec3 hemiLight = mix(ground, sky, hemi);
    vec3 ambient = hemiLight * albedo * 0.42;

    float bounce = pow(max(dot(N, normalize(V + vec3(0.0, 1.0, 0.0))), 0.0), 1.5);
    vec3 indirect = albedo * (0.16 + 0.24 * bounce) * (1.0 - metallic * 0.7);

    vec3 color = ambient + indirect + Lo;
  color = color / (color + vec3(1.0));
  color = pow(color, vec3(1.0 / 2.2));

  out_color = vec4(color, baseColor.a);
}
"#;
