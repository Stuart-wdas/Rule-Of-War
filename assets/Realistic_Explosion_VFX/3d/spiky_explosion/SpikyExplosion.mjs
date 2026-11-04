/**
 * Three.js adapter for particle_tracks.json. One instanced draw call, procedural
 * volume shading, no image downloads. Positions are converted from Blender Z-up.
 * The shader is a real-time approximation of the Blender materials, not Cycles.
 */
const vertexShader = `
attribute vec2 particleState;
varying vec2 vState;
varying vec3 vCenter, vWorld, vScale, vAxisX, vAxisY, vAxisZ;
void main() {
  mat4 transform = modelMatrix * instanceMatrix;
  vec4 world = transform * vec4(position, 1.0);
  vWorld = world.xyz;
  vCenter = transform[3].xyz;
  vScale = vec3(length(transform[0].xyz), length(transform[1].xyz), length(transform[2].xyz));
  vAxisX = transform[0].xyz / max(vScale.x, .0001);
  vAxisY = transform[1].xyz / max(vScale.y, .0001);
  vAxisZ = transform[2].xyz / max(vScale.z, .0001);
  vState = particleState;
  gl_Position = projectionMatrix * viewMatrix * world;
}`;

const fragmentShader = `
precision highp float;
uniform float effectTime, orthographic;
uniform vec3 cameraDirection;
varying vec2 vState;
varying vec3 vCenter, vWorld, vScale, vAxisX, vAxisY, vAxisZ;
float hash(vec3 p) {
  p = fract(p * .3183099 + vec3(.13,.17,.19));
  p *= 17.; return fract(p.x*p.y*p.z*(p.x+p.y+p.z));
}
float noise3(vec3 p) {
  vec3 i=floor(p),f=fract(p); f=f*f*(3.-2.*f);
  return mix(mix(mix(hash(i),hash(i+vec3(1,0,0)),f.x),
                 mix(hash(i+vec3(0,1,0)),hash(i+vec3(1,1,0)),f.x),f.y),
             mix(mix(hash(i+vec3(0,0,1)),hash(i+vec3(1,0,1)),f.x),
                 mix(hash(i+vec3(0,1,1)),hash(i+vec3(1,1,1)),f.x),f.y),f.z);
}
float fbm(vec3 p) {
  return .57*noise3(p)+.28*noise3(p*2.03+11.)+.15*noise3(p*4.11+23.);
}
void main() {
  if(vState.x<.002) discard;
  float radius=max(vScale.x,max(vScale.y,vScale.z));
  vec3 ray=normalize(mix(vWorld-cameraPosition,cameraDirection,orthographic));
  vec3 origin=mix(cameraPosition,vWorld-ray*radius*3.,orthographic);
  vec3 offset=origin-vCenter;
  float b=dot(offset,ray), discriminant=b*b-dot(offset,offset)+radius*radius;
  if(discriminant<=0.) discard;
  float root=sqrt(discriminant), start=max(0.,-b-root), finish=-b+root;
  float stepSize=(finish-start)/28.;
  vec3 lightDirection=normalize(vec3(.5,1.,.4));
  vec3 radiance=vec3(0.); float transmission=1.;
  bool fire=vState.y>.5;
  for(int s=0;s<28;s++) {
    vec3 p=origin+ray*(start+(float(s)+.5)*stepSize)-vCenter;
    vec3 q=vec3(dot(p,vAxisX),dot(p,vAxisY),dot(p,vAxisZ))/max(vScale,vec3(.0001));
    vec3 flow=q*3.3+vec3(0.,-effectTime*.3,effectTime*.16);
    float n=fbm(flow);
    float edge=1.-smoothstep(.45,1.,length(q)+.2*(noise3(flow*1.8)-.5));
    float density=smoothstep(.3,.68,n)*edge*vState.x*(fire?3.:11.);
    float absorbed=1.-exp(-density*stepSize);
    vec3 color;
    if(fire) {
      color=mix(vec3(1.,.045,.001),vec3(1.,.64,.09),smoothstep(.35,.75,n));
      color*=vState.y>1.5?4.:2.8;
    } else {
      float lit=.32+.68*max(0.,dot(normalize(q+vec3(.01)),lightDirection));
      color=vec3(.13,.112,.095)*lit;
    }
    radiance+=transmission*absorbed*color;
    transmission*=1.-absorbed;
    if(transmission<.015) break;
  }
  float alpha=1.-transmission;
  if(alpha<.002) discard;
  gl_FragColor=vec4(radiance/max(alpha,.0001),alpha);
  #include <tonemapping_fragment>
  #include <colorspace_fragment>
}`;

export function createSpikyExplosion(THREE, tracks) {
  if(tracks.frames!==64 || tracks.stride!==7 || !tracks.particles?.length)
    throw new Error('Expected the supplied 64-frame, stride-7 particle tracks.');
  const count=tracks.particles.length;
  const geometry=new THREE.SphereGeometry(1,10,6);
  const state=new THREE.InstancedBufferAttribute(new Float32Array(count*2),2);
  state.setUsage(THREE.DynamicDrawUsage);geometry.setAttribute('particleState',state);
  const material=new THREE.ShaderMaterial({vertexShader,fragmentShader,
    uniforms:{effectTime:{value:0},orthographic:{value:0},cameraDirection:{value:new THREE.Vector3(0,0,-1)}},
    transparent:true,depthWrite:false,depthTest:true,side:THREE.FrontSide});
  const mesh=new THREE.InstancedMesh(geometry,material,count);
  mesh.name='Spiky explosion - procedural volume particles';mesh.frustumCulled=false;
  mesh.instanceMatrix.setUsage(THREE.DynamicDrawUsage);
  const object=new THREE.Group();object.name='SpikyExplosion';object.add(mesh);
  const dummy=new THREE.Object3D(),world=new THREE.Vector3(),cam=new THREE.Vector3();
  const up=new THREE.Vector3(0,1,0),axisVector=new THREE.Vector3();
  const entries=tracks.particles.map(p=>({track:p,p:[0,0,0],s:[1,1,1],alpha:0,depth:0}));
  function update(ageSeconds,camera) {
    if(!Number.isFinite(ageSeconds))throw new Error('Age must be finite seconds.');
    const duration=tracks.frames/tracks.fps;
    if(ageSeconds<0 || ageSeconds>=duration){object.visible=false;mesh.count=0;return false;}
    const time=Math.min(tracks.frames-1,ageSeconds*tracks.fps);
    const f=Math.floor(time),next=Math.min(tracks.frames-1,f+1),blend=time-f;
    const values=new Float32Array(7);
    object.updateWorldMatrix(true,false);
    if(camera){
      camera.getWorldPosition(cam);camera.getWorldDirection(material.uniforms.cameraDirection.value);
      material.uniforms.orthographic.value=camera.isOrthographicCamera?1:0;
    }
    for(const e of entries){
      const a=e.track.samples;
      for(let k=0;k<7;k++)values[k]=a[f*7+k]*(1-blend)+a[next*7+k]*blend;
      e.p[0]=values[0];e.p[1]=values[2];e.p[2]=-values[1];
      e.s[0]=values[3];e.s[1]=values[5];e.s[2]=values[4];e.alpha=values[6];
      if(camera){world.set(...e.p).applyMatrix4(object.matrixWorld);e.depth=world.distanceToSquared(cam);}
    }
    // Back-to-front instance order avoids a fixed smoke/fire compositing order.
    if(camera)entries.sort((a,b)=>b.depth-a.depth);
    let active=0;
    for(const e of entries){
      if(e.alpha<.002)continue;
      dummy.position.set(...e.p);dummy.scale.set(...e.s);
      const axis=e.track.axis||[0,0,1];
      dummy.quaternion.setFromUnitVectors(up,axisVector.set(axis[0],axis[2],-axis[1]).normalize());
      dummy.updateMatrix();
      mesh.setMatrixAt(active,dummy.matrix);state.setXY(active,e.alpha,e.track.kind);active++;
    }
    mesh.count=active;mesh.instanceMatrix.needsUpdate=true;state.needsUpdate=true;
    material.uniforms.effectTime.value=ageSeconds;object.visible=active>0;
    return true;
  }
  update(0);
  return {object,update,duration:tracks.frames/tracks.fps,
    dispose(){object.removeFromParent();geometry.dispose();material.dispose();}};
}
