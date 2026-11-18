// League SKN mesh preview. Geometry only: rigging, animation, and material textures
// require companion files that are not always present in a mod package.
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
    if (flags & 1) take(u16()); // Direct blend index block
    const indicesOffset = take(indexCount * 2);
    const verticesOffset = take(vertexCount * stride);
    if (major >= 2) take(12); // End tab

    const positions = new Float32Array(vertexCount * 3);
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
    }
    const center = min.map((value, axis) => (value + max[axis]) / 2);
    const extent = Math.max(...max.map((value, axis) => value - min[axis]));
    if (!(extent > 0)) throw new Error("This SKN model has no visible geometry.");
    const scale = 1.7 / extent;
    // Expand indexed triangles into one interleaved position/normal/color buffer.
    // This keeps the renderer compatible with WebGL 1 and large source meshes.
    const vertices = new Float32Array(indexCount * 9);
    let output = 0, triangles = 0;
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
    return { vertices: vertices.subarray(0, output), triangles, parts: ranges.length, sourceVertices: vertexCount };
  };

  let cleanup = null;
  const clear = () => {
    if (cleanup) cleanup();
    cleanup = null;
  };
  const show = (canvas, bytes) => {
    clear();
    const mesh = parseSkn(bytes);
    const gl = canvas.getContext("webgl", { antialias: true, alpha: false });
    if (!gl) throw new Error("3D preview needs WebGL. Enable graphics acceleration to view this model.");
    const shader = (type, source) => {
      const result = gl.createShader(type);
      gl.shaderSource(result, source);
      gl.compileShader(result);
      if (!gl.getShaderParameter(result, gl.COMPILE_STATUS)) throw new Error("Could not prepare the 3D model renderer.");
      return result;
    };
    const vertexShader = shader(gl.VERTEX_SHADER, `
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
    const fragmentShader = shader(gl.FRAGMENT_SHADER, `
      precision mediump float;
      varying vec3 vNormal; varying vec3 vColor;
      void main() {
        float light = 0.35 + 0.65 * abs(dot(normalize(vNormal), normalize(vec3(0.5, 0.8, 1.0))));
        gl_FragColor = vec4(vColor * light, 1.0);
      }
    `);
    const program = gl.createProgram();
    gl.attachShader(program, vertexShader); gl.attachShader(program, fragmentShader);
    gl.linkProgram(program);
    if (!gl.getProgramParameter(program, gl.LINK_STATUS)) throw new Error("Could not link the 3D model renderer.");
    const buffer = gl.createBuffer();
    gl.bindBuffer(gl.ARRAY_BUFFER, buffer);
    gl.bufferData(gl.ARRAY_BUFFER, mesh.vertices, gl.STATIC_DRAW);
    gl.useProgram(program);
    for (const [name, offset] of [["position", 0], ["normal", 12], ["color", 24]]) {
      const location = gl.getAttribLocation(program, name);
      gl.enableVertexAttribArray(location);
      gl.vertexAttribPointer(location, 3, gl.FLOAT, false, 36, offset);
    }
    const yawUniform = gl.getUniformLocation(program, "yaw");
    const pitchUniform = gl.getUniformLocation(program, "pitch");
    const zoomUniform = gl.getUniformLocation(program, "zoom");
    const aspectUniform = gl.getUniformLocation(program, "aspect");
    let yaw = 0.55, pitch = -0.2, zoom = 1, pointer = null;
    const render = () => {
      const width = Math.max(1, Math.round(canvas.clientWidth * devicePixelRatio));
      const height = Math.max(1, Math.round(canvas.clientHeight * devicePixelRatio));
      if (canvas.width !== width || canvas.height !== height) { canvas.width = width; canvas.height = height; }
      gl.viewport(0, 0, width, height);
      gl.clearColor(0.09, 0.075, 0.10, 1);
      gl.clear(gl.COLOR_BUFFER_BIT | gl.DEPTH_BUFFER_BIT);
      gl.enable(gl.DEPTH_TEST);
      gl.disable(gl.CULL_FACE);
      gl.uniform1f(yawUniform, yaw); gl.uniform1f(pitchUniform, pitch);
      gl.uniform1f(zoomUniform, zoom); gl.uniform1f(aspectUniform, width / height);
      gl.drawArrays(gl.TRIANGLES, 0, mesh.triangles * 3);
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
      observer.disconnect();
      canvas.removeEventListener("pointerdown", down);
      canvas.removeEventListener("pointermove", move);
      canvas.removeEventListener("pointerup", up);
      canvas.removeEventListener("pointercancel", up);
      canvas.removeEventListener("wheel", wheel);
      canvas.removeEventListener("dblclick", reset);
      gl.deleteBuffer(buffer); gl.deleteProgram(program);
      gl.deleteShader(vertexShader); gl.deleteShader(fragmentShader);
    };
    render();
    return mesh;
  };
  window.SableModelPreview = { parseSkn, show, clear };
})();
