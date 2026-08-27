import { useEffect, useRef, useState } from "react";
import { summarise, type FrameStats } from "./perf";

interface Props {
  /** Node count of the active scene, shown alongside the frame rate. */
  nodeCount: number;
}

/**
 * Development-only frame-rate probe used to check the 60fps budget
 * (宪法 II / SC-002). Hidden in production builds.
 */
export function PerfOverlay({ nodeCount }: Props) {
  const [stats, setStats] = useState<FrameStats>({ fps: 0, worstFrameMs: 0 });
  const samples = useRef<number[]>([]);
  const rafRef = useRef(0);

  useEffect(() => {
    let previous = performance.now();
    const tick = (now: number) => {
      const delta = now - previous;
      previous = now;
      const buffer = samples.current;
      buffer.push(delta);
      if (buffer.length > 60) buffer.shift();
      setStats(summarise(buffer));
      rafRef.current = requestAnimationFrame(tick);
    };
    rafRef.current = requestAnimationFrame(tick);
    return () => {
      cancelAnimationFrame(rafRef.current);
    };
  }, []);

  const healthy = stats.fps >= 55;
  return (
    <div className={healthy ? "perf-overlay" : "perf-overlay perf-warn"} aria-hidden="true">
      <span>{stats.fps.toFixed(0)} fps</span>
      <span>{stats.worstFrameMs.toFixed(1)} ms</span>
      <span>{nodeCount} 节点</span>
    </div>
  );
}
