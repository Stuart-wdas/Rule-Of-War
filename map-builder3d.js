import * as THREE from "three";
import { OrbitControls } from "three/addons/controls/OrbitControls.js";
import { MAP_BUILDER_HEIGHT_SCALE } from "./config/map-builder-config.js";
import { smoothTerrainBrush } from "./core/terrain-smoothing.js";

function hexColor(value) {
  return new THREE.Color(value || "#668844");
}

function treeNoise(x, y, salt = 0) {
  const value = Math.sin(x * 127.1 + y * 311.7 + salt * 74.7) * 43758.5453;
  return value - Math.floor(value);
}

export class MapBuilder3D {
  constructor(canvas, getBrush, { onHeightPicked } = {}) {
    this.canvas = canvas;
    this.getBrush = getBrush;
    this.onHeightPicked = onHeightPicked;
    this.renderer = new THREE.WebGLRenderer({ canvas, antialias: true });
    this.renderer.setPixelRatio(Math.min(devicePixelRatio || 1, 1.5));
    this.renderer.outputColorSpace = THREE.SRGBColorSpace;
    this.scene = new THREE.Scene();
    this.scene.background = new THREE.Color(0x111611);
    this.camera = new THREE.PerspectiveCamera(42, 1, 0.1, 1000);
    this.camera.position.set(42, 46, 42);
    this.controls = new OrbitControls(this.camera, canvas);
    this.controls.enableDamping = true;
    this.controls.mouseButtons.LEFT = null;
    this.controls.mouseButtons.RIGHT = THREE.MOUSE.ROTATE;
    this.controls.mouseButtons.MIDDLE = THREE.MOUSE.PAN;
    this.controls.target.set(0, 0, 0);
    this.controls.update();
    this.scene.add(new THREE.HemisphereLight(0xdde9d4, 0x182018, 2));
    const light = new THREE.DirectionalLight(0xffffff, 2.2);
    light.position.set(30, 60, 20);
    this.scene.add(light);
    this.raycaster = new THREE.Raycaster();
    this.paintPlane = new THREE.Plane(new THREE.Vector3(0, 1, 0), 0);
    this.pointer = new THREE.Vector2();
    this.painting = false;
    this.terrainChanged = false;
    this.treesChanged = false;
    this.pickingHeight = false;
    this.lastCell = "";
    this.createBrushPreview();
    this.onPointerDown = (event) => {
      if (event.button !== 0) return;
      event.preventDefault();
      const hitPoint = this.surfacePointFromEvent(event);
      if (!hitPoint) return;
      if (this.pickingHeight) {
        const height = this.heightAtWorldPoint(hitPoint);
        this.pickingHeight = false;
        this.canvas.classList.remove("picking-height");
        this.onHeightPicked?.(height);
        this.updateBrushPreview(event);
        return;
      }
      this.painting = true;
      this.lastCell = "";
      canvas.setPointerCapture?.(event.pointerId);
      this.paintAt(hitPoint);
    };
    this.onPointerMove = (event) => {
      const hitPoint = this.updateBrushPreview(event);
      if (this.painting && hitPoint) this.paintAt(hitPoint);
    };
    this.onPointerUp = (event) => {
      if (!this.painting) return;
      this.painting = false;
      canvas.releasePointerCapture?.(event.pointerId);
      if (this.terrainChanged) {
        this.terrain.geometry.computeVertexNormals();
        this.terrainChanged = false;
      }
      if (this.treesChanged) {
        this.refreshTrees();
        this.treesChanged = false;
      }
    };
    this.onPointerLeave = () => {
      if (!this.painting) this.setBrushPreviewVisible(false);
    };
    canvas.addEventListener("pointerdown", this.onPointerDown);
    canvas.addEventListener("pointermove", this.onPointerMove);
    canvas.addEventListener("pointerup", this.onPointerUp);
    canvas.addEventListener("pointercancel", this.onPointerUp);
    canvas.addEventListener("pointerleave", this.onPointerLeave);
    canvas.addEventListener("contextmenu", (event) => event.preventDefault());
    this.resizeObserver = new ResizeObserver(() => this.resize());
    this.resizeObserver.observe(canvas);
    this.animate();
  }

  createBrushPreview() {
    const points = [];
    for (let index = 0; index < 65; index += 1) {
      const angle = (index / 64) * Math.PI * 2;
      points.push(new THREE.Vector3(Math.cos(angle), 0, Math.sin(angle)));
    }
    const geometry = new THREE.BufferGeometry().setFromPoints(points);
    this.brushOuter = new THREE.Line(
      geometry,
      new THREE.LineBasicMaterial({
        color: 0xf1e58b,
        transparent: true,
        opacity: 0.95,
        depthTest: false,
      }),
    );
    this.brushInner = new THREE.Line(
      geometry.clone(),
      new THREE.LineBasicMaterial({
        color: 0xff8a58,
        transparent: true,
        opacity: 0.8,
        depthTest: false,
      }),
    );
    this.brushOuter.renderOrder = 20;
    this.brushInner.renderOrder = 21;
    this.setBrushPreviewVisible(false);
    this.scene.add(this.brushOuter, this.brushInner);
  }

  setBrushPreviewVisible(visible) {
    this.brushOuter.visible = visible;
    this.brushInner.visible = visible;
  }

  beginHeightPick() {
    this.pickingHeight = true;
    this.canvas.classList.add("picking-height");
  }

  setMap(state) {
    this.state = state;
    // Keep editor geometry bounded even when the authored gameplay map is
    // extremely large. The saved height field remains full resolution.
    this.meshSize = Math.min(state.size, 257);
    this.terrain?.geometry.dispose();
    this.terrain?.material.dispose();
    this.terrain?.removeFromParent();
    const span = state.size - 1;
    const geometry = new THREE.PlaneGeometry(
      span,
      span,
      this.meshSize - 1,
      this.meshSize - 1,
    );
    geometry.rotateX(-Math.PI / 2);
    geometry.setAttribute(
      "color",
      new THREE.BufferAttribute(
        new Float32Array(this.meshSize * this.meshSize * 3),
        3,
      ),
    );
    const material = new THREE.MeshStandardMaterial({
      vertexColors: true,
      roughness: 0.94,
      side: THREE.DoubleSide,
    });
    this.terrain = new THREE.Mesh(geometry, material);
    this.scene.add(this.terrain);
    this.water?.geometry.dispose();
    this.water?.material.dispose();
    this.water?.removeFromParent();
    this.water = new THREE.Mesh(
      new THREE.PlaneGeometry(span, span),
      new THREE.MeshStandardMaterial({
        color: 0x2e91bd,
        transparent: true,
        opacity: 0.78,
        roughness: 0.32,
      }),
    );
    this.water.rotation.x = -Math.PI / 2;
    this.scene.add(this.water);
    this.camera.position.set(span * 0.72, span * 0.82, span * 0.72);
    this.camera.far = Math.max(500, span * 8);
    this.camera.updateProjectionMatrix();
    this.controls.target.set(0, 1.5, 0);
    this.controls.update();
    this.refreshAll();
  }

  refreshAll(includeTrees = true) {
    if (!this.state || !this.terrain) return;
    const position = this.terrain.geometry.attributes.position;
    for (let index = 0; index < position.count; index += 1)
      position.setY(
        index,
        this.elevationForVertex(index) * MAP_BUILDER_HEIGHT_SCALE,
      );
    position.needsUpdate = true;
    this.terrain.geometry.computeVertexNormals();
    this.refreshColors();
    if (includeTrees) this.refreshTrees();
  }

  refreshColors() {
    if (!this.state || !this.terrain) return;
    const brush = this.getBrush();
    const colors = this.terrain.geometry.attributes.color;
    const palette = {
      low: hexColor(brush.lowColor),
      middle: hexColor(brush.midColor),
      high: hexColor(brush.highColor),
      water: new THREE.Color(0x328fb7),
    };
    for (let index = 0; index < colors.count; index += 1) {
      const elevation = this.elevationForVertex(index);
      const color =
        elevation < brush.water
          ? palette.water
          : elevation < brush.lowBoundary
            ? palette.low
            : elevation < brush.highBoundary
              ? palette.middle
              : palette.high;
      colors.setXYZ(index, color.r, color.g, color.b);
    }
    colors.needsUpdate = true;
    this.water.position.y = brush.water * MAP_BUILDER_HEIGHT_SCALE + 0.025;
  }

  elevationForVertex(vertexIndex) {
    const vertexX = vertexIndex % this.meshSize;
    const vertexY = Math.floor(vertexIndex / this.meshSize);
    const mapX = Math.round(
      (vertexX / Math.max(1, this.meshSize - 1)) * (this.state.size - 1),
    );
    const mapY = Math.round(
      (vertexY / Math.max(1, this.meshSize - 1)) * (this.state.size - 1),
    );
    return this.state.elevation[mapY * this.state.size + mapX];
  }

  refreshTrees() {
    this.treeMesh?.geometry.dispose();
    this.treeMesh?.material.dispose();
    this.treeMesh?.removeFromParent();
    if (!this.state) return;
    const indices = [];
    const stride = Math.max(
      1,
      Math.ceil(Math.sqrt(this.state.trees.length / 50000)),
    );
    for (let y = 0; y < this.state.size; y += stride)
      for (let x = 0; x < this.state.size; x += stride) {
        const index = y * this.state.size + x;
        const density = this.state.trees[index];
        const placement = ((x * 73856093) ^ (y * 19349663)) & 255;
        if (density > placement) indices.push(index);
      }
    if (!indices.length) return;
    this.treeMesh = new THREE.InstancedMesh(
      new THREE.ConeGeometry(0.22, 0.75, 5),
      new THREE.MeshStandardMaterial({ color: 0x24552c, roughness: 1 }),
      indices.length,
    );
    const transform = new THREE.Object3D();
    const half = (this.state.size - 1) / 2;
    indices.forEach((index, instance) => {
      const x = index % this.state.size;
      const y = Math.floor(index / this.state.size);
      const jitteredX = THREE.MathUtils.clamp(
        x + (treeNoise(x, y, 7) - 0.5) * stride * 0.9,
        0,
        this.state.size - 1,
      );
      const jitteredY = THREE.MathUtils.clamp(
        y + (treeNoise(x, y, 11) - 0.5) * stride * 0.9,
        0,
        this.state.size - 1,
      );
      const elevationX = Math.round(jitteredX);
      const elevationY = Math.round(jitteredY);
      const elevationIndex = elevationY * this.state.size + elevationX;
      const scale = 0.78 + treeNoise(x, y, 17) * 0.44;
      transform.position.set(
        jitteredX - half,
        this.state.elevation[elevationIndex] * MAP_BUILDER_HEIGHT_SCALE +
          0.375 * scale,
        jitteredY - half,
      );
      transform.scale.setScalar(scale);
      transform.rotation.set(0, treeNoise(x, y, 19) * Math.PI * 2, 0);
      transform.updateMatrix();
      this.treeMesh.setMatrixAt(instance, transform.matrix);
    });
    this.treeMesh.instanceMatrix.needsUpdate = true;
    this.scene.add(this.treeMesh);
  }

  surfacePointFromEvent(event) {
    if (!this.state || !this.terrain) return null;
    const rect = this.canvas.getBoundingClientRect();
    this.pointer.set(
      ((event.clientX - rect.left) / rect.width) * 2 - 1,
      -((event.clientY - rect.top) / rect.height) * 2 + 1,
    );
    this.raycaster.setFromCamera(this.pointer, this.camera);
    let hitPoint = null;
    let surfaceHeight = 0;
    // Two cheap height-plane passes keep the brush aligned with the visible
    // surface without ray-testing every triangle on each pointer movement.
    for (let pass = 0; pass < 2; pass += 1) {
      this.paintPlane.constant = -surfaceHeight;
      hitPoint = this.raycaster.ray.intersectPlane(
        this.paintPlane,
        new THREE.Vector3(),
      );
      if (!hitPoint) return null;
      surfaceHeight =
        this.heightAtWorldPoint(hitPoint) * MAP_BUILDER_HEIGHT_SCALE;
    }
    return hitPoint;
  }

  mapPointFromWorld(hitPoint) {
    const half = (this.state.size - 1) / 2;
    return {
      x: Math.max(
        0,
        Math.min(this.state.size - 1, Math.round(hitPoint.x + half)),
      ),
      y: Math.max(
        0,
        Math.min(this.state.size - 1, Math.round(hitPoint.z + half)),
      ),
    };
  }

  heightAtWorldPoint(hitPoint) {
    const point = this.mapPointFromWorld(hitPoint);
    return this.state.elevation[point.y * this.state.size + point.x];
  }

  updateBrushPreview(event) {
    const hitPoint = this.surfacePointFromEvent(event);
    if (!hitPoint) {
      this.setBrushPreviewVisible(false);
      return null;
    }
    const brush = this.getBrush();
    const radius = this.effectiveRadius(brush.radius);
    const activeStrength =
      brush.tool === "smooth" ? brush.smoothing : brush.strength;
    const strength = Math.max(0.01, Math.min(1, activeStrength));
    const height =
      this.heightAtWorldPoint(hitPoint) * MAP_BUILDER_HEIGHT_SCALE + 0.09;
    this.brushOuter.position.set(hitPoint.x, height, hitPoint.z);
    this.brushInner.position.copy(this.brushOuter.position);
    this.brushOuter.scale.setScalar(radius);
    this.brushInner.scale.setScalar(Math.max(0.12, radius * strength));
    this.brushOuter.material.color.set(
      this.pickingHeight ? 0x72d6e5 : 0xf1e58b,
    );
    this.setBrushPreviewVisible(true);
    return hitPoint;
  }

  effectiveRadius(radius) {
    return Math.max(radius, radius * (this.state.size / this.meshSize));
  }

  paintAt(hitPoint) {
    const { x: centerX, y: centerY } = this.mapPointFromWorld(hitPoint);
    const cellKey = `${centerX},${centerY}`;
    if (cellKey === this.lastCell) return;
    this.lastCell = cellKey;
    const brush = this.getBrush();
    const effectiveRadius = this.effectiveRadius(brush.radius);
    const strength = Math.max(0.01, Math.min(1, brush.strength));
    const minX = Math.max(0, Math.floor(centerX - effectiveRadius));
    const maxX = Math.min(
      this.state.size - 1,
      Math.ceil(centerX + effectiveRadius),
    );
    const minY = Math.max(0, Math.floor(centerY - effectiveRadius));
    const maxY = Math.min(
      this.state.size - 1,
      Math.ceil(centerY + effectiveRadius),
    );
    if (brush.tool === "smooth") {
      smoothTerrainBrush(
        this.state.elevation,
        this.state.size,
        centerX,
        centerY,
        effectiveRadius,
        brush.smoothing,
      );
      this.terrainChanged = true;
      this.refreshTerrainRegion(minX, minY, maxX, maxY);
      return;
    }
    for (let y = minY; y <= maxY; y += 1)
      for (let x = minX; x <= maxX; x += 1) {
        const distance = Math.hypot(x - centerX, y - centerY);
        if (distance > effectiveRadius) continue;
        const normalizedDistance = distance / Math.max(1, effectiveRadius);
        const falloff = (1 - normalizedDistance * normalizedDistance) ** 2;
        const influence = strength * falloff;
        const index = y * this.state.size + x;
        if (brush.tool === "sculpt") {
          const direction = brush.mode === "subtract" ? -1 : 1;
          this.state.elevation[index] = Math.max(
            0,
            Math.min(
              1,
              this.state.elevation[index] + direction * 0.045 * influence,
            ),
          );
        } else if (brush.tool === "flatten") {
          this.state.elevation[index] +=
            (brush.height - this.state.elevation[index]) * influence;
        } else if (brush.tool === "trees") {
          this.state.trees[index] = Math.max(
            this.state.trees[index],
            Math.round(255 * influence),
          );
        } else if (brush.tool === "erase-trees") {
          this.state.trees[index] = Math.max(
            0,
            this.state.trees[index] - Math.round(255 * influence),
          );
        }
      }
    if (["trees", "erase-trees"].includes(brush.tool)) {
      this.treesChanged = true;
      return;
    }
    this.terrainChanged = true;
    this.refreshTerrainRegion(minX, minY, maxX, maxY);
  }

  refreshTerrainRegion(minX, minY, maxX, maxY) {
    const divisor = Math.max(1, this.state.size - 1);
    const meshSpan = this.meshSize - 1;
    const startX = Math.max(0, Math.floor((minX / divisor) * meshSpan) - 1);
    const endX = Math.min(meshSpan, Math.ceil((maxX / divisor) * meshSpan) + 1);
    const startY = Math.max(0, Math.floor((minY / divisor) * meshSpan) - 1);
    const endY = Math.min(meshSpan, Math.ceil((maxY / divisor) * meshSpan) + 1);
    const position = this.terrain.geometry.attributes.position;
    const colors = this.terrain.geometry.attributes.color;
    const brush = this.getBrush();
    const palette = {
      low: hexColor(brush.lowColor),
      middle: hexColor(brush.midColor),
      high: hexColor(brush.highColor),
      water: new THREE.Color(0x328fb7),
    };
    for (let vertexY = startY; vertexY <= endY; vertexY += 1)
      for (let vertexX = startX; vertexX <= endX; vertexX += 1) {
        const vertexIndex = vertexY * this.meshSize + vertexX;
        const elevation = this.elevationForVertex(vertexIndex);
        position.setY(vertexIndex, elevation * MAP_BUILDER_HEIGHT_SCALE);
        const color =
          elevation < brush.water
            ? palette.water
            : elevation < brush.lowBoundary
              ? palette.low
              : elevation < brush.highBoundary
                ? palette.middle
                : palette.high;
        colors.setXYZ(vertexIndex, color.r, color.g, color.b);
      }
    position.needsUpdate = true;
    colors.needsUpdate = true;
  }

  resize() {
    const width = Math.max(1, this.canvas.clientWidth);
    const height = Math.max(1, this.canvas.clientHeight);
    this.renderer.setSize(width, height, false);
    this.camera.aspect = width / height;
    this.camera.updateProjectionMatrix();
  }

  animate() {
    this.animationFrame = requestAnimationFrame(() => this.animate());
    this.controls.update();
    this.renderer.render(this.scene, this.camera);
  }
}
