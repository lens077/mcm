import type { EditCommand, ElementRef } from "../../ipc/types";
import type { SceneGraph } from "../../canvas/scene-types";
import { SceneView } from "../SceneView";
import { useTimelineInteractions } from "./interactions";

interface Props {
  scene: SceneGraph;
  selected: ElementRef | null;
  onSelect: (target: ElementRef | null) => void;
  onCommand?: (command: EditCommand) => void;
}

export function TimelineView({ scene, selected, onSelect, onCommand }: Props) {
  const interactions = useTimelineInteractions({
    scene,
    onCommand: onCommand ?? (() => undefined),
  });

  return (
    <SceneView
      scene={scene}
      selected={selected}
      onSelect={onSelect}
      label="时间线视图"
      gestures={{
        onPressStart: interactions.onPressStart,
        onPressMove: interactions.onPressMove,
        onPressEnd: interactions.onPressEnd,
      }}
    />
  );
}
