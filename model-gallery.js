import * as THREE from "three";
import { OrbitControls } from "three/addons/controls/OrbitControls.js";
import { GLTFLoader } from "three/addons/loaders/GLTFLoader.js";
import { OBJLoader } from "three/addons/loaders/OBJLoader.js";

const MODELS = [
  {
    name: "Commander / basic",
    file: "models/army_commander.obj",
    format: "OBJ",
  },
  {
    name: "Commander / UUID",
    file: "models/af9d7fab-8799-4939-931c-38881206161f",
    format: "OBJ",
    duplicateOf: "army_commander.obj",
  },
  {
    name: "Five primitives",
    file: "models/army_commander_5_primitives.obj",
    format: "OBJ",
  },
  {
    name: "Five primitives / copy",
    file: "models/army_commander_5_primitives (1).obj",
    format: "OBJ",
    duplicateOf: "army_commander_5_primitives.obj",
  },
  {
    name: "Five primitives / binary",
    file: "models/army_commander_5_primitives.glb",
    format: "GLB",
  },
  {
    name: "Codex commander / combined",
    file: "models/codex_field_commander.obj",
    format: "OBJ",
  },
];

const previews = [];
const objLoader = new OBJLoader();
const gltfLoader = new GLTFLoader();
const grid = document.querySelector("#model-grid");
const template = document.querySelector("#model-card-template");
const rotationButton = document.querySelector("#toggle-rotation");
const wireframeButton = document.querySelector("#toggle-wireframe");
const resetButton = document.querySelector("#reset-models");
let rotationPaused = false;
let wireframeEnabled = false;

function loadModel(spec) {
  if (spec.format === "GLB")
    return gltfLoader.loadAsync(encodeURI(spec.file)).then((result) => result.scene);
  return objLoader.loadAsync(encodeURI(spec.file));
}

function dimensionsLabel(size) {
  return [size.x, size.y, size.z].map((value) => value.toFixed(2)).join(" × ");
}

function triangleCount(geometry) {
  if (geometry.index) return Math.round(geometry.index.count / 3);
  return Math.round((geometry.attributes.position?.count || 0) / 3);
}

function tintForName(name, index) {
  const text = name.toLowerCase();
  if (text.includes("head")) return 0xa58462;
  if (
    text.includes("cap") ||
    text.includes("boot") ||
    text.includes("binocular") ||
    text.includes("radio") ||
    text.includes("antenna")
  )
    return 0x343a2c;
  if (text.includes("buckle") || text.includes("rank")) return 0xb39b55;
  if (
    text.includes("arm") ||
    text.includes("leg") ||
    text.includes("coat") ||
    text.includes("epaulette")
  )
    return 0x687252;
  return index % 2 === 0 ? 0x7d865e : 0x616b4e;
}

function prepareModel(model) {
  // Every source in this folder is authored Z-up. Rotate once into Three.js's
  // Y-up coordinate system before measuring, grounding, and displaying it.
  model.rotation.x = -Math.PI / 2;
  model.updateMatrixWorld(true);
  let meshCount = 0;
  let triangles = 0;
  const materials = [];
  model.traverse((object) => {
    if (!object.isMesh) return;
    object.geometry.computeVertexNormals();
    const material = new THREE.MeshStandardMaterial({
      color: tintForName(object.name, meshCount),
      roughness: 0.72,
      metalness: 0.08,
      side: THREE.DoubleSide,
    });
    object.material = material;
    object.castShadow = true;
    object.receiveShadow = true;
    materials.push(material);
    meshCount += 1;
    triangles += triangleCount(object.geometry);
  });

  const sourceBox = new THREE.Box3().setFromObject(model);
  const sourceSize = sourceBox.getSize(new THREE.Vector3());
  const displayScale = 2.9 / Math.max(sourceSize.y, 0.001);
  model.scale.setScalar(displayScale);
  const displayBox = new THREE.Box3().setFromObject(model);
  const center = displayBox.getCenter(new THREE.Vector3());
  model.position.x -= center.x;
  model.position.y -= displayBox.min.y;
  model.position.z -= center.z;

  return { meshCount, triangles, sourceSize, materials };
}

function createPreview(spec, index) {
  const card = template.content.firstElementChild.cloneNode(true);
  const canvas = card.querySelector("canvas");
  const status = card.querySelector(".model-status");
  const duplicateBadge = card.querySelector(".duplicate-badge");
  card.querySelector(".model-index").textContent = `MODEL ${String(index + 1).padStart(2, "0")}`;
  card.querySelector(".model-name").textContent = spec.name;
  card.querySelector(".model-format").textContent = spec.format;
  card.querySelector(".model-file").textContent = spec.file;
  if (spec.duplicateOf) {
    duplicateBadge.hidden = false;
    duplicateBadge.textContent = `Exact duplicate of ${spec.duplicateOf}`;
  }
  grid.appendChild(card);

  const renderer = new THREE.WebGLRenderer({
    canvas,
    antialias: true,
    alpha: true,
    powerPreference: "high-performance",
  });
  renderer.setPixelRatio(Math.min(window.devicePixelRatio || 1, 1.5));
  renderer.outputColorSpace = THREE.SRGBColorSpace;
  renderer.toneMapping = THREE.ACESFilmicToneMapping;
  renderer.toneMappingExposure = 1.08;
  renderer.shadowMap.enabled = true;
  renderer.shadowMap.type = THREE.PCFSoftShadowMap;

  const scene = new THREE.Scene();
  scene.fog = new THREE.Fog(0x141a12, 7, 12);
  const camera = new THREE.PerspectiveCamera(34, 1, 0.05, 30);
  camera.position.set(4.4, 2.9, 5.2);
  const controls = new OrbitControls(camera, canvas);
  controls.target.set(0, 1.35, 0);
  controls.enableDamping = true;
  controls.dampingFactor = 0.075;
  controls.minDistance = 3.2;
  controls.maxDistance = 10;
  controls.update();

  scene.add(new THREE.HemisphereLight(0xd9dfc4, 0x172014, 2.2));
  const key = new THREE.DirectionalLight(0xfff0c9, 3.8);
  key.position.set(3, 6, 4);
  key.castShadow = true;
  key.shadow.mapSize.set(512, 512);
  scene.add(key);
  const rim = new THREE.DirectionalLight(0xa8bf78, 1.6);
  rim.position.set(-4, 3, -3);
  scene.add(rim);

  const plinth = new THREE.Mesh(
    new THREE.CylinderGeometry(1.18, 1.32, 0.16, 32),
    new THREE.MeshStandardMaterial({
      color: 0x242c20,
      roughness: 0.9,
      metalness: 0.12,
    }),
  );
  plinth.position.y = -0.09;
  plinth.receiveShadow = true;
  scene.add(plinth);
  const ring = new THREE.Mesh(
    new THREE.TorusGeometry(1.05, 0.012, 6, 64),
    new THREE.MeshBasicMaterial({ color: 0xa5aa5b }),
  );
  ring.rotation.x = Math.PI / 2;
  ring.position.y = 0.002;
  scene.add(ring);

  const preview = {
    card,
    renderer,
    scene,
    camera,
    controls,
    model: null,
    rotationPivot: null,
    materials: [],
    initialCamera: camera.position.clone(),
  };
  previews.push(preview);

  loadModel(spec)
    .then((model) => {
      const details = prepareModel(model);
      preview.model = model;
      preview.materials = details.materials;
      preview.rotationPivot = new THREE.Group();
      preview.rotationPivot.add(model);
      scene.add(preview.rotationPivot);
      card.querySelector('[data-stat="size"]').textContent = dimensionsLabel(
        details.sourceSize,
      );
      card.querySelector('[data-stat="meshes"]').textContent = String(
        details.meshCount,
      );
      card.querySelector('[data-stat="triangles"]').textContent =
        details.triangles.toLocaleString();
      status.textContent = "Ready";
      status.classList.add("ready");
    })
    .catch((error) => {
      status.textContent = "Could not load";
      status.classList.add("error");
      console.error(`Could not load ${spec.file}`, error);
    });
}

function resizePreview(preview) {
  const width = Math.max(preview.card.querySelector(".model-stage").clientWidth, 1);
  const height = Math.max(preview.card.querySelector(".model-stage").clientHeight, 1);
  const pixelRatio = preview.renderer.getPixelRatio();
  const requiredWidth = Math.floor(width * pixelRatio);
  const requiredHeight = Math.floor(height * pixelRatio);
  if (
    preview.renderer.domElement.width !== requiredWidth ||
    preview.renderer.domElement.height !== requiredHeight
  ) {
    preview.renderer.setSize(width, height, false);
    preview.camera.aspect = width / height;
    preview.camera.updateProjectionMatrix();
  }
}

function render(now) {
  const seconds = now * 0.001;
  for (const preview of previews) {
    resizePreview(preview);
    if (preview.rotationPivot && !rotationPaused)
      preview.rotationPivot.rotation.y = seconds * 0.34;
    preview.controls.update();
    preview.renderer.render(preview.scene, preview.camera);
  }
  requestAnimationFrame(render);
}

rotationButton.addEventListener("click", () => {
  rotationPaused = !rotationPaused;
  rotationButton.setAttribute("aria-pressed", String(rotationPaused));
  rotationButton.textContent = rotationPaused ? "Resume rotation" : "Pause rotation";
});

wireframeButton.addEventListener("click", () => {
  wireframeEnabled = !wireframeEnabled;
  wireframeButton.setAttribute("aria-pressed", String(wireframeEnabled));
  for (const preview of previews)
    for (const material of preview.materials) material.wireframe = wireframeEnabled;
});

resetButton.addEventListener("click", () => {
  for (const preview of previews) {
    preview.camera.position.copy(preview.initialCamera);
    preview.controls.target.set(0, 1.35, 0);
    preview.controls.update();
  }
});

MODELS.forEach(createPreview);
requestAnimationFrame(render);
