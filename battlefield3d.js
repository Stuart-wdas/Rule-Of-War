import * as THREE from "three";
import { OrbitControls } from "three/addons/controls/OrbitControls.js";
import {
  createTerrainSampler,
  TERRAIN_MAX_ELEVATION,
  terrainColor,
} from "./maps/procedural-terrain.js";

const COLORS = {
  background: 0x0d110e,
  base: 0x1f271f,
  // Fog must remain dark, but not black: instance colours multiply the tile
  // texture, so near-black values erase all surface detail on large maps.
  fog: 0x2b342c,
  fogGlow: 0x445047,
  player: 0x718050,
  playerDark: 0x303a28,
  enemy: 0x766457,
  enemyDark: 0x382f2b,
  lime: 0xd2c779,
  paper: 0xd9d0b7,
  target: 0xd9654f,
};

const DEFAULT_BOARD_DIAGONAL = Math.hypot(12, 8);
const HOME_DIRECTION = new THREE.Vector3(0.47, 0.64, 0.61).normalize();
const DEFAULT_EFFECT_QUALITY = Object.freeze({
  smokePuffs: 8,
  debrisMultiplier: 0.1,
  trailPoints: 5,
  dynamicLights: false,
});

// Range circles are evaluated analytically in the strategic surface shader.
// These values control their world-space width and opacity without requiring
// a whole-map canvas rebuild when the active tool changes.
const RANGE_OVERLAY_QUALITY = Object.freeze({
  boundaryWidthInTiles: 0.08,
  fillAlpha: 0,
  boundaryAlpha: 0.95,
  constructionFillAlpha: 0.3,
  constructionBoundaryAlpha: 1,
  constructionBoundaryWidthInTiles: 0.11,
  constructionGridBoost: 0.72,
  // Movement gets a stronger treatment than construction and attack ranges.
  movementFillAlpha: 0,
  movementBoundaryAlpha: 1,
  movementBoundaryWidthInTiles: 0.12,
  movementTileStrength: 0.66,
});

// Virtual-map LOD is based on projected tile size, not camera pitch or the
// currently loaded chunk dimensions. Lower these values to retain detailed
// textured tiles farther into a zoom-out. The gap is intentional hysteresis.
const VIRTUAL_TILE_LOD = Object.freeze({
  detailExitPixelsPerTile: 2.6,
  detailEnterPixelsPerTile: 3.4,
});

// Detailed tiles stream in whole chunks. A larger chunk reduces how often a
// pan rebuilds the instance buffer, while a smaller chunk reduces overdraw and
// upload size around the viewport edge.
const VIRTUAL_CHUNK_SIZE = 64;

// Fog remains a compact visibility mask. Raise maximumTextureSize for a finer
// line-of-sight edge on large maps at the cost of GPU memory and upload time.
const VIRTUAL_FOG_QUALITY = Object.freeze({
  pixelsPerTile: 2,
  minimumTextureSize: 512,
  maximumTextureSize: 2048,
});

// GLSL uniform arrays have a fixed upper bound. Build ranges normally contain
// only a few sources, while 64 still covers unusually structure-heavy bases.
const MAX_VIRTUAL_RANGE_SOURCES = 64;

// Strategic grid lines need enough screen space to avoid moire patterns at
// shallow angles. Raise targetCellPixels for a coarser, cleaner zoomed-out
// grid. densityFade values are scaled-coordinate changes per screen pixel.
const VIRTUAL_GRID_QUALITY = Object.freeze({
  targetCellPixels: 12,
  lineHalfWidthPixels: 0.8,
  densityFadeStart: 0.12,
  densityFadeEnd: 0.38,
  stepIncreaseThreshold: 1.5,
  stepDecreaseThreshold: 0.55,
});

// Bomber flight timing is distance-based so long sorties remain readable.
// Increase millisecondsPerTile or the minimum values to slow them further.
const BOMBER_FLIGHT_TIMING = Object.freeze({
  outboundBaseMs: 1500,
  outboundMillisecondsPerTile: 105,
  outboundMinimumMs: 2200,
  outboundMaximumMs: 6000,
  returnBaseMs: 1750,
  returnMillisecondsPerTile: 112,
  returnMinimumMs: 2500,
  returnMaximumMs: 6800,
});

// Reticles preserve an approximate on-screen size at strategic zoom. Increase
// the pixel values for larger indicators or maxScale for extreme map sizes.
const TARGET_MARKER_VISIBILITY = Object.freeze({
  hoverPixels: 34,
  queuedPixels: 30,
  maximumScale: 72,
});

const ROUTE_FLAG_VISIBILITY = Object.freeze({ pixels: 12 });

/**
 * model = {
 *   tiles: [{ x, y, terrain, water, waterCoverage, visible, selected, available, reachable, targetable, fireCount }],
 *   pieces: [{ id, kind, type, owner, x, y, worldX, worldY, hp, maxHp, pending, selected, orderKind, heading }],
 *   fog: { enabled, sources: [{ x, y, radius }] },
 *   rangeOverlay: { kind, sources: [{ x, y, radius }] },
 *   orderTargets: [{ id, kind, x, y }],
 *   movementRoutes: [{ id, points: [{ x, y }], flagPoints: [{ x, y }] }],
 *   routePlanning: boolean
 * }
 * moves = [{ id, owner, from: {x,y}, to: {x,y} }]
 */
export class Battlefield3D {
  constructor(
    container,
    {
      cols,
      rows,
      tiles = null,
      mapBounds = null,
      virtualized = false,
      tileCapacity = 12000,
      renderChunkSize = VIRTUAL_CHUNK_SIZE,
      terrainSeed = "rule-of-war-terrain",
      terrain = null,
      terrainFlatteningAreas = [],
      terrainDebug = false,
      showElevationValues = false,
      deferVegetation = false,
      deferFogClouds = false,
      authoredTreeDensity = null,
      resolvePlacementPreview = null,
      onPlacementPreviewChange = null,
      onBoxSelect = null,
      onTileClick,
      onViewportChange,
    },
  ) {
    if (!container) throw new Error("A battlefield container is required.");
    this.container = container;
    this.tileDescriptors = tiles;
    this.virtualized = virtualized;
    this.tileCapacity = tileCapacity;
    this.renderChunkSize = Math.max(8, Math.floor(renderChunkSize));
    this.resolvePlacementPreview = resolvePlacementPreview;
    this.onPlacementPreviewChange = onPlacementPreviewChange;
    this.onBoxSelect = onBoxSelect;
    this.onViewportChange = onViewportChange;
    this.viewportWindowKey = null;
    this.viewportChangeFrame = null;
    this.tileDescriptorByKey = new Map(
      (tiles ?? []).map((tile) => [`${tile.x},${tile.y}`, tile]),
    );
    this.mapBounds = mapBounds
      ? { ...mapBounds }
      : tiles?.length
        ? {
            minX: Number.POSITIVE_INFINITY,
            maxX: Number.NEGATIVE_INFINITY,
            minY: Number.POSITIVE_INFINITY,
            maxY: Number.NEGATIVE_INFINITY,
          }
        : { minX: -0.5, maxX: cols - 0.5, minY: -0.5, maxY: rows - 0.5 };
    for (const tile of mapBounds ? [] : (tiles ?? [])) {
      for (const vertex of tile.vertices) {
        this.mapBounds.minX = Math.min(this.mapBounds.minX, vertex.x);
        this.mapBounds.maxX = Math.max(this.mapBounds.maxX, vertex.x);
        this.mapBounds.minY = Math.min(this.mapBounds.minY, vertex.y);
        this.mapBounds.maxY = Math.max(this.mapBounds.maxY, vertex.y);
      }
    }
    this.mapCenter = {
      x: (this.mapBounds.minX + this.mapBounds.maxX) / 2,
      y: (this.mapBounds.minY + this.mapBounds.maxY) / 2,
    };
    this.terrainSampler = terrain?.sample
      ? (x, y) => terrain.sample(x, y)
      : createTerrainSampler(this.mapBounds, terrainSeed);
    this.terrain = terrain?.sample ? terrain : this.terrainSampler.terrain;
    this.terrainDebug = Boolean(terrainDebug);
    this.showElevationValues = Boolean(showElevationValues);
    this.deferVegetation = Boolean(deferVegetation);
    this.deferFogClouds = Boolean(deferFogClouds);
    this.authoredTreeDensity = authoredTreeDensity;
    this.terrainElevationScale = Number.isFinite(terrain?.config?.maxElevation)
      ? terrain.config.maxElevation
      : TERRAIN_MAX_ELEVATION;
    this.terrainFlatteningAreas = this._normalizeTerrainFlatteningAreas(
      terrainFlatteningAreas,
    );
    this.terrainFlatteningSignature = JSON.stringify(
      this.terrainFlatteningAreas,
    );
    this.cols = this.mapBounds.maxX - this.mapBounds.minX;
    this.rows = this.mapBounds.maxY - this.mapBounds.minY;
    this.boardDiagonal = Math.hypot(this.cols, this.rows);
    this.largeScale = this.boardDiagonal > 180;
    this.hasFittedView = false;
    this.hasFocusedInitialView = false;
    this.onTileClick = onTileClick;
    this.model = { tiles: [], pieces: [] };
    this.tileMeshes = [];
    this.tileByKey = new Map();
    this.tileTransform = new THREE.Object3D();
    this.pieceMeshes = new Map();
    this.hoveredTile = null;
    this.pointer = new THREE.Vector2(4, 4);
    this.raycaster = new THREE.Raycaster();
    this.lodRaycaster = new THREE.Raycaster();
    this.boardPlane = new THREE.Plane(new THREE.Vector3(0, 1, 0), -0.08);
    this.pointerIntersection = new THREE.Vector3();
    this.spinningObjects = [];
    this.pulsingObjects = [];
    this.routeFlags = [];
    this.hoveredMapPoint = null;
    this.hoveredTargetId = null;
    this.debugLabelSignature = "";
    this.clock = new THREE.Clock();
    this.destroyed = false;
    this.timedAnimations = new Set();
    this.activeTransientLights = 0;
    this.maxTransientLights = 6;
    this.missTexture = null;
    this.hitTexture = null;
    this.proceduralTerrainTexture = null;
    this.proceduralTerrainHeightTexture = null;
    this.proceduralTerrainCanvas = null;
    this.proceduralTerrainHeightCanvas = null;
    this.treePlacements = null;
    this.treeMesh = null;
    this.dynamicModelSignature = "";
    this.routeModelSignature = "";
    this.virtualFogSignature = "";
    this.virtualRangeSignature = "";
    this.virtualDetailVisible = true;
    this.virtualGridStep = 1;

    this.scene = new THREE.Scene();
    this.scene.background = new THREE.Color(COLORS.background);
    const fogScale = Math.min(1, DEFAULT_BOARD_DIAGONAL / this.boardDiagonal);
    this.scene.fog = new THREE.FogExp2(0x0a0d09, 0.012 * fogScale);

    this.camera = new THREE.PerspectiveCamera(
      38,
      1,
      this.largeScale ? 1 : 0.1,
      Math.max(180, this.boardDiagonal * 6),
    );
    const cameraScale = Math.max(this.cols / 12, this.rows / 8, 0.72);
    this.camera.position.set(1 * cameraScale, 1 * cameraScale, 1 * cameraScale);
    this.camera.lookAt(0, 0, 0);

    this.renderer = new THREE.WebGLRenderer({
      antialias: true,
      powerPreference: "high-performance",
    });
    const pixelRatioLimit = this.largeScale
      ? 1
      : this.boardDiagonal > 45
        ? 1.25
        : this.boardDiagonal > 28
          ? 1.5
          : 2;
    this.renderer.setPixelRatio(
      Math.min(window.devicePixelRatio || 1, pixelRatioLimit),
    );
    this.renderer.outputColorSpace = THREE.SRGBColorSpace;
    this.renderer.toneMapping = THREE.ACESFilmicToneMapping;
    this.renderer.toneMappingExposure = 1.22;
    this.renderer.shadowMap.enabled = !this.largeScale;
    this.renderer.shadowMap.type = THREE.PCFSoftShadowMap;
    this.renderer.domElement.setAttribute(
      "aria-label",
      "Interactive 3D battlefield",
    );
    container.appendChild(this.renderer.domElement);
    this.selectionBox = document.createElement("div");
    this.selectionBox.className = "unit-selection-box";
    this.selectionBox.hidden = true;
    container.appendChild(this.selectionBox);

    this.controls = new OrbitControls(this.camera, this.renderer.domElement);
    this.controls.enableDamping = true;
    this.controls.dampingFactor = 0.075;
    this.controls.enableRotate = true;
    this.controls.enableZoom = true;
    this.controls.enablePan = true;
    this.controls.rotateSpeed = 0.55;
    this.controls.zoomSpeed = 0.85;
    this.controls.panSpeed = 0.65;
    this.controls.minDistance = Math.max(
      5,
      Math.min(8, this.boardDiagonal * 0.55),
    );
    this.controls.maxDistance = Math.max(20, this.boardDiagonal * 2);
    this.controls.minPolarAngle = Math.PI * 0.2;
    this.controls.maxPolarAngle = Math.PI * 0.43;
    this.controls.target.set(0, 0, 0);
    this.controls.update();
    this._onControlsChange = () => {
      if (!this.virtualized || this.destroyed) return;
      const tileMinimumX = Math.ceil(this.mapBounds.minX);
      const tileMinimumY = Math.ceil(this.mapBounds.minY);
      const visibleBounds = this._visibleGroundBounds();
      const key = visibleBounds
        ? [
            Math.floor(
              (visibleBounds.minX - tileMinimumX) / this.renderChunkSize,
            ),
            Math.floor(
              (visibleBounds.maxX - tileMinimumX) / this.renderChunkSize,
            ),
            Math.floor(
              (visibleBounds.minY - tileMinimumY) / this.renderChunkSize,
            ),
            Math.floor(
              (visibleBounds.maxY - tileMinimumY) / this.renderChunkSize,
            ),
          ].join(",")
        : "strategic";
      if (key === this.viewportWindowKey) return;
      this.viewportWindowKey = key;
      if (this.viewportChangeFrame) clearTimeout(this.viewportChangeFrame);
      // Orbiting changes the projected chunk bounds every few frames on large
      // maps. Wait briefly for camera motion to settle so those boundary
      // crossings produce one streamed-tile refresh instead of repeated full
      // battlefield renders during the gesture.
      this.viewportChangeFrame = setTimeout(() => {
        this.viewportChangeFrame = null;
        this.onViewportChange?.();
      }, 90);
    };
    this.controls.addEventListener("change", this._onControlsChange);

    this.boardGroup = new THREE.Group();
    this.routeGroup = new THREE.Group();
    this.rangeRingGroup = new THREE.Group();
    this.debugLabelGroup = new THREE.Group();
    this.vegetationGroup = new THREE.Group();
    this.pieceGroup = new THREE.Group();
    this.effectGroup = new THREE.Group();
    this.interactionGroup = new THREE.Group();
    this.placementPreviewGroup = new THREE.Group();
    this.fogGroup = new THREE.Group();
    this.scene.add(
      this.boardGroup,
      this.routeGroup,
      this.rangeRingGroup,
      this.debugLabelGroup,
      this.vegetationGroup,
      this.pieceGroup,
      this.effectGroup,
      this.interactionGroup,
      this.placementPreviewGroup,
      this.fogGroup,
    );

    this._addLights();
    this._buildBoard();
    this._buildHoverReticle();

    this._onPointerMove = this._onPointerMove.bind(this);
    this._onPointerLeave = this._onPointerLeave.bind(this);
    this._onPointerDown = this._onPointerDown.bind(this);
    this._onPointerUp = this._onPointerUp.bind(this);
    this.renderer.domElement.addEventListener(
      "pointermove",
      this._onPointerMove,
      true,
    );
    this.renderer.domElement.addEventListener(
      "pointerleave",
      this._onPointerLeave,
    );
    this.renderer.domElement.addEventListener(
      "pointerdown",
      this._onPointerDown,
      true,
    );
    this.renderer.domElement.addEventListener(
      "pointerup",
      this._onPointerUp,
      true,
    );

    this.resizeObserver = new ResizeObserver(() => this._resize());
    this.resizeObserver.observe(container);
    this._resize();
    this._renderLoop();
  }

  _addLights() {
    // A soft ambient floor keeps large instanced battlefields readable even
    // when the directional light reaches them at a very shallow camera angle.
    const ambient = new THREE.AmbientLight(0xa8b5a1, 0.72);
    this.scene.add(ambient);

    const hemisphere = new THREE.HemisphereLight(0xc8d5bf, 0x182019, 1.55);
    this.scene.add(hemisphere);

    const key = new THREE.DirectionalLight(0xf4edcf, 2.8);
    const shadowExtent = Math.max(this.cols, this.rows) / 2 + 2;
    key.position.set(
      -this.cols * 0.35,
      Math.max(12, this.boardDiagonal * 0.85),
      this.rows * 0.85,
    );
    key.castShadow = !this.largeScale;
    key.shadow.mapSize.set(
      this.largeScale ? 512 : 2048,
      this.largeScale ? 512 : 2048,
    );
    key.shadow.camera.left = -shadowExtent;
    key.shadow.camera.right = shadowExtent;
    key.shadow.camera.top = shadowExtent;
    key.shadow.camera.bottom = -shadowExtent;
    key.shadow.camera.far = Math.max(35, this.boardDiagonal * 2.5);
    key.shadow.bias = -0.0004;
    this.scene.add(key);

    const rim = new THREE.DirectionalLight(0x8e9e87, 1.05);
    rim.position.set(8, 5, -7);
    this.scene.add(rim);
  }

  _buildBoard() {
    const base = new THREE.Mesh(
      new THREE.BoxGeometry(this.cols + 0.55, 0.34, this.rows + 0.55),
      new THREE.MeshStandardMaterial({
        color: COLORS.base,
        roughness: 0.94,
        metalness: 0.02,
      }),
    );
    base.position.y = -0.23;
    base.receiveShadow = true;
    this.boardGroup.add(base);

    const rim = new THREE.Mesh(
      new THREE.BoxGeometry(this.cols + 0.75, 0.12, this.rows + 0.75),
      new THREE.MeshStandardMaterial({
        color: 0x66705e,
        roughness: 0.7,
        metalness: 0.25,
      }),
    );
    rim.position.y = -0.11;
    this.boardGroup.add(rim);
    base.renderOrder = 1;

    if (this.virtualized) {
      this._buildVirtualTileLayer();
    } else {
      const geometryCache = new Map();
      const instanceGroups = new Map();
      for (const descriptor of this.tileDescriptors) {
        const localVertices = descriptor.vertices.map((vertex) => ({
          x: vertex.x - descriptor.center.x,
          y: vertex.y - descriptor.center.y,
        }));
        const geometryKey = localVertices
          .map(({ x, y }) => `${x.toFixed(5)},${y.toFixed(5)}`)
          .join(";");
        let tileGeometry = geometryCache.get(geometryKey);
        if (!tileGeometry) {
          const contour = localVertices.map(
            ({ x, y }) => new THREE.Vector2(x, y),
          );
          const faces = THREE.ShapeUtils.triangulateShape(contour, []);
          tileGeometry = new THREE.BufferGeometry();
          tileGeometry.setAttribute(
            "position",
            new THREE.Float32BufferAttribute(
              localVertices.flatMap(({ x, y }) => [x, 0, y]),
              3,
            ),
          );
          tileGeometry.setAttribute(
            "normal",
            new THREE.Float32BufferAttribute(
              localVertices.flatMap(() => [0, 1, 0]),
              3,
            ),
          );
          const minX = Math.min(...localVertices.map(({ x }) => x));
          const maxX = Math.max(...localVertices.map(({ x }) => x));
          const minY = Math.min(...localVertices.map(({ y }) => y));
          const maxY = Math.max(...localVertices.map(({ y }) => y));
          const width = Math.max(maxX - minX, 1e-6);
          const height = Math.max(maxY - minY, 1e-6);
          tileGeometry.setAttribute(
            "uv",
            new THREE.Float32BufferAttribute(
              localVertices.flatMap(({ x, y }) => [
                (x - minX) / width,
                (y - minY) / height,
              ]),
              2,
            ),
          );
          tileGeometry.setIndex(
            faces.flatMap(([first, second, third]) => [first, third, second]),
          );
          tileGeometry.computeBoundingBox();
          tileGeometry.computeBoundingSphere();
          geometryCache.set(geometryKey, tileGeometry);
        }

        const groupKey = geometryKey;
        const group = instanceGroups.get(groupKey) ?? {
          geometry: tileGeometry,
          descriptors: [],
        };
        group.descriptors.push(descriptor);
        instanceGroups.set(groupKey, group);
      }

      for (const group of instanceGroups.values()) {
        const material = this._createTerrainMaterial();
        const instances = new THREE.InstancedMesh(
          group.geometry,
          material,
          group.descriptors.length,
        );
        instances.receiveShadow = true;
        instances.userData.kind = "tile-instances";
        instances.userData.instanceTiles = [];
        group.descriptors.forEach((descriptor, instanceId) => {
          const position = this._cellPosition(descriptor.x, descriptor.y);
          this.tileTransform.position.set(position.x, 0.06, position.z);
          this.tileTransform.scale.setScalar(1);
          this.tileTransform.updateMatrix();
          instances.setMatrixAt(instanceId, this.tileTransform.matrix);
          instances.setColorAt(instanceId, new THREE.Color(0xffffff));
          const tile = {
            x: descriptor.x,
            y: descriptor.y,
            descriptor,
            mesh: instances,
            instanceId,
            state: null,
          };
          instances.userData.instanceTiles[instanceId] = tile;
          this.tileByKey.set(`${descriptor.x},${descriptor.y}`, tile);
        });
        instances.instanceMatrix.setUsage(THREE.DynamicDrawUsage);
        instances.instanceColor?.setUsage(THREE.DynamicDrawUsage);
        this.tileMeshes.push(instances);
        this.boardGroup.add(instances);
      }

      // Navigation polygons remain available for rules and ray picking, but
      // their border geometry is deliberately not rendered on the terrain.
    }

    // Fog is one continuous, elevated cloud sheet for every map size. The
    // gameplay visibility mask cuts circular holes through it.
    this._buildVirtualFogLayer();

    this._buildTerrainSkirt();

    const blueEdge = new THREE.Mesh(
      new THREE.BoxGeometry(0.07, 0.035, this.rows + 0.3),
      new THREE.MeshBasicMaterial({ color: COLORS.player }),
    );
    blueEdge.position.set(-this.cols / 2 - 0.2, 0.09, 0);
    this.boardGroup.add(blueEdge);
    const redEdge = blueEdge.clone();
    redEdge.material = new THREE.MeshBasicMaterial({ color: COLORS.enemy });
    redEdge.position.x *= -1;
    this.boardGroup.add(redEdge);
  }

  _buildTerrainSkirt() {
    const boundaryEdges = this._terrainBoundaryEdges();
    if (!boundaryEdges.length) return;
    const positions = [];
    const colors = [];
    const indices = [];
    const bottomY = -0.085;
    const surfaceBaseY = this.virtualized ? 0.02 : 0.06;
    const bottomColor = new THREE.Color(0x252a22);

    const appendColor = (color) => colors.push(color.r, color.g, color.b);
    const colorAt = (point) => {
      const [red, green, blue] = terrainColor(
        this.terrainSampler(point.x, point.y),
        { debug: this.terrainDebug },
      );
      return new THREE.Color(red / 255, green / 255, blue / 255)
        .convertSRGBToLinear()
        .lerp(bottomColor, 0.28);
    };

    for (const edge of boundaryEdges) {
      const offset = positions.length / 3;
      const startTopY =
        surfaceBaseY +
        this._terrainHeightAtMapPoint(edge.start.x, edge.start.y);
      const endTopY =
        surfaceBaseY + this._terrainHeightAtMapPoint(edge.end.x, edge.end.y);
      positions.push(
        edge.start.x - this.mapCenter.x,
        startTopY,
        edge.start.y - this.mapCenter.y,
        edge.end.x - this.mapCenter.x,
        endTopY,
        edge.end.y - this.mapCenter.y,
        edge.end.x - this.mapCenter.x,
        bottomY,
        edge.end.y - this.mapCenter.y,
        edge.start.x - this.mapCenter.x,
        bottomY,
        edge.start.y - this.mapCenter.y,
      );
      appendColor(colorAt(edge.start));
      appendColor(colorAt(edge.end));
      appendColor(bottomColor);
      appendColor(bottomColor);
      indices.push(
        offset,
        offset + 2,
        offset + 1,
        offset,
        offset + 3,
        offset + 2,
      );
    }

    const geometry = new THREE.BufferGeometry();
    geometry.setAttribute(
      "position",
      new THREE.Float32BufferAttribute(positions, 3),
    );
    geometry.setAttribute("color", new THREE.Float32BufferAttribute(colors, 3));
    geometry.setIndex(indices);
    geometry.computeVertexNormals();
    geometry.computeBoundingBox();
    geometry.computeBoundingSphere();
    const skirt = new THREE.Mesh(
      geometry,
      new THREE.MeshStandardMaterial({
        color: 0xffffff,
        vertexColors: true,
        roughness: 0.98,
        metalness: 0,
        side: THREE.DoubleSide,
      }),
    );
    skirt.receiveShadow = true;
    skirt.raycast = () => {};
    skirt.renderOrder = 2;
    this.terrainSkirt = skirt;
    this.boardGroup.add(skirt);
  }

  _terrainBoundaryEdges() {
    if (this.virtualized || !this.tileDescriptors?.length) {
      const { minX, maxX, minY, maxY } = this.mapBounds;
      const edges = [];
      const appendSide = (start, end, segments) => {
        for (let index = 0; index < segments; index += 1) {
          const first = index / segments;
          const second = (index + 1) / segments;
          edges.push({
            start: {
              x: THREE.MathUtils.lerp(start.x, end.x, first),
              y: THREE.MathUtils.lerp(start.y, end.y, first),
            },
            end: {
              x: THREE.MathUtils.lerp(start.x, end.x, second),
              y: THREE.MathUtils.lerp(start.y, end.y, second),
            },
          });
        }
      };
      appendSide(
        { x: minX, y: minY },
        { x: maxX, y: minY },
        Math.max(1, Math.ceil(this.cols)),
      );
      appendSide(
        { x: maxX, y: minY },
        { x: maxX, y: maxY },
        Math.max(1, Math.ceil(this.rows)),
      );
      appendSide(
        { x: maxX, y: maxY },
        { x: minX, y: maxY },
        Math.max(1, Math.ceil(this.cols)),
      );
      appendSide(
        { x: minX, y: maxY },
        { x: minX, y: minY },
        Math.max(1, Math.ceil(this.rows)),
      );
      return edges;
    }

    // Group collinear tile edges, split them at every overlap boundary, and
    // retain only segments covered by one tile. This supports square, hex, and
    // mixed polygon maps without building internal walls at partial edges.
    const epsilon = 1e-5;
    const quantize = (value) => Math.round(value / epsilon);
    const lines = new Map();
    for (const descriptor of this.tileDescriptors) {
      for (let index = 0; index < descriptor.vertices.length; index += 1) {
        const start = descriptor.vertices[index];
        const end =
          descriptor.vertices[(index + 1) % descriptor.vertices.length];
        let directionX = end.x - start.x;
        let directionY = end.y - start.y;
        const length = Math.hypot(directionX, directionY);
        if (length <= epsilon) continue;
        directionX /= length;
        directionY /= length;
        if (
          directionX < -epsilon ||
          (Math.abs(directionX) <= epsilon && directionY < 0)
        ) {
          directionX *= -1;
          directionY *= -1;
        }
        const normalX = -directionY;
        const normalY = directionX;
        const lineOffset = normalX * start.x + normalY * start.y;
        const first = directionX * start.x + directionY * start.y;
        const second = directionX * end.x + directionY * end.y;
        const key = `${quantize(directionX)}:${quantize(directionY)}:${quantize(lineOffset)}`;
        const line = lines.get(key) ?? {
          directionX,
          directionY,
          normalX,
          normalY,
          lineOffset,
          spans: [],
        };
        line.spans.push({
          start: Math.min(first, second),
          end: Math.max(first, second),
        });
        lines.set(key, line);
      }
    }

    const boundaryEdges = [];
    for (const line of lines.values()) {
      const cutByKey = new Map();
      for (const span of line.spans) {
        if (!cutByKey.has(quantize(span.start)))
          cutByKey.set(quantize(span.start), span.start);
        if (!cutByKey.has(quantize(span.end)))
          cutByKey.set(quantize(span.end), span.end);
      }
      const cuts = [...cutByKey.values()].sort(
        (first, second) => first - second,
      );
      for (let index = 0; index < cuts.length - 1; index += 1) {
        const first = cuts[index];
        const second = cuts[index + 1];
        if (second - first <= epsilon) continue;
        const midpoint = (first + second) / 2;
        const coverage = line.spans.filter(
          (span) =>
            midpoint >= span.start - epsilon && midpoint <= span.end + epsilon,
        ).length;
        if (coverage !== 1) continue;
        const pointAt = (distance) => ({
          x: line.directionX * distance + line.normalX * line.lineOffset,
          y: line.directionY * distance + line.normalY * line.lineOffset,
        });
        boundaryEdges.push({ start: pointAt(first), end: pointAt(second) });
      }
    }
    return boundaryEdges;
  }

  _buildVirtualTileLayer() {
    // Tiles remain the gameplay/picking units, but each visual tile has a
    // sub-cell mesh so continuous land noise is not reduced to two triangles.
    // Full-size geometry also removes the old 3.5% gap between adjacent cells.
    const geometry = new THREE.PlaneGeometry(1, 1, 8, 8);
    geometry.rotateX(-Math.PI / 2);
    geometry.computeBoundingBox();
    geometry.computeBoundingSphere();
    const material = this._createTerrainMaterial();
    const instances = new THREE.InstancedMesh(
      geometry,
      material,
      this.tileCapacity,
    );
    instances.count = 0;
    instances.receiveShadow = !this.largeScale;
    instances.frustumCulled = false;
    instances.userData.kind = "virtual-tile-instances";
    instances.userData.instanceTiles = [];
    instances.instanceMatrix.setUsage(THREE.DynamicDrawUsage);
    this.virtualTileMesh = instances;
    this.tileMeshes.push(instances);
    this.boardGroup.add(instances);

    const rangeSources = Array.from(
      { length: MAX_VIRTUAL_RANGE_SOURCES },
      () => new THREE.Vector3(),
    );
    // This is the strategic proxy for the entire map. Terrain tone, fog,
    // adaptive grid lines, and range circles are composited in one opaque
    // fragment. No coplanar transparent surfaces remain to z-fight.
    const surface = new THREE.Mesh(
      new THREE.PlaneGeometry(
        this.cols,
        this.rows,
        Math.min(512, Math.max(1, Math.ceil(this.cols * 4))),
        Math.min(512, Math.max(1, Math.ceil(this.rows * 4))),
      ),
      new THREE.ShaderMaterial({
        uniforms: {
          boardSize: { value: new THREE.Vector2(this.cols, this.rows) },
          mapOrigin: {
            value: new THREE.Vector2(this.mapBounds.minX, this.mapBounds.minY),
          },
          fogColor: { value: new THREE.Color(COLORS.fog) },
          terrainMap: { value: this._getProceduralTerrainTexture() },
          terrainHeightMap: {
            value: this._getProceduralTerrainHeightTexture(),
          },
          terrainElevationScale: { value: this.terrainElevationScale },
          gridEnabled: { value: false },
          gridStep: { value: 1 },
          gridColor: { value: new THREE.Color(0x263127) },
          gridOpacity: { value: 0.7 },
          lineHalfWidthPixels: {
            value: VIRTUAL_GRID_QUALITY.lineHalfWidthPixels,
          },
          densityFadeStart: { value: VIRTUAL_GRID_QUALITY.densityFadeStart },
          densityFadeEnd: { value: VIRTUAL_GRID_QUALITY.densityFadeEnd },
          fogMask: { value: null },
          fogEnabled: { value: false },
          rangeSourceCount: { value: 0 },
          rangeSources: { value: rangeSources },
          rangeColor: { value: new THREE.Color(COLORS.lime) },
          rangeFillAlpha: { value: 0 },
          rangeBoundaryAlpha: { value: 0 },
          rangeBoundaryWidth: {
            value: RANGE_OVERLAY_QUALITY.boundaryWidthInTiles,
          },
          rangeGridBoost: { value: 0 },
        },
        vertexShader: `
          uniform vec2 boardSize;
          uniform vec2 mapOrigin;
          uniform sampler2D terrainHeightMap;
          uniform float terrainElevationScale;
          varying vec2 gridCoordinate;
          varying vec2 mapCoordinate;
          varying vec2 gridUv;
          varying float terrainElevation;
          void main() {
            gridCoordinate = uv * boardSize;
            // Rotating PlaneGeometry onto XZ reverses its V/world-Z axis.
            // Logical map Y therefore runs from max to min across UV V.
            mapCoordinate = vec2(
              mapOrigin.x + gridCoordinate.x,
              mapOrigin.y + boardSize.y - gridCoordinate.y
            );
            gridUv = uv;
            terrainElevation = texture2D(terrainHeightMap, uv).r;
            vec3 elevatedPosition = position +
              normal * terrainElevation * terrainElevationScale;
            gl_Position = projectionMatrix * modelViewMatrix *
              vec4(elevatedPosition, 1.0);
          }
        `,
        fragmentShader: `
          #define MAX_RANGE_SOURCES ${MAX_VIRTUAL_RANGE_SOURCES}
          uniform vec3 fogColor;
          uniform sampler2D terrainMap;
          uniform bool gridEnabled;
          uniform float gridStep;
          uniform vec3 gridColor;
          uniform float gridOpacity;
          uniform float lineHalfWidthPixels;
          uniform float densityFadeStart;
          uniform float densityFadeEnd;
          uniform sampler2D fogMask;
          uniform bool fogEnabled;
          uniform int rangeSourceCount;
          uniform vec3 rangeSources[MAX_RANGE_SOURCES];
          uniform vec3 rangeColor;
          uniform float rangeFillAlpha;
          uniform float rangeBoundaryAlpha;
          uniform float rangeBoundaryWidth;
          uniform float rangeGridBoost;
          varying vec2 gridCoordinate;
          varying vec2 mapCoordinate;
          varying vec2 gridUv;
          varying float terrainElevation;

          void main() {
            float fogAmount = 0.0;
            if (fogEnabled) {
              float fogAlpha = texture2D(fogMask, gridUv).a;
              fogAmount = smoothstep(0.02, 0.82, fogAlpha);
            }
            float fogReveal = 1.0 - fogAmount;

            // One map-space texture spans the whole board. Biome and river
            // boundaries therefore continue underneath the tactical grid.
            vec3 revealedSurface = texture2D(terrainMap, gridUv).rgb;
            vec3 color = revealedSurface * mix(0.95, 1.045, terrainElevation);
            color = mix(color, fogColor, fogAmount);

            float gridStrength = 0.0;
            if (gridEnabled) {
              vec2 scaled = gridCoordinate / gridStep;
              vec2 distanceToLine = abs(fract(scaled + 0.5) - 0.5);
              vec2 pixelDerivative = max(fwidth(scaled), vec2(0.0001));
              vec2 pixelDistance = distanceToLine / pixelDerivative;
              vec2 line = 1.0 - smoothstep(
                vec2(0.0),
                vec2(lineHalfWidthPixels),
                pixelDistance
              );
              vec2 densityFade = 1.0 - smoothstep(
                vec2(densityFadeStart),
                vec2(densityFadeEnd),
                pixelDerivative
              );
              gridStrength = max(
                line.x * densityFade.x,
                line.y * densityFade.y
              );
              color = mix(
                color,
                gridColor,
                gridStrength * gridOpacity * fogReveal
              );
            }

            float rangeCoverage = 0.0;
            float rangeBoundary = 0.0;
            for (int index = 0; index < MAX_RANGE_SOURCES; index++) {
              if (index >= rangeSourceCount) break;
              vec3 source = rangeSources[index];
              float distanceToSource = distance(mapCoordinate, source.xy);
              float edgeSoftness = max(fwidth(distanceToSource) * 1.25, 0.015);
              float coverage = 1.0 - smoothstep(
                source.z - edgeSoftness,
                source.z + edgeSoftness,
                distanceToSource
              );
              float boundary = 1.0 - smoothstep(
                rangeBoundaryWidth,
                rangeBoundaryWidth + edgeSoftness,
                abs(distanceToSource - source.z)
              );
              rangeCoverage = max(rangeCoverage, coverage);
              rangeBoundary = max(rangeBoundary, boundary);
            }
            float rangeAlpha = max(
              rangeCoverage * rangeFillAlpha,
              rangeBoundary * rangeBoundaryAlpha
            );
            color = mix(color, rangeColor, clamp(rangeAlpha, 0.0, 1.0));
            // Construction mode strengthens the actual grid lines within its
            // valid-range wash, keeping individual placement cells readable.
            color = mix(
              color,
              rangeColor,
              clamp(
                gridStrength * rangeCoverage * rangeGridBoost,
                0.0,
                1.0
              )
            );
            gl_FragColor = vec4(color, 1.0);
            // ShaderMaterial does not add output conversion automatically.
            // Convert the linear palette to the renderer's display colour
            // space so the strategic proxy matches the detailed tile layer.
            #include <colorspace_fragment>
          }
        `,
        transparent: false,
        depthTest: true,
        depthWrite: true,
        side: THREE.DoubleSide,
        toneMapped: false,
      }),
    );
    surface.rotation.x = -Math.PI / 2;
    surface.position.y = 0.02;
    surface.renderOrder = 2;
    surface.raycast = () => {};
    this.virtualGrid = surface;
    this.virtualSurface = surface;
    this.virtualRangeSources = rangeSources;
    this.boardGroup.add(surface);
  }

  _buildVirtualFogLayer() {
    // The canvas is only a one-channel-style visibility mask sampled by the
    // strategic surface. It does not create another mesh or depth layer.
    const longestSide = Math.max(this.cols, this.rows, 1);
    const hardwareTextureLimit =
      this.renderer.capabilities.maxTextureSize ||
      VIRTUAL_FOG_QUALITY.maximumTextureSize;
    const longestTextureSide = Math.min(
      VIRTUAL_FOG_QUALITY.maximumTextureSize,
      hardwareTextureLimit,
      Math.max(
        VIRTUAL_FOG_QUALITY.minimumTextureSize,
        Math.ceil(longestSide * VIRTUAL_FOG_QUALITY.pixelsPerTile),
      ),
    );
    const width = Math.max(
      64,
      Math.round((this.cols / longestSide) * longestTextureSide),
    );
    const height = Math.max(
      64,
      Math.round((this.rows / longestSide) * longestTextureSide),
    );
    const canvas = document.createElement("canvas");
    canvas.width = width;
    canvas.height = height;
    const texture = new THREE.CanvasTexture(canvas);
    texture.minFilter = THREE.LinearFilter;
    texture.magFilter = THREE.LinearFilter;
    texture.generateMipmaps = false;
    this.virtualFogCanvas = canvas;
    this.virtualFogTexture = texture;
    const gridUniforms = this.virtualGrid?.material?.uniforms;
    if (gridUniforms?.fogMask) gridUniforms.fogMask.value = texture;
    if (this.deferFogClouds) return;
    const maximumClouds = 30000;
    const spacing = Math.max(
      3,
      Math.sqrt((this.cols * this.rows) / maximumClouds),
    );
    this.fogCloudSpacing = spacing;
    this.fogCloudPlacements = [];
    let row = 0;
    for (
      let mapY = this.mapBounds.minY + spacing / 2;
      mapY < this.mapBounds.maxY;
      mapY += spacing
    ) {
      let column = 0;
      for (
        let mapX = this.mapBounds.minX + spacing / 2;
        mapX < this.mapBounds.maxX;
        mapX += spacing
      ) {
        this.fogCloudPlacements.push({
          x: mapX + (this._treeNoise(column, row, 61) - 0.5) * spacing * 0.42,
          y: mapY + (this._treeNoise(column, row, 67) - 0.5) * spacing * 0.42,
          height: this._treeNoise(column, row, 71),
          scale: 0.82 + this._treeNoise(column, row, 73) * 0.42,
          rotation: this._treeNoise(column, row, 79) * Math.PI * 2,
        });
        column += 1;
      }
      row += 1;
    }
    this.virtualFogMesh = new THREE.InstancedMesh(
      new THREE.IcosahedronGeometry(1, 0),
      new THREE.MeshStandardMaterial({
        color: 0xf2f4ef,
        roughness: 1,
        metalness: 0,
        transparent: true,
        opacity: 0.93,
        depthWrite: false,
      }),
      this.fogCloudPlacements.length,
    );
    this.virtualFogMesh.count = 0;
    this.virtualFogMesh.frustumCulled = false;
    this.virtualFogMesh.castShadow = false;
    this.virtualFogMesh.receiveShadow = false;
    this.virtualFogMesh.renderOrder = 80;
    this.virtualFogMesh.raycast = () => {};
    this.fogGroup.add(this.virtualFogMesh);
    if (gridUniforms?.fogMask) gridUniforms.fogMask.value = texture;
  }

  enableFogClouds() {
    if (!this.deferFogClouds) return;
    this.deferFogClouds = false;
    this.virtualFogTexture?.dispose();
    this._buildVirtualFogLayer();
    this.virtualFogSignature = "";
  }

  enableVegetation() {
    if (!this.deferVegetation) return;
    this.deferVegetation = false;
    this._updateTrees();
  }

  getViewState() {
    return {
      position: this.camera.position.toArray(),
      target: this.controls.target.toArray(),
    };
  }

  restoreViewState(viewState) {
    if (!viewState?.position || !viewState?.target) return;
    this.camera.position.fromArray(viewState.position);
    this.controls.target.fromArray(viewState.target);
    this.controls.update();
  }

  _updateVirtualFog(fog) {
    const enabled = Boolean(fog?.enabled);
    if (!this.virtualFogCanvas || !this.virtualFogTexture) return;
    const gridUniforms = this.virtualGrid?.material?.uniforms;
    // The mask drives gameplay visibility and cloud-instance removal only;
    // it is no longer drawn as a flat tint on the terrain surface.
    if (gridUniforms?.fogEnabled) gridUniforms.fogEnabled.value = false;
    if (this.virtualFogMesh) this.virtualFogMesh.visible = enabled;
    if (!enabled) {
      this.virtualFogSignature = "disabled";
      return;
    }

    const sources = (fog.sources ?? [])
      .filter(
        (source) =>
          Number.isFinite(source.x) &&
          Number.isFinite(source.y) &&
          Number.isFinite(source.radius) &&
          source.radius >= 0,
      )
      .map((source) => ({
        x: Number(source.x),
        y: Number(source.y),
        radius: Number(source.radius),
      }));
    const signature = JSON.stringify(sources);
    if (signature === this.virtualFogSignature) return;
    this.virtualFogSignature = signature;
    this._updateFogCloudInstances(sources);

    const canvas = this.virtualFogCanvas;
    const context = canvas.getContext("2d");
    if (!context) return;
    context.globalCompositeOperation = "source-over";
    context.clearRect(0, 0, canvas.width, canvas.height);
    context.fillStyle = "rgba(205, 215, 190, 0.88)";
    context.fillRect(0, 0, canvas.width, canvas.height);

    context.globalCompositeOperation = "destination-out";
    for (const source of sources) {
      const centerX =
        ((source.x - this.mapBounds.minX) / this.cols) * canvas.width;
      // PlaneGeometry maps canvas top (UV v=1) to the map's minimum Y after
      // rotating onto XZ, so map Y and canvas pixel Y increase together.
      const centerY =
        ((source.y - this.mapBounds.minY) / this.rows) * canvas.height;
      const radiusX = Math.max(
        0.75,
        (source.radius / this.cols) * canvas.width,
      );
      const radiusY = Math.max(
        0.75,
        (source.radius / this.rows) * canvas.height,
      );
      context.beginPath();
      context.ellipse(centerX, centerY, radiusX, radiusY, 0, 0, Math.PI * 2);
      context.fill();
    }
    context.globalCompositeOperation = "source-over";
    this.virtualFogTexture.needsUpdate = true;
  }

  _updateFogCloudInstances(sources) {
    if (!this.virtualFogMesh || !this.fogCloudPlacements) return;
    const transform = new THREE.Object3D();
    const spacing = this.fogCloudSpacing ?? 3;
    let instance = 0;
    for (const cloud of this.fogCloudPlacements) {
      const revealed = sources.some(
        (source) =>
          Math.hypot(cloud.x - source.x, cloud.y - source.y) <=
          source.radius + spacing * 0.62,
      );
      if (revealed) continue;
      transform.position.set(
        cloud.x - this.mapCenter.x,
        this.terrainElevationScale + 0.5 + cloud.height * 0.72,
        cloud.y - this.mapCenter.y,
      );
      transform.rotation.set(0, cloud.rotation, 0);
      transform.scale.set(
        spacing * 0.7 * cloud.scale,
        0.5 + cloud.height * 0.34,
        spacing * 0.66 * cloud.scale,
      );
      transform.updateMatrix();
      this.virtualFogMesh.setMatrixAt(instance, transform.matrix);
      instance += 1;
    }
    this.virtualFogMesh.count = instance;
    this.virtualFogMesh.instanceMatrix.needsUpdate = true;
  }

  _updateVirtualRange(rangeOverlay) {
    const uniforms = this.virtualGrid?.material?.uniforms;
    if (!uniforms?.rangeSourceCount || !this.virtualRangeSources) return;
    const kind = rangeOverlay?.kind ?? null;
    const sources = (rangeOverlay?.sources ?? []).filter(
      (source) =>
        Number.isFinite(source.x) &&
        Number.isFinite(source.y) &&
        Number.isFinite(source.radius) &&
        source.radius > 0,
    );
    const enabled = Boolean(kind && sources.length);
    const signature = enabled ? JSON.stringify({ kind, sources }) : "disabled";
    if (signature === this.virtualRangeSignature) return;
    this.virtualRangeSignature = signature;
    if (!enabled) {
      uniforms.rangeSourceCount.value = 0;
      uniforms.rangeFillAlpha.value = 0;
      uniforms.rangeBoundaryAlpha.value = 0;
      uniforms.rangeGridBoost.value = 0;
      return;
    }

    const palette = {
      available: 0xd2c779,
      reachable: 0xa6be7a,
      targetable: 0xd9654f,
    };
    const movementOverlay = kind === "reachable";
    const constructionOverlay = kind === "available";
    uniforms.rangeColor.value.set(palette[kind] ?? palette.reachable);
    uniforms.rangeFillAlpha.value = movementOverlay
      ? RANGE_OVERLAY_QUALITY.movementFillAlpha
      : constructionOverlay
        ? RANGE_OVERLAY_QUALITY.constructionFillAlpha
        : RANGE_OVERLAY_QUALITY.fillAlpha;
    uniforms.rangeBoundaryAlpha.value = movementOverlay
      ? RANGE_OVERLAY_QUALITY.movementBoundaryAlpha
      : constructionOverlay
        ? RANGE_OVERLAY_QUALITY.constructionBoundaryAlpha
        : RANGE_OVERLAY_QUALITY.boundaryAlpha;
    uniforms.rangeBoundaryWidth.value = movementOverlay
      ? RANGE_OVERLAY_QUALITY.movementBoundaryWidthInTiles
      : constructionOverlay
        ? RANGE_OVERLAY_QUALITY.constructionBoundaryWidthInTiles
        : RANGE_OVERLAY_QUALITY.boundaryWidthInTiles;
    uniforms.rangeGridBoost.value = constructionOverlay
      ? RANGE_OVERLAY_QUALITY.constructionGridBoost
      : 0;
    const sourceCount = Math.min(sources.length, MAX_VIRTUAL_RANGE_SOURCES);
    for (let index = 0; index < sourceCount; index += 1) {
      const source = sources[index];
      this.virtualRangeSources[index].set(
        Number(source.x),
        Number(source.y),
        Number(source.radius),
      );
    }
    uniforms.rangeSourceCount.value = sourceCount;
  }

  _cellPosition(x, y) {
    const descriptor = this.tileDescriptorByKey.get(`${x},${y}`);
    if (descriptor) {
      const height = this._terrainHeightAtMapPoint(
        descriptor.center.x,
        descriptor.center.y,
      );
      return new THREE.Vector3(
        descriptor.center.x - this.mapCenter.x,
        0.12 + height,
        descriptor.center.y - this.mapCenter.y,
      );
    }
    const height = this._terrainHeightAtMapPoint(x, y);
    return new THREE.Vector3(
      x - (this.cols - 1) / 2,
      0.12 + height,
      y - (this.rows - 1) / 2,
    );
  }

  _mapPosition(x, y, worldX = null, worldY = null) {
    if (Number.isFinite(worldX) && Number.isFinite(worldY)) {
      return new THREE.Vector3(
        worldX - this.mapCenter.x,
        0.12 + this._terrainHeightAtMapPoint(worldX, worldY),
        worldY - this.mapCenter.y,
      );
    }
    return this._cellPosition(x, y);
  }

  _normalizeTerrainFlatteningAreas(areas = []) {
    return (Array.isArray(areas) ? areas : [])
      .filter(
        (area) =>
          Number.isFinite(area?.minX) &&
          Number.isFinite(area?.maxX) &&
          Number.isFinite(area?.minY) &&
          Number.isFinite(area?.maxY) &&
          Number.isFinite(area?.targetElevation),
      )
      .map((area) => ({
        id: String(area.id ?? ""),
        minX: Math.min(area.minX, area.maxX),
        maxX: Math.max(area.minX, area.maxX),
        minY: Math.min(area.minY, area.maxY),
        maxY: Math.max(area.minY, area.maxY),
        targetElevation: THREE.MathUtils.clamp(area.targetElevation, 0, 1),
        edge: Number.isFinite(area.edge) ? Math.max(0, area.edge) : 0.75,
      }))
      .sort((first, second) => first.id.localeCompare(second.id));
  }

  _setTerrainFlatteningAreas(areas = [], { refresh = true } = {}) {
    const normalized = this._normalizeTerrainFlatteningAreas(areas);
    const signature = JSON.stringify(normalized);
    if (signature === this.terrainFlatteningSignature) return;
    const previous = this.terrainFlatteningAreas;
    this.terrainFlatteningAreas = normalized;
    this.terrainFlatteningSignature = signature;
    if (!refresh) return;

    const previousKeys = new Set(previous.map((area) => JSON.stringify(area)));
    const nextKeys = new Set(normalized.map((area) => JSON.stringify(area)));
    const changedAreas = [
      ...previous.filter((area) => !nextKeys.has(JSON.stringify(area))),
      ...normalized.filter((area) => !previousKeys.has(JSON.stringify(area))),
    ];
    if (
      this.proceduralTerrainCanvas &&
      this.proceduralTerrainHeightCanvas &&
      this.proceduralTerrainTexture &&
      this.proceduralTerrainHeightTexture
    )
      this._updateProceduralTerrainRegions(changedAreas);
    else this._getProceduralTerrainTexture();
    this.dynamicModelSignature = "";
    this.routeModelSignature = "";
  }

  _updateProceduralTerrainRegions(areas) {
    if (!areas.length) return;
    const colorCanvas = this.proceduralTerrainCanvas;
    const heightCanvas = this.proceduralTerrainHeightCanvas;
    const colorContext = colorCanvas.getContext("2d");
    const heightContext = heightCanvas.getContext("2d");
    const spanX = this.mapBounds.maxX - this.mapBounds.minX;
    const spanY = this.mapBounds.maxY - this.mapBounds.minY;
    for (const area of areas) {
      const padding = (area.edge ?? 0) + 0.2;
      const minPixelX = THREE.MathUtils.clamp(
        Math.floor(
          ((area.minX - padding - this.mapBounds.minX) / spanX) *
            colorCanvas.width,
        ) - 1,
        0,
        colorCanvas.width - 1,
      );
      const maxPixelX = THREE.MathUtils.clamp(
        Math.ceil(
          ((area.maxX + padding - this.mapBounds.minX) / spanX) *
            colorCanvas.width,
        ) + 1,
        minPixelX + 1,
        colorCanvas.width,
      );
      const minPixelY = THREE.MathUtils.clamp(
        Math.floor(
          ((area.minY - padding - this.mapBounds.minY) / spanY) *
            colorCanvas.height,
        ) - 1,
        0,
        colorCanvas.height - 1,
      );
      const maxPixelY = THREE.MathUtils.clamp(
        Math.ceil(
          ((area.maxY + padding - this.mapBounds.minY) / spanY) *
            colorCanvas.height,
        ) + 1,
        minPixelY + 1,
        colorCanvas.height,
      );
      const regionWidth = maxPixelX - minPixelX;
      const regionHeight = maxPixelY - minPixelY;
      const colorImage = colorContext.createImageData(
        regionWidth,
        regionHeight,
      );
      const heightImage = heightContext.createImageData(
        regionWidth,
        regionHeight,
      );
      for (let localY = 0; localY < regionHeight; localY += 1) {
        const pixelY = minPixelY + localY;
        const mapY =
          this.mapBounds.minY + ((pixelY + 0.5) / colorCanvas.height) * spanY;
        for (let localX = 0; localX < regionWidth; localX += 1) {
          const pixelX = minPixelX + localX;
          const mapX =
            this.mapBounds.minX + ((pixelX + 0.5) / colorCanvas.width) * spanX;
          const sample = this._terrainSampleAtMapPoint(mapX, mapY);
          const color = terrainColor(sample, { debug: this.terrainDebug });
          const offset = (localY * regionWidth + localX) * 4;
          colorImage.data[offset] = color[0];
          colorImage.data[offset + 1] = color[1];
          colorImage.data[offset + 2] = color[2];
          colorImage.data[offset + 3] = 255;
          const elevation = Math.round(
            (sample.surfaceElevation ?? sample.elevation) * 255,
          );
          heightImage.data[offset] = elevation;
          heightImage.data[offset + 1] = elevation;
          heightImage.data[offset + 2] = elevation;
          heightImage.data[offset + 3] = 255;
        }
      }
      colorContext.putImageData(colorImage, minPixelX, minPixelY);
      heightContext.putImageData(heightImage, minPixelX, minPixelY);
    }
    this.proceduralTerrainTexture.needsUpdate = true;
    this.proceduralTerrainHeightTexture.needsUpdate = true;
  }

  _flattenedElevationAt(x, y, baseElevation) {
    let elevation = baseElevation;
    for (const area of this.terrainFlatteningAreas) {
      const outsideDistance = Math.max(
        area.minX - x,
        x - area.maxX,
        area.minY - y,
        y - area.maxY,
        0,
      );
      if (outsideDistance <= 0) {
        elevation = area.targetElevation;
        continue;
      }
      if (outsideDistance >= area.edge || area.edge <= 0) continue;
      const amount = 1 - outsideDistance / area.edge;
      const smoothed = amount * amount * (3 - 2 * amount);
      elevation = THREE.MathUtils.lerp(
        elevation,
        area.targetElevation,
        smoothed,
      );
    }
    return elevation;
  }

  _terrainSampleAtMapPoint(x, y) {
    const sample = this.terrainSampler(x, y);
    const baseElevation = sample.surfaceElevation ?? sample.elevation ?? 0;
    const flattenedElevation = this._flattenedElevationAt(x, y, baseElevation);
    if (Math.abs(flattenedElevation - baseElevation) <= 1e-5) return sample;
    return {
      ...sample,
      elevation: flattenedElevation,
      surfaceElevation: flattenedElevation,
    };
  }

  _terrainHeightAtMapPoint(x, y) {
    const sample = this._terrainSampleAtMapPoint(x, y);
    return (
      (sample.surfaceElevation ?? sample.elevation) * this.terrainElevationScale
    );
  }

  update(model) {
    this.model = model;
    this._setTerrainFlatteningAreas(model.terrainFlatteningAreas ?? []);
    if (this.virtualized) {
      this._updateVirtualTiles(model.tiles);
      this._updateVirtualRange(model.rangeOverlay);
    } else
      for (const tileState of model.tiles) {
        const tile = this.tileByKey.get(`${tileState.x},${tileState.y}`);
        if (!tile) continue;
        tile.state = tileState;
        this._applyTileAppearance(tile, tile === this.hoveredTile);
      }

    this._updateVirtualFog(model.fog);

    this._updateTerrainDebugLabels(model.tiles);
    this._updateRangeRings(model.rangeOverlay);
    this._updateTrees(model.tiles, model.pieces);

    const routeModelSignature = JSON.stringify(model.movementRoutes ?? []);
    if (routeModelSignature !== this.routeModelSignature) {
      this.routeModelSignature = routeModelSignature;
      this._clearDynamicGroup(this.routeGroup);
      this.routeFlags.length = 0;
      for (const route of model.movementRoutes ?? [])
        this._addMovementRoute(route);
    }

    const dynamicModelSignature = JSON.stringify({
      pieces: model.pieces,
      orderTargets: model.orderTargets ?? [],
    });
    if (dynamicModelSignature !== this.dynamicModelSignature) {
      this.dynamicModelSignature = dynamicModelSignature;
      this._clearDynamicGroup(this.pieceGroup);
      this._clearDynamicGroup(this.effectGroup);
      this.pieceMeshes.clear();
      this.spinningObjects.length = 0;
      this.pulsingObjects.length = 0;
      for (const marker of model.orderTargets ?? [])
        this._addOrderTargetMarker(marker);
      for (const piece of model.pieces) this._addPiece(piece);
    }
    this._updateHoverReticle(this.hoveredTile);
    this._updatePlacementPreview(this.hoveredTile);
    if (!this.hasFocusedInitialView && this._focusPlayerUnits())
      this.hasFocusedInitialView = true;
  }

  _treeNoise(x, y, salt = 0) {
    const value = Math.sin(x * 127.1 + y * 311.7 + salt * 74.7) * 43758.5453;
    return value - Math.floor(value);
  }

  _updateRangeRings(rangeOverlay) {
    const kind = rangeOverlay?.kind ?? null;
    const sources = (rangeOverlay?.sources ?? []).filter(
      (source) =>
        Number.isFinite(source.x) &&
        Number.isFinite(source.y) &&
        Number.isFinite(source.radius) &&
        source.radius > 0,
    );
    const signature = JSON.stringify({
      kind,
      sources,
      terrain: this.terrainFlatteningSignature,
    });
    if (signature === this.rangeRingSignature) return;
    this.rangeRingSignature = signature;
    this._clearDynamicGroup(this.rangeRingGroup);
    if (kind !== "reachable" && kind !== "targetable") return;
    for (const source of sources) {
      const positions = [];
      const segments = 128;
      for (let index = 0; index < segments; index += 1) {
        const angle = (index / segments) * Math.PI * 2;
        const mapX = source.x + Math.cos(angle) * source.radius;
        const mapY = source.y + Math.sin(angle) * source.radius;
        positions.push(
          mapX - this.mapCenter.x,
          this._terrainHeightAtMapPoint(mapX, mapY) + 0.16,
          mapY - this.mapCenter.y,
        );
      }
      const line = new THREE.LineLoop(
        new THREE.BufferGeometry().setAttribute(
          "position",
          new THREE.Float32BufferAttribute(positions, 3),
        ),
        new THREE.LineBasicMaterial({
          color: kind === "targetable" ? COLORS.target : COLORS.paper,
          transparent: true,
          opacity: 0.95,
          depthTest: false,
          toneMapped: false,
        }),
      );
      line.renderOrder = 91;
      line.raycast = () => {};
      this.rangeRingGroup.add(line);
    }
  }

  _updateTrees() {
    if (this.deferVegetation) return;
    if (!this.treePlacements) {
      const terrain = this.terrain;
      if (!terrain?.cellAtMapPoint) return;
      const placements = [];
      const cellCount = terrain.width * terrain.height;
      const densityScale = Math.min(1, 60000 / Math.max(cellCount * 0.16, 1));
      // Large maps keep a bounded vegetation candidate set. Tree placement is
      // visual only, so sampling representative cells avoids a multi-million
      // cell main-thread scan without changing terrain or gameplay data.
      const vegetationStride = Math.max(
        1,
        Math.ceil(Math.sqrt(cellCount / 250000)),
      );
      const sampledDensityScale = Math.min(
        1,
        densityScale * vegetationStride * vegetationStride,
      );
      const spanX = this.mapBounds.maxX - this.mapBounds.minX;
      const spanY = this.mapBounds.maxY - this.mapBounds.minY;
      for (let gridY = 0; gridY < terrain.height; gridY += vegetationStride) {
        const centerY =
          this.mapBounds.minY + ((gridY + 0.5) / terrain.height) * spanY;
        for (let gridX = 0; gridX < terrain.width; gridX += vegetationStride) {
          const centerX =
            this.mapBounds.minX + ((gridX + 0.5) / terrain.width) * spanX;
          const cell = terrain.cellAtMapPoint(centerX, centerY);
          const authoredTree =
            this.authoredTreeDensity?.[gridY * terrain.width + gridX] ?? 0;
          if (
            cell.water ||
            !["lowland", "plains", "hill"].includes(cell.terrainType) ||
            (!authoredTree && cell.moisture < 0.48)
          )
            continue;
          const clusterNoise = this._treeNoise(
            Math.floor(centerX / 3),
            Math.floor(centerY / 3),
            2,
          );
          const density =
            THREE.MathUtils.clamp(
              (cell.moisture - 0.44) * 0.75 + clusterNoise * 0.18,
              0.04,
              0.34,
            ) * sampledDensityScale;
          if (
            authoredTree
              ? this._treeNoise(gridX, gridY, 5) > authoredTree / 255
              : this._treeNoise(gridX, gridY, 5) > density
          )
            continue;
          const point = {
            x: centerX + (this._treeNoise(gridX, gridY, 7) - 0.5) * 0.5,
            y: centerY + (this._treeNoise(gridX, gridY, 11) - 0.5) * 0.5,
          };
          const sample = this.terrainSampler(point.x, point.y);
          placements.push({
            point,
            baseElevation: sample.surfaceElevation ?? sample.elevation ?? 0,
            scale: 0.75 + this._treeNoise(gridX, gridY, 17) * 0.42,
            rotation: this._treeNoise(gridX, gridY, 19) * Math.PI * 2,
            colorAmount: this._treeNoise(gridX, gridY, 23),
          });
        }
      }
      this.treePlacements = placements;
      if (!placements.length) return;
      const geometry = new THREE.ConeGeometry(0.23, 0.74, 4);
      const material = new THREE.MeshStandardMaterial({
        color: 0x2b5427,
        roughness: 1,
      });
      this.treeMesh = new THREE.InstancedMesh(
        geometry,
        material,
        placements.length,
      );
      // One global draw call intentionally replaces viewport/chunk tree
      // rebuilding. Let the GPU clip instances instead of culling the mesh as
      // one enormous, frequently changing chunk.
      this.treeMesh.frustumCulled = false;
      this.treeMesh.castShadow = false;
      this.treeMesh.receiveShadow = false;
      this.vegetationGroup.add(this.treeMesh);
    }

    if (this.treeModelSignature === this.terrainFlatteningSignature) return;
    this.treeModelSignature = this.terrainFlatteningSignature;
    const transform = new THREE.Object3D();
    let visibleIndex = 0;
    for (const placement of this.treePlacements) {
      const insideFoundation = this.terrainFlatteningAreas.some(
        (area) =>
          placement.point.x >= area.minX &&
          placement.point.x <= area.maxX &&
          placement.point.y >= area.minY &&
          placement.point.y <= area.maxY,
      );
      if (insideFoundation) continue;
      const elevation = this._flattenedElevationAt(
        placement.point.x,
        placement.point.y,
        placement.baseElevation,
      );
      transform.position.set(
        placement.point.x - this.mapCenter.x,
        0.12 + elevation * this.terrainElevationScale + 0.37 * placement.scale,
        placement.point.y - this.mapCenter.y,
      );
      transform.scale.setScalar(placement.scale);
      transform.rotation.set(0, placement.rotation, 0);
      transform.updateMatrix();
      this.treeMesh.setMatrixAt(visibleIndex, transform.matrix);
      this.treeMesh.setColorAt(
        visibleIndex,
        new THREE.Color(0x244923).lerp(
          new THREE.Color(0x3b642e),
          placement.colorAmount,
        ),
      );
      visibleIndex += 1;
    }
    this.treeMesh.count = visibleIndex;
    this.treeMesh.instanceMatrix.needsUpdate = true;
    if (this.treeMesh.instanceColor)
      this.treeMesh.instanceColor.needsUpdate = true;
  }

  _updateVirtualTiles(tileStates) {
    const mesh = this.virtualTileMesh;
    if (!mesh) return;
    const states = tileStates.slice(0, this.tileCapacity);
    this.hoveredTile = null;
    this.tileByKey.clear();
    this.tileDescriptorByKey.clear();
    mesh.userData.instanceTiles.length = states.length;
    mesh.count = states.length;
    this.virtualTileBounds = null;
    if (states.length) {
      const bounds = {
        minX: Number.POSITIVE_INFINITY,
        maxX: Number.NEGATIVE_INFINITY,
        minY: Number.POSITIVE_INFINITY,
        maxY: Number.NEGATIVE_INFINITY,
      };
      for (const state of states) {
        bounds.minX = Math.min(bounds.minX, state.x);
        bounds.maxX = Math.max(bounds.maxX, state.x);
        bounds.minY = Math.min(bounds.minY, state.y);
        bounds.maxY = Math.max(bounds.maxY, state.y);
      }
      this.virtualTileBounds = bounds;
    }
    for (let instanceId = 0; instanceId < states.length; instanceId += 1) {
      const state = states[instanceId];
      const tile = {
        x: state.x,
        y: state.y,
        descriptor: state,
        mesh,
        instanceId,
        state,
        appearanceKey: null,
      };
      const key = `${state.x},${state.y}`;
      mesh.userData.instanceTiles[instanceId] = tile;
      this.tileByKey.set(key, tile);
      this.tileDescriptorByKey.set(key, state);
      this._applyTileAppearance(tile, false);
    }
    mesh.instanceMatrix.needsUpdate = true;
    if (mesh.instanceColor) mesh.instanceColor.needsUpdate = true;
  }

  _applyTileAppearance(tile, hovered = false) {
    const state = tile.state;
    if (!state) return;
    const appearanceKey =
      Number(state.visible) |
      (Number(state.available) << 1) |
      (Number(state.selected) << 4);
    if (tile.appearanceKey === appearanceKey) return;
    tile.appearanceKey = appearanceKey;
    // The map-spanning procedural texture supplies terrain colour. Instance
    // colour is now reserved for fog and interaction overlays.
    const color = new THREE.Color(0xffffff);
    // Fog presentation is handled by one continuous cloud mesh. Tile state
    // still controls contact visibility without introducing square tinting.
    let accent = null;
    let strength = 0;
    if (state.available) {
      accent = COLORS.lime;
      strength = 0.54;
    }
    if (state.selected) {
      accent = COLORS.lime;
      strength = 0.68;
    }
    if (accent !== null) color.lerp(new THREE.Color(accent), strength);
    tile.mesh.setColorAt(tile.instanceId, color);
    tile.mesh.instanceColor.needsUpdate = true;

    const position = this._cellPosition(tile.x, tile.y);
    this.tileTransform.position.set(
      position.x,
      this.virtualized ? 0.065 : state.visible ? 0.06 : 0.075,
      position.z,
    );
    this.tileTransform.scale.setScalar(
      this.virtualized ? 1 : state.visible ? 1 : 0.985,
    );
    this.tileTransform.updateMatrix();
    tile.mesh.setMatrixAt(tile.instanceId, this.tileTransform.matrix);
    tile.mesh.instanceMatrix.needsUpdate = true;
  }

  _createTerrainMaterial() {
    const material = new THREE.MeshStandardMaterial({
      color: 0xffffff,
      map: this._getProceduralTerrainTexture(),
      bumpMap: this._getProceduralTerrainHeightTexture(),
      bumpScale: 0.7,
      displacementMap: this._getProceduralTerrainHeightTexture(),
      displacementScale: this.terrainElevationScale,
      displacementBias: 0,
      emissive: 0x10150e,
      emissiveIntensity: 0.24,
      roughness: 0.94,
      metalness: 0,
      side: THREE.FrontSide,
    });
    const boardSize = new THREE.Vector2(this.cols, this.rows);
    material.onBeforeCompile = (shader) => {
      shader.uniforms.terrainBoardSize = { value: boardSize };
      shader.vertexShader = shader.vertexShader
        .replace(
          "#include <common>",
          "#include <common>\nuniform vec2 terrainBoardSize;",
        )
        .replace(
          "#include <uv_vertex>",
          `#include <uv_vertex>
          vec4 terrainPosition = vec4(position, 1.0);
          #ifdef USE_INSTANCING
            terrainPosition = instanceMatrix * terrainPosition;
          #endif
          #ifdef USE_MAP
            vMapUv = vec2(
              terrainPosition.x / terrainBoardSize.x + 0.5,
              0.5 - terrainPosition.z / terrainBoardSize.y
            );
          #endif
          #ifdef USE_BUMPMAP
            vBumpMapUv = vec2(
              terrainPosition.x / terrainBoardSize.x + 0.5,
              0.5 - terrainPosition.z / terrainBoardSize.y
            );
          #endif
          #ifdef USE_DISPLACEMENTMAP
            vDisplacementMapUv = vec2(
              terrainPosition.x / terrainBoardSize.x + 0.5,
              0.5 - terrainPosition.z / terrainBoardSize.y
            );
          #endif`,
        );
    };
    material.userData.terrainMaterial = true;
    material.customProgramCacheKey = () => "continuous-terrain-elevation-v4";
    return material;
  }

  _getProceduralTerrainTexture() {
    if (this.proceduralTerrainTexture) return this.proceduralTerrainTexture;
    const longestSide = Math.max(this.cols, this.rows, 1);
    const longestTextureSide =
      longestSide > 128 ? 1024 : longestSide > 36 ? 768 : 512;
    const canvas = document.createElement("canvas");
    canvas.width = Math.max(
      256,
      Math.round((this.cols / longestSide) * longestTextureSide),
    );
    canvas.height = Math.max(
      256,
      Math.round((this.rows / longestSide) * longestTextureSide),
    );
    const heightCanvas = document.createElement("canvas");
    heightCanvas.width = canvas.width;
    heightCanvas.height = canvas.height;
    const context = canvas.getContext("2d");
    const heightContext = heightCanvas.getContext("2d");
    const image = context.createImageData(canvas.width, canvas.height);
    const heightImage = heightContext.createImageData(
      heightCanvas.width,
      heightCanvas.height,
    );
    const spanX = this.mapBounds.maxX - this.mapBounds.minX;
    const spanY = this.mapBounds.maxY - this.mapBounds.minY;
    for (let pixelY = 0; pixelY < canvas.height; pixelY += 1) {
      const mapY =
        this.mapBounds.minY + ((pixelY + 0.5) / canvas.height) * spanY;
      for (let pixelX = 0; pixelX < canvas.width; pixelX += 1) {
        const mapX =
          this.mapBounds.minX + ((pixelX + 0.5) / canvas.width) * spanX;
        const sample = this._terrainSampleAtMapPoint(mapX, mapY);
        const color = terrainColor(sample, { debug: this.terrainDebug });
        const offset = (pixelY * canvas.width + pixelX) * 4;
        image.data[offset] = color[0];
        image.data[offset + 1] = color[1];
        image.data[offset + 2] = color[2];
        image.data[offset + 3] = 255;
        const elevation = Math.round(
          (sample.surfaceElevation ?? sample.elevation) * 255,
        );
        heightImage.data[offset] = elevation;
        heightImage.data[offset + 1] = elevation;
        heightImage.data[offset + 2] = elevation;
        heightImage.data[offset + 3] = 255;
      }
    }
    context.putImageData(image, 0, 0);
    heightContext.putImageData(heightImage, 0, 0);

    const texture = new THREE.CanvasTexture(canvas);
    texture.colorSpace = THREE.SRGBColorSpace;
    texture.minFilter = THREE.LinearMipmapLinearFilter;
    texture.magFilter = THREE.LinearFilter;
    texture.anisotropy = Math.min(
      8,
      this.renderer.capabilities.getMaxAnisotropy(),
    );
    texture.needsUpdate = true;
    const heightTexture = new THREE.CanvasTexture(heightCanvas);
    heightTexture.minFilter = THREE.LinearMipmapLinearFilter;
    heightTexture.magFilter = THREE.LinearFilter;
    heightTexture.anisotropy = texture.anisotropy;
    heightTexture.needsUpdate = true;
    this.proceduralTerrainTexture = texture;
    this.proceduralTerrainHeightTexture = heightTexture;
    this.proceduralTerrainCanvas = canvas;
    this.proceduralTerrainHeightCanvas = heightCanvas;
    return texture;
  }

  _getProceduralTerrainHeightTexture() {
    if (!this.proceduralTerrainHeightTexture)
      this._getProceduralTerrainTexture();
    return this.proceduralTerrainHeightTexture;
  }

  getTerrainMapImage() {
    if (!this.proceduralTerrainCanvas) this._getProceduralTerrainTexture();
    return this.proceduralTerrainCanvas;
  }

  _addFireMarker(tile) {
    const group = new THREE.Group();
    const ring = new THREE.Mesh(
      new THREE.TorusGeometry(0.34, 0.035, 8, 28),
      new THREE.MeshBasicMaterial({
        color: COLORS.target,
        transparent: true,
        opacity: 0.9,
      }),
    );
    ring.rotation.x = Math.PI / 2;
    group.add(ring);
    const count = Math.min(tile.fireCount, 4);
    for (let i = 0; i < count; i += 1) {
      const pip = new THREE.Mesh(
        new THREE.SphereGeometry(0.045, 8, 6),
        new THREE.MeshBasicMaterial({ color: COLORS.paper }),
      );
      pip.position.set((i - (count - 1) / 2) * 0.12, 0.04, 0);
      group.add(pip);
    }
    const position = this._cellPosition(tile.x, tile.y);
    group.position.set(position.x, position.y + 0.14, position.z);
    group.userData.pulse = true;
    group.userData.markerPixels = TARGET_MARKER_VISIBILITY.queuedPixels;
    this.pulsingObjects.push(group);
    this.effectGroup.add(group);
  }

  _addOrderTargetMarker(marker) {
    const hostileTarget = marker.kind === "bomb" || marker.kind === "attack";
    const color = hostileTarget ? COLORS.target : COLORS.lime;
    const group = new THREE.Group();
    const material = new THREE.MeshBasicMaterial({
      color,
      transparent: true,
      opacity: 0.98,
      depthTest: false,
      depthWrite: false,
      toneMapped: false,
    });
    const ring = new THREE.Mesh(
      new THREE.TorusGeometry(0.4, 0.045, 8, 32),
      material,
    );
    ring.rotation.x = Math.PI / 2;
    ring.renderOrder = 91;
    group.add(ring);
    const pin = new THREE.Mesh(
      new THREE.OctahedronGeometry(0.11, 0),
      material.clone(),
    );
    pin.position.y = 0.22;
    pin.renderOrder = 92;
    group.add(pin);
    const position = this._mapPosition(
      marker.x,
      marker.y,
      marker.worldX,
      marker.worldY,
    );
    group.position.set(position.x, position.y + 0.12, position.z);
    group.userData.pulse = true;
    group.userData.markerPixels = TARGET_MARKER_VISIBILITY.queuedPixels;
    this.pulsingObjects.push(group);
    this.effectGroup.add(group);
  }

  _addMovementRoute(route) {
    if (!Array.isArray(route.points) || route.points.length < 2) return;
    const routePoints = route.points.map((point) => {
      const position = this._mapPosition(
        point.x,
        point.y,
        point.worldX,
        point.worldY,
      );
      position.y += 0.11;
      return position;
    });
    const line = new THREE.Line(
      new THREE.BufferGeometry().setFromPoints(routePoints),
      new THREE.LineBasicMaterial({
        color: COLORS.lime,
        transparent: true,
        opacity: 0.88,
        depthTest: false,
        depthWrite: false,
        toneMapped: false,
      }),
    );
    line.renderOrder = 86;
    this.routeGroup.add(line);

    const flagPoints = Array.isArray(route.flagPoints)
      ? route.flagPoints.map((point) => {
          const position = this._mapPosition(
            point.x,
            point.y,
            point.worldX,
            point.worldY,
          );
          position.y += 0.11;
          return position;
        })
      : routePoints.slice(1);
    for (let index = 0; index < flagPoints.length; index += 1) {
      const final = index === flagPoints.length - 1;
      const flag = this._createRouteFlag(final);
      flag.position.copy(flagPoints[index]);
      flag.position.y += 0.015;
      flag.userData.markerPixels = ROUTE_FLAG_VISIBILITY.pixels;
      this.routeFlags.push(flag);
      this.routeGroup.add(flag);
    }
  }

  _updateTerrainDebugLabels(tileStates) {
    if (!this.showElevationValues) return;
    const visible = tileStates
      .filter(
        (tile) => tile.visible !== false && Number.isFinite(tile.elevation),
      )
      .slice(0, 2500);
    const signature = visible
      .map((tile) => `${tile.x},${tile.y}:${tile.elevation.toFixed(3)}`)
      .join(";");
    if (signature === this.debugLabelSignature) return;
    this.debugLabelSignature = signature;
    for (const child of [...this.debugLabelGroup.children]) {
      this.debugLabelGroup.remove(child);
      child.material?.map?.dispose?.();
      child.material?.dispose?.();
    }
    for (const tile of visible) {
      const canvas = document.createElement("canvas");
      canvas.width = 64;
      canvas.height = 28;
      const context = canvas.getContext("2d");
      context.font = "700 17px monospace";
      context.textAlign = "center";
      context.textBaseline = "middle";
      context.lineWidth = 4;
      context.strokeStyle = "rgba(0,0,0,.9)";
      context.strokeText(tile.elevation.toFixed(2), 32, 14);
      context.fillStyle = "#fff";
      context.fillText(tile.elevation.toFixed(2), 32, 14);
      const texture = new THREE.CanvasTexture(canvas);
      texture.colorSpace = THREE.SRGBColorSpace;
      const label = new THREE.Sprite(
        new THREE.SpriteMaterial({
          map: texture,
          transparent: true,
          depthTest: false,
          depthWrite: false,
          toneMapped: false,
        }),
      );
      const position = this._cellPosition(tile.x, tile.y);
      label.position.set(position.x, position.y + 0.38, position.z);
      label.scale.set(0.58, 0.25, 1);
      label.renderOrder = 91;
      this.debugLabelGroup.add(label);
    }
  }

  _createRouteFlag(final = false) {
    const group = new THREE.Group();
    const pole = new THREE.Mesh(
      new THREE.CylinderGeometry(0.012, 0.014, 0.34, 7),
      new THREE.MeshBasicMaterial({
        color: COLORS.paper,
        depthTest: false,
        depthWrite: false,
        toneMapped: false,
      }),
    );
    pole.position.y = 0.17;
    pole.renderOrder = 88;
    group.add(pole);

    const pennantShape = new THREE.Shape();
    pennantShape.moveTo(0, 0);
    pennantShape.lineTo(0.2, -0.055);
    pennantShape.lineTo(0, -0.115);
    pennantShape.closePath();
    const pennant = new THREE.Mesh(
      new THREE.ShapeGeometry(pennantShape),
      new THREE.MeshBasicMaterial({
        color: final ? COLORS.paper : COLORS.lime,
        side: THREE.DoubleSide,
        depthTest: false,
        depthWrite: false,
        toneMapped: false,
      }),
    );
    pennant.position.set(0.012, 0.335, 0);
    pennant.renderOrder = 89;
    group.add(pennant);
    return group;
  }

  _buildHoverReticle() {
    const material = new THREE.MeshBasicMaterial({
      color: COLORS.paper,
      transparent: true,
      opacity: 1,
      depthTest: false,
      depthWrite: false,
      toneMapped: false,
    });
    const group = new THREE.Group();
    const arrowHead = new THREE.Mesh(
      new THREE.ConeGeometry(0.13, 0.32, 12),
      material,
    );
    arrowHead.rotation.z = Math.PI;
    arrowHead.position.y = 0.16;
    arrowHead.renderOrder = 94;
    group.add(arrowHead);
    const stem = new THREE.Mesh(
      new THREE.CylinderGeometry(0.025, 0.025, 0.28, 8),
      material.clone(),
    );
    stem.position.y = 0.43;
    stem.renderOrder = 95;
    group.add(stem);
    group.visible = false;
    group.userData.materials = [arrowHead.material, stem.material];
    group.userData.markerPixels = TARGET_MARKER_VISIBILITY.hoverPixels;
    this.hoverReticle = group;
    this.interactionGroup.add(group);
  }

  _rangeKindAtTile(tile) {
    if (!tile) return null;
    const state = tile.state;
    if (state?.targetable) return "targetable";
    if (state?.reachable) return "reachable";
    if (state?.available) return "available";
    const overlay = this.model.rangeOverlay;
    if (!overlay?.kind || !overlay.sources?.length)
      return this.model.routePlanning ? "route" : null;
    const descriptor = this.tileDescriptorByKey.get(`${tile.x},${tile.y}`);
    const center = descriptor?.center ?? { x: tile.x, y: tile.y };
    const insideOverlay = overlay.sources.some(
      (source) =>
        Math.hypot(center.x - source.x, center.y - source.y) <=
        source.radius + 0.001,
    );
    return insideOverlay
      ? overlay.kind
      : this.model.routePlanning
        ? "route"
        : null;
  }

  _updateHoverReticle(tile) {
    if (!this.hoverReticle) return;
    const kind = this._rangeKindAtTile(tile);
    this.hoverReticle.visible = Boolean(kind);
    if (!kind) return;
    const color =
      kind === "targetable"
        ? COLORS.target
        : kind === "available"
          ? COLORS.lime
          : kind === "route"
            ? COLORS.lime
            : COLORS.paper;
    for (const material of this.hoverReticle.userData.materials)
      material.color.setHex(color);
    const lockedTarget = this.hoveredTargetId
      ? this.model.pieces.find((piece) => piece.id === this.hoveredTargetId)
      : null;
    const position = lockedTarget
      ? this._mapPosition(
          lockedTarget.x,
          lockedTarget.y,
          lockedTarget.worldX,
          lockedTarget.worldY,
        )
      : this._mapPosition(
          tile.x,
          tile.y,
          this.hoveredMapPoint?.x,
          this.hoveredMapPoint?.y,
        );
    this.hoverReticle.position.set(position.x, position.y + 0.05, position.z);
  }

  _attackTargetUnderPointer() {
    if (this.model.rangeOverlay?.kind !== "targetable" || !this.hoveredMapPoint)
      return null;
    const overlay = this.model.rangeOverlay;
    const viewport = this.renderer.domElement.getBoundingClientRect();
    const pointerPixels = {
      x: ((this.pointer.x + 1) / 2) * viewport.width,
      y: ((1 - this.pointer.y) / 2) * viewport.height,
    };
    return (
      this.model.pieces
        .map((piece) => {
          if (piece.owner !== 1 || piece.pending || piece.hp <= 0) return false;
          const center =
            Number.isFinite(piece.worldX) && Number.isFinite(piece.worldY)
              ? { x: piece.worldX, y: piece.worldY }
              : (this.tileDescriptorByKey.get(`${piece.x},${piece.y}`)
                  ?.center ?? { x: piece.x, y: piece.y });
          const inRange = (overlay.sources ?? []).some(
            (source) =>
              Math.hypot(center.x - source.x, center.y - source.y) <=
              source.radius,
          );
          if (!inRange) return null;
          const projected = this._mapPosition(
            piece.x,
            piece.y,
            piece.worldX,
            piece.worldY,
          );
          projected.y += piece.kind === "structure" ? 0.35 : 0.42;
          projected.project(this.camera);
          if (projected.z < -1 || projected.z > 1) return null;
          const targetPixels = {
            x: ((projected.x + 1) / 2) * viewport.width,
            y: ((1 - projected.y) / 2) * viewport.height,
          };
          const distance = Math.hypot(
            pointerPixels.x - targetPixels.x,
            pointerPixels.y - targetPixels.y,
          );
          const snapPixels =
            piece.kind === "structure"
              ? Math.max(34, (piece.footprintSize ?? 1) * 12)
              : 28;
          return distance <= snapPixels ? { piece, distance } : null;
        })
        .filter(Boolean)
        .sort((first, second) => first.distance - second.distance)[0]?.piece ??
      null
    );
  }

  _markerScale(marker) {
    const cameraDistance = this.camera.position.distanceTo(
      this.controls.target,
    );
    const viewportHeight = Math.max(this.container.clientHeight, 1);
    const worldUnitsPerPixel =
      (2 *
        cameraDistance *
        Math.tan(THREE.MathUtils.degToRad(this.camera.fov) / 2)) /
      viewportHeight;
    return THREE.MathUtils.clamp(
      worldUnitsPerPixel * (marker.userData.markerPixels ?? 30),
      1,
      TARGET_MARKER_VISIBILITY.maximumScale,
    );
  }

  _addPiece(piece) {
    const group = new THREE.Group();
    const position =
      piece.kind === "unit"
        ? this._mapPosition(piece.x, piece.y, piece.worldX, piece.worldY)
        : this._cellPosition(piece.x, piece.y);
    group.position.set(
      position.x,
      position.y + (piece.airborne && piece.type === "plane" ? 1.6 : 0.01),
      position.z,
    );
    group.userData = {
      pieceId: piece.id,
      selected: piece.selected,
      orderKind: piece.orderKind,
    };

    if (piece.kind === "structure") this._buildStructure(group, piece);
    else {
      this._buildUnit(group, piece);
      group.rotation.y = piece.heading ?? (piece.owner === 0 ? 0 : Math.PI);
    }
    if (!piece.guide) this._addHealth(group, piece);

    if (piece.selected) this._addSelectionRing(group);
    if (piece.orderKind) this._addOrderRing(group, piece.orderKind);
    group.traverse((child) => {
      if (child.isMesh) {
        child.castShadow = true;
        child.receiveShadow = true;
      }
    });
    this.pieceGroup.add(group);
    this.pieceMeshes.set(piece.id, group);
  }

  _updatePlacementPreview(tile) {
    this._clearDynamicGroup(this.placementPreviewGroup);
    if (!tile || typeof this.resolvePlacementPreview !== "function") {
      this.onPlacementPreviewChange?.(null);
      return;
    }
    const preview = this.resolvePlacementPreview(tile.x, tile.y);
    this.onPlacementPreviewChange?.(preview ?? null);
    if (!preview) return;
    this.hoverReticle.visible = false;

    const group = new THREE.Group();
    const position = this._cellPosition(
      Number.isFinite(preview.x) ? preview.x : tile.x,
      Number.isFinite(preview.y) ? preview.y : tile.y,
    );
    group.position.set(position.x, position.y + 0.025, position.z);
    group.userData.placementPreview = true;
    this._buildStructure(group, { ...preview, pending: true });
    group.traverse((child) => {
      if (!child.isMesh) return;
      child.castShadow = false;
      child.receiveShadow = false;
      child.renderOrder = 9;
      if (child.material) {
        child.material.depthWrite = false;
        child.material.opacity = preview.previewValid === false ? 0.58 : 0.76;
      }
    });
    this.placementPreviewGroup.add(group);
  }

  _pieceMaterials(piece) {
    const neutral = piece.owner < 0;
    const primary = neutral
      ? 0x777d72
      : piece.owner === 0
        ? COLORS.player
        : COLORS.enemy;
    const secondary = neutral
      ? 0x404640
      : piece.owner === 0
        ? COLORS.playerDark
        : COLORS.enemyDark;
    const pendingColor =
      piece.previewValid === false ? COLORS.target : COLORS.lime;
    const options = piece.pending
      ? {
          color: pendingColor,
          wireframe: true,
          transparent: true,
          opacity: 0.75,
        }
      : { color: primary, roughness: 0.48, metalness: 0.22 };
    const auxiliary = (color, roughness, metalness = 0) =>
      new THREE.MeshStandardMaterial(
        piece.pending
          ? {
              color: pendingColor,
              wireframe: true,
              transparent: true,
              opacity: 0.75,
              roughness,
              metalness,
            }
          : { color, roughness, metalness },
      );
    return {
      primary: new THREE.MeshStandardMaterial(options),
      secondary: new THREE.MeshStandardMaterial({
        color: piece.pending ? pendingColor : secondary,
        roughness: 0.72,
        metalness: 0.08,
        wireframe: Boolean(piece.pending),
        transparent: Boolean(piece.pending),
        opacity: piece.pending ? 0.75 : 1,
      }),
      pale: auxiliary(0x9b9b88, 0.52, 0.38),
      dark: auxiliary(0x20241f, 0.8, 0.18),
      wood: auxiliary(0x6b4a31, 0.88),
      skin: auxiliary(0xb49372, 0.95),
      canvas: auxiliary(0x827b59, 1),
    };
  }

  _buildUnit(group, piece) {
    const materials = this._pieceMaterials(piece);
    const base = new THREE.Mesh(
      new THREE.CylinderGeometry(0.32, 0.36, 0.08, 16),
      materials.dark,
    );
    base.position.y = 0.05;
    if (piece.type !== "plane") group.add(base);

    if (piece.type === "plane") {
      const aircraftModel = new THREE.Group();
      aircraftModel.scale.setScalar(0.9);
      const fuselage = new THREE.Mesh(
        new THREE.CylinderGeometry(0.085, 0.13, 0.9, 10),
        materials.primary,
      );
      fuselage.rotation.z = Math.PI / 2;
      fuselage.position.y = 0.24;
      const nose = new THREE.Mesh(
        new THREE.ConeGeometry(0.085, 0.24, 10),
        materials.pale,
      );
      nose.rotation.z = -Math.PI / 2;
      nose.position.set(0.56, 0.24, 0);
      const wing = new THREE.Mesh(
        new THREE.BoxGeometry(0.38, 0.045, 1.15),
        materials.secondary,
      );
      wing.position.set(-0.03, 0.23, 0);
      const tailWing = new THREE.Mesh(
        new THREE.BoxGeometry(0.2, 0.035, 0.5),
        materials.secondary,
      );
      tailWing.position.set(-0.42, 0.27, 0);
      const tailFin = new THREE.Mesh(
        new THREE.BoxGeometry(0.2, 0.28, 0.035),
        materials.secondary,
      );
      tailFin.position.set(-0.4, 0.38, 0);
      tailFin.rotation.z = -0.18;
      const cockpit = new THREE.Mesh(
        new THREE.SphereGeometry(0.105, 10, 7),
        materials.dark,
      );
      cockpit.scale.set(1.5, 0.62, 0.82);
      cockpit.position.set(0.2, 0.32, 0);
      const engineLeft = new THREE.Mesh(
        new THREE.CylinderGeometry(0.075, 0.09, 0.34, 8),
        materials.dark,
      );
      engineLeft.rotation.z = Math.PI / 2;
      engineLeft.position.set(0.03, 0.18, -0.34);
      const engineRight = engineLeft.clone();
      engineRight.position.z = 0.34;
      aircraftModel.add(
        fuselage,
        nose,
        wing,
        tailWing,
        tailFin,
        cockpit,
        engineLeft,
        engineRight,
      );
      group.add(aircraftModel);
    } else if (
      piece.type === "short" ||
      piece.type === "commander" ||
      piece.type === "medic" ||
      piece.type === "scout" ||
      piece.type === "engineer"
    ) {
      const isCommander = piece.type === "commander";
      const isMedic = piece.type === "medic";
      const leftLeg = new THREE.Mesh(
        new THREE.BoxGeometry(0.09, 0.28, 0.1),
        materials.secondary,
      );
      leftLeg.position.set(0, 0.23, -0.075);
      const rightLeg = leftLeg.clone();
      rightLeg.position.z = 0.075;
      const torso = new THREE.Mesh(
        new THREE.BoxGeometry(0.25, 0.32, 0.28),
        materials.primary,
      );
      torso.position.y = 0.49;
      const pack = new THREE.Mesh(
        new THREE.BoxGeometry(0.1, 0.24, 0.24),
        materials.canvas,
      );
      pack.position.set(-0.16, 0.5, 0);
      const head = new THREE.Mesh(
        new THREE.SphereGeometry(0.105, 12, 9),
        materials.skin,
      );
      head.position.y = 0.73;
      const headgear = new THREE.Mesh(
        isCommander
          ? new THREE.CylinderGeometry(0.12, 0.14, 0.075, 10)
          : new THREE.SphereGeometry(
              0.135,
              14,
              8,
              0,
              Math.PI * 2,
              0,
              Math.PI * 0.58,
            ),
        materials.secondary,
      );
      headgear.position.y = isCommander ? 0.825 : 0.775;
      if (!isCommander) headgear.scale.y = 0.72;
      const capBrim = isCommander
        ? new THREE.Mesh(
            new THREE.BoxGeometry(0.2, 0.025, 0.18),
            materials.secondary,
          )
        : null;
      if (capBrim) capBrim.position.set(0.055, 0.795, 0);
      const weaponStock = new THREE.Mesh(
        new THREE.BoxGeometry(
          isMedic ? 0.12 : isCommander ? 0.24 : 0.46,
          0.055,
          0.065,
        ),
        isCommander ? materials.dark : materials.wood,
      );
      weaponStock.position.set(isCommander ? 0.13 : 0.17, 0.55, 0.17);
      weaponStock.rotation.z = -0.08;
      const weaponBarrel = new THREE.Mesh(
        new THREE.CylinderGeometry(
          isCommander ? 0.018 : 0.014,
          isCommander ? 0.018 : 0.014,
          isMedic ? 0.08 : isCommander ? 0.16 : 0.34,
          7,
        ),
        materials.dark,
      );
      weaponBarrel.rotation.z = Math.PI / 2;
      weaponBarrel.position.set(isCommander ? 0.32 : 0.55, 0.58, 0.17);
      const insignia = isCommander
        ? new THREE.Mesh(
            new THREE.BoxGeometry(0.018, 0.075, 0.065),
            materials.pale,
          )
        : null;
      const medicalMark = isMedic
        ? new THREE.Mesh(
            new THREE.BoxGeometry(0.025, 0.16, 0.055),
            new THREE.MeshBasicMaterial({ color: 0xf2f0dc }),
          )
        : null;
      const medicalMarkCross = isMedic
        ? new THREE.Mesh(
            new THREE.BoxGeometry(0.025, 0.055, 0.16),
            new THREE.MeshBasicMaterial({ color: 0xf2f0dc }),
          )
        : null;
      if (medicalMark) medicalMark.position.set(-0.225, 0.52, 0);
      if (medicalMarkCross)
        medicalMarkCross.position.set(-0.226, 0.52, 0);
      if (insignia) insignia.position.set(0.135, 0.56, -0.065);
      let commandStar = null;
      if (isCommander) {
        const starShape = new THREE.Shape();
        for (let point = 0; point < 10; point += 1) {
          const angle = -Math.PI / 2 + (point * Math.PI) / 5;
          const radius = point % 2 === 0 ? 0.13 : 0.055;
          const x = Math.cos(angle) * radius;
          const y = Math.sin(angle) * radius;
          if (point === 0) starShape.moveTo(x, y);
          else starShape.lineTo(x, y);
        }
        starShape.closePath();
        commandStar = new THREE.Mesh(
          new THREE.ExtrudeGeometry(starShape, {
            depth: 0.025,
            bevelEnabled: false,
          }),
          new THREE.MeshBasicMaterial({
            color: COLORS.lime,
            side: THREE.DoubleSide,
          }),
        );
        commandStar.position.set(0, 1.22, 0);
        commandStar.rotation.y = Math.PI / 2;
      }
      group.add(
        leftLeg,
        rightLeg,
        torso,
        pack,
        head,
        headgear,
        weaponStock,
        weaponBarrel,
      );
      if (capBrim) group.add(capBrim);
      if (insignia) group.add(insignia);
      if (medicalMark) group.add(medicalMark);
      if (medicalMarkCross) group.add(medicalMarkCross);
      if (commandStar) group.add(commandStar);
    } else if (piece.type === "medium" || piece.type === "antiAir") {
      const leftTrack = new THREE.Mesh(
        new THREE.BoxGeometry(0.62, 0.18, 0.13),
        materials.dark,
      );
      leftTrack.position.set(0, 0.19, -0.25);
      const rightTrack = leftTrack.clone();
      rightTrack.position.z = 0.25;
      const hull = new THREE.Mesh(
        new THREE.BoxGeometry(0.58, 0.25, 0.43),
        materials.primary,
      );
      hull.position.y = 0.29;
      const glacis = new THREE.Mesh(
        new THREE.BoxGeometry(0.18, 0.19, 0.39),
        materials.secondary,
      );
      glacis.position.set(0.32, 0.32, 0);
      glacis.rotation.z = -0.18;
      const turret = new THREE.Mesh(
        new THREE.CylinderGeometry(0.18, 0.22, 0.17, 12),
        materials.secondary,
      );
      turret.position.y = 0.51;
      const hatch = new THREE.Mesh(
        new THREE.CylinderGeometry(0.09, 0.09, 0.05, 10),
        materials.dark,
      );
      hatch.position.y = 0.62;
      const barrel = new THREE.Mesh(
        new THREE.CylinderGeometry(0.025, 0.035, 0.55, 8),
        materials.pale,
      );
      barrel.rotation.z = Math.PI / 2;
      barrel.position.set(0.38, 0.54, 0);
      group.add(leftTrack, rightTrack, hull, glacis, turret, hatch, barrel);
    } else if (piece.type === "long") {
      const basePlate = new THREE.Mesh(
        new THREE.CylinderGeometry(0.25, 0.29, 0.045, 14),
        materials.pale,
      );
      basePlate.position.y = 0.11;
      const mortarTube = new THREE.Mesh(
        new THREE.CylinderGeometry(0.045, 0.06, 0.62, 10),
        materials.dark,
      );
      mortarTube.rotation.z = -Math.PI / 3;
      mortarTube.position.set(0.18, 0.41, 0);
      const tubeRim = new THREE.Mesh(
        new THREE.TorusGeometry(0.05, 0.012, 6, 12),
        materials.pale,
      );
      tubeRim.rotation.y = Math.PI / 2;
      tubeRim.rotation.z = Math.PI / 6;
      tubeRim.position.set(0.45, 0.57, 0);
      const bipod = new THREE.Mesh(
        new THREE.CylinderGeometry(0.018, 0.025, 0.48, 7),
        materials.pale,
      );
      bipod.rotation.z = Math.PI / 4;
      bipod.position.set(-0.02, 0.29, -0.13);
      const bipodRight = bipod.clone();
      bipodRight.position.z = 0.13;
      const ammoCrate = new THREE.Mesh(
        new THREE.BoxGeometry(0.24, 0.17, 0.2),
        materials.canvas,
      );
      ammoCrate.position.set(-0.26, 0.18, 0.2);
      group.add(basePlate, mortarTube, tubeRim, bipod, bipodRight, ammoCrate);
    } else {
      const carriage = new THREE.Mesh(
        new THREE.BoxGeometry(0.58, 0.18, 0.4),
        materials.primary,
      );
      carriage.position.y = 0.23;
      const shield = new THREE.Mesh(
        new THREE.BoxGeometry(0.12, 0.42, 0.54),
        materials.secondary,
      );
      shield.position.set(0.08, 0.47, 0);
      shield.rotation.z = -0.12;
      const breech = new THREE.Mesh(
        new THREE.CylinderGeometry(0.09, 0.11, 0.28, 10),
        materials.dark,
      );
      breech.rotation.z = Math.PI / 2;
      breech.position.set(0.15, 0.5, 0);
      const artilleryBarrel = new THREE.Mesh(
        new THREE.CylinderGeometry(0.035, 0.055, 0.88, 10),
        materials.pale,
      );
      artilleryBarrel.rotation.z = -Math.PI / 2 - 0.17;
      artilleryBarrel.position.set(0.5, 0.59, 0);
      const muzzleBrake = new THREE.Mesh(
        new THREE.CylinderGeometry(0.07, 0.07, 0.13, 10),
        materials.dark,
      );
      muzzleBrake.rotation.z = artilleryBarrel.rotation.z;
      muzzleBrake.position.set(0.93, 0.67, 0);
      const leftWheel = new THREE.Mesh(
        new THREE.CylinderGeometry(0.23, 0.23, 0.1, 14),
        materials.dark,
      );
      leftWheel.rotation.x = Math.PI / 2;
      leftWheel.position.set(-0.08, 0.24, -0.27);
      const rightWheel = leftWheel.clone();
      rightWheel.position.z = 0.27;
      const leftTrail = new THREE.Mesh(
        new THREE.BoxGeometry(0.5, 0.07, 0.08),
        materials.secondary,
      );
      leftTrail.position.set(-0.43, 0.12, -0.13);
      leftTrail.rotation.z = -0.12;
      const rightTrail = leftTrail.clone();
      rightTrail.position.z = 0.13;
      group.add(
        carriage,
        shield,
        breech,
        artilleryBarrel,
        muzzleBrake,
        leftWheel,
        rightWheel,
        leftTrail,
        rightTrail,
      );
    }
  }

  _buildBridge(group, piece, materials) {
    const bridgehead = new THREE.Mesh(
      new THREE.BoxGeometry(0.72, 0.14, 0.82),
      materials.secondary,
    );
    bridgehead.position.y = 0.09;
    group.add(bridgehead);
    if (
      piece.bridgeAnchorOnly ||
      !Number.isFinite(piece.endX) ||
      !Number.isFinite(piece.endY)
    ) {
      const marker = new THREE.Mesh(
        new THREE.BoxGeometry(0.1, 0.62, 0.1),
        materials.primary,
      );
      marker.position.y = 0.39;
      group.add(marker);
      return;
    }

    const start = this._cellPosition(piece.x, piece.y);
    const end = this._cellPosition(piece.endX, piece.endY);
    const span = end.clone().sub(start);
    const length = span.length();
    if (length <= 0.1) return;

    const assembly = new THREE.Group();
    assembly.position.set(span.x / 2, span.y / 2 + 0.2, span.z / 2);
    assembly.quaternion.setFromUnitVectors(
      new THREE.Vector3(1, 0, 0),
      span.clone().normalize(),
    );
    const deckLength = length + 0.55;
    const deck = new THREE.Mesh(
      new THREE.BoxGeometry(deckLength, 0.14, 0.78),
      materials.wood,
    );
    const underBeamLeft = new THREE.Mesh(
      new THREE.BoxGeometry(deckLength, 0.16, 0.1),
      materials.dark,
    );
    underBeamLeft.position.set(0, -0.13, -0.31);
    const underBeamRight = underBeamLeft.clone();
    underBeamRight.position.z = 0.31;
    const railLeft = new THREE.Mesh(
      new THREE.BoxGeometry(deckLength, 0.08, 0.07),
      materials.primary,
    );
    railLeft.position.set(0, 0.29, -0.39);
    const railRight = railLeft.clone();
    railRight.position.z = 0.39;
    assembly.add(deck, underBeamLeft, underBeamRight, railLeft, railRight);

    const postCount = THREE.MathUtils.clamp(Math.ceil(length / 0.7), 2, 48);
    for (let index = 0; index <= postCount; index += 1) {
      const x = THREE.MathUtils.lerp(
        -deckLength / 2 + 0.18,
        deckLength / 2 - 0.18,
        index / postCount,
      );
      const sleeper = new THREE.Mesh(
        new THREE.BoxGeometry(0.08, 0.07, 0.92),
        materials.pale,
      );
      sleeper.position.set(x, 0.11, 0);
      assembly.add(sleeper);
      if (index % 2 !== 0 && index !== postCount) continue;
      for (const z of [-0.39, 0.39]) {
        const post = new THREE.Mesh(
          new THREE.BoxGeometry(0.07, 0.48, 0.07),
          materials.primary,
        );
        post.position.set(x, 0.18, z);
        assembly.add(post);
      }
    }

    const farBridgehead = bridgehead.clone();
    farBridgehead.position.set(span.x, span.y + 0.09, span.z);
    group.add(assembly, farBridgehead);
  }

  _buildStructure(group, piece) {
    const materials = this._pieceMaterials(piece);
    if (piece.type === "bridge") {
      this._buildBridge(group, piece, materials);
      return;
    }
    const span = Math.max(1, piece.footprintSize ?? 1);
    const modelWidth = Math.max(span, piece.modelWidth ?? span);
    const modelDepth = Math.max(span, piece.modelDepth ?? span);
    const foundation = new THREE.Mesh(
      new THREE.BoxGeometry(modelWidth * 0.92, 0.14, modelDepth * 0.92),
      materials.secondary,
    );
    foundation.position.y = 0.09;
    group.add(foundation);

    if (piece.type === "objective") {
      const mast = new THREE.Mesh(
        new THREE.CylinderGeometry(0.035, 0.05, 1.85, 8),
        materials.pale,
      );
      mast.position.y = 1;
      const flag = new THREE.Mesh(
        new THREE.PlaneGeometry(0.82, 0.48),
        materials.primary,
      );
      flag.position.set(0.42, 1.65, 0);
      const beacon = new THREE.Mesh(
        new THREE.CylinderGeometry(0.18, 0.28, 0.32, 8),
        materials.secondary,
      );
      beacon.position.y = 0.26;
      group.add(mast, flag, beacon);
    } else if (piece.type === "airfield") {
      const runway = new THREE.Mesh(
        new THREE.BoxGeometry(modelWidth * 0.58, 0.055, modelDepth * 0.42),
        materials.dark,
      );
      runway.position.set(modelWidth * 0.17, 0.18, -modelDepth * 0.16);
      const stripeMaterial = piece.pending
        ? materials.pale
        : new THREE.MeshStandardMaterial({
            color: 0xd2c779,
            roughness: 0.7,
            metalness: 0.05,
          });
      const stripes = [-0.95, -0.48, 0, 0.48, 0.95].map((x) => {
        const stripe = new THREE.Mesh(
          new THREE.BoxGeometry(0.18, 0.012, 0.045),
          stripeMaterial,
        );
        stripe.position.set(modelWidth * 0.17 + x, 0.214, -modelDepth * 0.16);
        return stripe;
      });
      const hangar = new THREE.Mesh(
        new THREE.BoxGeometry(span * 0.42, 0.58, span * 0.3),
        materials.canvas,
      );
      hangar.position.set(-modelWidth * 0.3, 0.45, modelDepth * 0.25);
      const hangarRoof = new THREE.Mesh(
        new THREE.CylinderGeometry(
          span * 0.15,
          span * 0.15,
          span * 0.44,
          12,
          1,
          false,
          0,
          Math.PI,
        ),
        materials.primary,
      );
      hangarRoof.rotation.z = Math.PI / 2;
      hangarRoof.position.set(-modelWidth * 0.3, 0.74, modelDepth * 0.25);
      const tower = new THREE.Mesh(
        new THREE.BoxGeometry(0.34, 0.78, 0.34),
        materials.secondary,
      );
      tower.position.set(-modelWidth * 0.08, 0.56, modelDepth * 0.28);
      const towerCab = new THREE.Mesh(
        new THREE.BoxGeometry(0.52, 0.24, 0.52),
        materials.pale,
      );
      towerCab.position.set(-modelWidth * 0.08, 1.02, modelDepth * 0.28);

      const apron = new THREE.Mesh(
        new THREE.BoxGeometry(modelWidth * 0.48, 0.045, modelDepth * 0.35),
        materials.dark,
      );
      apron.position.set(modelWidth * 0.22, 0.18, modelDepth * 0.27);
      const bayLines = [];
      for (let bay = 0; bay < 3; bay += 1) {
        const bayX = modelWidth * 0.02 + bay * modelWidth * 0.2;
        for (const offsetX of [-0.075, 0.075]) {
          const line = new THREE.Mesh(
            new THREE.BoxGeometry(0.025, 0.012, modelDepth * 0.25),
            stripeMaterial,
          );
          line.position.set(bayX + offsetX, 0.209, modelDepth * 0.27);
          bayLines.push(line);
        }
        const stopLine = new THREE.Mesh(
          new THREE.BoxGeometry(0.18, 0.012, 0.025),
          stripeMaterial,
        );
        stopLine.position.set(bayX, 0.21, modelDepth * 0.39);
        bayLines.push(stopLine);
      }
      group.add(
        runway,
        ...stripes,
        apron,
        ...bayLines,
        hangar,
        hangarRoof,
        tower,
        towerCab,
      );
    } else if (piece.type === "fieldHospital") {
      const tent = new THREE.Mesh(
        new THREE.CylinderGeometry(span * 0.34, span * 0.52, 0.82, 4),
        materials.canvas,
      );
      tent.position.y = 0.52;
      tent.rotation.y = Math.PI / 4;
      const crossVertical = new THREE.Mesh(
        new THREE.BoxGeometry(0.08, 0.42, 0.05),
        materials.pale,
      );
      crossVertical.position.set(0, 0.62, span * 0.49);
      const crossHorizontal = new THREE.Mesh(
        new THREE.BoxGeometry(0.32, 0.09, 0.05),
        materials.pale,
      );
      crossHorizontal.position.copy(crossVertical.position);
      group.add(tent, crossVertical, crossHorizontal);
    } else if (piece.type === "watchtower") {
      const legs = [];
      for (const x of [-0.27, 0.27])
        for (const z of [-0.27, 0.27]) {
          const leg = new THREE.Mesh(
            new THREE.CylinderGeometry(0.045, 0.065, 1.45, 5),
            materials.wood,
          );
          leg.position.set(x, 0.82, z);
          legs.push(leg);
        }
      const platform = new THREE.Mesh(
        new THREE.BoxGeometry(0.82, 0.12, 0.82),
        materials.secondary,
      );
      platform.position.y = 1.48;
      const cabin = new THREE.Mesh(
        new THREE.BoxGeometry(0.62, 0.42, 0.62),
        materials.canvas,
      );
      cabin.position.y = 1.74;
      const roof = new THREE.Mesh(
        new THREE.ConeGeometry(0.58, 0.32, 4),
        materials.primary,
      );
      roof.position.y = 2.1;
      roof.rotation.y = Math.PI / 4;
      const antenna = new THREE.Mesh(
        new THREE.CylinderGeometry(0.018, 0.018, 0.72, 5),
        materials.pale,
      );
      antenna.position.set(0.2, 2.45, 0.08);
      group.add(...legs, platform, cabin, roof, antenna);
    } else if (piece.type === "barracks") {
      const building = new THREE.Mesh(
        new THREE.BoxGeometry(span * 0.62, 0.68, span * 0.56),
        materials.canvas,
      );
      building.position.y = 0.48;
      const roof = new THREE.Mesh(
        new THREE.ConeGeometry(span * 0.48, 0.42, 4),
        materials.primary,
      );
      roof.position.y = 1.03;
      roof.rotation.y = Math.PI / 4;
      const door = new THREE.Mesh(
        new THREE.BoxGeometry(0.36, 0.48, 0.035),
        materials.dark,
      );
      door.position.set(0, 0.42, span * 0.285);
      const sandbags = [-0.8, -0.4, 0, 0.4, 0.8].map((x) => {
        const bag = new THREE.Mesh(
          new THREE.BoxGeometry(0.3, 0.1, 0.16),
          materials.secondary,
        );
        bag.position.set(x, 0.2, span * 0.38);
        return bag;
      });
      const antenna = new THREE.Mesh(
        new THREE.CylinderGeometry(0.012, 0.012, 1.2, 6),
        materials.pale,
      );
      antenna.position.set(-span * 0.34, 1.25, -span * 0.22);
      group.add(building, roof, door, antenna, ...sandbags);
    } else {
      const tankPositions = [
        [-0.72, -0.62],
        [-0.72, 0],
        [-0.72, 0.62],
        [0, -0.62],
        [0, 0],
        [0, 0.62],
      ];
      const barrels = tankPositions.map(([x, z], index) => {
        const drum = new THREE.Mesh(
          new THREE.CylinderGeometry(0.24, 0.24, 0.62, 12),
          index % 2 ? materials.secondary : materials.primary,
        );
        drum.position.set(x, 0.43, z);
        return drum;
      });
      const crate = new THREE.Mesh(
        new THREE.BoxGeometry(0.62, 0.42, 0.62),
        materials.wood,
      );
      crate.position.set(0.72, 0.34, -0.55);
      const pump = new THREE.Mesh(
        new THREE.CylinderGeometry(0.06, 0.08, 0.9, 8),
        materials.pale,
      );
      pump.position.set(0.75, 0.65, 0.45);
      const pumpHead = new THREE.Mesh(
        new THREE.BoxGeometry(0.42, 0.1, 0.1),
        materials.dark,
      );
      pumpHead.position.set(0.86, 1.08, 0.45);
      group.add(...barrels, crate, pump, pumpHead);
    }
  }

  _addHealth(group, piece) {
    const total = Math.max(piece.maxHp || 1, 1);
    for (let i = 0; i < total; i += 1) {
      const alive = i < piece.hp;
      const segment = new THREE.Mesh(
        new THREE.BoxGeometry(0.13, 0.045, 0.05),
        new THREE.MeshBasicMaterial({
          color: alive
            ? piece.owner === 0
              ? COLORS.player
              : COLORS.enemy
            : 0x303630,
        }),
      );
      segment.position.set(
        (i - (total - 1) / 2) * 0.16,
        piece.kind === "structure" ? 1.5 : 1.03,
        0,
      );
      group.add(segment);
    }
  }

  _addSelectionRing(group) {
    const ring = new THREE.Mesh(
      new THREE.TorusGeometry(0.46, 0.035, 8, 32),
      new THREE.MeshBasicMaterial({ color: COLORS.lime }),
    );
    ring.rotation.x = Math.PI / 2;
    ring.position.y = 0.045;
    ring.userData.spin = true;
    this.spinningObjects.push(ring);
    group.add(ring);
  }

  _addOrderRing(group, kind) {
    const ring = new THREE.Mesh(
      new THREE.TorusGeometry(0.41, 0.022, 6, 24),
      new THREE.MeshBasicMaterial({
        color: kind === "attack" ? COLORS.target : COLORS.lime,
        transparent: true,
        opacity: 0.8,
      }),
    );
    ring.rotation.x = Math.PI / 2;
    ring.position.y = 0.12;
    ring.userData.spin = true;
    this.spinningObjects.push(ring);
    group.add(ring);
  }

  animateMoves(moves) {
    const active = moves
      .map((move) => {
        const object = this.pieceMeshes.get(move.id);
        const end = this._mapPosition(
          move.to.x,
          move.to.y,
          move.to.worldX,
          move.to.worldY,
        );
        const start = object?.position.clone();
        const routeHeight = move.actorType === "plane" ? 1.6 : 0.01;
        end.y += routeHeight;
        const via = (Array.isArray(move.via) ? move.via : []).map((point) => {
          const position = this._mapPosition(
            point.x,
            point.y,
            point.worldX,
            point.worldY,
          );
          position.y += routeHeight;
          return position;
        });
        const route = [start, ...via, end].filter(Boolean);
        const cumulative = [0];
        for (let index = 1; index < route.length; index += 1)
          cumulative.push(
            cumulative[index - 1] + route[index - 1].distanceTo(route[index]),
          );
        const flightDistance = cumulative[cumulative.length - 1] ?? 0;
        const duration =
          move.actorType === "plane"
            ? THREE.MathUtils.clamp(
                BOMBER_FLIGHT_TIMING.outboundBaseMs +
                  flightDistance *
                    BOMBER_FLIGHT_TIMING.outboundMillisecondsPerTile,
                BOMBER_FLIGHT_TIMING.outboundMinimumMs,
                BOMBER_FLIGHT_TIMING.outboundMaximumMs,
              )
            : 760;
        return { move, object, start, end, route, cumulative, duration };
      })
      .filter((item) => item.object);
    for (const item of active) item.facing = item.object.rotation.y;
    if (!active.length) return Promise.resolve();

    return new Promise((resolve) => {
      let complete = false;
      let frame = 0;
      const started = performance.now();
      const maximumDuration = Math.max(...active.map((item) => item.duration));
      const finish = () => {
        if (complete) return;
        complete = true;
        cancelAnimationFrame(frame);
        for (const item of active) {
          item.object.position.copy(item.end);
          item.object.rotation.y = item.facing;
        }
        resolve();
      };
      const tick = (now) => {
        if (complete) return;
        let allFinished = true;
        for (const item of active) {
          const progress = Math.min((now - started) / item.duration, 1);
          const eased = 1 - Math.pow(1 - progress, 3);
          const routeDistance =
            item.cumulative[item.cumulative.length - 1] * eased;
          let routeIndex = 1;
          while (
            routeIndex < item.cumulative.length - 1 &&
            item.cumulative[routeIndex] < routeDistance
          )
            routeIndex += 1;
          const segmentStart = item.cumulative[routeIndex - 1] ?? 0;
          const segmentLength = Math.max(
            (item.cumulative[routeIndex] ?? segmentStart) - segmentStart,
            1e-6,
          );
          item.object.position.lerpVectors(
            item.route[routeIndex - 1],
            item.route[routeIndex],
            THREE.MathUtils.clamp(
              (routeDistance - segmentStart) / segmentLength,
              0,
              1,
            ),
          );
          item.object.position.y =
            item.object.position.y +
            Math.sin(progress * Math.PI) *
              (item.move.actorType === "plane" ? 0.35 : 0.25);
          item.object.rotation.y =
            item.facing + Math.sin(progress * Math.PI) * 0.12;
          if (progress < 1) allFinished = false;
        }
        if (allFinished) finish();
        else frame = requestAnimationFrame(tick);
      };
      frame = requestAnimationFrame(tick);
      window.setTimeout(finish, maximumDuration + 180);
    });
  }

  async animateBomberBombs(attacks) {
    for (const attack of attacks) {
      if (this.destroyed) return;
      await this._animateBomberBomb(attack);
    }
  }

  _animateBomberBomb(attack) {
    const bomber = this.pieceMeshes.get(attack.actorId);
    const target = this._cellPosition(attack.to.x, attack.to.y);
    target.y += 0.08;
    const flightAltitude = target.y + 1.52;
    const start =
      bomber?.position.clone() ?? target.clone().setY(flightAltitude);
    start.y = Math.max(start.y, flightAltitude);
    const bomb = new THREE.Mesh(
      new THREE.CapsuleGeometry(0.055, 0.18, 5, 8),
      new THREE.MeshStandardMaterial({
        color: 0x282d27,
        roughness: 0.5,
        metalness: 0.65,
      }),
    );
    bomb.rotation.z = Math.PI / 2;
    bomb.position.copy(start);
    this.effectGroup.add(bomb);
    return this._animateTimed(680, (progress) => {
      const eased = progress * progress;
      bomb.position.lerpVectors(start, target, eased);
      bomb.rotation.x += 0.13;
      bomb.rotation.z += 0.18;
    }).then(async () => {
      this._disposeEffect(bomb);
      await this._animateImpact(target, {
        mortar: true,
        extraHeavy: true,
        destructive: Boolean(attack.hit && attack.targetId),
        hit: attack.hit,
        quality: DEFAULT_EFFECT_QUALITY,
      });
    });
  }

  animateBomberReturns(missions) {
    return Promise.all(
      missions.map((mission) => {
        const bomber = this.pieceMeshes.get(mission.id);
        if (!bomber) return Promise.resolve();
        const start = bomber.position.clone();
        const destination = this._mapPosition(
          mission.to.x,
          mission.to.y,
          mission.to.worldX,
          mission.to.worldY,
        );
        destination.y += 0.01;
        start.y = Math.max(start.y, destination.y + 1.59);
        const facing = Math.atan2(
          -(destination.z - start.z),
          destination.x - start.x,
        );
        const flightDistance = Math.hypot(
          destination.x - start.x,
          destination.z - start.z,
        );
        const duration = THREE.MathUtils.clamp(
          BOMBER_FLIGHT_TIMING.returnBaseMs +
            flightDistance * BOMBER_FLIGHT_TIMING.returnMillisecondsPerTile,
          BOMBER_FLIGHT_TIMING.returnMinimumMs,
          BOMBER_FLIGHT_TIMING.returnMaximumMs,
        );
        bomber.rotation.y = facing;
        return this._animateTimed(duration, (progress) => {
          const eased = 1 - Math.pow(1 - progress, 2.5);
          bomber.position.x = THREE.MathUtils.lerp(
            start.x,
            destination.x,
            eased,
          );
          bomber.position.z = THREE.MathUtils.lerp(
            start.z,
            destination.z,
            eased,
          );
          const descent = Math.max(0, (progress - 0.68) / 0.32);
          bomber.position.y = THREE.MathUtils.lerp(
            start.y,
            destination.y,
            descent * descent,
          );
          bomber.rotation.z = Math.sin(progress * Math.PI) * -0.12;
        }).then(() => {
          bomber.position.copy(destination);
          bomber.rotation.z = 0;
        });
      }),
    );
  }

  async animateAttacks(attacks) {
    if (!attacks.length || this.destroyed) return Promise.resolve();
    for (const attack of attacks) {
      if (this.destroyed) return;
      try {
        await this._animateAttack(attack, DEFAULT_EFFECT_QUALITY);
      } catch (error) {
        console.error("A projectile animation was skipped.", error);
      }
    }
  }

  async animateInterceptions(attacks) {
    if (!attacks.length || this.destroyed) return;
    const results = await Promise.allSettled(
      attacks.map((attack) =>
        this._animateAttack(attack, DEFAULT_EFFECT_QUALITY),
      ),
    );
    for (const result of results)
      if (result.status === "rejected")
        console.error("An interception animation was skipped.", result.reason);
  }

  async animateDestructions(pieceIds) {
    if (!pieceIds.length || this.destroyed) return Promise.resolve();
    const results = await Promise.allSettled(
      pieceIds.map((pieceId) => {
        const object = this.pieceMeshes.get(pieceId);
        if (!object) return Promise.resolve();
        const origin = object.position.clone();
        const startingRotation = object.rotation.clone();
        return Promise.all([
          this._animateImpact(origin, {
            mortar: true,
            destructive: true,
            hit: true,
          }),
          this._animateTimed(720, (progress) => {
            const shake = (1 - progress) * 0.045;
            object.position.x = origin.x + Math.sin(progress * 70) * shake;
            object.position.z = origin.z + Math.cos(progress * 63) * shake;
            object.position.y = origin.y - Math.pow(progress, 1.7) * 0.35;
            object.rotation.x = startingRotation.x + progress * 0.7;
            object.rotation.z = startingRotation.z + progress * 0.45;
            object.scale.setScalar(Math.max(0.16, 1 - progress * 0.72));
          }),
        ]);
      }),
    );
    for (const result of results) {
      if (result.status === "rejected")
        console.error("A destruction animation was skipped.", result.reason);
    }
  }

  _animateAttack(attack, quality = DEFAULT_EFFECT_QUALITY) {
    const shooter = this.pieceMeshes.get(attack.actorId);
    const target = this._mapPosition(
      attack.to.x,
      attack.to.y,
      attack.to.worldX,
      attack.to.worldY,
    );
    target.y += Number.isFinite(attack.targetAltitude)
      ? attack.targetAltitude - 0.12
      : 0.12;
    const isArtillery =
      attack.actorType === "long" || attack.actorType === "xtralong";
    const extraHeavy = attack.actorType === "xtralong";
    const showMiss =
      attack.owner === 0 && attack.hit === false && Boolean(attack.targetId);
    const showHit = attack.hit === true && Boolean(attack.targetId);
    if (!shooter) {
      return this._delay(isArtillery ? (extraHeavy ? 640 : 520) : 180).then(
        () =>
          Promise.all([
            this._animateImpact(target, {
              mortar: isArtillery,
              extraHeavy,
              hit: attack.hit,
              quality,
            }),
            showMiss ? this._animateMissLabel(target) : Promise.resolve(),
            showHit ? this._animateHitLabel(target) : Promise.resolve(),
          ]),
      );
    }

    const start = shooter.position.clone();
    start.y +=
      attack.actorType === "short"
        ? 0.54
        : attack.actorType === "medium"
          ? 0.46
          : 0.52;
    const muzzleDirection = target.clone().sub(start);
    muzzleDirection.y = 0;
    if (muzzleDirection.lengthSq() > 0)
      start.addScaledVector(
        muzzleDirection.normalize(),
        attack.actorType === "medium"
          ? 0.62
          : attack.actorType === "short"
            ? 0.5
            : extraHeavy
              ? 0.78
              : 0.38,
      );
    const facing = Math.atan2(-(target.z - start.z), target.x - start.x);
    const previousFacing = shooter.rotation.y;
    shooter.rotation.y = facing;

    const restoreFacing = () => {
      if (shooter.parent) shooter.rotation.y = previousFacing;
    };
    if (isArtillery)
      return this._animateMortarShot(
        start,
        target,
        attack.hit,
        shooter,
        showMiss,
        showHit,
        quality,
        extraHeavy,
      ).finally(restoreFacing);
    return this._animateDirectShot(
      start,
      target,
      attack.hit,
      attack.actorType,
      shooter,
      showMiss,
      showHit,
      quality,
    ).finally(restoreFacing);
  }

  _animateDirectShot(
    start,
    target,
    hit,
    actorType,
    shooter,
    showMiss = false,
    showHit = false,
    quality = DEFAULT_EFFECT_QUALITY,
  ) {
    const projectile = new THREE.Group();
    const tracer = new THREE.Mesh(
      new THREE.SphereGeometry(actorType === "medium" ? 0.065 : 0.045, 8, 6),
      new THREE.MeshBasicMaterial({
        color: actorType === "medium" ? 0xffc46b : 0xffe6a3,
      }),
    );
    tracer.scale.set(2.8, 0.72, 0.72);
    const light = this._createTransientLight(
      0xffa64f,
      actorType === "medium" ? 2.5 : 1.6,
      2.4,
      quality.dynamicLights,
    );
    projectile.add(tracer);
    if (light) projectile.add(light);
    projectile.position.copy(start);
    projectile.quaternion.setFromUnitVectors(
      new THREE.Vector3(1, 0, 0),
      target.clone().sub(start).normalize(),
    );
    this.effectGroup.add(projectile);

    const flash = this._createMuzzleFlash(
      start,
      actorType === "medium" ? 1.25 : 0.9,
      0xffb451,
      quality.dynamicLights,
    );
    const shotDistance = start.distanceTo(target);
    const duration = THREE.MathUtils.clamp(240 + shotDistance * 34, 300, 620);
    const recoilOrigin = shooter.position.clone();
    return this._animateTimed(duration, (progress) => {
      const eased = 1 - Math.pow(1 - progress, 2);
      projectile.position.lerpVectors(start, target, eased);
      projectile.position.y += Math.sin(progress * Math.PI) * 0.08;
      const recoil = Math.sin(Math.min(progress * 4, 1) * Math.PI) * 0.055;
      shooter.position.x =
        recoilOrigin.x - Math.cos(shooter.rotation.y) * recoil;
      shooter.position.z =
        recoilOrigin.z + Math.sin(shooter.rotation.y) * recoil;
      flash.scale.setScalar(1 + progress * 2.4);
      flash.traverse((item) => {
        if (item.material)
          item.material.opacity = Math.max(0, 1 - progress * 4);
      });
    }).then(async () => {
      shooter.position.copy(recoilOrigin);
      this._disposeEffect(projectile);
      this._disposeEffect(flash);
      await Promise.all([
        this._animateImpact(target, {
          mortar: false,
          hit,
          heavy: actorType === "medium",
          quality,
        }),
        showMiss ? this._animateMissLabel(target) : Promise.resolve(),
        showHit ? this._animateHitLabel(target) : Promise.resolve(),
      ]);
    });
  }

  _animateMortarShot(
    start,
    target,
    hit,
    shooter,
    showMiss = false,
    showHit = false,
    quality = DEFAULT_EFFECT_QUALITY,
    extraHeavy = false,
  ) {
    const shell = new THREE.Group();
    const shellBody = new THREE.Mesh(
      new THREE.CylinderGeometry(0.032, 0.045, 0.18, 8),
      new THREE.MeshStandardMaterial({
        color: 0x252821,
        roughness: 0.48,
        metalness: 0.62,
      }),
    );
    const shellTip = new THREE.Mesh(
      new THREE.ConeGeometry(0.033, 0.07, 8),
      new THREE.MeshStandardMaterial({
        color: 0x77735d,
        roughness: 0.55,
        metalness: 0.38,
      }),
    );
    shellTip.position.y = 0.125;
    const shellGlow = new THREE.Mesh(
      new THREE.SphereGeometry(0.072, 8, 6),
      new THREE.MeshBasicMaterial({
        color: 0xffd36f,
        transparent: true,
        opacity: 0.95,
        blending: THREE.AdditiveBlending,
        depthWrite: false,
        toneMapped: false,
      }),
    );
    const shellLight = this._createTransientLight(
      0xffa63d,
      2.8,
      3.5,
      quality.dynamicLights,
    );
    shell.add(shellBody, shellTip, shellGlow);
    if (shellLight) shell.add(shellLight);
    if (extraHeavy) shell.scale.setScalar(1.28);
    shell.position.copy(start);
    this.effectGroup.add(shell);

    const trailPointCount = quality.trailPoints;
    const trailPositions = new Float32Array(trailPointCount * 3);
    const trailColors = new Float32Array(trailPointCount * 3);
    const dimTrailColor = new THREE.Color(0x6b260b);
    const brightTrailColor = new THREE.Color(0xffe39a);
    for (let index = 0; index < trailPointCount; index += 1) {
      const offset = index * 3;
      trailPositions[offset] = start.x;
      trailPositions[offset + 1] = start.y;
      trailPositions[offset + 2] = start.z;
      const brightness = Math.pow(index / (trailPointCount - 1), 1.7);
      const color = dimTrailColor.clone().lerp(brightTrailColor, brightness);
      trailColors[offset] = color.r;
      trailColors[offset + 1] = color.g;
      trailColors[offset + 2] = color.b;
    }
    const trailGeometry = new THREE.BufferGeometry();
    const trailPositionAttribute = new THREE.BufferAttribute(trailPositions, 3);
    trailPositionAttribute.setUsage(THREE.DynamicDrawUsage);
    trailGeometry.setAttribute("position", trailPositionAttribute);
    trailGeometry.setAttribute(
      "color",
      new THREE.BufferAttribute(trailColors, 3),
    );
    const lightTrail = new THREE.Group();
    const trailLine = new THREE.Line(
      trailGeometry,
      new THREE.LineBasicMaterial({
        vertexColors: true,
        transparent: true,
        opacity: 0.9,
        blending: THREE.AdditiveBlending,
        depthWrite: false,
        toneMapped: false,
      }),
    );
    const trailPoints = new THREE.Points(
      trailGeometry,
      new THREE.PointsMaterial({
        size: 0.075,
        vertexColors: true,
        transparent: true,
        opacity: 0.72,
        blending: THREE.AdditiveBlending,
        depthWrite: false,
        toneMapped: false,
      }),
    );
    trailLine.frustumCulled = false;
    trailPoints.frustumCulled = false;
    lightTrail.add(trailLine, trailPoints);
    this.effectGroup.add(lightTrail);

    const smokeMaterial = new THREE.MeshStandardMaterial({
      color: 0x6c6d61,
      transparent: true,
      opacity: 0.32,
      roughness: 1,
      depthWrite: false,
    });
    const smokePuffCount = quality.smokePuffs;
    const smokePuffs = new THREE.InstancedMesh(
      new THREE.SphereGeometry(0.11, 7, 5),
      smokeMaterial,
      smokePuffCount,
    );
    smokePuffs.instanceMatrix.setUsage(THREE.DynamicDrawUsage);
    smokePuffs.frustumCulled = false;
    const smokePositions = Array(smokePuffCount).fill(null);
    const smokeTransform = new THREE.Object3D();
    smokeTransform.scale.setScalar(0.001);
    smokeTransform.updateMatrix();
    for (let index = 0; index < smokePuffCount; index += 1)
      smokePuffs.setMatrixAt(index, smokeTransform.matrix);
    smokePuffs.instanceMatrix.needsUpdate = true;
    this.effectGroup.add(smokePuffs);
    const flash = this._createMuzzleFlash(
      start,
      extraHeavy ? 1.8 : 1.45,
      0xb9a77c,
      quality.dynamicLights,
    );
    const shotDistance = start.distanceTo(target);
    const arcHeight = THREE.MathUtils.clamp(
      (extraHeavy ? 2.8 : 2.1) + shotDistance * (extraHeavy ? 0.32 : 0.28),
      extraHeavy ? 3.6 : 2.8,
      extraHeavy ? 8 : 6.2,
    );
    const duration = THREE.MathUtils.clamp(
      (extraHeavy ? 1120 : 960) + shotDistance * (extraHeavy ? 30 : 28),
      extraHeavy ? 1000 : 820,
      extraHeavy ? 2400 : 2000,
    );
    const recoilOrigin = shooter.position.clone();
    const shellPosition = new THREE.Vector3();
    const previousShellPosition = new THREE.Vector3();
    const shellDirection = new THREE.Vector3();
    const trailPosition = new THREE.Vector3();
    const shellUp = new THREE.Vector3(0, 1, 0);
    const positionAt = (amount, output) => {
      output.lerpVectors(start, target, amount);
      output.y += Math.sin(amount * Math.PI) * arcHeight;
      return output;
    };

    return this._animateTimed(duration, (progress) => {
      const previousProgress = Math.max(0, progress - 0.012);
      positionAt(progress, shellPosition);
      positionAt(previousProgress, previousShellPosition);
      shell.position.copy(shellPosition);
      shell.quaternion.setFromUnitVectors(
        shellUp,
        shellDirection
          .copy(shellPosition)
          .sub(previousShellPosition)
          .normalize(),
      );
      for (let index = 0; index < trailPointCount; index += 1) {
        const trailFactor = index / (trailPointCount - 1);
        const trailProgress = Math.max(0, progress - (1 - trailFactor) * 0.32);
        positionAt(trailProgress, trailPosition);
        const offset = index * 3;
        trailPositions[offset] = trailPosition.x;
        trailPositions[offset + 1] = trailPosition.y;
        trailPositions[offset + 2] = trailPosition.z;
      }
      trailPositionAttribute.needsUpdate = true;
      const visiblePuffs = Math.floor(progress * smokePuffCount);
      for (let index = 0; index < smokePuffCount; index += 1) {
        if (index > visiblePuffs) continue;
        if (!smokePositions[index])
          smokePositions[index] = positionAt(
            Math.max(0, index / smokePuffCount),
            new THREE.Vector3(),
          );
        const age = Math.max(0, progress - index / smokePuffCount);
        smokeTransform.position.copy(smokePositions[index]);
        smokeTransform.scale.setScalar(1 + age * 2.8);
        smokeTransform.updateMatrix();
        smokePuffs.setMatrixAt(index, smokeTransform.matrix);
      }
      smokePuffs.instanceMatrix.needsUpdate = true;
      smokeMaterial.opacity = Math.max(0, 0.32 - progress * 0.12);
      const recoil = Math.sin(Math.min(progress * 5, 1) * Math.PI) * 0.07;
      shooter.position.y = recoilOrigin.y - recoil;
      flash.scale.setScalar(1 + progress * 3.2);
      flash.traverse((item) => {
        if (item.material)
          item.material.opacity = Math.max(0, 1 - progress * 5);
      });
    }).then(async () => {
      shooter.position.copy(recoilOrigin);
      this._disposeEffect(shell);
      this._disposeEffect(flash);
      lightTrail.parent?.remove(lightTrail);
      trailGeometry.dispose();
      trailLine.material.dispose();
      trailPoints.material.dispose();
      this._disposeEffect(smokePuffs);
      await Promise.all([
        this._animateImpact(target, { mortar: true, extraHeavy, hit, quality }),
        showMiss ? this._animateMissLabel(target) : Promise.resolve(),
        showHit ? this._animateHitLabel(target) : Promise.resolve(),
      ]);
    });
  }

  _createTransientLight(color, intensity, distance, enabled = true) {
    if (!enabled || this.activeTransientLights >= this.maxTransientLights)
      return null;
    const light = new THREE.PointLight(color, intensity, distance);
    light.userData.transientLightActive = true;
    this.activeTransientLights += 1;
    return light;
  }

  _releaseTransientLights(effect) {
    effect?.traverse((object) => {
      if (!object.userData.transientLightActive) return;
      object.userData.transientLightActive = false;
      this.activeTransientLights = Math.max(0, this.activeTransientLights - 1);
    });
  }

  _createMuzzleFlash(position, scale = 1, color = 0xffb451, allowLight = true) {
    const flash = new THREE.Group();
    const core = new THREE.Mesh(
      new THREE.OctahedronGeometry(0.11 * scale),
      new THREE.MeshBasicMaterial({
        color,
        transparent: true,
        opacity: 1,
        depthWrite: false,
      }),
    );
    const light = this._createTransientLight(
      color,
      3.8 * scale,
      3.2 * scale,
      allowLight,
    );
    flash.add(core);
    if (light) flash.add(light);
    flash.position.copy(position);
    this.effectGroup.add(flash);
    return flash;
  }

  _animateImpact(
    position,
    {
      mortar = false,
      destructive = false,
      hit = false,
      heavy = false,
      extraHeavy = false,
      quality = DEFAULT_EFFECT_QUALITY,
    } = {},
  ) {
    const scale = destructive
      ? 1.65
      : extraHeavy
        ? 1.55
        : mortar
          ? 1.25
          : heavy
            ? 0.72
            : 0.48;
    const duration = destructive ? 760 : extraHeavy ? 820 : mortar ? 680 : 420;
    const effect = new THREE.Group();
    effect.position.copy(position);

    const core = new THREE.Mesh(
      new THREE.IcosahedronGeometry(0.18 * scale, 1),
      new THREE.MeshBasicMaterial({
        color: hit || mortar ? 0xff8b3d : 0xc8b58b,
        transparent: true,
        opacity: 0.95,
        depthWrite: false,
      }),
    );
    const smoke = new THREE.Mesh(
      new THREE.SphereGeometry(0.28 * scale, 10, 7),
      new THREE.MeshStandardMaterial({
        color: destructive ? 0x292a25 : 0x55564d,
        transparent: true,
        opacity: 0.68,
        roughness: 1,
        depthWrite: false,
      }),
    );
    smoke.position.y = 0.15 * scale;
    const shockwave = new THREE.Mesh(
      new THREE.TorusGeometry(0.2 * scale, 0.025 * scale, 6, 24),
      new THREE.MeshBasicMaterial({
        color: 0xf2c36f,
        transparent: true,
        opacity: 0.8,
        depthWrite: false,
      }),
    );
    shockwave.rotation.x = Math.PI / 2;
    const light = this._createTransientLight(
      0xff7438,
      6 * scale,
      5 * scale,
      quality.dynamicLights,
    );
    if (light) light.position.y = 0.25;
    effect.add(core, smoke, shockwave);
    if (light) effect.add(light);

    const fullDebrisCount = extraHeavy ? 18 : mortar || destructive ? 14 : 7;
    const debrisCount = Math.max(
      3,
      Math.round(fullDebrisCount * quality.debrisMultiplier),
    );
    const debris = new THREE.InstancedMesh(
      new THREE.BoxGeometry(0.035 * scale, 0.035 * scale, 0.035 * scale),
      new THREE.MeshStandardMaterial({ color: 0xffffff, roughness: 0.9 }),
      debrisCount,
    );
    debris.instanceMatrix.setUsage(THREE.DynamicDrawUsage);
    const debrisVelocities = [];
    const debrisTransform = new THREE.Object3D();
    for (let index = 0; index < debrisCount; index += 1) {
      const angle = (index / debrisCount) * Math.PI * 2;
      debrisVelocities.push(
        new THREE.Vector3(
          Math.cos(angle) * (0.75 + (index % 3) * 0.18) * scale,
          (0.8 + (index % 4) * 0.22) * scale,
          Math.sin(angle) * (0.75 + (index % 2) * 0.24) * scale,
        ),
      );
      debris.setColorAt(
        index,
        new THREE.Color(index % 3 === 0 ? 0xc66a3b : 0x4a4438),
      );
      debrisTransform.updateMatrix();
      debris.setMatrixAt(index, debrisTransform.matrix);
    }
    debris.instanceMatrix.needsUpdate = true;
    if (debris.instanceColor) debris.instanceColor.needsUpdate = true;
    effect.add(debris);
    this.effectGroup.add(effect);

    return this._animateTimed(duration, (progress) => {
      const flashProgress = Math.min(progress * 4, 1);
      core.scale.setScalar(0.35 + Math.sin(flashProgress * Math.PI) * 2.3);
      core.material.opacity = Math.max(0, 1 - progress * 2.6);
      smoke.scale.setScalar(
        0.35 + progress * (destructive ? 4.6 : mortar ? 3.8 : 2.4),
      );
      smoke.position.y =
        0.12 + progress * (destructive ? 1.1 : mortar ? 0.85 : 0.35);
      smoke.material.opacity = Math.max(0, 0.68 - progress * 0.58);
      shockwave.scale.setScalar(
        0.5 + progress * (destructive ? 5.6 : mortar ? 4.2 : 2.6),
      );
      shockwave.material.opacity = Math.max(0, 0.8 - progress);
      if (light) light.intensity = Math.max(0, 6 * scale * (1 - progress * 3));
      const seconds = (progress * duration) / 1000;
      debrisVelocities.forEach((velocity, index) => {
        debrisTransform.position.x = velocity.x * seconds;
        debrisTransform.position.z = velocity.z * seconds;
        debrisTransform.position.y = Math.max(
          0,
          velocity.y * seconds - 2.6 * seconds * seconds,
        );
        debrisTransform.rotation.x = progress * 9 + index * 0.31;
        debrisTransform.rotation.z = progress * 7 + index * 0.23;
        debrisTransform.updateMatrix();
        debris.setMatrixAt(index, debrisTransform.matrix);
      });
      debris.instanceMatrix.needsUpdate = true;
    }).then(() => this._disposeEffect(effect));
  }

  _createCombatLabelTexture(text, fillStyle, shadowColor) {
    const canvas = document.createElement("canvas");
    canvas.width = 256;
    canvas.height = 96;
    const context = canvas.getContext("2d");
    if (!context) return null;

    context.clearRect(0, 0, canvas.width, canvas.height);
    context.textAlign = "center";
    context.textBaseline = "middle";
    context.font = "900 58px Arial, sans-serif";
    context.lineJoin = "round";
    context.lineWidth = 11;
    context.strokeStyle = "rgba(35, 0, 0, 0.92)";
    context.shadowColor = shadowColor;
    context.shadowBlur = 18;
    context.strokeText(text, canvas.width / 2, canvas.height / 2 + 2);
    context.fillStyle = fillStyle;
    context.fillText(text, canvas.width / 2, canvas.height / 2 + 2);

    const texture = new THREE.CanvasTexture(canvas);
    texture.colorSpace = THREE.SRGBColorSpace;
    texture.needsUpdate = true;
    return texture;
  }

  _getMissTexture() {
    if (this.missTexture) return this.missTexture;
    this.missTexture = this._createCombatLabelTexture(
      "MISS",
      "#ff3b30",
      "rgba(255, 35, 25, 1)",
    );
    return this.missTexture;
  }

  _getHitTexture() {
    if (this.hitTexture) return this.hitTexture;
    this.hitTexture = this._createCombatLabelTexture(
      "HIT",
      "#ffd54a",
      "rgba(255, 170, 25, 1)",
    );
    return this.hitTexture;
  }

  _animateCombatLabel(position, texture, width, duration) {
    if (!texture) return Promise.resolve();
    const material = new THREE.SpriteMaterial({
      map: texture,
      color: 0xffffff,
      transparent: true,
      opacity: 0,
      depthTest: false,
      depthWrite: false,
      toneMapped: false,
    });
    const label = new THREE.Sprite(material);
    label.position.copy(position);
    label.position.y += 1.08;
    label.scale.set(width, 0.58, 1);
    label.renderOrder = 50;
    this.effectGroup.add(label);

    const startY = label.position.y;
    return this._animateTimed(duration, (progress) => {
      const appear = Math.min(progress / 0.12, 1);
      const fade = Math.min((1 - progress) / 0.32, 1);
      const flash = 0.68 + Math.abs(Math.sin(progress * Math.PI * 5)) * 0.32;
      material.opacity = appear * fade * flash;
      const scale =
        0.78 + Math.sin(Math.min(progress * 2, 1) * Math.PI * 0.5) * 0.28;
      label.scale.set(width * scale, 0.58 * scale, 1);
      label.position.y = startY + progress * 0.52;
    }).then(() => {
      this._disposeEffect(label);
    });
  }

  _animateMissLabel(position) {
    return this._animateCombatLabel(
      position,
      this._getMissTexture(),
      1.55,
      820,
    );
  }

  _animateHitLabel(position) {
    return this._animateCombatLabel(position, this._getHitTexture(), 1.28, 720);
  }

  _animateTimed(duration, update) {
    if (this.destroyed) return Promise.resolve();
    return new Promise((resolve, reject) => {
      const animation = {
        duration,
        update,
        resolve,
        reject,
        started: performance.now(),
        timeout: 0,
      };
      animation.timeout = window.setTimeout(
        () => this._finishTimedAnimation(animation),
        duration + 180,
      );
      this.timedAnimations.add(animation);
    });
  }

  _finishTimedAnimation(animation, applyFinalFrame = true, error = null) {
    if (!this.timedAnimations.delete(animation)) return;
    window.clearTimeout(animation.timeout);
    if (error) {
      animation.reject(error);
      return;
    }
    if (applyFinalFrame) {
      try {
        animation.update(1);
      } catch (finalFrameError) {
        console.error("An animation final frame was skipped.", finalFrameError);
      }
    }
    animation.resolve();
  }

  _updateTimedAnimations(now) {
    for (const animation of [...this.timedAnimations]) {
      const progress = Math.min(
        (now - animation.started) / animation.duration,
        1,
      );
      try {
        animation.update(progress);
      } catch (error) {
        console.error("An animation frame was skipped.", error);
        this._finishTimedAnimation(animation, false);
        continue;
      }
      if (progress >= 1) this._finishTimedAnimation(animation, false);
    }
  }

  _delay(duration) {
    return new Promise((resolve) => window.setTimeout(resolve, duration));
  }

  _disposeEffect(effect) {
    if (!effect) return;
    this._releaseTransientLights(effect);
    effect.parent?.remove(effect);
    effect.traverse((object) => {
      object.dispose?.();
      object.geometry?.dispose?.();
      if (Array.isArray(object.material))
        object.material.forEach((material) => material.dispose());
      else object.material?.dispose?.();
    });
  }

  _onPointerMove(event) {
    if (this.pointerDown) {
      const movement = Math.hypot(
        event.clientX - this.pointerDown.x,
        event.clientY - this.pointerDown.y,
      );
      if (movement > 5) this.pointerDown.dragged = true;
      if (this.pointerDown.boxSelecting && this.pointerDown.dragged) {
        event.preventDefault();
        event.stopImmediatePropagation();
        this._updateSelectionBox(event.clientX, event.clientY);
      }
    }
    const rect = this.renderer.domElement.getBoundingClientRect();
    this.pointer.x = ((event.clientX - rect.left) / rect.width) * 2 - 1;
    this.pointer.y = -((event.clientY - rect.top) / rect.height) * 2 + 1;
    this.raycaster.setFromCamera(this.pointer, this.camera);
    const terrainPoint = this._terrainSurfaceUnderPointer();
    const hit = this._tileUnderTerrainPoint(terrainPoint);
    this.hoveredMapPoint =
      hit && terrainPoint
        ? {
            x: terrainPoint.x + this.mapCenter.x,
            y: terrainPoint.z + this.mapCenter.y,
          }
        : null;
    this.hoveredTargetId = this._attackTargetUnderPointer()?.id ?? null;
    if (hit !== this.hoveredTile) {
      if (this.hoveredTile) this._applyTileAppearance(this.hoveredTile, false);
      this.hoveredTile = hit;
      if (this.hoveredTile) this._applyTileAppearance(this.hoveredTile, true);
      this._updatePlacementPreview(this.hoveredTile);
    }
    // Movement targeting follows the exact terrain intersection instead of
    // snapping its marker to the navigation cell centre.
    this._updateHoverReticle(this.hoveredTile);
    this.renderer.domElement.style.cursor = this.pointerDown?.dragged
      ? "grabbing"
      : this.hoveredTargetId
        ? "none"
      : this.model.routePlanning && hit
        ? "none"
        : this.placementPreviewGroup.children.length
          ? "crosshair"
          : hit
            ? "grab"
            : "default";
  }

  _terrainSurfaceUnderPointer() {
    const ray = this.raycaster.ray;
    if (ray.direction.y >= -1e-6) return null;

    // Limit ray marching to the horizontal footprint of the battlefield. This
    // prevents the raised terrain skirt from being mistaken for a top tile.
    const localBounds = {
      minX: this.mapBounds.minX - this.mapCenter.x,
      maxX: this.mapBounds.maxX - this.mapCenter.x,
      minZ: this.mapBounds.minY - this.mapCenter.y,
      maxZ: this.mapBounds.maxY - this.mapCenter.y,
    };
    let near = 0;
    let far = Number.POSITIVE_INFINITY;
    for (const [axis, minimum, maximum] of [
      ["x", localBounds.minX, localBounds.maxX],
      ["z", localBounds.minZ, localBounds.maxZ],
    ]) {
      const origin = ray.origin[axis];
      const direction = ray.direction[axis];
      if (Math.abs(direction) < 1e-8) {
        if (origin < minimum || origin > maximum) return null;
        continue;
      }
      const first = (minimum - origin) / direction;
      const second = (maximum - origin) / direction;
      near = Math.max(near, Math.min(first, second));
      far = Math.min(far, Math.max(first, second));
    }
    const surfaceBase = this.virtualized ? 0.02 : 0.06;
    const belowBoard = (surfaceBase - 0.08 - ray.origin.y) / ray.direction.y;
    far = Math.min(far, belowBoard);
    if (!Number.isFinite(far) || far <= near) return null;

    const heightDeltaAt = (distance) => {
      ray.at(distance, this.pointerIntersection);
      const mapX = this.pointerIntersection.x + this.mapCenter.x;
      const mapY = this.pointerIntersection.z + this.mapCenter.y;
      return (
        this.pointerIntersection.y -
        (surfaceBase + this._terrainHeightAtMapPoint(mapX, mapY))
      );
    };
    let previousDistance = near;
    let previousDelta = heightDeltaAt(previousDistance);
    // If the ray enters through the vertical skirt below the terrain surface,
    // there is no selectable top surface at this pointer location.
    if (previousDelta < 0) return null;
    const horizontalDistance =
      Math.hypot(ray.direction.x, ray.direction.z) * (far - near);
    const steps = Math.max(
      32,
      Math.min(256, Math.ceil(horizontalDistance * 2)),
    );
    let lower = null;
    let upper = null;
    for (let step = 1; step <= steps; step += 1) {
      const distance = near + ((far - near) * step) / steps;
      const delta = heightDeltaAt(distance);
      if (previousDelta >= 0 && delta <= 0) {
        lower = previousDistance;
        upper = distance;
        break;
      }
      previousDistance = distance;
      previousDelta = delta;
    }
    if (lower === null || upper === null) return null;

    // Refine the first height-field crossing so the selected cell follows the
    // same displaced surface the player actually sees.
    for (let iteration = 0; iteration < 10; iteration += 1) {
      const middle = (lower + upper) / 2;
      if (heightDeltaAt(middle) > 0) lower = middle;
      else upper = middle;
    }
    ray.at((lower + upper) / 2, this.pointerIntersection);
    return this.pointerIntersection;
  }

  _tileUnderTerrainPoint(point) {
    if (!point) return null;
    if (!this.virtualized) {
      this.lodRaycaster.ray.origin.set(
        point.x,
        this.terrainElevationScale + 2,
        point.z,
      );
      this.lodRaycaster.ray.direction.set(0, -1, 0);
      const intersection =
        this.lodRaycaster.intersectObjects(this.tileMeshes, false)[0] ?? null;
      return intersection
        ? (intersection.object.userData.instanceTiles?.[
            intersection.instanceId
          ] ?? null)
        : null;
    }

    const mapX = point.x + this.mapCenter.x;
    const mapY = point.z + this.mapCenter.y;
    const x = Math.floor(mapX + 0.5);
    const y = Math.floor(mapY + 0.5);
    if (
      x < Math.ceil(this.mapBounds.minX) ||
      x > Math.floor(this.mapBounds.maxX) ||
      y < Math.ceil(this.mapBounds.minY) ||
      y > Math.floor(this.mapBounds.maxY)
    )
      return null;
    const detailed = this.tileByKey.get(`${x},${y}`);
    if (detailed) return detailed;
    if (
      this.hoveredTile?.state === null &&
      this.hoveredTile.x === x &&
      this.hoveredTile.y === y
    )
      return this.hoveredTile;
    return { x, y, state: null };
  }

  _onPointerLeave() {
    if (this.hoveredTile) this._applyTileAppearance(this.hoveredTile, false);
    this.hoveredTile = null;
    this.hoveredMapPoint = null;
    this.hoveredTargetId = null;
    this._updateHoverReticle(null);
    this._updatePlacementPreview(null);
  }

  _onPointerDown(event) {
    const boxSelecting = event.button === 0 && event.shiftKey;
    this.pointerDown = {
      x: event.clientX,
      y: event.clientY,
      button: event.button,
      dragged: false,
      boxSelecting,
    };
    if (boxSelecting) {
      event.preventDefault();
      event.stopImmediatePropagation();
      this.controls.enabled = false;
      this.renderer.domElement.setPointerCapture?.(event.pointerId);
      this._updateSelectionBox(event.clientX, event.clientY);
    }
    this.renderer.domElement.style.cursor =
      this.model.routePlanning && event.button === 0
        ? "none"
        : event.button === 0 || event.button === 2
          ? "grabbing"
          : "default";
  }

  _onPointerUp(event) {
    const gesture = this.pointerDown;
    this.pointerDown = null;
    if (gesture?.boxSelecting) {
      this.controls.enabled = true;
      this.renderer.domElement.releasePointerCapture?.(event.pointerId);
      this.selectionBox.hidden = true;
      if (gesture.dragged) {
        event.preventDefault();
        event.stopImmediatePropagation();
        this._selectUnitsInScreenBox(gesture, event);
        return;
      }
    }
    this._onPointerMove(event);
    if (
      !gesture ||
      gesture.button !== 0 ||
      gesture.dragged ||
      !this.hoveredTile
    )
      return;
    const { x, y } = this.hoveredTile;
    this.onTileClick?.(x, y, {
      clientX: event.clientX,
      clientY: event.clientY,
      shiftKey: event.shiftKey,
      worldX: this.hoveredMapPoint?.x,
      worldY: this.hoveredMapPoint?.y,
      targetId: this.hoveredTargetId,
    });
  }

  _updateSelectionBox(clientX, clientY) {
    if (!this.pointerDown?.boxSelecting) return;
    const rect = this.renderer.domElement.getBoundingClientRect();
    const startX = this.pointerDown.x - rect.left;
    const startY = this.pointerDown.y - rect.top;
    const endX = clientX - rect.left;
    const endY = clientY - rect.top;
    this.selectionBox.hidden = false;
    this.selectionBox.style.left = `${Math.min(startX, endX)}px`;
    this.selectionBox.style.top = `${Math.min(startY, endY)}px`;
    this.selectionBox.style.width = `${Math.abs(endX - startX)}px`;
    this.selectionBox.style.height = `${Math.abs(endY - startY)}px`;
  }

  _selectUnitsInScreenBox(start, end) {
    const rect = this.renderer.domElement.getBoundingClientRect();
    const minimumX = Math.min(start.x, end.clientX);
    const maximumX = Math.max(start.x, end.clientX);
    const minimumY = Math.min(start.y, end.clientY);
    const maximumY = Math.max(start.y, end.clientY);
    const worldPosition = new THREE.Vector3();
    const selectedIds = [];
    for (const piece of this.model.pieces) {
      if (piece.kind !== "unit" || piece.owner !== 0) continue;
      const object = this.pieceMeshes.get(piece.id);
      if (!object) continue;
      object.getWorldPosition(worldPosition);
      const projected = worldPosition.clone().project(this.camera);
      if (projected.z < -1 || projected.z > 1) continue;
      const screenX = rect.left + ((projected.x + 1) / 2) * rect.width;
      const screenY = rect.top + ((1 - projected.y) / 2) * rect.height;
      if (
        screenX >= minimumX &&
        screenX <= maximumX &&
        screenY >= minimumY &&
        screenY <= maximumY
      )
        selectedIds.push(piece.id);
    }
    this.onBoxSelect?.(selectedIds);
  }

  _resize() {
    if (this.destroyed) return;
    const width = Math.max(this.container.clientWidth, 1);
    const height = Math.max(this.container.clientHeight, 1);
    this.renderer.setSize(width, height, false);
    this.camera.aspect = width / height;
    this.camera.updateProjectionMatrix();
    if (!this.hasFittedView) {
      this.resetView();
      return;
    }
    const fitDistance = this._fitDistance();
    const currentDistance = this.camera.position.distanceTo(
      this.controls.target,
    );
    if (!this.hasFocusedInitialView && currentDistance < fitDistance) {
      const direction = this.camera.position
        .clone()
        .sub(this.controls.target)
        .normalize();
      this.camera.position
        .copy(this.controls.target)
        .addScaledVector(direction, fitDistance);
    }
    this.controls.maxDistance = Math.max(
      this.controls.maxDistance,
      fitDistance * 1.8,
    );
    this.camera.far = Math.max(180, fitDistance + this.boardDiagonal * 3);
    this.camera.updateProjectionMatrix();
  }

  _fitDistance() {
    const verticalHalfFov = THREE.MathUtils.degToRad(this.camera.fov) / 2;
    const horizontalHalfFov = Math.atan(
      Math.tan(verticalHalfFov) * this.camera.aspect,
    );
    const right = new THREE.Vector3()
      .crossVectors(new THREE.Vector3(0, 1, 0), HOME_DIRECTION)
      .normalize();
    const up = new THREE.Vector3()
      .crossVectors(HOME_DIRECTION, right)
      .normalize();
    const halfWidth = this.cols / 2 + 0.5;
    const halfDepth = this.rows / 2 + 0.5;
    const margin = 1.12;
    let distance = 0;
    for (const x of [-halfWidth, halfWidth]) {
      for (const z of [-halfDepth, halfDepth]) {
        const corner = new THREE.Vector3(x, 0, z);
        const depthOffset = corner.dot(HOME_DIRECTION);
        distance = Math.max(
          distance,
          depthOffset +
            (Math.abs(corner.dot(right)) * margin) /
              Math.tan(horizontalHalfFov),
          depthOffset +
            (Math.abs(corner.dot(up)) * margin) / Math.tan(verticalHalfFov),
        );
      }
    }
    return distance + 1.5;
  }

  _focusPlayerUnits() {
    let playerPieces = this.model.pieces.filter(
      (piece) => piece.owner === 0 && piece.kind === "unit",
    );
    if (!playerPieces.length)
      playerPieces = this.model.pieces.filter((piece) => piece.owner === 0);
    if (!playerPieces.length) return false;

    const positions = playerPieces.map((piece) =>
      this._cellPosition(piece.x, piece.y),
    );
    const minX = Math.min(...positions.map((position) => position.x));
    const maxX = Math.max(...positions.map((position) => position.x));
    const minZ = Math.min(...positions.map((position) => position.z));
    const maxZ = Math.max(...positions.map((position) => position.z));
    const center = new THREE.Vector3(
      (minX + maxX) / 2,
      positions.reduce((total, position) => total + position.y, 0) /
        positions.length,
      (minZ + maxZ) / 2,
    );
    // Look slightly ahead of the formation so the player's next moves remain
    // visible while keeping the units near the foreground.
    center.x = Math.min(center.x + 1.25, (this.cols - 1) / 2);

    const verticalHalfFov = THREE.MathUtils.degToRad(this.camera.fov) / 2;
    const horizontalHalfFov = Math.atan(
      Math.tan(verticalHalfFov) * this.camera.aspect,
    );
    const formationWidth = maxX - minX + 3.5;
    const formationDepth = maxZ - minZ + 3.5;
    const focusDistance =
      Math.max(
        formationWidth / (2 * Math.tan(horizontalHalfFov)),
        formationDepth / (2 * Math.tan(verticalHalfFov)),
        7,
      ) * 1.18;
    const fullBoardDistance = this._fitDistance();
    const distance = Math.min(focusDistance, fullBoardDistance * 0.62);

    this.controls.target.copy(center);
    this.camera.position.copy(center).addScaledVector(HOME_DIRECTION, distance);
    this.camera.up.set(0, 1, 0);
    this.controls.minDistance = Math.max(
      3,
      Math.min(8, Math.min(this.cols, this.rows) * 0.14),
    );
    this.controls.maxDistance = Math.max(20, fullBoardDistance * 1.8);
    this.camera.far = Math.max(180, fullBoardDistance + this.boardDiagonal * 3);
    this.camera.lookAt(this.controls.target);
    this.camera.updateProjectionMatrix();
    this.controls.update();
    this.hasFittedView = true;
    return true;
  }

  getRenderBounds(maximumTiles = this.tileCapacity) {
    const aspect = Math.max(0.5, Math.min(2.5, this.camera.aspect || 1));
    const chunkSize = this.renderChunkSize;
    const tileMinimumX = Math.ceil(this.mapBounds.minX);
    const tileMaximumX = Math.floor(this.mapBounds.maxX);
    const tileMinimumY = Math.ceil(this.mapBounds.minY);
    const tileMaximumY = Math.floor(this.mapBounds.maxY);
    const mapTileWidth = Math.max(1, tileMaximumX - tileMinimumX + 1);
    const mapTileHeight = Math.max(1, tileMaximumY - tileMinimumY + 1);
    const chunkColumns = Math.ceil(mapTileWidth / chunkSize);
    const chunkRows = Math.ceil(mapTileHeight / chunkSize);
    const visibleBounds = this._visibleGroundBounds();

    if (visibleBounds) {
      const visibleMinimumTileX = THREE.MathUtils.clamp(
        Math.floor(visibleBounds.minX + 0.5),
        tileMinimumX,
        tileMaximumX,
      );
      const visibleMaximumTileX = THREE.MathUtils.clamp(
        Math.floor(visibleBounds.maxX + 0.5),
        tileMinimumX,
        tileMaximumX,
      );
      const visibleMinimumTileY = THREE.MathUtils.clamp(
        Math.floor(visibleBounds.minY + 0.5),
        tileMinimumY,
        tileMaximumY,
      );
      const visibleMaximumTileY = THREE.MathUtils.clamp(
        Math.floor(visibleBounds.maxY + 0.5),
        tileMinimumY,
        tileMaximumY,
      );
      const firstVisibleChunkX = Math.max(
        0,
        Math.floor((visibleMinimumTileX - tileMinimumX) / chunkSize),
      );
      const lastVisibleChunkX = Math.min(
        chunkColumns - 1,
        Math.floor((visibleMaximumTileX - tileMinimumX) / chunkSize),
      );
      const firstVisibleChunkY = Math.max(
        0,
        Math.floor((visibleMinimumTileY - tileMinimumY) / chunkSize),
      );
      const lastVisibleChunkY = Math.min(
        chunkRows - 1,
        Math.floor((visibleMaximumTileY - tileMinimumY) / chunkSize),
      );
      const visibleChunkBounds = {
        minX: tileMinimumX + firstVisibleChunkX * chunkSize,
        maxX: Math.min(
          tileMaximumX,
          tileMinimumX + (lastVisibleChunkX + 1) * chunkSize - 1,
        ),
        minY: tileMinimumY + firstVisibleChunkY * chunkSize,
        maxY: Math.min(
          tileMaximumY,
          tileMinimumY + (lastVisibleChunkY + 1) * chunkSize - 1,
        ),
      };
      const visibleChunkTileCount =
        (visibleChunkBounds.maxX - visibleChunkBounds.minX + 1) *
        (visibleChunkBounds.maxY - visibleChunkBounds.minY + 1);
      if (visibleChunkTileCount <= maximumTiles) {
        this.virtualRenderBounds = visibleChunkBounds;
        this.virtualRenderCoversViewport = true;
        return visibleChunkBounds;
      }
    }

    const maximumChunks = Math.max(
      1,
      Math.floor(maximumTiles / (chunkSize * chunkSize)),
    );
    let widthInChunks = Math.max(
      1,
      Math.floor(Math.sqrt(maximumChunks * aspect)),
    );
    let heightInChunks = Math.max(1, Math.floor(maximumChunks / widthInChunks));
    widthInChunks = Math.min(chunkColumns, widthInChunks);
    heightInChunks = Math.min(chunkRows, heightInChunks);
    const centerX = this.controls.target.x + this.mapCenter.x;
    const centerY = this.controls.target.z + this.mapCenter.y;
    const centerChunkX = Math.floor((centerX - tileMinimumX) / chunkSize);
    const centerChunkY = Math.floor((centerY - tileMinimumY) / chunkSize);
    const firstChunkX = Math.max(
      0,
      Math.min(
        chunkColumns - widthInChunks,
        centerChunkX - Math.floor(widthInChunks / 2),
      ),
    );
    const firstChunkY = Math.max(
      0,
      Math.min(
        chunkRows - heightInChunks,
        centerChunkY - Math.floor(heightInChunks / 2),
      ),
    );
    const fallbackBounds = {
      minX: tileMinimumX + firstChunkX * chunkSize,
      maxX: Math.min(
        tileMaximumX,
        tileMinimumX + (firstChunkX + widthInChunks) * chunkSize - 1,
      ),
      minY: tileMinimumY + firstChunkY * chunkSize,
      maxY: Math.min(
        tileMaximumY,
        tileMinimumY + (firstChunkY + heightInChunks) * chunkSize - 1,
      ),
    };
    this.virtualRenderBounds = fallbackBounds;
    this.virtualRenderCoversViewport = false;
    return fallbackBounds;
  }

  resetView() {
    const fitDistance = this._fitDistance();
    const centerHeight =
      0.12 + this._terrainHeightAtMapPoint(this.mapCenter.x, this.mapCenter.y);
    this.controls.target.set(0, centerHeight, 0);
    this.camera.position
      .copy(this.controls.target)
      .addScaledVector(HOME_DIRECTION, fitDistance);
    this.camera.up.set(0, 1, 0);
    this.controls.minDistance = Math.max(
      3,
      Math.min(8, Math.min(this.cols, this.rows) * 0.14),
    );
    this.controls.maxDistance = Math.max(20, fitDistance * 1.8);
    this.camera.far = Math.max(180, fitDistance + this.boardDiagonal * 3);
    this.camera.lookAt(this.controls.target);
    this.camera.updateProjectionMatrix();
    this.controls.update();
    this.hasFittedView = true;
  }

  focusTile(x, y, { distance = null } = {}) {
    const target = this._cellPosition(x, y);
    const direction = this.camera.position.clone().sub(this.controls.target);
    if (direction.lengthSq() < 0.0001) direction.copy(HOME_DIRECTION);
    else direction.normalize();
    const currentDistance = this.camera.position.distanceTo(
      this.controls.target,
    );
    const focusDistance = THREE.MathUtils.clamp(
      Number.isFinite(distance) ? distance : currentDistance,
      this.controls.minDistance,
      this.controls.maxDistance,
    );
    this.controls.target.copy(target);
    this.camera.position.copy(target).addScaledVector(direction, focusDistance);
    this.camera.lookAt(target);
    this.controls.update();
  }

  _renderLoop() {
    if (this.destroyed) return;
    const now = performance.now();
    const elapsed = this.clock.getElapsedTime();
    this._updateTimedAnimations(now);
    this._constrainControls();
    this.controls.update();
    if (this.virtualized) this._updateVirtualLod();
    for (const object of this.spinningObjects)
      object.rotation.z = elapsed * 0.75;
    const pulseScale = 1 + Math.sin(elapsed * 4) * 0.06;
    for (const object of this.pulsingObjects)
      object.scale.setScalar(pulseScale * this._markerScale(object));
    if (this.hoverReticle?.visible)
      this.hoverReticle.scale.setScalar(this._markerScale(this.hoverReticle));
    for (const flag of this.routeFlags) {
      flag.lookAt(
        this.camera.position.x,
        flag.position.y,
        this.camera.position.z,
      );
      flag.scale.setScalar(this._markerScale(flag));
    }
    this.renderer.render(this.scene, this.camera);
    this.animationFrame = requestAnimationFrame(() => this._renderLoop());
  }

  _updateVirtualLod() {
    const cameraDistance = this.camera.position.distanceTo(
      this.controls.target,
    );
    this._updateVirtualGridStep(cameraDistance);
    const verticalSpan =
      2 *
      cameraDistance *
      Math.tan(THREE.MathUtils.degToRad(this.camera.fov) / 2);
    const viewportHeight = Math.max(this.container.clientHeight, 1);
    const pixelsPerTile = viewportHeight / Math.max(verticalSpan, 1);
    const requiredPixels = this.virtualDetailVisible
      ? VIRTUAL_TILE_LOD.detailExitPixelsPerTile
      : VIRTUAL_TILE_LOD.detailEnterPixelsPerTile;
    const visibleBounds = this._visibleGroundBounds();
    const renderBounds = this.virtualRenderBounds;
    const viewportCovered = Boolean(
      visibleBounds &&
      renderBounds &&
      visibleBounds.minX >= renderBounds.minX - 0.5 &&
      visibleBounds.maxX <= renderBounds.maxX + 0.5 &&
      visibleBounds.minY >= renderBounds.minY - 0.5 &&
      visibleBounds.maxY <= renderBounds.maxY + 0.5,
    );
    // Orbiting at a constant distance now preserves LOD. Only projected tile
    // size can cross the separate enter/exit thresholds. The coverage guard
    // prevents a streamed chunk edge from ever entering the visible board.
    const nextVisible = pixelsPerTile >= requiredPixels && viewportCovered;
    if (nextVisible !== this.virtualDetailVisible) {
      this.virtualDetailVisible = nextVisible;
      if (this.virtualTileMesh) this.virtualTileMesh.visible = nextVisible;
    }
    const uniforms = this.virtualGrid?.material?.uniforms;
    if (uniforms?.gridEnabled) uniforms.gridEnabled.value = false;
  }

  _updateVirtualGridStep(cameraDistance) {
    const uniforms = this.virtualGrid?.material?.uniforms;
    if (!uniforms?.gridStep) return;
    const viewportHeight = Math.max(this.container.clientHeight, 1);
    const verticalSpan =
      2 *
      cameraDistance *
      Math.tan(THREE.MathUtils.degToRad(this.camera.fov) / 2);
    const desiredStep = Math.max(
      1,
      (verticalSpan / viewportHeight) * VIRTUAL_GRID_QUALITY.targetCellPixels,
    );
    const magnitude = 10 ** Math.floor(Math.log10(desiredStep));
    const normalized = desiredStep / magnitude;
    const interval =
      normalized <= 1 ? 1 : normalized <= 2 ? 2 : normalized <= 5 ? 5 : 10;
    const candidateStep = Math.max(1, interval * magnitude);
    const currentStep = Math.max(1, this.virtualGridStep || 1);
    const shouldIncrease =
      candidateStep > currentStep &&
      desiredStep >= currentStep * VIRTUAL_GRID_QUALITY.stepIncreaseThreshold;
    const shouldDecrease =
      candidateStep < currentStep &&
      desiredStep <= currentStep * VIRTUAL_GRID_QUALITY.stepDecreaseThreshold;
    if (shouldIncrease || shouldDecrease) {
      this.virtualGridStep = candidateStep;
      uniforms.gridStep.value = candidateStep;
    }
  }

  _visibleGroundBounds() {
    const samples = [
      [-1, -1],
      [0, -1],
      [1, -1],
      [-1, 0],
      [1, 0],
      [-1, 1],
      [0, 1],
      [1, 1],
    ];
    const points = [];
    for (const [x, y] of samples) {
      this.lodRaycaster.setFromCamera({ x, y }, this.camera);
      const point = this.lodRaycaster.ray.intersectPlane(
        this.boardPlane,
        new THREE.Vector3(),
      );
      // A ray above the horizon means the detailed patch boundary could enter
      // the frame, so strategic rendering is safer for this camera angle.
      if (!point) return null;
      points.push({
        x: THREE.MathUtils.clamp(
          point.x + this.mapCenter.x,
          this.mapBounds.minX,
          this.mapBounds.maxX,
        ),
        y: THREE.MathUtils.clamp(
          point.z + this.mapCenter.y,
          this.mapBounds.minY,
          this.mapBounds.maxY,
        ),
      });
    }
    return {
      minX: Math.min(...points.map((point) => point.x)),
      maxX: Math.max(...points.map((point) => point.x)),
      minY: Math.min(...points.map((point) => point.y)),
      maxY: Math.max(...points.map((point) => point.y)),
    };
  }

  _constrainControls() {
    const target = this.controls.target;
    const clampedX = THREE.MathUtils.clamp(
      target.x,
      -(this.cols - 1) / 2,
      (this.cols - 1) / 2,
    );
    const clampedZ = THREE.MathUtils.clamp(
      target.z,
      -(this.rows - 1) / 2,
      (this.rows - 1) / 2,
    );
    const clamped = new THREE.Vector3(
      clampedX,
      0.12 +
        this._terrainHeightAtMapPoint(
          clampedX + this.mapCenter.x,
          clampedZ + this.mapCenter.y,
        ),
      clampedZ,
    );
    const correction = clamped.sub(target);
    if (correction.lengthSq() > 0) {
      target.add(correction);
      this.camera.position.add(correction);
    }
  }

  _clearDynamicGroup(group) {
    for (const child of [...group.children]) {
      this._releaseTransientLights(child);
      group.remove(child);
      child.traverse((object) => {
        object.dispose?.();
        object.geometry?.dispose?.();
        if (Array.isArray(object.material))
          object.material.forEach((material) => material.dispose());
        else object.material?.dispose?.();
      });
    }
  }

  destroy() {
    this.destroyed = true;
    cancelAnimationFrame(this.animationFrame);
    for (const animation of [...this.timedAnimations])
      this._finishTimedAnimation(
        animation,
        false,
        new Error("Battlefield destroyed."),
      );
    this.resizeObserver.disconnect();
    this.renderer.domElement.removeEventListener(
      "pointermove",
      this._onPointerMove,
      true,
    );
    this.renderer.domElement.removeEventListener(
      "pointerleave",
      this._onPointerLeave,
    );
    this.renderer.domElement.removeEventListener(
      "pointerdown",
      this._onPointerDown,
      true,
    );
    this.renderer.domElement.removeEventListener(
      "pointerup",
      this._onPointerUp,
      true,
    );
    this.selectionBox?.remove();
    this.controls.removeEventListener("change", this._onControlsChange);
    if (this.viewportChangeFrame) clearTimeout(this.viewportChangeFrame);
    this.controls.dispose();
    this.scene.traverse((object) => {
      object.dispose?.();
      object.geometry?.dispose?.();
      if (Array.isArray(object.material))
        object.material.forEach((material) => material.dispose());
      else object.material?.dispose?.();
    });
    this.scene.clear();
    this.proceduralTerrainTexture?.dispose();
    this.proceduralTerrainTexture = null;
    this.proceduralTerrainHeightTexture?.dispose();
    this.proceduralTerrainHeightTexture = null;
    this.proceduralTerrainCanvas = null;
    this.proceduralTerrainHeightCanvas = null;
    this.virtualFogTexture?.dispose();
    this.virtualFogTexture = null;
    this.virtualCloudTexture?.dispose();
    this.virtualCloudTexture = null;
    this.virtualRangeTexture?.dispose();
    this.virtualRangeTexture = null;
    this.missTexture?.dispose();
    this.missTexture = null;
    this.hitTexture?.dispose();
    this.hitTexture = null;
    this.renderer.dispose();
    this.renderer.forceContextLoss();
    this.renderer.domElement.remove();
  }
}
