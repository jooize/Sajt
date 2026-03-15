// Canvas 2D snow particle system — variant 9
// Self-contained SnowCanvas class with start/stop/resize/setDark/setWind API.
// Pre-rendered radial gradient textures for performance.
// 3 depth layers (far/mid/near) with compound sinusoidal drift.
"use strict";

(function () {
  var TAU = Math.PI * 2;
  // Golden ratio conjugate — irrational, good for incommensurate frequencies
  var PHI = 0.6180339887;

  // Layer definitions: [minSize, maxSize, minSpeed, maxSpeed, minOpacity, maxOpacity, windFactor]
  var LAYERS = [
    { minR: 1.0, maxR: 1.8, minSpd: 18, maxSpd: 26, minA: 0.15, maxA: 0.35, wind: 0.3 },  // far
    { minR: 1.5, maxR: 2.5, minSpd: 14, maxSpd: 20, minA: 0.30, maxA: 0.55, wind: 0.6 },  // mid
    { minR: 2.2, maxR: 3.5, minSpd: 10, maxSpd: 16, minA: 0.50, maxA: 0.85, wind: 1.0 },  // near
  ];

  // Edge fade zones (fraction of viewport height)
  var FADE_TOP = 0.05;
  var FADE_BOT = 0.03;

  function rand(lo, hi) {
    return lo + Math.random() * (hi - lo);
  }

  // Pre-render a soft radial gradient dot onto an offscreen canvas
  function makeTexture(radius, r, g, b, alpha) {
    var size = Math.ceil(radius * 2 + 4);
    var oc = document.createElement("canvas");
    oc.width = size;
    oc.height = size;
    var ctx = oc.getContext("2d");
    var cx = size / 2, cy = size / 2;
    var grad = ctx.createRadialGradient(cx, cy, 0, cx, cy, radius);
    grad.addColorStop(0, "rgba(" + r + "," + g + "," + b + "," + alpha + ")");
    grad.addColorStop(0.5, "rgba(" + r + "," + g + "," + b + "," + (alpha * 0.7) + ")");
    grad.addColorStop(1, "rgba(" + r + "," + g + "," + b + ",0)");
    ctx.fillStyle = grad;
    ctx.fillRect(0, 0, size, size);
    return { canvas: oc, size: size };
  }

  function Flake(layer, w, h, dark) {
    this.layer = layer;
    var L = LAYERS[layer];
    this.radius = rand(L.minR, L.maxR);
    this.speed = rand(L.minSpd, L.maxSpd); // pixels per second
    this.baseAlpha = rand(L.minA, L.maxA);
    this.windFactor = L.wind;

    // Position
    this.x = rand(0, w);
    this.y = rand(-h * 0.1, h);

    // Compound drift — two incommensurate frequencies
    this.driftAmpA = rand(15, 40);
    this.driftFreqA = rand(0.3, 0.7);
    this.driftPhaseA = rand(0, TAU);
    this.driftAmpB = rand(5, 15);
    this.driftFreqB = this.driftFreqA * PHI; // irrational ratio
    this.driftPhaseB = rand(0, TAU);

    // Sparkle — opacity oscillation
    this.sparkleFreq = rand(1.5, 3.5);
    this.sparklePhase = rand(0, TAU);
    this.sparkleAmp = rand(0.05, 0.15);

    this.t = rand(0, 100); // time accumulator for drift

    // Pre-render texture
    this._rebuildTexture(dark);
  }

  Flake.prototype._rebuildTexture = function (dark) {
    var r, g, b;
    if (dark) {
      r = 255; g = 255; b = 255;
    } else {
      // Cool blue-gray on light backgrounds
      r = 180; g = 195; b = 210;
    }
    var tex = makeTexture(this.radius * 2, r, g, b, 1);
    this.tex = tex.canvas;
    this.texSize = tex.size;
  };

  Flake.prototype.update = function (dt, w, h, wind) {
    this.t += dt;
    this.y += this.speed * dt;

    // Compound sinusoidal horizontal drift
    var driftX = Math.sin(this.t * this.driftFreqA + this.driftPhaseA) * this.driftAmpA
               + Math.sin(this.t * this.driftFreqB + this.driftPhaseB) * this.driftAmpB;
    // Wind effect (pixels per second, scaled by layer's wind factor)
    var windPx = wind * 2.0 * this.windFactor;
    this.x += windPx * dt;

    // Wrap horizontally
    if (this.x > w + 50) this.x -= w + 100;
    if (this.x < -50) this.x += w + 100;

    // Reset if fallen below viewport
    if (this.y > h + 20) {
      this.y = rand(-40, -10);
      this.x = rand(0, w);
    }

    // Compute drawable x including drift
    this.drawX = this.x + driftX;

    // Sparkle
    var sparkle = Math.sin(this.t * this.sparkleFreq + this.sparklePhase) * this.sparkleAmp;
    this.alpha = Math.max(0, Math.min(1, this.baseAlpha + sparkle));

    // Edge fade
    if (this.y < h * FADE_TOP) {
      this.alpha *= Math.max(0, this.y / (h * FADE_TOP));
    }
    var fadeStart = h * (1 - FADE_BOT);
    if (this.y > fadeStart) {
      this.alpha *= Math.max(0, 1 - (this.y - fadeStart) / (h * FADE_BOT));
    }
  };

  Flake.prototype.draw = function (ctx) {
    if (this.alpha < 0.01) return;
    ctx.globalAlpha = this.alpha;
    ctx.drawImage(this.tex, this.drawX - this.texSize / 2, this.y - this.texSize / 2);
  };

  // ---- SnowCanvas class ----

  function SnowCanvas(canvas, opts) {
    opts = opts || {};
    this.canvas = canvas;
    this.ctx = canvas.getContext("2d");
    this.w = canvas.width;
    this.h = canvas.height;
    this.dark = opts.dark !== undefined ? opts.dark : true;
    this.wind = opts.wind || 0;
    this.flakes = [];
    this.raf = 0;
    this.lastTime = 0;

    this._initFlakes();
  }

  SnowCanvas.prototype._density = function () {
    var area = this.w * this.h;
    // Scale: ~100 on mobile (360x640 = 230k), ~300 on desktop (1920x1080 = 2M)
    var d = Math.round(80 + (area / 2073600) * 220);
    return Math.max(80, Math.min(400, d));
  };

  SnowCanvas.prototype._initFlakes = function () {
    var count = this._density();
    this.flakes = [];
    for (var i = 0; i < count; i++) {
      // Distribute across layers: 40% far, 35% mid, 25% near
      var layer = i < count * 0.4 ? 0 : i < count * 0.75 ? 1 : 2;
      this.flakes.push(new Flake(layer, this.w, this.h, this.dark));
    }
  };

  SnowCanvas.prototype.start = function () {
    if (this.raf) return;
    var self = this;
    this.lastTime = performance.now();
    function frame(now) {
      self.raf = requestAnimationFrame(frame);
      var dt = Math.min((now - self.lastTime) / 1000, 0.1); // cap at 100ms
      self.lastTime = now;
      self.ctx.clearRect(0, 0, self.w, self.h);
      for (var i = 0; i < self.flakes.length; i++) {
        self.flakes[i].update(dt, self.w, self.h, self.wind);
        self.flakes[i].draw(self.ctx);
      }
      self.ctx.globalAlpha = 1;
    }
    this.raf = requestAnimationFrame(frame);
  };

  SnowCanvas.prototype.stop = function () {
    if (this.raf) {
      cancelAnimationFrame(this.raf);
      this.raf = 0;
    }
  };

  SnowCanvas.prototype.resize = function (w, h) {
    this.w = w;
    this.h = h;
    // Adjust flake count if needed
    var target = this._density();
    while (this.flakes.length < target) {
      var layer = Math.random() < 0.4 ? 0 : Math.random() < 0.58 ? 1 : 2;
      this.flakes.push(new Flake(layer, w, h, this.dark));
    }
    while (this.flakes.length > target) {
      this.flakes.pop();
    }
  };

  SnowCanvas.prototype.setDark = function (dark) {
    this.dark = dark;
    for (var i = 0; i < this.flakes.length; i++) {
      this.flakes[i]._rebuildTexture(dark);
    }
  };

  SnowCanvas.prototype.setWind = function (speed) {
    this.wind = speed;
  };

  window.SnowCanvas = SnowCanvas;
})();
