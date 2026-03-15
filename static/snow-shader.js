// Variant 11: 3D particle snow with ShaderProgram
// Based on https://codepen.io/bsehovac/pen/GPwXxq by bsehovac
// ShaderProgram library: https://github.com/bsehovac/shader-program
// Adapted as a self-contained overlay with start/stop/resize/setDark API.
"use strict";

(function () {

// --- ShaderProgram library (inlined, adapted for transparent overlay) ---

function ShaderProgram(holder, options) {

  options = Object.assign({
    antialias: false,
    depthTest: false,
    mousemove: false,
    autosize: false,
    msaa: 0,
    vertex: [
      "precision highp float;",
      "attribute vec4 a_position;",
      "attribute vec4 a_color;",
      "uniform float u_time;",
      "uniform vec2 u_resolution;",
      "uniform vec2 u_mousemove;",
      "uniform mat4 u_projection;",
      "varying vec4 v_color;",
      "void main() {",
      "  gl_Position = u_projection * a_position;",
      "  gl_PointSize = (10.0 / gl_Position.w) * 100.0;",
      "  v_color = a_color;",
      "}"
    ].join("\n"),
    fragment: [
      "precision highp float;",
      "uniform sampler2D u_texture;",
      "uniform int u_hasTexture;",
      "varying vec4 v_color;",
      "void main() {",
      "  if (u_hasTexture == 1) {",
      "    gl_FragColor = v_color * texture2D(u_texture, gl_PointCoord);",
      "  } else {",
      "    gl_FragColor = v_color;",
      "  }",
      "}"
    ].join("\n"),
    uniforms: {},
    buffers: {},
    camera: {},
    texture: null,
    onUpdate: function () {},
    onResize: function () {},
  }, options);

  var uniforms = Object.assign({
    time: { type: "float", value: 0 },
    hasTexture: { type: "int", value: 0 },
    resolution: { type: "vec2", value: [0, 0] },
    mousemove: { type: "vec2", value: [0, 0] },
    projection: { type: "mat4", value: [1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1] },
  }, options.uniforms);

  var buffers = Object.assign({
    position: { size: 3, data: [] },
    color: { size: 4, data: [] },
  }, options.buffers);

  var camera = Object.assign({
    fov: 60,
    near: 1,
    far: 10000,
    aspect: 1,
    z: 100,
    perspective: true,
  }, options.camera);

  var canvas = document.createElement("canvas");
  canvas.style.display = "block";
  var gl = canvas.getContext("webgl", { antialias: options.antialias, alpha: true, premultipliedAlpha: true });

  if (!gl) return false;

  this.count = 0;
  this.gl = gl;
  this.canvas = canvas;
  this.camera = camera;
  this.holder = holder;
  this.msaa = options.msaa;
  this.onUpdate = options.onUpdate;
  this.onResize = options.onResize;
  this.data = {};
  this._stopped = false;

  holder.appendChild(canvas);

  this.createProgram(options.vertex, options.fragment);

  this.createBuffers(buffers);
  this.createUniforms(uniforms);

  this.updateBuffers();
  this.updateUniforms();

  this.createTexture(options.texture);

  gl.enable(gl.BLEND);
  gl.enable(gl.CULL_FACE);
  // Premultiplied alpha: correct for compositing transparent WebGL over HTML
  gl.blendFunc(gl.ONE, gl.ONE_MINUS_SRC_ALPHA);
  gl[options.depthTest ? "enable" : "disable"](gl.DEPTH_TEST);

  this.resize();

  this.update = this.update.bind(this);
  this.time = { start: performance.now(), old: performance.now() };
  this.update();
}

ShaderProgram.prototype.resize = function () {
  var holder = this.holder;
  var canvas = this.canvas;
  var gl = this.gl;

  var width = this.width = holder.offsetWidth;
  var height = this.height = holder.offsetHeight;
  var aspect = this.aspect = width / height;
  var dpi = this.dpi = Math.max(this.msaa ? 2 : 1, devicePixelRatio);

  canvas.width = width * dpi;
  canvas.height = height * dpi;
  canvas.style.width = width + "px";
  canvas.style.height = height + "px";

  gl.viewport(0, 0, width * dpi, height * dpi);
  gl.clearColor(0, 0, 0, 0);

  this.uniforms.resolution = [width, height];
  this.uniforms.projection = this.setProjection(aspect);

  this.onResize(width, height, dpi);
};

ShaderProgram.prototype.setProjection = function (aspect) {
  var camera = this.camera;
  if (camera.perspective) {
    camera.aspect = aspect;
    var fovRad = camera.fov * (Math.PI / 180);
    var f = Math.tan(Math.PI * 0.5 - 0.5 * fovRad);
    var rangeInv = 1.0 / (camera.near - camera.far);
    var matrix = [
      f / camera.aspect, 0, 0, 0,
      0, f, 0, 0,
      0, 0, (camera.near + camera.far) * rangeInv, -1,
      0, 0, camera.near * camera.far * rangeInv * 2, 0
    ];
    matrix[14] += camera.z;
    matrix[15] += camera.z;
    return matrix;
  } else {
    return [
      2 / this.width, 0, 0, 0,
      0, -2 / this.height, 0, 0,
      0, 0, 1, 0,
      -1, 1, 0, 1,
    ];
  }
};

ShaderProgram.prototype.createShader = function (type, source) {
  var gl = this.gl;
  var shader = gl.createShader(type);
  gl.shaderSource(shader, source);
  gl.compileShader(shader);
  if (gl.getShaderParameter(shader, gl.COMPILE_STATUS)) {
    return shader;
  } else {
    console.error("ShaderProgram shader error:", gl.getShaderInfoLog(shader));
    gl.deleteShader(shader);
  }
};

ShaderProgram.prototype.createProgram = function (vertex, fragment) {
  var gl = this.gl;
  var vertexShader = this.createShader(gl.VERTEX_SHADER, vertex);
  var fragmentShader = this.createShader(gl.FRAGMENT_SHADER, fragment);
  var program = gl.createProgram();
  gl.attachShader(program, vertexShader);
  gl.attachShader(program, fragmentShader);
  gl.linkProgram(program);
  if (gl.getProgramParameter(program, gl.LINK_STATUS)) {
    gl.useProgram(program);
    this.program = program;
  } else {
    console.error("ShaderProgram link error:", gl.getProgramInfoLog(program));
    gl.deleteProgram(program);
  }
};

ShaderProgram.prototype.createUniforms = function (data) {
  var gl = this.gl;
  var uniforms = this.data.uniforms = data;
  var values = this.uniforms = {};
  var self = this;

  Object.keys(uniforms).forEach(function (name) {
    var uniform = uniforms[name];
    uniform.location = gl.getUniformLocation(self.program, "u_" + name);
    Object.defineProperty(values, name, {
      set: function (value) {
        uniforms[name].value = value;
        self.setUniform(name, value);
      },
      get: function () { return uniforms[name].value; }
    });
  });
};

ShaderProgram.prototype.setUniform = function (name, value) {
  var gl = this.gl;
  var uniform = this.data.uniforms[name];
  uniform.value = value;
  switch (uniform.type) {
    case "int":   gl.uniform1i(uniform.location, value); break;
    case "float": gl.uniform1f(uniform.location, value); break;
    case "vec2":  gl.uniform2f(uniform.location, value[0], value[1]); break;
    case "vec3":  gl.uniform3f(uniform.location, value[0], value[1], value[2]); break;
    case "vec4":  gl.uniform4f(uniform.location, value[0], value[1], value[2], value[3]); break;
    case "mat2":  gl.uniformMatrix2fv(uniform.location, false, value); break;
    case "mat3":  gl.uniformMatrix3fv(uniform.location, false, value); break;
    case "mat4":  gl.uniformMatrix4fv(uniform.location, false, value); break;
  }
};

ShaderProgram.prototype.updateUniforms = function () {
  var uniforms = this.data.uniforms;
  var self = this;
  Object.keys(uniforms).forEach(function (name) {
    self.uniforms[name] = uniforms[name].value;
  });
};

ShaderProgram.prototype.createBuffers = function (data) {
  var gl = this.gl;
  var buffers = this.data.buffers = data;
  var values = this.buffers = {};
  var self = this;

  Object.keys(buffers).forEach(function (name) {
    var buffer = buffers[name];
    buffer.buffer = self.createBuffer("a_" + name, buffer.size);
    Object.defineProperty(values, name, {
      set: function (data) {
        buffers[name].data = data;
        self.setBuffer(name, data);
        if (name === "position") self.count = buffers.position.data.length / 3;
      },
      get: function () { return buffers[name].data; }
    });
  });
};

ShaderProgram.prototype.createBuffer = function (name, size) {
  var gl = this.gl;
  var program = this.program;
  var index = gl.getAttribLocation(program, name);
  var buffer = gl.createBuffer();
  gl.bindBuffer(gl.ARRAY_BUFFER, buffer);
  if (index >= 0) {
    gl.enableVertexAttribArray(index);
    gl.vertexAttribPointer(index, size, gl.FLOAT, false, 0, 0);
  }
  return buffer;
};

ShaderProgram.prototype.setBuffer = function (name, data) {
  var gl = this.gl;
  var buffers = this.data.buffers;
  if (name == null) { gl.bindBuffer(gl.ARRAY_BUFFER, null); return; }
  gl.bindBuffer(gl.ARRAY_BUFFER, buffers[name].buffer);
  gl.bufferData(gl.ARRAY_BUFFER, new Float32Array(data), gl.STATIC_DRAW);
};

ShaderProgram.prototype.updateBuffers = function () {
  this.setBuffer(null);
};

ShaderProgram.prototype.createTexture = function (src) {
  var gl = this.gl;
  var texture = gl.createTexture();
  gl.activeTexture(gl.TEXTURE0);
  gl.bindTexture(gl.TEXTURE_2D, texture);

  if (typeof src === "string") {
    // Data URI or URL — load via Image with canvas workaround for Safari
    gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA, 1, 1, 0, gl.RGBA, gl.UNSIGNED_BYTE, new Uint8Array([255, 255, 255, 255]));
    this.texture = texture;
    this.uniforms.hasTexture = 1;
    var self = this;
    var img = new Image();
    img.onload = function () {
      var c = document.createElement("canvas");
      c.width = img.width;
      c.height = img.height;
      c.getContext("2d").drawImage(img, 0, 0);
      gl.bindTexture(gl.TEXTURE_2D, texture);
      gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA, gl.RGBA, gl.UNSIGNED_BYTE, c);
      gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.LINEAR);
      gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.LINEAR);
      gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE);
      gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE);
    };
    img.src = src;
  } else if (src instanceof Uint8Array) {
    // Raw RGBA pixel data (assumed 64x64)
    var size = Math.sqrt(src.length / 4);
    gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA, size, size, 0, gl.RGBA, gl.UNSIGNED_BYTE, src);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.LINEAR);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.LINEAR);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE);
    this.texture = texture;
    this.uniforms.hasTexture = 1;
  } else {
    gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA, 1, 1, 0, gl.RGBA, gl.UNSIGNED_BYTE, new Uint8Array([255, 255, 255, 255]));
    this.texture = texture;
  }
};

ShaderProgram.prototype.update = function () {
  if (this._stopped) return;

  var gl = this.gl;
  var now = performance.now();
  var elapsed = (now - this.time.start) / 5000;
  var delta = now - this.time.old;
  this.time.old = now;

  this.uniforms.time = elapsed;

  if (this.count > 0) {
    gl.clear(gl.COLOR_BUFFER_BIT);
    gl.drawArrays(gl.POINTS, 0, this.count);
  }

  this.onUpdate(delta);

  requestAnimationFrame(this.update);
};

ShaderProgram.prototype.stop = function () {
  this._stopped = true;
};

// --- Snowfall effect ---

// Generate a soft radial snowflake texture procedurally (64x64 RGBA).
// The original bsehovac PNG had max alpha of ~4/255 — designed for opaque
// backgrounds, not transparent overlays. This procedural texture has proper
// alpha for compositing over page content.
function generateSnowflakeTexture() {
  var S = 64;
  var data = new Uint8Array(S * S * 4);
  var half = S / 2;
  for (var y = 0; y < S; y++) {
    for (var x = 0; x < S; x++) {
      var dx = (x - half + 0.5) / half;
      var dy = (y - half + 0.5) / half;
      var d = Math.sqrt(dx * dx + dy * dy);
      // Soft radial falloff with slight crystalline shimmer
      var a = Math.max(0, 1 - d);
      a = a * a * a; // cubic falloff for soft edges
      // Subtle 6-fold star pattern
      var angle = Math.atan2(dy, dx);
      var star = 0.7 + 0.3 * Math.pow(Math.abs(Math.cos(angle * 3)), 4);
      a *= star;
      var idx = (y * S + x) * 4;
      data[idx] = 255;     // R
      data[idx + 1] = 255; // G
      data[idx + 2] = 255; // B
      data[idx + 3] = Math.round(Math.min(1, a) * 255); // A
    }
  }
  return data;
}

var SNOWFLAKE_TEX = generateSnowflakeTexture();

var PARTICLE_COUNT = 7000;

var VERTEX_SHADER = [
  "precision highp float;",
  "",
  "attribute vec4 a_position;",
  "attribute vec4 a_color;",
  "attribute vec3 a_rotation;",
  "attribute vec3 a_speed;",
  "attribute float a_size;",
  "",
  "uniform float u_time;",
  "uniform vec2 u_mousemove;",
  "uniform vec2 u_resolution;",
  "uniform mat4 u_projection;",
  "uniform vec3 u_worldSize;",
  "uniform float u_gravity;",
  "uniform float u_wind;",
  "",
  "varying vec4 v_color;",
  "varying float v_rotation;",
  "",
  "void main() {",
  "  v_color = a_color;",
  "  v_rotation = a_rotation.x + u_time * a_rotation.y;",
  "",
  "  vec3 pos = a_position.xyz;",
  "",
  "  pos.x = mod(pos.x + u_time + u_wind * a_speed.x, u_worldSize.x * 2.0) - u_worldSize.x;",
  "  pos.y = mod(pos.y - u_time * a_speed.y * u_gravity, u_worldSize.y * 2.0) - u_worldSize.y;",
  "",
  "  pos.x += sin(u_time * a_speed.z) * a_rotation.z;",
  "  pos.z += cos(u_time * a_speed.z) * a_rotation.z;",
  "",
  "  gl_Position = u_projection * vec4(pos.xyz, a_position.w);",
  "  gl_PointSize = (a_size / gl_Position.w) * 100.0;",
  "}"
].join("\n");

var FRAGMENT_SHADER = [
  "precision highp float;",
  "",
  "uniform sampler2D u_texture;",
  "uniform float u_dark;",
  "",
  "varying vec4 v_color;",
  "varying float v_rotation;",
  "",
  "void main() {",
  "  vec2 rotated = vec2(",
  "    cos(v_rotation) * (gl_PointCoord.x - 0.5) + sin(v_rotation) * (gl_PointCoord.y - 0.5) + 0.5,",
  "    cos(v_rotation) * (gl_PointCoord.y - 0.5) - sin(v_rotation) * (gl_PointCoord.x - 0.5) + 0.5",
  "  );",
  "",
  "  vec4 snowflake = texture2D(u_texture, rotated);",
  "  vec3 tint = mix(vec3(0.65, 0.72, 0.82), vec3(1.0), u_dark);",
  "  float a = snowflake.a * v_color.a;",
  "  gl_FragColor = vec4(snowflake.rgb * tint * a, a);",
  "}"
].join("\n");

// --- SnowShader public API ---

function SnowShader(holderEl, opts) {
  opts = opts || {};
  this.holder = holderEl;
  this.dark = opts.dark !== undefined ? opts.dark : true;
  this._program = null;

  var wind = this._wind = {
    current: 0,
    force: 0.1,
    target: 0.1,
    min: 0.1,
    max: 0.25,
    easing: 0.005
  };

  if (opts.wind && opts.wind > 0) {
    var scaled = Math.min(opts.wind / 30, 1);
    wind.min = 0.1 + scaled * 0.3;
    wind.max = wind.min + 0.15;
    wind.target = wind.min;
    wind.force = wind.min;
  }

  this._program = new ShaderProgram(holderEl, {
    depthTest: false,
    texture: SNOWFLAKE_TEX,
    uniforms: {
      worldSize: { type: "vec3", value: [0, 0, 0] },
      gravity: { type: "float", value: 100 },
      wind: { type: "float", value: 0 },
      dark: { type: "float", value: this.dark ? 1.0 : 0.0 },
    },
    buffers: {
      size: { size: 1, data: [] },
      rotation: { size: 3, data: [] },
      speed: { size: 3, data: [] },
    },
    vertex: VERTEX_SHADER,
    fragment: FRAGMENT_SHADER,
    onResize: function (w, h, dpi) {
      var position = [], color = [], size = [], rotation = [], speed = [];

      var height = 110;
      var width = w / h * height;
      var depth = 80;

      var flakeCount = Math.round(w / h * PARTICLE_COUNT);
      for (var i = 0; i < flakeCount; i++) {
        position.push(
          -width + Math.random() * width * 2,
          -height + Math.random() * height * 2,
          Math.random() * depth * 2
        );
        speed.push(
          1 + Math.random(),
          1 + Math.random(),
          Math.random() * 10
        );
        rotation.push(
          Math.random() * 2 * Math.PI,
          Math.random() * 20,
          Math.random() * 10
        );
        color.push(1, 1, 1, 0.2 + Math.random() * 0.4);
        size.push(5 * Math.random() * 5 * (h * dpi / 1000));
      }

      this.uniforms.worldSize = [width, height, depth];
      this.buffers.position = position;
      this.buffers.color = color;
      this.buffers.rotation = rotation;
      this.buffers.size = size;
      this.buffers.speed = speed;
    },
    onUpdate: function (delta) {
      wind.force += (wind.target - wind.force) * wind.easing;
      wind.current += wind.force * (delta * 0.2);
      this.uniforms.wind = wind.current;

      if (Math.random() > 0.995) {
        wind.target = (wind.min + Math.random() * (wind.max - wind.min)) * (Math.random() > 0.5 ? -1 : 1);
      }
    },
  });
}

SnowShader.prototype.start = function () {};

SnowShader.prototype.stop = function () {
  if (this._program) {
    this._program.stop();
    this._program = null;
  }
};

SnowShader.prototype.resize = function () {
  if (this._program) {
    this._program.resize();
  }
};

SnowShader.prototype.setDark = function (dark) {
  this.dark = dark;
  if (this._program) {
    this._program.uniforms.dark = dark ? 1.0 : 0.0;
  }
};

SnowShader.prototype.setWind = function (speed) {
  if (!this._wind) return;
  var scaled = Math.min(speed / 30, 1);
  this._wind.min = 0.1 + scaled * 0.3;
  this._wind.max = this._wind.min + 0.15;
};

window.SnowShader = SnowShader;

})();
