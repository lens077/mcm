import { useCallback, useRef, useState } from "react";
import type { EditCommand } from "../../ipc/types";
import type { SceneGraph, SceneNode } from "../../canvas/scene-types";
import { hitNode } from "../../canvas/hit";
import {
  type BarGrip,
  type Point,
  daysFromDelta,
  gripFor,
  scheduleCommandFor,
} from "../interaction-model";

/** Logical units per day; must match layout::timeline::DAY_WIDTH. */
export const DAY_WIDTH = 26;

export interface DragPreview {
  barKey: string;
  grip: BarGrip;
  days: number;
}

interface Options {
  scene: SceneGraph;
  onCommand: (command: EditCommand) => void;
}

/**
 * Timeline editing: drag a bar to shift it, or drag an edge to change the
 * start/end date. Both emit SetSchedule with explicit dates.
 */
export function useTimelineInteractions({ scene, onCommand }: Options) {
  const dragRef = useRef<{ bar: SceneNode; grip: BarGrip; startX: number } | null>(null);
  const [preview, setPreview] = useState<DragPreview | null>(null);

  const onPressStart = useCallback(
    (world: Point): boolean => {
      const bar = hitNode(scene, world);
      // Undated bars have no window to shift.
      if (!bar || !bar.sub_text?.includes("..")) return false;
      dragRef.current = { bar, grip: gripFor(bar, world), startX: world.x };
      return true;
    },
    [scene],
  );

  const onPressMove = useCallback((world: Point) => {
    const drag = dragRef.current;
    if (!drag) return;
    const days = daysFromDelta(world.x - drag.startX, DAY_WIDTH);
    setPreview({
      barKey: drag.bar.text,
      grip: drag.grip,
      days,
    });
  }, []);

  const onPressEnd = useCallback(
    (world: Point) => {
      const drag = dragRef.current;
      dragRef.current = null;
      setPreview(null);
      if (!drag) return;
      const days = daysFromDelta(world.x - drag.startX, DAY_WIDTH);
      const command = scheduleCommandFor(drag.bar, drag.grip, days);
      if (command) onCommand(command);
    },
    [onCommand],
  );

  return { preview, onPressStart, onPressMove, onPressEnd };
}
