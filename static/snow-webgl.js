// WebGL fragment shader snow — variant 10
// Self-contained SnowGL class with start/stop/resize/setDark API.
// Fragment shader: tiled grid at 3 scales with procedural snowflakes.
// O(1) cost per pixel regardless of flake count.
"use strict";

(function () {
  var VERT_SRC = [
    "attribute vec2 a_pos;",
    "void main() { gl_Position = vec4(a_pos, 0.0, 1.0); }"
  ].join("\n");

  var FRAG_SRC = [
    "precision highp float;",
    "uniform vec2 u_res;",
    "uniform float u_time;",
    "uniform float u_wind;",
    "uniform float u_dark;",
    "",
    "// Hash — fast pseudo-random per cell",
    "float hash(vec2 p) {",
    "  return fract(sin(dot(p, vec2(127.1, 311.7))) * 43758.5453);",
    "}",
    "",
    "// Single layer of snow",
    "// grid: tile size, speed: fall speed multiplier, sz: flake radius range",
    "float snowLayer(vec2 uv, float grid, float speed, float minSz, float maxSz,",
    "                float minA, float maxA, float windF, float t) {",
    "  float total = 0.0;",
    "  // Vertical fall",
    "  float fall = t * speed;",
    "  // Wind drift",
    "  float drift = u_wind * windF * t * 0.02;",
    "  vec2 offset = vec2(drift, fall);",
    "  vec2 st = (uv + offset) / grid;",
    "  vec2 cell = floor(st);",
    "  vec2 f = fract(st);",
    "",
    "  // Check 3x3 neighborhood for overlapping flakes",
    "  for (int j = -1; j <= 1; j++) {",
    "    for (int i = -1; i <= 1; i++) {",
    "      vec2 nb = vec2(float(i), float(j));",
    "      vec2 id = cell + nb;",
    "      float h = hash(id);",
    "      float h2 = hash(id + vec2(97.0, 31.0));",
    "      float h3 = hash(id + vec2(13.0, 71.0));",
    "",
    "      // Randomized position within cell",
    "      vec2 pos = vec2(h, h2) * 0.6 + 0.2;",
    "      // Compound sinusoidal drift for organic movement",
    "      float driftSin = sin(t * (0.4 + h * 0.3) + h * 6.28) * 0.15",
    "                     + sin(t * (0.25 + h2 * 0.2) + h2 * 6.28) * 0.08;",
    "      pos.x += driftSin;",
    "",
    "      vec2 diff = f - nb - pos;",
    "      float d = length(diff);",
    "",
    "      // Flake radius",
    "      float r = (minSz + h3 * (maxSz - minSz)) / grid;",
    "      // Soft radial falloff",
    "      float flake = smoothstep(r, r * 0.2, d);",
    "",
    "      // Per-flake opacity",
    "      float alpha = minA + h * (maxA - minA);",
    "      // Sparkle oscillation",
    "      alpha += sin(t * (2.0 + h2 * 2.0) + h3 * 6.28) * 0.08;",
    "      alpha = clamp(alpha, 0.0, 1.0);",
    "",
    "      total += flake * alpha;",
    "    }",
    "  }",
    "  return total;",
    "}",
    "",
    "void main() {",
    "  vec2 uv = gl_FragCoord.xy;",
    "  uv.y = u_res.y - uv.y; // flip Y so snow falls downward",
    "",
    "  float t = u_time;",
    "",
    "  // Three depth layers — different grid sizes, speeds, flake sizes",
    "  float far  = snowLayer(uv, 120.0, 15.0, 1.0, 2.0, 0.12, 0.30, 0.3, t);",
    "  float mid  = snowLayer(uv, 90.0,  22.0, 1.5, 2.8, 0.25, 0.50, 0.6, t);",
    "  float near = snowLayer(uv, 65.0,  32.0, 2.0, 3.8, 0.40, 0.80, 1.0, t);",
    "",
    "  float snow = far + mid + near;",
    "",
    "  // Edge fade — smooth entry at top, smooth exit at bottom",
    "  float fadeTop = smoothstep(0.0, u_res.y * 0.05, u_res.y - uv.y);",
    "  float fadeBot = smoothstep(0.0, u_res.y * 0.03, uv.y);",
    "  snow *= fadeTop * fadeBot;",
    "",
    "  // Flake color: white on dark, cool blue-gray on light",
    "  vec3 col = mix(vec3(0.71, 0.76, 0.82), vec3(1.0), u_dark);",
    "",
    "  gl_FragColor = vec4(col, snow);",
    "}"
  ].join("\n");

  function compileShader(gl, type, src) {
    var s = gl.createShader(type);
    gl.shaderSource(s, src);
    gl.compileShader(s);
    if (!gl.getShaderParameter(s, gl.COMPILE_STATUS)) {
      console.error("SnowGL shader error:", gl.getShaderInfoLog(s));
      gl.deleteShader(s);
      return null;
    }
    return s;
  }

  function SnowGL(canvas, opts) {
    opts = opts || {};
    this.canvas = canvas;
    this.dark = opts.dark !== undefined ? opts.dark : true;
    this.wind = opts.wind || 0;
    this.raf = 0;
    this.startTime = 0;

    var gl = canvas.getContext("webgl2", { alpha: true, premultipliedAlpha: false });
    if (!gl) {
      console.warn("SnowGL: WebGL2 not available");
      this.gl = null;
      return;
    }
    this.gl = gl;

    // Compile shaders
    var vs = compileShader(gl, gl.VERTEX_SHADER, VERT_SRC);
    var fs = compileShader(gl, gl.FRAGMENT_SHADER, FRAG_SRC);
    if (!vs || !fs) { this.gl = null; return; }

    var prog = gl.createProgram();
    gl.attachShader(prog, vs);
    gl.attachShader(prog, fs);
    gl.linkProgram(prog);
    if (!gl.getProgramParameter(prog, gl.LINK_STATUS)) {
      console.error("SnowGL link error:", gl.getProgramInfoLog(prog));
      this.gl = null;
      return;
    }
    this.prog = prog;

    // Fullscreen quad
    var buf = gl.createBuffer();
    gl.bindBuffer(gl.ARRAY_BUFFER, buf);
    gl.bufferData(gl.ARRAY_BUFFER, new Float32Array([-1, -1, 1, -1, -1, 1, 1, 1]), gl.STATIC_DRAW);
    var aPos = gl.getAttribLocation(prog, "a_pos");
    gl.enableVertexAttribArray(aPos);
    gl.vertexAttribPointer(aPos, 2, gl.FLOAT, false, 0, 0);

    // Uniforms
    gl.useProgram(prog);
    this.uRes = gl.getUniformLocation(prog, "u_res");
    this.uTime = gl.getUniformLocation(prog, "u_time");
    this.uWind = gl.getUniformLocation(prog, "u_wind");
    this.uDark = gl.getUniformLocation(prog, "u_dark");

    // Blending for transparent overlay
    gl.enable(gl.BLEND);
    gl.blendFunc(gl.SRC_ALPHA, gl.ONE_MINUS_SRC_ALPHA);

    this._updateUniforms();
  }

  SnowGL.prototype._updateUniforms = function () {
    var gl = this.gl;
    if (!gl) return;
    gl.useProgram(this.prog);
    gl.uniform2f(this.uRes, this.canvas.width, this.canvas.height);
    gl.uniform1f(this.uWind, this.wind);
    gl.uniform1f(this.uDark, this.dark ? 1.0 : 0.0);
  };

  SnowGL.prototype.start = function () {
    if (!this.gl || this.raf) return;
    this.startTime = performance.now();
    var self = this;
    var gl = this.gl;
    gl.viewport(0, 0, this.canvas.width, this.canvas.height);

    function frame(now) {
      self.raf = requestAnimationFrame(frame);
      var t = (now - self.startTime) / 1000;
      gl.uniform1f(self.uTime, t);
      gl.clearColor(0, 0, 0, 0);
      gl.clear(gl.COLOR_BUFFER_BIT);
      gl.drawArrays(gl.TRIANGLE_STRIP, 0, 4);
    }
    this.raf = requestAnimationFrame(frame);
  };

  SnowGL.prototype.stop = function () {
    if (this.raf) {
      cancelAnimationFrame(this.raf);
      this.raf = 0;
    }
  };

  SnowGL.prototype.resize = function (w, h) {
    if (!this.gl) return;
    this.gl.viewport(0, 0, w, h);
    this.gl.useProgram(this.prog);
    this.gl.uniform2f(this.uRes, w, h);
  };

  SnowGL.prototype.setDark = function (dark) {
    this.dark = dark;
    if (!this.gl) return;
    this.gl.useProgram(this.prog);
    this.gl.uniform1f(this.uDark, dark ? 1.0 : 0.0);
  };

  SnowGL.prototype.setWind = function (speed) {
    this.wind = speed;
    if (!this.gl) return;
    this.gl.useProgram(this.prog);
    this.gl.uniform1f(this.uWind, speed);
  };

  window.SnowGL = SnowGL;
})();
