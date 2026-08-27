import type { ElementRef } from "../../ipc/types";
import type { SceneGraph } from "../../canvas/scene-types";
import { SceneView } from "../SceneView";

interface Props {
  scene: SceneGraph;
  selected: ElementRef | null;
  onSelect: (target: ElementRef | null) => void;
}

export function MilestonesView(props: Props) {
  return <SceneView {...props} label="里程碑视图" />;
}
