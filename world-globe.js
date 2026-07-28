import * as THREE from "three";

const OWNER_COLORS = {
  neutral: 0x87916f,
  friendly: 0x63b58a,
  hostile: 0xc86558,
};

const OCEAN_DETAIL = 8;
const LAND_DETAIL = 12;

const CONTINENTS = [
  [[-168, 72], [-142, 70], [-124, 58], [-112, 50], [-125, 32], [-106, 20], [-86, 24], [-80, 32], [-62, 47], [-82, 57], [-98, 72]],
  [[-82, 13], [-69, 10], [-50, 2], [-35, -20], [-52, -55], [-69, -43], [-79, -8]],
  [[-18, 36], [4, 45], [33, 32], [51, 12], [42, -13], [28, -35], [11, -34], [-6, 5]],
  [[-10, 36], [2, 58], [28, 71], [72, 76], [120, 67], [158, 59], [177, 43], [143, 34], [122, 20], [105, 8], [78, 8], [56, 24], [36, 34]],
  [[112, -11], [133, -10], [154, -25], [145, -43], [119, -35]],
  [[-58, 84], [-24, 80], [-18, 60], [-44, 58]],
  [[95, 7], [121, 4], [133, -10], [111, -8]],
  [[166, -34], [179, -38], [174, -48]],
];

function pointInPolygon(longitude, latitude, polygon) {
  let inside = false;
  for (
    let current = 0, previous = polygon.length - 1;
    current < polygon.length;
    previous = current++
  ) {
    const [currentX, currentY] = polygon[current];
    const [previousX, previousY] = polygon[previous];
    if (
      currentY > latitude !== previousY > latitude &&
      longitude <
        ((previousX - currentX) * (latitude - currentY)) /
          (previousY - currentY) +
          currentX
    )
      inside = !inside;
  }
  return inside;
}

function earthCoordinates(direction) {
  return {
    longitude: THREE.MathUtils.radToDeg(Math.atan2(direction.z, direction.x)),
    latitude: THREE.MathUtils.radToDeg(Math.asin(direction.y)),
  };
}

function isLand(direction) {
  const { longitude, latitude } = earthCoordinates(direction);
  return CONTINENTS.some((polygon) =>
    pointInPolygon(longitude, latitude, polygon),
  );
}

function sectorDirection(sector, world) {
  const latitude =
    world.height <= 1 ? 0 : 58 - (sector.y / (world.height - 1)) * 116;
  const longitude =
    -155 +
    (sector.x / world.width) * 360 +
    (sector.y % 2) * (180 / world.width);
  const directionAt = (candidateLongitude, candidateLatitude) => {
    const phi = THREE.MathUtils.degToRad(90 - candidateLatitude);
    const theta = THREE.MathUtils.degToRad(candidateLongitude);
    return new THREE.Vector3(
      Math.sin(phi) * Math.cos(theta),
      Math.cos(phi),
      Math.sin(phi) * Math.sin(theta),
    );
  };
  const direct = directionAt(longitude, latitude);
  if (isLand(direct)) return direct;
  const angleOffset = (sector.x * 2.17 + sector.y * 1.31) % (Math.PI * 2);
  for (let radius = 4; radius <= 88; radius += 4) {
    for (let step = 0; step < 16; step += 1) {
      const angle = angleOffset + (step / 16) * Math.PI * 2;
      const candidateLatitude = THREE.MathUtils.clamp(
        latitude + Math.sin(angle) * radius,
        -82,
        82,
      );
      const longitudeScale = Math.max(
        0.3,
        Math.cos(THREE.MathUtils.degToRad(candidateLatitude)),
      );
      const candidate = directionAt(
        longitude + (Math.cos(angle) * radius) / longitudeScale,
        candidateLatitude,
      );
      if (isLand(candidate)) return candidate;
    }
  }
  return direct;
}

export class WorldGlobe {
  constructor(container, { onSelect } = {}) {
    this.container = container;
    this.onSelect = onSelect;
    this.active = false;
    this.dragging = false;
    this.dragDistance = 0;
    this.pointer = new THREE.Vector2();
    this.raycaster = new THREE.Raycaster();
    this.clock = new THREE.Timer();
    this.sectorMeshes = [];
    this.selectedSectorId = null;
    this.hoveredSectorId = null;

    this.scene = new THREE.Scene();
    this.camera = new THREE.PerspectiveCamera(38, 1, 0.1, 40);
    this.camera.position.set(0, 0.2, 7.4);
    this.renderer = new THREE.WebGLRenderer({
      canvas: container.querySelector("canvas"),
      antialias: true,
      alpha: true,
      powerPreference: "high-performance",
    });
    this.renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2));
    this.renderer.outputColorSpace = THREE.SRGBColorSpace;
    this.renderer.setClearColor(0x000000, 0);

    this.globe = new THREE.Group();
    this.globe.rotation.set(-0.14, -0.55, 0.03);
    this.scene.add(this.globe);
    this._buildPlanet();

    this.scene.add(new THREE.HemisphereLight(0xb8d7c2, 0x141810, 1.7));
    const sun = new THREE.DirectionalLight(0xffefc7, 3.4);
    sun.position.set(-4, 5, 6);
    this.scene.add(sun);

    this.resizeObserver = new ResizeObserver(() => this.resize());
    this.resizeObserver.observe(container);
    this._bindInput();
    this.resize();
    this.renderer.setAnimationLoop(() => this.render());
  }

  _buildPlanet() {
    // Keep ocean facets broad while using a denser independent mesh for
    // recognizable coastlines and raised continental relief.
    const sourceGeometry = new THREE.IcosahedronGeometry(
      2.28,
      OCEAN_DETAIL,
    );
    const geometry = sourceGeometry.index
      ? sourceGeometry.toNonIndexed()
      : sourceGeometry;
    const oceanColors = [];
    const position = geometry.attributes.position;
    const oceanDeep = new THREE.Color(0x07539a);
    const oceanLight = new THREE.Color(0x1688d4);
    for (let index = 0; index < position.count; index += 3) {
      const center = new THREE.Vector3();
      for (let offset = 0; offset < 3; offset += 1)
        center.add(
          new THREE.Vector3().fromBufferAttribute(position, index + offset),
        );
      center.normalize();
      const latitude = Math.abs(earthCoordinates(center).latitude) / 90;
      const tone =
        0.48 +
        Math.sin(center.x * 33 + center.y * 17 - center.z * 25) * 0.15 +
        Math.cos(center.z * 71 - center.x * 23) * 0.07 -
        latitude * 0.08;
      const color = oceanDeep.clone().lerp(oceanLight, tone);
      for (let offset = 0; offset < 3; offset += 1)
        oceanColors.push(color.r, color.g, color.b);
    }
    geometry.setAttribute(
      "color",
      new THREE.Float32BufferAttribute(oceanColors, 3),
    );
    this.planet = new THREE.Mesh(
      geometry,
      new THREE.MeshStandardMaterial({
        vertexColors: true,
        flatShading: true,
        roughness: 0.92,
        metalness: 0.02,
        dithering: true,
      }),
    );
    this.globe.add(this.planet);

    const landSourceGeometry = new THREE.IcosahedronGeometry(
      2.28,
      LAND_DETAIL,
    );
    const landSource = landSourceGeometry.index
      ? landSourceGeometry.toNonIndexed()
      : landSourceGeometry;
    const landSourcePosition = landSource.attributes.position;
    const landPositions = [];
    const landColors = [];
    const skirtPositions = [];
    const skirtColors = [];
    const skirtRadius = 2.275;
    const lowland = new THREE.Color(0x61a052);
    const forest = new THREE.Color(0x347640);
    const scrub = new THREE.Color(0x7f9447);
    const desert = new THREE.Color(0xc7a45e);
    const highland = new THREE.Color(0xc4a15c);
    const snow = new THREE.Color(0xf3eee5);
    for (let index = 0; index < landSourcePosition.count; index += 3) {
      const vertices = [0, 1, 2].map((offset) =>
        new THREE.Vector3().fromBufferAttribute(
          landSourcePosition,
          index + offset,
        ),
      );
      const center = vertices
        .reduce((sum, vertex) => sum.add(vertex), new THREE.Vector3())
        .normalize();
      if (!isLand(center)) continue;
      const { latitude, longitude } = earthCoordinates(center);
      const terrain =
        Math.sin(center.x * 41 + center.z * 29) * 0.5 +
        Math.cos(center.y * 37 - center.x * 19) * 0.5;
      const moisture =
        Math.sin(center.x * 67 - center.y * 31 + center.z * 43) * 0.55 +
        Math.cos(center.z * 23 + center.x * 17) * 0.45;
      const altitude = THREE.MathUtils.clamp(0.45 + terrain * 0.35, 0, 1);
      const desertBelt =
        Math.abs(latitude) < 34 &&
        ((longitude > -18 && longitude < 58) ||
          (longitude > 112 && longitude < 155) ||
          moisture > 0.56);
      let color = (desertBelt
        ? desert
        : moisture < -0.14
          ? forest
          : terrain > 0.18
            ? scrub
            : lowland)
        .clone()
        .lerp(
          highland,
          THREE.MathUtils.smoothstep(altitude, 0.55, 0.95),
        );
      if (Math.abs(latitude) > 72)
        color = color.lerp(
          snow,
          THREE.MathUtils.smoothstep(Math.abs(latitude), 72, 84),
        );
      const radius = 2.315 + altitude * 0.075;
      for (const vertex of vertices) {
        vertex.normalize().multiplyScalar(radius);
        landPositions.push(vertex.x, vertex.y, vertex.z);
        landColors.push(color.r, color.g, color.b);
      }
      const skirtColor = color.clone().multiplyScalar(0.62);
      for (let edge = 0; edge < 3; edge += 1) {
        const topStart = vertices[edge];
        const topEnd = vertices[(edge + 1) % 3];
        const bottomStart = topStart
          .clone()
          .normalize()
          .multiplyScalar(skirtRadius);
        const bottomEnd = topEnd
          .clone()
          .normalize()
          .multiplyScalar(skirtRadius);
        for (const vertex of [
          topStart,
          bottomStart,
          bottomEnd,
          topStart,
          bottomEnd,
          topEnd,
        ]) {
          skirtPositions.push(vertex.x, vertex.y, vertex.z);
          skirtColors.push(skirtColor.r, skirtColor.g, skirtColor.b);
        }
      }
    }
    const landGeometry = new THREE.BufferGeometry();
    landGeometry.setAttribute(
      "position",
      new THREE.Float32BufferAttribute(landPositions, 3),
    );
    landGeometry.setAttribute(
      "color",
      new THREE.Float32BufferAttribute(landColors, 3),
    );
    landGeometry.computeVertexNormals();
    this.land = new THREE.Mesh(
      landGeometry,
      new THREE.MeshStandardMaterial({
        vertexColors: true,
        flatShading: true,
        roughness: 0.88,
        dithering: true,
      }),
    );
    this.globe.add(this.land);

    const skirtGeometry = new THREE.BufferGeometry();
    skirtGeometry.setAttribute(
      "position",
      new THREE.Float32BufferAttribute(skirtPositions, 3),
    );
    skirtGeometry.setAttribute(
      "color",
      new THREE.Float32BufferAttribute(skirtColors, 3),
    );
    skirtGeometry.computeVertexNormals();
    this.landSkirt = new THREE.Mesh(
      skirtGeometry,
      new THREE.MeshStandardMaterial({
        vertexColors: true,
        flatShading: true,
        roughness: 0.94,
        side: THREE.DoubleSide,
      }),
    );
    this.globe.add(this.landSkirt);
  }

  setWorld(world, selectedSectorId, contestableIds = new Set()) {
    for (const marker of this.sectorMeshes) {
      marker.traverse((object) => {
        object.geometry?.dispose();
        object.material?.dispose();
      });
      this.globe.remove(marker);
    }
    this.sectorMeshes = [];
    this.world = world;
    this.contestableIds = contestableIds;
    for (const sector of world?.sectors ?? []) {
      const direction = sectorDirection(sector, world);
      const color =
        sector.owner === null
          ? OWNER_COLORS.neutral
          : sector.owner === 0
            ? OWNER_COLORS.friendly
            : OWNER_COLORS.hostile;
      const contestable = contestableIds.has(sector.id);
      const marker = new THREE.Group();
      marker.position.copy(direction).multiplyScalar(2.415);
      marker.quaternion.setFromUnitVectors(
        new THREE.Vector3(0, 0, 1),
        direction,
      );

      const pole = new THREE.Mesh(
        new THREE.CylinderGeometry(0.012, 0.018, 0.42, 5),
        new THREE.MeshStandardMaterial({
          color: 0xd8cfaa,
          roughness: 0.55,
          metalness: 0.18,
        }),
      );
      pole.rotation.x = Math.PI / 2;
      pole.position.z = 0.22;
      const flagMaterial = new THREE.MeshStandardMaterial({
          color,
          emissive: color,
          emissiveIntensity: contestable ? 0.55 : 0.12,
          roughness: 0.72,
          side: THREE.DoubleSide,
          transparent: !contestable && sector.owner !== 0,
          opacity: !contestable && sector.owner !== 0 ? 0.55 : 1,
        });
      const cloth = new THREE.Mesh(
        new THREE.PlaneGeometry(0.22, 0.13, 1, 1),
        flagMaterial,
      );
      cloth.rotation.x = Math.PI / 2;
      cloth.position.set(0.105, 0, 0.37);
      const foot = new THREE.Mesh(
        new THREE.CylinderGeometry(0.045, 0.065, 0.045, 6),
        new THREE.MeshStandardMaterial({
          color: 0x3a4034,
          roughness: 0.9,
        }),
      );
      foot.rotation.x = Math.PI / 2;
      foot.position.z = 0.025;
      const proxy = new THREE.Mesh(
        new THREE.OctahedronGeometry(0.055, 0),
        new THREE.MeshBasicMaterial({
          color,
          transparent: true,
          opacity: 0.88,
        }),
      );
      proxy.position.z = 0.1;
      proxy.visible = false;
      const beacon = new THREE.Mesh(
        new THREE.TorusGeometry(0.095, 0.009, 4, 24),
        new THREE.MeshBasicMaterial({
          color: 0xd8ef82,
          transparent: true,
          opacity: contestable ? 0.72 : 0,
          depthWrite: false,
        }),
      );
      beacon.position.z = 0.015;
      const hitTarget = new THREE.Mesh(
        new THREE.CircleGeometry(0.15, 10),
        new THREE.MeshBasicMaterial({
          transparent: true,
          opacity: 0,
          depthWrite: false,
          side: THREE.DoubleSide,
        }),
      );
      hitTarget.position.z = 0.08;
      marker.add(foot, pole, cloth, proxy, beacon, hitTarget);
      marker.userData = {
        sectorId: sector.id,
        contestable,
        baseScale: sector.contested ? 1.08 : 1,
        flagMaterial,
        cloth,
        detailObjects: [foot, pole, cloth],
        proxy,
        beacon,
      };
      marker.traverse((object) => {
        object.userData.sectorMarker = marker;
      });
      this.globe.add(marker);
      this.sectorMeshes.push(marker);
    }
    this.setSelected(selectedSectorId);
  }

  setSelected(sectorId) {
    this.selectedSectorId = sectorId;
    for (const marker of this.sectorMeshes) {
      const selected = marker.userData.sectorId === sectorId;
      marker.userData.flagMaterial.emissiveIntensity = selected
        ? 1.2
        : marker.userData.contestable
          ? 0.55
          : 0.12;
      marker.scale.setScalar(
        (selected ? 1.16 : 1) * marker.userData.baseScale,
      );
    }
  }

  setActive(active) {
    this.active = Boolean(active);
    if (this.active) this.resize();
  }

  _bindInput() {
    const canvas = this.renderer.domElement;
    canvas.addEventListener("pointerdown", (event) => {
      this.dragging = true;
      this.dragDistance = 0;
      this.previousPointer = { x: event.clientX, y: event.clientY };
      canvas.setPointerCapture(event.pointerId);
      canvas.classList.add("dragging");
    });
    canvas.addEventListener("pointermove", (event) => {
      if (this.dragging) {
        const dx = event.clientX - this.previousPointer.x;
        const dy = event.clientY - this.previousPointer.y;
        this.dragDistance += Math.abs(dx) + Math.abs(dy);
        this.globe.rotation.y += dx * 0.006;
        this.globe.rotation.x = THREE.MathUtils.clamp(
          this.globe.rotation.x + dy * 0.004,
          -1.05,
          1.05,
        );
        this.previousPointer = { x: event.clientX, y: event.clientY };
        return;
      }
      this._pick(event, false);
    });
    const release = (event) => {
      if (!this.dragging) return;
      this.dragging = false;
      canvas.classList.remove("dragging");
      if (this.dragDistance < 6) this._pick(event, true);
    };
    canvas.addEventListener("pointerup", release);
    canvas.addEventListener("pointercancel", release);
    canvas.addEventListener("pointerleave", () => {
      if (!this.dragging) this.hoveredSectorId = null;
    });
    canvas.addEventListener(
      "wheel",
      (event) => {
        event.preventDefault();
        this.camera.position.z = THREE.MathUtils.clamp(
          this.camera.position.z + event.deltaY * 0.004,
          5.8,
          9.2,
        );
      },
      { passive: false },
    );
    canvas.addEventListener("keydown", (event) => {
      if (!["ArrowLeft", "ArrowRight", "ArrowUp", "ArrowDown"].includes(event.key))
        return;
      event.preventDefault();
      const sectors = this.world?.sectors ?? [];
      if (!sectors.length) return;
      const current = Math.max(
        0,
        sectors.findIndex((sector) => sector.id === this.selectedSectorId),
      );
      const horizontal = event.key === "ArrowLeft" ? -1 : event.key === "ArrowRight" ? 1 : 0;
      const vertical =
        event.key === "ArrowUp"
          ? -this.world.width
          : event.key === "ArrowDown"
            ? this.world.width
            : 0;
      const next = (current + horizontal + vertical + sectors.length) % sectors.length;
      this.setSelected(sectors[next].id);
      this.onSelect?.(sectors[next].id);
    });
  }

  _pick(event, select) {
    const bounds = this.renderer.domElement.getBoundingClientRect();
    this.pointer.set(
      ((event.clientX - bounds.left) / bounds.width) * 2 - 1,
      -((event.clientY - bounds.top) / bounds.height) * 2 + 1,
    );
    this.raycaster.setFromCamera(this.pointer, this.camera);
    const hit = this.raycaster
      .intersectObjects(this.sectorMeshes, true)
      .find(({ object, point }) => {
        const marker = object.userData.sectorMarker;
        const normal = marker.position
          .clone()
          .applyQuaternion(this.globe.quaternion)
          .normalize();
        return normal.dot(this.camera.position.clone().sub(point).normalize()) > 0;
      });
    this.hoveredSectorId =
      hit?.object.userData.sectorMarker?.userData.sectorId ?? null;
    this.renderer.domElement.style.cursor = hit
      ? select
        ? "grabbing"
        : "pointer"
      : this.dragging
        ? "grabbing"
        : "grab";
    if (select && this.hoveredSectorId) {
      this.setSelected(this.hoveredSectorId);
      this.onSelect?.(this.hoveredSectorId);
    }
  }

  resize() {
    const width = Math.max(1, this.container.clientWidth);
    const height = Math.max(1, this.container.clientHeight);
    this.camera.aspect = width / height;
    this.camera.updateProjectionMatrix();
    this.renderer.setSize(width, height, false);
  }

  render() {
    if (!this.active) return;
    this.clock.update();
    const delta = Math.min(0.05, this.clock.getDelta());
    if (!this.dragging) this.globe.rotation.y += delta * 0.055;
    const elapsed = this.clock.getElapsed();
    for (let index = 0; index < this.sectorMeshes.length; index += 1) {
      const marker = this.sectorMeshes[index];
      const normal = marker.position
        .clone()
        .applyQuaternion(this.globe.quaternion)
        .normalize();
      const facing = normal.dot(this.camera.position.clone().normalize());
      marker.visible = facing > -0.1;
      if (!marker.visible) continue;
      const selected = marker.userData.sectorId === this.selectedSectorId;
      const showDetail =
        selected ||
        marker.userData.contestable ||
        (this.camera.position.z < 8.15 && facing > 0.34);
      for (const object of marker.userData.detailObjects)
        object.visible = showDetail;
      marker.userData.proxy.visible = !showDetail;
      if (!marker.userData.contestable) continue;
      const phase = elapsed * 3.1 + index * 0.7;
      const pulse = 1 + Math.sin(phase) * 0.16;
      marker.userData.beacon.scale.setScalar(pulse);
      marker.userData.beacon.material.opacity =
        0.42 + Math.sin(phase) * 0.24;
      marker.userData.cloth.rotation.z = Math.sin(phase * 0.72) * 0.08;
    }
    this.renderer.render(this.scene, this.camera);
  }
}
