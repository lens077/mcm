export interface FrameStats {
  fps: number;
  /** Slowest frame in the sample window; the real 60fps risk indicator. */
  worstFrameMs: number;
}

/** Summarises frame deltas (ms) into an average FPS plus the worst frame. */
export function summarise(deltas: number[]): FrameStats {
  const usable = deltas.filter((delta) => delta > 0);
  if (usable.length === 0) return { fps: 0, worstFrameMs: 0 };
  const total = usable.reduce((sum, delta) => sum + delta, 0);
  const average = total / usable.length;
  return {
    fps: average > 0 ? 1000 / average : 0,
    worstFrameMs: Math.max(...usable),
  };
}

/** True when the sample window meets the 60fps budget with a small tolerance. */
export function meetsFrameBudget(stats: FrameStats, minimumFps = 55): boolean {
  return stats.fps >= minimumFps;
}
