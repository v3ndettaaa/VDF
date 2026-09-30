/**
 * WebGL2 tile compositor: owns the document viewport hot path.
 *
 * Draws the page layout as textured quads (one texture per tile, raw RGBA
 * from vdf-tile://). No DOM in the render path (MASTER_PLAN.md §5).
 *
 * No-blanking invariant (§9): textures persist across zoom changes; when a
 * slot has no tile at the current step, the nearest lower-step tile is drawn
 * scaled into its place — the viewport never intentionally blanks.
 */

import type { LayoutJson } from "../app/ipc";
import type { TilePixels } from "./compositor-helpers";

const VERT = `#version 300 es
in vec2 a_pos;
in vec2 a_uv;
uniform vec2 u_resolution;
uniform vec4 u_rect;
out vec2 v_uv;
void main() {
  float x0 = (u_rect.x / u_resolution.x) * 2.0 - 1.0;
  float x1 = ((u_rect.x + u_rect.z) / u_resolution.x) * 2.0 - 1.0;
  float y0 = 1.0 - (u_rect.y / u_resolution.y) * 2.0;
  float y1 = 1.0 - ((u_rect.y + u_rect.w) / u_resolution.y) * 2.0;
  v_uv = a_uv;
  gl_Position = vec4(mix(x0, x1, a_pos.x), mix(y0, y1, a_pos.y), 0.0, 1.0);
}`;

const FRAG = `#version 300 es
precision mediump float;
in vec2 v_uv;
uniform sampler2D u_tex;
out vec4 out_color;
void main() {
  out_color = texture(u_tex, v_uv);
}`;

interface GpuTile {
  tex: WebGLTexture;
  width: number;
  height: number;
  /** zoom step the tile was rendered at */
  zoomStep: number;
  lastUsed: number;
}

const MAX_GPU_TILES = 512;

export class Compositor {
  private gl: WebGL2RenderingContext;
  private prog: WebGLProgram;
  private vao: WebGLVertexArrayObject;
  private uResolution: WebGLUniformLocation;
  private uRect: WebGLUniformLocation;
  private textures = new Map<string, GpuTile>();
  private frame = 0;

  constructor(private canvas: HTMLCanvasElement) {
    const gl = canvas.getContext("webgl2", {
      alpha: false,
      antialias: false,
      premultipliedAlpha: false,
      preserveDrawingBuffer: false,
    });
    if (!gl) throw new Error("WebGL2 unavailable");
    this.gl = gl;
    this.prog = this.buildProgram(VERT, FRAG);
    this.uResolution = this.loc("u_resolution");
    this.uRect = this.loc("u_rect");
    this.vao = this.buildQuadVao();
    gl.useProgram(this.prog);
  }

  private loc(name: string): WebGLUniformLocation {
    const l = this.gl.getUniformLocation(this.prog, name);
    if (!l) throw new Error(`missing uniform ${name}`);
    return l;
  }

  private compile(type: number, src: string): WebGLShader {
    const gl = this.gl;
    const sh = gl.createShader(type)!;
    gl.shaderSource(sh, src);
    gl.compileShader(sh);
    if (!gl.getShaderParameter(sh, gl.COMPILE_STATUS)) {
      throw new Error(`shader: ${gl.getShaderInfoLog(sh) ?? "?"}`);
    }
    return sh;
  }

  private buildProgram(vs: string, fs: string): WebGLProgram {
    const gl = this.gl;
    const prog = gl.createProgram()!;
    gl.attachShader(prog, this.compile(gl.VERTEX_SHADER, vs));
    gl.attachShader(prog, this.compile(gl.FRAGMENT_SHADER, fs));
    gl.linkProgram(prog);
    if (!gl.getProgramParameter(prog, gl.LINK_STATUS)) {
      throw new Error(`program: ${gl.getProgramInfoLog(prog) ?? "?"}`);
    }
    return prog;
  }

  private buildQuadVao(): WebGLVertexArrayObject {
    const gl = this.gl;
    const vao = gl.createVertexArray()!;
    gl.bindVertexArray(vao);
    const posBuf = gl.createBuffer()!;
    gl.bindBuffer(gl.ARRAY_BUFFER, posBuf);
    gl.bufferData(
      gl.ARRAY_BUFFER,
      new Float32Array([0, 0, 1, 0, 0, 1, 0, 1, 1, 0, 1, 1]),
      gl.STATIC_DRAW,
    );
    const aPos = gl.getAttribLocation(this.prog, "a_pos");
    gl.enableVertexAttribArray(aPos);
    gl.vertexAttribPointer(aPos, 2, gl.FLOAT, false, 0, 0);
    const uvBuf = gl.createBuffer()!;
    gl.bindBuffer(gl.ARRAY_BUFFER, uvBuf);
    gl.bufferData(
      gl.ARRAY_BUFFER,
      new Float32Array([0, 0, 1, 0, 0, 1, 0, 0, 1, 0, 1, 1]),
      gl.STATIC_DRAW,
    );
    const aUv = gl.getAttribLocation(this.prog, "a_uv");
    gl.enableVertexAttribArray(aUv);
    gl.vertexAttribPointer(aUv, 2, gl.FLOAT, false, 0, 0);
    return vao;
  }

  resize(cssW: number, cssH: number, dpr: number): void {
    const w = Math.max(1, Math.round(cssW * dpr));
    const h = Math.max(1, Math.round(cssH * dpr));
    if (this.canvas.width !== w || this.canvas.height !== h) {
      this.canvas.width = w;
      this.canvas.height = h;
    }
  }

  /** Uploads raw RGBA tile bytes under a key. */
  putTile(key: string, zoomStep: number, tile: TilePixels): void {
    const gl = this.gl;
    let entry = this.textures.get(key);
    if (!entry) {
      const tex = gl.createTexture()!;
      gl.bindTexture(gl.TEXTURE_2D, tex);
      gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.LINEAR);
      gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.LINEAR);
      gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE);
      gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE);
      gl.texImage2D(
        gl.TEXTURE_2D,
        0,
        gl.RGBA,
        tile.width,
        tile.height,
        0,
        gl.RGBA,
        gl.UNSIGNED_BYTE,
        tile.rgba,
      );
      entry = { tex, width: tile.width, height: tile.height, zoomStep, lastUsed: 0 };
      this.textures.set(key, entry);
      this.evictOld();
    } else {
      gl.bindTexture(gl.TEXTURE_2D, entry.tex);
      gl.texSubImage2D(
        gl.TEXTURE_2D,
        0,
        0,
        0,
        tile.width,
        tile.height,
        gl.RGBA,
        gl.UNSIGNED_BYTE,
        tile.rgba,
      );
      entry.width = tile.width;
      entry.height = tile.height;
    }
  }

  dropTile(key: string): void {
    const entry = this.textures.get(key);
    if (entry) {
      this.gl.deleteTexture(entry.tex);
      this.textures.delete(key);
    }
  }

  private evictOld(): void {
    if (this.textures.size <= MAX_GPU_TILES) return;
    const sorted = [...this.textures.entries()].sort((a, b) => a[1].lastUsed - b[1].lastUsed);
    for (const [key, entry] of sorted.slice(0, this.textures.size - MAX_GPU_TILES)) {
      this.gl.deleteTexture(entry.tex);
      this.textures.delete(key);
    }
  }

  hasTile(key: string): boolean {
    return this.textures.has(key);
  }

  /**
   * Draws one frame: page background rects, then tiles. `tiles` maps
   * page-local tile coords to GPU keys; `zoomScale` converts tile-space
   * pixels (rendered at the quantized step) into current view pixels.
   */
  draw(
    layout: LayoutJson | null,
    scroll: { x: number; y: number },
    tiles: Iterable<{ page: number; x: number; y: number; w: number; h: number; key: string; zoomScale: number }>,
  ): void {
    const gl = this.gl;
    this.frame++;
    gl.viewport(0, 0, this.canvas.width, this.canvas.height);
    gl.clearColor(0.05, 0.05, 0.06, 1);
    gl.clear(gl.COLOR_BUFFER_BIT);
    gl.useProgram(this.prog);
    gl.bindVertexArray(this.vao);
    gl.activeTexture(gl.TEXTURE0);
    gl.uniform2f(this.uResolution, this.canvas.width, this.canvas.height);
    gl.uniform4f(this.uRect, 0, 0, 0, 0);

    if (layout) {
      // page "paper" background
      gl.bindTexture(gl.TEXTURE_2D, this.paperTexture());
      for (const p of layout.pages) {
        this.drawQuad(p.x - scroll.x, p.y - scroll.y, p.w, p.h, 0, 0, 1, 1);
      }
      // tiles
      for (const t of tiles) {
        const entry = this.textures.get(t.key);
        if (!entry) continue;
        entry.lastUsed = this.frame;
        const page = layout.pages[t.page];
        if (!page) continue;
        gl.bindTexture(gl.TEXTURE_2D, entry.tex);
        const w = t.w * t.zoomScale;
        const h = t.h * t.zoomScale;
        this.drawQuad(
          page.x + t.x * t.zoomScale - scroll.x,
          page.y + t.y * t.zoomScale - scroll.y,
          w,
          h,
          0,
          0,
          1,
          1,
        );
      }
    }
  }

  private paper: WebGLTexture | null = null;
  private paperTexture(): WebGLTexture {
    if (this.paper) return this.paper;
    const gl = this.gl;
    const tex = gl.createTexture()!;
    gl.bindTexture(gl.TEXTURE_2D, tex);
    gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA, 1, 1, 0, gl.RGBA, gl.UNSIGNED_BYTE, new Uint8Array([255, 255, 255, 255]));
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.NEAREST);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.NEAREST);
    this.paper = tex;
    return tex;
  }

  private drawQuad(x: number, y: number, w: number, h: number, u0: number, v0: number, u1: number, v1: number): void {
    const gl = this.gl;
    gl.uniform4f(this.uRect, x, y, w, h);
    // uv attribute is static [0..1]; sub-rect uv not needed for full tiles
    void u0; void v0; void u1; void v1;
    gl.drawArrays(gl.TRIANGLES, 0, 6);
  }
}
