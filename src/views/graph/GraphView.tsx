import type { EditCommand, ElementRef } from "../../ipc/types";
import type { SceneGraph } from "../../canvas/scene-types";
import { SceneView } from "../SceneView";
import { useGraphInteractions } from "./interactions";

interface Props {
  scene: SceneGraph;
  selected: ElementRef | null;
  onSelect: (target: ElementRef | null) => void;
  onCommand?: (command: EditCommand) => void;
}

export function GraphView({ scene, selected, onSelect, onCommand }: Props) {
  const interactions = useGraphInteractions({
    scene,
    selected,
    onCommand: onCommand ?? (() => undefined),
    onSelect,
  });

  return (
    <SceneView
      scene={scene}
      selected={selected}
      onSelect={onSelect}
      label="依赖网络视图"
      gestures={{
        onPressStart: interactions.onPressStart,
        onPressMove: interactions.onPressMove,
        onPressEnd: interactions.onPressEnd,
        onClick: interactions.onClick,
        onKeyDown: interactions.onKeyDown,
      }}
    />
  );
}
