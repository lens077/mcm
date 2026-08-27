import type { EditCommand, ElementRef } from "../../ipc/types";
import type { SceneGraph } from "../../canvas/scene-types";
import { SceneView } from "../SceneView";
import { useWbsInteractions } from "./interactions";

interface Props {
  scene: SceneGraph;
  selected: ElementRef | null;
  onSelect: (target: ElementRef | null) => void;
  onCommand?: (command: EditCommand) => void;
}

export function WbsView({ scene, selected, onSelect, onCommand }: Props) {
  const interactions = useWbsInteractions({
    scene,
    selected,
    onCommand: onCommand ?? (() => undefined),
    promptTitle: (current) => window.prompt("任务名称", current),
    confirmDelete: (title) => window.confirm(`删除「${title}」及其全部子任务？`),
  });

  return (
    <SceneView
      scene={scene}
      selected={selected}
      onSelect={onSelect}
      label="任务分解视图"
      gestures={{
        onDoubleClick: interactions.onDoubleClick,
        onPressStart: (world) => {
          interactions.onDragStart(world);
          return false;
        },
        onPressMove: interactions.onDragMove,
        onPressEnd: interactions.onDragEnd,
        onKeyDown: interactions.onKeyDown,
      }}
    />
  );
}
