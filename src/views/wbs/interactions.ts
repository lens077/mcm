import { useCallback, useRef, useState } from "react";
import type { EditCommand, ElementRef } from "../../ipc/types";
import type { SceneGraph, SceneNode } from "../../canvas/scene-types";
import { refKey } from "../../canvas/scene-types";
import { hitNode } from "../../canvas/hit";
import {
  type DropMode,
  type Point,
  dropModeFor,
  moveCommandFor,
  parentKeyOf,
  siblingIndex,
} from "../interaction-model";

export interface DropIndicator {
  targetKey: string;
  mode: DropMode;
}

interface Options {
  scene: SceneGraph;
  selected: ElementRef | null;
  onCommand: (command: EditCommand) => void;
  /** Asks the shell for a new title; returns null when cancelled. */
  promptTitle: (current: string) => string | null;
  confirmDelete: (title: string) => boolean;
}

/**
 * WBS editing gestures: double-click renames, Enter/Tab add siblings and
 * children, Delete removes, and dragging re-parents with a drop indicator.
 */
export function useWbsInteractions({
  scene,
  selected,
  onCommand,
  promptTitle,
  confirmDelete,
}: Options) {
  const dragRef = useRef<SceneNode | null>(null);
  const [indicator, setIndicator] = useState<DropIndicator | null>(null);

  const nodeFor = useCallback(
    (ref: ElementRef | null): SceneNode | null => {
      if (!ref) return null;
      const key = refKey(ref);
      return scene.nodes.find((node) => refKey(node.ref) === key) ?? null;
    },
    [scene],
  );

  const rename = useCallback(
    (node: SceneNode) => {
      if (node.ref.kind !== "task") return;
      const title = promptTitle(node.text);
      if (title === null || title.trim().length === 0) return;
      onCommand({ kind: "rename_task", id: node.ref.id, title: title.trim() });
    },
    [onCommand, promptTitle],
  );

  const onDoubleClick = useCallback(
    (world: Point) => {
      const node = hitNode(scene, world);
      if (node) rename(node);
    },
    [scene, rename],
  );

  const onDragStart = useCallback(
    (world: Point) => {
      dragRef.current = hitNode(scene, world);
    },
    [scene],
  );

  const onDragMove = useCallback(
    (world: Point) => {
      if (!dragRef.current) return;
      const target = hitNode(scene, world);
      if (!target || refKey(target.ref) === refKey(dragRef.current.ref)) {
        setIndicator(null);
        return;
      }
      setIndicator({ targetKey: refKey(target.ref), mode: dropModeFor(target, world) });
    },
    [scene],
  );

  const onDragEnd = useCallback(
    (world: Point) => {
      const dragged = dragRef.current;
      dragRef.current = null;
      setIndicator(null);
      if (!dragged) return;
      const target = hitNode(scene, world);
      if (!target) return;
      const command = moveCommandFor(scene, dragged, target, dropModeFor(target, world));
      if (command) onCommand(command);
    },
    [scene, onCommand],
  );

  const onKeyDown = useCallback(
    (event: KeyboardEvent): boolean => {
      const node = nodeFor(selected);
      if (!node || node.ref.kind !== "task") return false;
      const id = node.ref.id;

      switch (event.key) {
        case "F2":
          rename(node);
          return true;
        case "Enter": {
          // Enter adds a sibling directly below the selection.
          const parentKey = parentKeyOf(scene, node);
          const parentNode = parentKey
            ? scene.nodes.find((candidate) => refKey(candidate.ref) === parentKey)
            : undefined;
          const parentId =
            parentNode && parentNode.ref.kind === "task" ? parentNode.ref.id : null;
          const index = siblingIndex(scene, parentKey, refKey(node.ref)) + 1;
          onCommand({ kind: "add_task", parent: parentId, index, title: "新任务" });
          return true;
        }
        case "Tab":
          // Tab nests a new child under the selection.
          onCommand({ kind: "add_task", parent: id, index: 0, title: "新子任务" });
          return true;
        case "Delete":
        case "Backspace":
          if (confirmDelete(node.text)) {
            onCommand({ kind: "delete_task", id });
          }
          return true;
        default:
          return false;
      }
    },
    [scene, selected, nodeFor, rename, onCommand, confirmDelete],
  );

  return { indicator, onDoubleClick, onDragStart, onDragMove, onDragEnd, onKeyDown };
}
