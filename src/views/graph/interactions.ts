import { useCallback, useRef, useState } from "react";
import type { EditCommand, ElementRef } from "../../ipc/types";
import type { SceneGraph, SceneNode } from "../../canvas/scene-types";
import { refKey } from "../../canvas/scene-types";
import { hitEdge, hitNode } from "../../canvas/hit";
import { type Point, isOnLinkAnchor, linkCommandFor, unlinkCommandFor } from "../interaction-model";

export interface PendingLink {
  fromKey: string;
  from: Point;
  to: Point;
}

interface Options {
  scene: SceneGraph;
  selected: ElementRef | null;
  onCommand: (command: EditCommand) => void;
  onSelect: (ref: ElementRef | null) => void;
}

/**
 * Dependency-graph editing: drag from a node's right anchor to another node to
 * create a dependency; select an edge and press Delete to remove it.
 */
export function useGraphInteractions({ scene, selected, onCommand, onSelect }: Options) {
  const originRef = useRef<SceneNode | null>(null);
  const [pending, setPending] = useState<PendingLink | null>(null);

  /** Returns true when the press started a link drag (so panning is skipped). */
  const onPressStart = useCallback(
    (world: Point): boolean => {
      const node = hitNode(scene, world);
      if (node && isOnLinkAnchor(node, world)) {
        originRef.current = node;
        setPending({
          fromKey: refKey(node.ref),
          from: { x: node.x + node.w, y: node.y + node.h / 2 },
          to: world,
        });
        return true;
      }
      return false;
    },
    [scene],
  );

  const onPressMove = useCallback((world: Point) => {
    if (!originRef.current) return;
    setPending((prev) => (prev ? { ...prev, to: world } : prev));
  }, []);

  const onPressEnd = useCallback(
    (world: Point) => {
      const origin = originRef.current;
      originRef.current = null;
      setPending(null);
      if (!origin) return;
      const target = hitNode(scene, world);
      if (!target) return;
      const command = linkCommandFor(origin, target);
      if (command) onCommand(command);
    },
    [scene, onCommand],
  );

  /** Clicking an edge selects the dependency it represents. */
  const onClick = useCallback(
    (world: Point): boolean => {
      const edge = hitEdge(scene, world);
      if (!edge) return false;
      if (edge.from.kind === "task" && edge.to.kind === "task") {
        onSelect({
          kind: "dependency",
          predecessor: edge.from.id,
          successor: edge.to.id,
        });
        return true;
      }
      return false;
    },
    [scene, onSelect],
  );

  const onKeyDown = useCallback(
    (event: KeyboardEvent): boolean => {
      if (event.key !== "Delete" && event.key !== "Backspace") return false;
      if (!selected || selected.kind !== "dependency") return false;
      const command = unlinkCommandFor({
        from: { kind: "task", id: selected.predecessor },
        to: { kind: "task", id: selected.successor },
      });
      if (command) onCommand(command);
      return true;
    },
    [selected, onCommand],
  );

  return { pending, onPressStart, onPressMove, onPressEnd, onClick, onKeyDown };
}
