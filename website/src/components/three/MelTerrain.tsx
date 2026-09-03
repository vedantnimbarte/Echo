"use client";

import { useEffect, useMemo, useRef, useState } from "react";
import { Canvas, useFrame, useThree } from "@react-three/fiber";
import * as THREE from "three";
import { envelope, now, phase } from "./wakeCycle";

/* ------------------------------------------------------------------
   A mel spectrogram, extruded.

   Echo's wake word runs on a 32-bin mel matrix, 76 frames at a time. This is
   that matrix as a heightfield: bins across, time running toward you, energy
   as elevation. When the phrase lands, a bright ridge crosses the surface.
   ------------------------------------------------------------------ */

const BINS = 32; // mel bins — the real input width
const FRAMES = 76; // frames per classifier window

const vertexShader = /* glsl */ `
  uniform float uTime;
  uniform float uEvent;
  uniform float uRidge;

  varying float vH;
  varying vec2 vUv;

  float hash(vec2 p) {
    return fract(sin(dot(p, vec2(127.1, 311.7))) * 43758.5453);
  }

  float noise(vec2 p) {
    vec2 i = floor(p);
    vec2 f = fract(p);
    vec2 u = f * f * (3.0 - 2.0 * f);
    return mix(
      mix(hash(i), hash(i + vec2(1.0, 0.0)), u.x),
      mix(hash(i + vec2(0.0, 1.0)), hash(i + vec2(1.0, 1.0)), u.x),
      u.y
    );
  }

  void main() {
    vUv = uv;

    // Time runs toward the viewer, so features drift out of the far edge.
    float t = uTime * 0.16;
    vec2 p = vec2(uv.x * 4.5, uv.y * 6.0 + t * 6.0);

    float base =
      noise(p) * 0.52 +
      noise(p * 2.3) * 0.27 +
      noise(p * 5.1) * 0.13;

    // Speech energy concentrates in the low bins, as it does in a real mel bank.
    float tilt = pow(1.0 - uv.x, 1.0) * 0.28 + 0.72;

    // Utterances arrive in bursts rather than as a constant hum.
    float burst = smoothstep(0.3, 0.78, noise(vec2(7.0, uv.y * 2.6 + t * 6.0)));

    float h = base * tilt * mix(0.3, 1.0, burst);

    // The detected phrase: one loud band travelling far edge -> near edge.
    float band = exp(-pow((uv.y - uRidge) * 17.0, 2.0));
    h += band * uEvent * (0.45 + 0.55 * base) * 1.3;

    vH = h;

    vec3 pos = position;
    pos.z += h * 1.15;

    gl_Position = projectionMatrix * modelViewMatrix * vec4(pos, 1.0);
  }
`;

const fragmentShader = /* glsl */ `
  uniform vec3 uLow;
  uniform vec3 uMid;
  uniform vec3 uHot;

  varying float vH;
  varying vec2 vUv;

  void main() {
    // Anti-aliased cell grid at the true matrix resolution: 32 bins x 76 frames.
    vec2 cells = vUv * vec2(${BINS}.0, ${FRAMES}.0);
    vec2 g = abs(fract(cells) - 0.5) / fwidth(cells);
    float line = 1.0 - min(min(g.x, g.y), 1.0);

    vec3 col = mix(uLow, uMid, smoothstep(0.04, 0.42, vH));
    col = mix(col, uHot, smoothstep(0.44, 0.95, vH));

    // Depth haze: the far edge dissolves into the room.
    float haze = smoothstep(0.0, 0.34, vUv.y);
    // Feather the side edges so the mesh has no visible boundary.
    float sides = smoothstep(0.0, 0.1, vUv.x) * smoothstep(1.0, 0.9, vUv.x);

    // The matrix stays legible everywhere; energy decides how hot it burns.
    float energy = smoothstep(0.02, 0.55, vH);
    float alpha = (line * (0.5 + 0.9 * energy) + 0.07 * energy) * haze * sides;

    if (alpha < 0.004) discard;
    gl_FragColor = vec4(col, alpha);
  }
`;

function Terrain({ still }: { still: boolean }) {
  const material = useRef<THREE.ShaderMaterial>(null);
  const { size } = useThree();

  const uniforms = useMemo(
    () => ({
      uTime: { value: still ? 4.2 : 0 },
      uEvent: { value: still ? 0.85 : 0 },
      uRidge: { value: still ? 0.62 : 0 },
      uLow: { value: new THREE.Color("#241f5e") },
      uMid: { value: new THREE.Color("#4ff0e6") },
      uHot: { value: new THREE.Color("#ff8a4c") },
    }),
    [still],
  );

  useFrame(() => {
    if (still || !material.current) return;
    const t = now();
    const u = material.current.uniforms;
    u.uTime.value = t;
    u.uEvent.value = envelope(t);
    u.uRidge.value = phase(t);
  });

  // Denser mesh on wide screens; phones get a lighter one.
  const seg = size.width < 720 ? [80, 96] : [140, 168];

  return (
    <mesh rotation={[-Math.PI / 2, 0, 0]} position={[0, -0.58, 0.35]}>
      <planeGeometry args={[9.6, 6.4, seg[0], seg[1]]} />
      <shaderMaterial
        ref={material}
        uniforms={uniforms}
        vertexShader={vertexShader}
        fragmentShader={fragmentShader}
        transparent
        depthWrite={false}
        blending={THREE.AdditiveBlending}
      />
    </mesh>
  );
}

/** Eases the camera toward the pointer so the surface has real parallax. */
function CameraDrift({ enabled }: { enabled: boolean }) {
  const target = useRef({ x: 0, y: 0 });

  useEffect(() => {
    if (!enabled) return;
    const onMove = (e: PointerEvent) => {
      target.current = {
        x: (e.clientX / window.innerWidth - 0.5) * 2,
        y: (e.clientY / window.innerHeight - 0.5) * 2,
      };
    };
    window.addEventListener("pointermove", onMove, { passive: true });
    return () => window.removeEventListener("pointermove", onMove);
  }, [enabled]);

  useFrame(({ camera }, delta) => {
    if (!enabled) return;
    const k = 1 - Math.pow(0.001, delta);
    camera.position.x += (target.current.x * 0.34 - camera.position.x) * k;
    camera.position.y += (1.5 - target.current.y * 0.2 - camera.position.y) * k;
    camera.lookAt(0, -0.45, 0.2);
  });

  return null;
}

export default function MelTerrain({ className = "" }: { className?: string }) {
  const host = useRef<HTMLDivElement>(null);
  const [visible, setVisible] = useState(true);
  // ssr:false, so the media query is safe to read on the first render.
  const [still, setStill] = useState(
    () => window.matchMedia("(prefers-reduced-motion: reduce)").matches,
  );

  // No WebGL (old GPU, blocked context) — the section's floor grid stands in.
  const supported = useMemo(() => {
    try {
      const probe = document.createElement("canvas");
      return !!(probe.getContext("webgl2") || probe.getContext("webgl"));
    } catch {
      return false;
    }
  }, []);

  useEffect(() => {
    const motion = window.matchMedia("(prefers-reduced-motion: reduce)");
    const onChange = () => setStill(motion.matches);
    motion.addEventListener("change", onChange);
    return () => motion.removeEventListener("change", onChange);
  }, []);

  // Stop rendering entirely once the hero scrolls away.
  useEffect(() => {
    const el = host.current;
    if (!el) return;
    const io = new IntersectionObserver(([entry]) => setVisible(entry.isIntersecting));
    io.observe(el);
    return () => io.disconnect();
  }, []);

  return (
    <div ref={host} className={className}>
      {supported && (
      <Canvas
        frameloop={still ? "demand" : visible ? "always" : "never"}
        dpr={[1, 1.75]}
        gl={{ antialias: true, alpha: true, powerPreference: "high-performance" }}
        camera={{ position: [0, 1.5, 4.6], fov: 44 }}
        onCreated={({ camera }) => camera.lookAt(0, -0.45, 0.2)}
      >
        <Terrain still={still} />
        <CameraDrift enabled={!still} />
      </Canvas>
      )}
    </div>
  );
}
