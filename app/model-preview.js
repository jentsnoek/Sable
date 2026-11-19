// League SKN geometry and companion skeleton/animation preview.
(() => {
  const SKN_MAGIC = 0x00112233;
  const MAX_VERTICES = 250000;
  const MAX_INDICES = 750000;
  const MAX_RANGES = 2048;
  const PALETTE = [
    [0.91, 0.68, 0.79], [0.59, 0.77, 0.89], [0.79, 0.71, 0.91],
    [0.85, 0.78, 0.58], [0.59, 0.83, 0.72], [0.94, 0.61, 0.64]
  ];

  const parseSkn = (input) => {
    const bytes = input instanceof Uint8Array ? input : new Uint8Array(input);
    const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
    let cursor = 0;
    const take = (size) => {
      if (!Number.isSafeInteger(size) || size < 0 || cursor + size > bytes.length) {
        throw new Error("The SKN model is incomplete or has invalid offsets.");
      }
      const offset = cursor;
      cursor += size;
      return offset;
    };
    const u16 = () => view.getUint16(take(2), true);
    const u32 = () => view.getUint32(take(4), true);
    const i32 = () => view.getInt32(take(4), true);
    if (u32() !== SKN_MAGIC) throw new Error("This file is not a League SKN model.");
    const major = u16();
    const minor = u16();
    if (![0, 1, 2, 4].includes(major) || minor !== 1) {
      throw new Error(`SKN version ${major}.${minor} is not supported.`);
    }
    let ranges = [];
    let indexCount, vertexCount, flags = 0, stride = 52;
    if (major === 0) {
      indexCount = u32();
      vertexCount = u32();
      ranges = [{ startVertex: 0, vertexCount, startIndex: 0, indexCount }];
    } else {
      const rangeCount = u32();
      if (rangeCount > MAX_RANGES) throw new Error("This SKN model has too many parts to preview.");
      for (let i = 0; i < rangeCount; i++) {
        const nameOffset = take(64);
        const nameEnd = bytes.indexOf(0, nameOffset);
        const material = new TextDecoder().decode(bytes.subarray(nameOffset, nameEnd < nameOffset || nameEnd >= nameOffset + 64 ? nameOffset + 64 : nameEnd));
        ranges.push({ material, startVertex: i32(), vertexCount: i32(), startIndex: i32(), indexCount: i32() });
      }
      if (major === 4) {
        flags = u32();
        indexCount = u32();
        vertexCount = u32();
        stride = u32();
        const vertexType = u32();
        if ([52, 56, 72, 104][vertexType] !== stride) {
          throw new Error("This SKN model has an unsupported vertex layout.");
        }
        take(40); // AABB and bounding sphere
      } else {
        indexCount = u32();
        vertexCount = u32();
      }
    }
    if (!vertexCount || vertexCount > MAX_VERTICES || !indexCount || indexCount > MAX_INDICES) {
      throw new Error("This SKN model is too large or empty to preview.");
    }
    if (flags & 1) take(u16()); // Direct blend index block length; payload is opaque to the viewer.
    const indicesOffset = take(indexCount * 2);
    const verticesOffset = take(vertexCount * stride);
    if (major >= 2) take(12); // End tab

    const positions = new Float32Array(vertexCount * 3);
    const blendIndices = new Uint8Array(vertexCount * 4);
    const blendWeights = new Float32Array(vertexCount * 4);
    const min = [Infinity, Infinity, Infinity], max = [-Infinity, -Infinity, -Infinity];
    for (let i = 0; i < vertexCount; i++) {
      const offset = verticesOffset + i * stride;
      for (let axis = 0; axis < 3; axis++) {
        const value = view.getFloat32(offset + axis * 4, true);
        if (!Number.isFinite(value)) throw new Error("This SKN model has invalid vertex positions.");
        positions[i * 3 + axis] = value;
        min[axis] = Math.min(min[axis], value);
        max[axis] = Math.max(max[axis], value);
      }
      for (let slot = 0; slot < 4; slot++) {
        blendIndices[i * 4 + slot] = view.getUint8(offset + 12 + slot);
        const weight = view.getFloat32(offset + 16 + slot * 4, true);
        blendWeights[i * 4 + slot] = Number.isFinite(weight) && weight > 0 ? weight : 0;
      }
    }
    const center = min.map((value, axis) => (value + max[axis]) / 2);
    const extent = Math.max(...max.map((value, axis) => value - min[axis]));
    if (!(extent > 0)) throw new Error("This SKN model has no visible geometry.");
    const scale = 1.7 / extent;
    // Expand indexed triangles into one interleaved position/normal/color buffer.
    // This keeps the renderer compatible with WebGL 1 and large source meshes.
    const vertices = new Float32Array(indexCount * 9);
    let output = 0, triangles = 0;
    const sourceIds = [];
    for (let part = 0; part < ranges.length; part++) {
      const range = ranges[part];
      const { startVertex, startIndex } = range;
      if (startVertex < 0 || range.vertexCount < 0 || startIndex < 0 || range.indexCount < 0 ||
          startVertex + range.vertexCount > vertexCount || startIndex + range.indexCount > indexCount) {
        throw new Error("This SKN model has an invalid part range.");
      }
      const color = PALETTE[part % PALETTE.length];
      for (let i = startIndex; i + 2 < startIndex + range.indexCount; i += 3) {
        const ids = [0, 1, 2].map((corner) => {
          const raw = view.getUint16(indicesOffset + (i + corner) * 2, true);
          return (flags & 2) ? startVertex + raw : raw;
        });
        if (ids.some((id) => id < startVertex || id >= startVertex + range.vertexCount)) {
          throw new Error("This SKN model has an invalid triangle index.");
        }
        const a = ids.map((id) => id * 3);
        sourceIds.push(...ids);
        const e1 = [0, 1, 2].map((axis) => positions[a[1] + axis] - positions[a[0] + axis]);
        const e2 = [0, 1, 2].map((axis) => positions[a[2] + axis] - positions[a[0] + axis]);
        const normal = [e1[1] * e2[2] - e1[2] * e2[1], e1[2] * e2[0] - e1[0] * e2[2], e1[0] * e2[1] - e1[1] * e2[0]];
        const length = Math.hypot(...normal) || 1;
        for (let corner = 0; corner < 3; corner++) {
          const base = a[corner];
          for (let axis = 0; axis < 3; axis++) vertices[output++] = (positions[base + axis] - center[axis]) * scale;
          for (let axis = 0; axis < 3; axis++) vertices[output++] = normal[axis] / length;
          for (let axis = 0; axis < 3; axis++) vertices[output++] = color[axis];
        }
        triangles++;
      }
    }
    if (!triangles) throw new Error("This SKN model has no triangles to preview.");
    return {
      vertices: vertices.subarray(0, output), triangles, parts: ranges.length, sourceVertices: vertexCount,
      positions, blendIndices, blendWeights, sourceIds: Uint32Array.from(sourceIds), center, scale, flags
    };
  };

  const matrixMultiply = (a, b) => {
    const out = new Float32Array(16);
    for (let column = 0; column < 4; column++) {
      const offset = column * 4;
      for (let row = 0; row < 4; row++) {
        out[offset + row] = a[row] * b[offset] + a[4 + row] * b[offset + 1] + a[8 + row] * b[offset + 2] + a[12 + row] * b[offset + 3];
      }
    }
    return out;
  };
  const matrixFromTrs = (translation, rotation, scale) => {
    const [x, y, z, w] = rotation;
    const sx = scale[0], sy = scale[1], sz = scale[2];
    const x2 = x + x, y2 = y + y, z2 = z + z;
    const xx = x * x2, xy = x * y2, xz = x * z2;
    const yy = y * y2, yz = y * z2, zz = z * z2;
    const wx = w * x2, wy = w * y2, wz = w * z2;
    return new Float32Array([
      (1 - (yy + zz)) * sx, (xy + wz) * sx, (xz - wy) * sx, 0,
      (xy - wz) * sy, (1 - (xx + zz)) * sy, (yz + wx) * sy, 0,
      (xz + wy) * sz, (yz - wx) * sz, (1 - (xx + yy)) * sz, 0,
      translation[0], translation[1], translation[2], 1
    ]);
  };
  const transformPoint = (matrix, x, y, z) => [
    matrix[0] * x + matrix[4] * y + matrix[8] * z + matrix[12],
    matrix[1] * x + matrix[5] * y + matrix[9] * z + matrix[13],
    matrix[2] * x + matrix[6] * y + matrix[10] * z + matrix[14]
  ];

  const deformMesh = (mesh, skeleton, animation, framePosition) => {
    const joints = skeleton?.joints;
    if (!joints?.length || !animation?.frames?.length) return;
    const boundedPosition = Math.max(0, Math.min(animation.frames.length - 1, framePosition));
    const frameA = Math.min(animation.frames.length - 1, Math.floor(boundedPosition));
    const frameB = Math.min(animation.frames.length - 1, frameA + 1);
    const fraction = boundedPosition - frameA;
    const samplesA = new Map((animation.frames[frameA] || []).map((sample) => [sample.joint, sample]));
    const samplesB = new Map((animation.frames[frameB] || []).map((sample) => [sample.joint, sample]));
    const jointIndexById = new Map(joints.map((joint, index) => [joint.id, index]));
    const local = joints.map((joint, index) => {
      const a = samplesA.get(index), b = samplesB.get(index);
      if (!a) return matrixFromTrs(joint.localTranslation, joint.localRotation, joint.localScale);
      const first = b || a;
      const rotation = quatSlerp(a.rotation, first.rotation, fraction);
      const translation = lerp3(a.translation, first.translation, fraction);
      const scale = lerp3(a.scale, first.scale, fraction);
      return matrixFromTrs(translation, rotation, scale);
    });
    const global = new Array(joints.length);
    const state = new Uint8Array(joints.length);
    const resolveGlobal = (index, depth = 0) => {
      if (global[index]) return global[index];
      if (state[index] || depth > joints.length) return local[index];
      state[index] = 1;
      const parent = jointIndexById.get(joints[index].parentId);
      global[index] = parent === undefined || parent === index
        ? local[index]
        : matrixMultiply(resolveGlobal(parent, depth + 1), local[index]);
      state[index] = 2;
      return global[index];
    };
    const skinMatrices = joints.map((joint, index) => matrixMultiply(resolveGlobal(index), new Float32Array(joint.inverseBindMatrix)));
    const rigInfluences = skeleton.influences || [];
    const skinnedPositions = new Float32Array(mesh.positions.length);
    for (let vertex = 0; vertex < mesh.sourceVertices; vertex++) {
      const source = vertex * 3;
      const bind = [mesh.positions[source], mesh.positions[source + 1], mesh.positions[source + 2]];
      const result = [0, 0, 0];
      let total = 0;
      for (let slot = 0; slot < 4; slot++) {
        const weight = mesh.blendWeights[vertex * 4 + slot];
        if (weight <= 0) continue;
        const rawIndex = mesh.blendIndices[vertex * 4 + slot];
        const jointId = (mesh.flags & 1) ? rawIndex : rigInfluences[rawIndex];
        const jointIndex = jointIndexById.get(jointId);
        if (jointIndex === undefined) continue;
        const point = transformPoint(skinMatrices[jointIndex], bind[0], bind[1], bind[2]);
        result[0] += point[0] * weight; result[1] += point[1] * weight; result[2] += point[2] * weight;
        total += weight;
      }
      const target = vertex * 3;
      if (total > 0) {
        skinnedPositions[target] = result[0] / total;
        skinnedPositions[target + 1] = result[1] / total;
        skinnedPositions[target + 2] = result[2] / total;
      } else {
        skinnedPositions[target] = bind[0]; skinnedPositions[target + 1] = bind[1]; skinnedPositions[target + 2] = bind[2];
      }
    }
    const vertices = mesh.vertices;
    const toView = (id) => {
      const point = id >= 0 ? id * 3 : 0;
      return [
        (skinnedPositions[point] - mesh.center[0]) * mesh.scale,
        (skinnedPositions[point + 1] - mesh.center[1]) * mesh.scale,
        (skinnedPositions[point + 2] - mesh.center[2]) * mesh.scale
      ];
    };
    for (let triangle = 0; triangle < mesh.triangles; triangle++) {
      const offset = triangle * 3;
      const p0 = toView(mesh.sourceIds[offset]), p1 = toView(mesh.sourceIds[offset + 1]), p2 = toView(mesh.sourceIds[offset + 2]);
      const e1 = [p1[0] - p0[0], p1[1] - p0[1], p1[2] - p0[2]];
      const e2 = [p2[0] - p0[0], p2[1] - p0[1], p2[2] - p0[2]];
      const normal = [e1[1] * e2[2] - e1[2] * e2[1], e1[2] * e2[0] - e1[0] * e2[2], e1[0] * e2[1] - e1[1] * e2[0]];
      const length = Math.hypot(...normal) || 1;
      for (let corner = 0; corner < 3; corner++) {
        const point = [p0, p1, p2][corner];
        const base = (offset + corner) * 9;
        vertices[base] = point[0]; vertices[base + 1] = point[1]; vertices[base + 2] = point[2];
        vertices[base + 3] = normal[0] / length; vertices[base + 4] = normal[1] / length; vertices[base + 5] = normal[2] / length;
      }
    }
  };

  const lerp3 = (a, b, t) => [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t];
  const quatSlerp = (a, b, t) => {
    let cosine = a[0] * b[0] + a[1] * b[1] + a[2] * b[2] + a[3] * b[3];
    let end = b;
    if (cosine < 0) { cosine = -cosine; end = b.map((value) => -value); }
    if (cosine > 0.9995) {
      const q = a.map((value, i) => value + (end[i] - value) * t);
      const length = Math.hypot(...q) || 1;
      return q.map((value) => value / length);
    }
    const angle = Math.acos(Math.max(-1, Math.min(1, cosine)));
    const sine = Math.sin(angle) || 1;
    const left = Math.sin((1 - t) * angle) / sine, right = Math.sin(t * angle) / sine;
    return a.map((value, i) => value * left + end[i] * right);
  };

  let cleanup = null;
  const clear = () => {
    if (cleanup) cleanup();
    cleanup = null;
  };
  const show = (canvas, bytes, skeleton = null) => {
    clear();
    const mesh = parseSkn(bytes);
    const gl = canvas.getContext("webgl", { antialias: true, alpha: false });
    const software = !gl;
    const context2d = software ? canvas.getContext("2d", { alpha: false }) : null;
    if (!gl && !context2d) throw new Error("This device could not create a graphics surface for the 3D preview.");
    const shader = (type, source) => {
      const result = gl.createShader(type);
      gl.shaderSource(result, source);
      gl.compileShader(result);
      if (!gl.getShaderParameter(result, gl.COMPILE_STATUS)) throw new Error("Could not prepare the 3D model renderer.");
      return result;
    };
    let vertexShader = null, fragmentShader = null, program = null, buffer = null;
    let yawUniform = null, pitchUniform = null, zoomUniform = null, aspectUniform = null;
    if (gl) {
      vertexShader = shader(gl.VERTEX_SHADER, `
        attribute vec3 position; attribute vec3 normal; attribute vec3 color;
        uniform float yaw; uniform float pitch; uniform float zoom; uniform float aspect;
        varying vec3 vNormal; varying vec3 vColor;
        vec3 rotate(vec3 p) {
          float cy = cos(yaw), sy = sin(yaw), cx = cos(pitch), sx = sin(pitch);
          vec3 y = vec3(cy*p.x + sy*p.z, p.y, -sy*p.x + cy*p.z);
          return vec3(y.x, cx*y.y - sx*y.z, sx*y.y + cx*y.z);
        }
        void main() {
          vec3 p = rotate(position), n = rotate(normal);
          float distance = 2.8 / zoom;
          gl_Position = vec4(p.x * 1.7 / aspect, p.y * 1.7, distance * 0.7 + p.z, distance + p.z);
          vNormal = n; vColor = color;
        }
      `);
      fragmentShader = shader(gl.FRAGMENT_SHADER, `
        precision mediump float;
        varying vec3 vNormal; varying vec3 vColor;
        void main() {
          float light = 0.35 + 0.65 * abs(dot(normalize(vNormal), normalize(vec3(0.5, 0.8, 1.0))));
          gl_FragColor = vec4(vColor * light, 1.0);
        }
      `);
      program = gl.createProgram();
      gl.attachShader(program, vertexShader); gl.attachShader(program, fragmentShader);
      gl.linkProgram(program);
      if (!gl.getProgramParameter(program, gl.LINK_STATUS)) throw new Error("Could not link the 3D model renderer.");
      buffer = gl.createBuffer();
      gl.bindBuffer(gl.ARRAY_BUFFER, buffer);
      gl.bufferData(gl.ARRAY_BUFFER, mesh.vertices, gl.DYNAMIC_DRAW);
      gl.useProgram(program);
      for (const [name, offset] of [["position", 0], ["normal", 12], ["color", 24]]) {
        const location = gl.getAttribLocation(program, name);
        gl.enableVertexAttribArray(location);
        gl.vertexAttribPointer(location, 3, gl.FLOAT, false, 36, offset);
      }
      yawUniform = gl.getUniformLocation(program, "yaw");
      pitchUniform = gl.getUniformLocation(program, "pitch");
      zoomUniform = gl.getUniformLocation(program, "zoom");
      aspectUniform = gl.getUniformLocation(program, "aspect");
    }
    let yaw = 0.55, pitch = -0.2, zoom = 1, pointer = null;
    let activeSkeleton = skeleton;
    let animation = null, animationStart = 0, animationPosition = 0, speed = 1, playing = false, animationFrame = 0, lastAnimationUpdate = 0;
    const render = () => {
      const pixelRatio = software ? Math.min(1.5, window.devicePixelRatio || 1) : window.devicePixelRatio || 1;
      const width = Math.max(1, Math.round(canvas.clientWidth * pixelRatio));
      const height = Math.max(1, Math.round(canvas.clientHeight * pixelRatio));
      if (canvas.width !== width || canvas.height !== height) { canvas.width = width; canvas.height = height; }
      if (gl) {
        gl.viewport(0, 0, width, height);
        gl.clearColor(0.09, 0.075, 0.10, 1);
        gl.clear(gl.COLOR_BUFFER_BIT | gl.DEPTH_BUFFER_BIT);
        gl.enable(gl.DEPTH_TEST);
        gl.disable(gl.CULL_FACE);
        gl.uniform1f(yawUniform, yaw); gl.uniform1f(pitchUniform, pitch);
        gl.uniform1f(zoomUniform, zoom); gl.uniform1f(aspectUniform, width / height);
        gl.drawArrays(gl.TRIANGLES, 0, mesh.triangles * 3);
        return;
      }

      const ctx = context2d;
      ctx.clearRect(0, 0, width, height);
      ctx.fillStyle = "#171319"; ctx.fillRect(0, 0, width, height);
      const cy = Math.cos(yaw), sy = Math.sin(yaw), cx = Math.cos(pitch), sx = Math.sin(pitch);
      const distance = 2.8 / zoom, aspect = width / height;
      const rotate = (x, y, z) => {
        const rotatedX = cy * x + sy * z, rotatedZ = -sy * x + cy * z;
        return [rotatedX, cx * y - sx * rotatedZ, sx * y + cx * rotatedZ];
      };
      const project = (point) => {
        const w = distance + point[2];
        return w > 0.1 ? [
          (point[0] * 1.7 / aspect / w * 0.5 + 0.5) * width,
          (0.5 - point[1] * 1.7 / w * 0.5) * height,
          point[2]
        ] : null;
      };
      const triangleStep = Math.max(1, Math.ceil(mesh.triangles / 50000));
      const triangles = [];
      for (let triangle = 0; triangle < mesh.triangles; triangle += triangleStep) {
        const offset = triangle * 27;
        const p0 = rotate(mesh.vertices[offset], mesh.vertices[offset + 1], mesh.vertices[offset + 2]);
        const p1 = rotate(mesh.vertices[offset + 9], mesh.vertices[offset + 10], mesh.vertices[offset + 11]);
        const p2 = rotate(mesh.vertices[offset + 18], mesh.vertices[offset + 19], mesh.vertices[offset + 20]);
        const a = project(p0), b = project(p1), c = project(p2);
        if (!a || !b || !c) continue;
        const normal = rotate(mesh.vertices[offset + 3], mesh.vertices[offset + 4], mesh.vertices[offset + 5]);
        const normalLength = Math.hypot(...normal) || 1;
        const light = 0.35 + 0.65 * Math.abs((normal[0] * 0.5 + normal[1] * 0.8 + normal[2]) / (normalLength * Math.hypot(0.5, 0.8, 1)));
        const red = Math.round(mesh.vertices[offset + 6] * light * 255);
        const green = Math.round(mesh.vertices[offset + 7] * light * 255);
        const blue = Math.round(mesh.vertices[offset + 8] * light * 255);
        triangles.push({ a, b, c, depth: (p0[2] + p1[2] + p2[2]) / 3, fill: `rgb(${red}, ${green}, ${blue})` });
      }
      triangles.sort((a, b) => a.depth - b.depth);
      for (const triangle of triangles) {
        ctx.beginPath(); ctx.moveTo(triangle.a[0], triangle.a[1]);
        ctx.lineTo(triangle.b[0], triangle.b[1]); ctx.lineTo(triangle.c[0], triangle.c[1]);
        ctx.closePath(); ctx.fillStyle = triangle.fill; ctx.fill();
      }
    };
    const uploadMesh = () => {
      if (gl) {
        gl.bindBuffer(gl.ARRAY_BUFFER, buffer);
        gl.bufferSubData(gl.ARRAY_BUFFER, 0, mesh.vertices);
      }
      render();
    };
    const stopAnimationFrame = () => {
      if (animationFrame) cancelAnimationFrame(animationFrame);
      animationFrame = 0;
    };
    const currentAnimationTime = (timestamp = performance.now()) => {
      const duration = Math.max(0.001, animation?.duration || 0.001);
      return (animationPosition + (playing ? (timestamp - animationStart) / 1000 * speed : 0)) % duration;
    };
    const tick = (timestamp) => {
      animationFrame = 0;
      if (!playing || !animation || !activeSkeleton) return;
      if (timestamp - lastAnimationUpdate < 1000 / (software ? 12 : 24)) {
        animationFrame = requestAnimationFrame(tick);
        return;
      }
      lastAnimationUpdate = timestamp;
      const duration = Math.max(0.001, animation.duration || 0.001);
      const elapsed = currentAnimationTime(timestamp);
      deformMesh(mesh, activeSkeleton, animation, elapsed * animation.sampleRate);
      uploadMesh();
      animationFrame = requestAnimationFrame(tick);
    };
    const setAnimation = (nextAnimation) => {
      stopAnimationFrame();
      animation = nextAnimation || null;
      playing = Boolean(animation && activeSkeleton);
      animationPosition = 0;
      if (animation) {
        animationStart = performance.now();
        deformMesh(mesh, activeSkeleton, animation, 0);
        if (playing) animationFrame = requestAnimationFrame(tick);
      } else {
        mesh.vertices.set(mesh.bindVertices);
      }
      uploadMesh();
    };
    const setPlaying = (value) => {
      if (!animation || !activeSkeleton) return;
      const next = Boolean(value);
      if (playing && !next) animationPosition = currentAnimationTime();
      playing = next;
      if (playing) {
        animationStart = performance.now();
        if (!animationFrame) animationFrame = requestAnimationFrame(tick);
      } else stopAnimationFrame();
    };
    const setSpeed = (value) => {
      const position = currentAnimationTime();
      speed = Math.max(0.25, Math.min(2, Number(value) || 1));
      animationPosition = position;
      if (playing) animationStart = performance.now();
    };
    const setSkeleton = (nextSkeleton) => {
      activeSkeleton = nextSkeleton || null;
      if (animation && activeSkeleton) {
        deformMesh(mesh, activeSkeleton, animation, 0);
        if (playing) animationStart = performance.now();
        uploadMesh();
      } else if (!activeSkeleton) {
        mesh.vertices.set(mesh.bindVertices);
        uploadMesh();
      }
    };
    const down = (event) => { pointer = { x: event.clientX, y: event.clientY }; canvas.setPointerCapture(event.pointerId); };
    const move = (event) => {
      if (!pointer) return;
      yaw += (event.clientX - pointer.x) * 0.01;
      pitch = Math.max(-1.45, Math.min(1.45, pitch + (event.clientY - pointer.y) * 0.01));
      pointer = { x: event.clientX, y: event.clientY };
      render();
    };
    const up = () => { pointer = null; };
    const wheel = (event) => { event.preventDefault(); zoom = Math.max(0.45, Math.min(2.5, zoom * Math.exp(-event.deltaY * 0.001))); render(); };
    const reset = () => { yaw = 0.55; pitch = -0.2; zoom = 1; render(); };
    canvas.addEventListener("pointerdown", down);
    canvas.addEventListener("pointermove", move);
    canvas.addEventListener("pointerup", up);
    canvas.addEventListener("pointercancel", up);
    canvas.addEventListener("wheel", wheel, { passive: false });
    canvas.addEventListener("dblclick", reset);
    const observer = new ResizeObserver(render);
    observer.observe(canvas);
    cleanup = () => {
      playing = false;
      stopAnimationFrame();
      observer.disconnect();
      canvas.removeEventListener("pointerdown", down);
      canvas.removeEventListener("pointermove", move);
      canvas.removeEventListener("pointerup", up);
      canvas.removeEventListener("pointercancel", up);
      canvas.removeEventListener("wheel", wheel);
      canvas.removeEventListener("dblclick", reset);
      if (gl) {
        gl.deleteBuffer(buffer); gl.deleteProgram(program);
        gl.deleteShader(vertexShader); gl.deleteShader(fragmentShader);
      }
    };
    mesh.bindVertices = mesh.vertices.slice();
    render();
    return { ...mesh, renderingMode: software ? "software" : "webgl", setAnimation, setPlaying, setSpeed, setSkeleton };
  };
  window.SableModelPreview = { parseSkn, show, clear };
})();
