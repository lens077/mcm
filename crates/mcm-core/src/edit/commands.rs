//! Closed edit-command set (data-model.md §EditCommand).
//!
//! Every mutation goes through `apply`, which returns the exact inverse command
//! so undo/redo can replay history precisely (spec FR-012).

use serde::{Deserialize, Serialize};

use crate::model::{Dependency, Milestone, MilestoneId, Plan, Schedule, Task, TaskId, parse_date};
use crate::scene::ViewKind;

/// Which views must be re-fetched after a command
/// (contracts/ipc-commands.md `scene_stale`).
#[must_use]
pub fn stale_views(command: &EditCommand) -> Vec<ViewKind> {
    match command {
        // Structure changes ripple into every projection.
        EditCommand::AddTask { .. }
        | EditCommand::DeleteTask { .. }
        | EditCommand::MoveTask { .. }
        | EditCommand::ReplaceFromOutline { .. }
        | EditCommand::RestoreTasks { .. } => ViewKind::all(),

        // Scheduling feeds dates, which every view surfaces.
        EditCommand::SetSchedule { .. } | EditCommand::SetPlanMeta { .. } => ViewKind::all(),

        // Text-only edits: nothing moves, but every view shows the label.
        EditCommand::RenameTask { .. }
        | EditCommand::SetAssignee { .. }
        | EditCommand::SetNotes { .. }
        | EditCommand::SetDone { .. } => ViewKind::all(),

        EditCommand::AddDependency { .. } | EditCommand::RemoveDependency { .. } => {
            vec![ViewKind::DepGraph, ViewKind::Timeline, ViewKind::Wbs]
        }

        EditCommand::AddMilestone { .. }
        | EditCommand::UpdateMilestone { .. }
        | EditCommand::RemoveMilestone { .. } => {
            vec![ViewKind::Milestones, ViewKind::Timeline]
        }
    }
}

/// Everything that can change a plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum EditCommand {
    AddTask {
        parent: Option<TaskId>,
        index: u32,
        title: String,
        /// Set when undo re-creates a task and must restore its original id.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        id: Option<TaskId>,
    },
    RenameTask {
        id: TaskId,
        title: String,
    },
    DeleteTask {
        id: TaskId,
    },
    /// Inverse of `DeleteTask`: restores a subtree plus its references.
    RestoreTasks {
        tasks: Vec<Task>,
        dependencies: Vec<Dependency>,
        milestone_links: Vec<(MilestoneId, TaskId)>,
        /// Document order of every task before the delete, so undo restores
        /// sibling sequence exactly.
        #[serde(default)]
        sibling_orders: Vec<(TaskId, u32)>,
    },
    MoveTask {
        id: TaskId,
        new_parent: Option<TaskId>,
        index: u32,
    },
    SetSchedule {
        id: TaskId,
        schedule: Schedule,
    },
    SetAssignee {
        id: TaskId,
        assignee: Option<String>,
    },
    SetNotes {
        id: TaskId,
        notes: Option<String>,
    },
    SetDone {
        id: TaskId,
        done: bool,
    },
    AddDependency {
        predecessor: TaskId,
        successor: TaskId,
    },
    RemoveDependency {
        predecessor: TaskId,
        successor: TaskId,
    },
    AddMilestone {
        name: String,
        date: String,
        linked_tasks: Vec<TaskId>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        id: Option<MilestoneId>,
    },
    UpdateMilestone {
        id: MilestoneId,
        name: String,
        date: String,
        linked_tasks: Vec<TaskId>,
    },
    RemoveMilestone {
        id: MilestoneId,
    },
    SetPlanMeta {
        title: String,
        description: Option<String>,
        project_start: Option<String>,
    },
    /// Whole-document replacement; one undo boundary (spec Edge Case).
    ReplaceFromOutline {
        text: String,
    },
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum EditError {
    #[error("找不到目标元素：{0}")]
    BadTarget(String),
    #[error("日期格式非法：{0}")]
    BadDate(String),
    #[error("不能把任务移动到它自己的子树下")]
    CyclicMove,
}

/// Applies `command`, returning the inverse that undoes it exactly.
pub fn apply(
    plan: &mut Plan,
    next_task_id: &mut impl FnMut() -> TaskId,
    next_milestone_id: &mut impl FnMut() -> MilestoneId,
    command: &EditCommand,
) -> Result<EditCommand, EditError> {
    match command {
        EditCommand::AddTask {
            parent,
            index,
            title,
            id,
        } => {
            if let Some(parent_id) = parent {
                require_task(plan, *parent_id)?;
            }
            let new_id = id.unwrap_or_else(next_task_id);
            let mut task = Task::new(new_id, title.clone());
            task.parent = *parent;
            insert_at(plan, task, *parent, *index);
            Ok(EditCommand::DeleteTask { id: new_id })
        }

        EditCommand::RenameTask { id, title } => {
            let task = plan.task_mut(*id).ok_or_else(|| bad_target(*id))?;
            let previous = std::mem::replace(&mut task.title, title.clone());
            Ok(EditCommand::RenameTask {
                id: *id,
                title: previous,
            })
        }

        EditCommand::DeleteTask { id } => {
            require_task(plan, *id)?;
            // Sibling ordering is renumbered elsewhere, so snapshot every
            // task's order to restore document order exactly on undo.
            let sibling_orders_snapshot: Vec<(TaskId, u32)> = plan
                .tasks
                .iter()
                .map(|task| (task.id, task.order))
                .collect();
            // Collect the whole subtree so the inverse can restore it verbatim.
            let doomed = subtree_ids(plan, *id);
            let mut removed_tasks: Vec<Task> = Vec::new();
            for task_id in &doomed {
                if let Some(position) = plan.tasks.iter().position(|task| task.id == *task_id) {
                    removed_tasks.push(plan.tasks.remove(position));
                }
            }
            let removed_deps: Vec<Dependency> = plan
                .dependencies
                .iter()
                .filter(|dep| doomed.contains(&dep.predecessor) || doomed.contains(&dep.successor))
                .copied()
                .collect();
            plan.dependencies.retain(|dep| {
                !doomed.contains(&dep.predecessor) && !doomed.contains(&dep.successor)
            });

            let mut removed_links: Vec<(MilestoneId, TaskId)> = Vec::new();
            for milestone in &mut plan.milestones {
                milestone.linked_tasks.retain(|task_id| {
                    let keep = !doomed.contains(task_id);
                    if !keep {
                        removed_links.push((milestone.id, *task_id));
                    }
                    keep
                });
            }

            Ok(EditCommand::RestoreTasks {
                tasks: removed_tasks,
                dependencies: removed_deps,
                milestone_links: removed_links,
                sibling_orders: sibling_orders_snapshot,
            })
        }

        EditCommand::RestoreTasks {
            tasks,
            dependencies,
            milestone_links,
            sibling_orders,
        } => {
            let restored: Vec<TaskId> = tasks.iter().map(|task| task.id).collect();
            for task in tasks {
                plan.tasks.push(task.clone());
            }
            for dep in dependencies {
                if !plan.dependencies.contains(dep) {
                    plan.dependencies.push(*dep);
                }
            }
            for (milestone_id, task_id) in milestone_links {
                if let Some(milestone) = plan.milestones.iter_mut().find(|m| m.id == *milestone_id)
                {
                    if !milestone.linked_tasks.contains(task_id) {
                        milestone.linked_tasks.push(*task_id);
                    }
                }
            }
            // Restore the exact sibling ordering captured before the delete.
            for (id, order) in sibling_orders {
                if let Some(task) = plan.task_mut(*id) {
                    task.order = *order;
                }
            }
            // Undoing a restore deletes the roots again; children follow.
            let root = restored
                .first()
                .copied()
                .ok_or_else(|| bad_target(TaskId(0)))?;
            Ok(EditCommand::DeleteTask { id: root })
        }

        EditCommand::MoveTask {
            id,
            new_parent,
            index,
        } => {
            require_task(plan, *id)?;
            if let Some(parent_id) = new_parent {
                require_task(plan, *parent_id)?;
                // Moving a task under its own descendant would orphan the tree.
                if *parent_id == *id || plan.is_ancestor_of(*id, *parent_id) {
                    return Err(EditError::CyclicMove);
                }
            }
            let task = plan.task(*id).ok_or_else(|| bad_target(*id))?;
            let previous_parent = task.parent;
            let previous_index = task.order;

            let position = plan
                .tasks
                .iter()
                .position(|candidate| candidate.id == *id)
                .ok_or_else(|| bad_target(*id))?;
            let mut moved = plan.tasks.remove(position);
            reindex_siblings(plan, previous_parent);
            moved.parent = *new_parent;
            insert_at(plan, moved, *new_parent, *index);

            Ok(EditCommand::MoveTask {
                id: *id,
                new_parent: previous_parent,
                index: previous_index,
            })
        }

        EditCommand::SetSchedule { id, schedule } => {
            let task = plan.task_mut(*id).ok_or_else(|| bad_target(*id))?;
            let previous = std::mem::replace(&mut task.schedule, *schedule);
            Ok(EditCommand::SetSchedule {
                id: *id,
                schedule: previous,
            })
        }

        EditCommand::SetAssignee { id, assignee } => {
            let task = plan.task_mut(*id).ok_or_else(|| bad_target(*id))?;
            let previous = std::mem::replace(&mut task.assignee, assignee.clone());
            Ok(EditCommand::SetAssignee {
                id: *id,
                assignee: previous,
            })
        }

        EditCommand::SetNotes { id, notes } => {
            let task = plan.task_mut(*id).ok_or_else(|| bad_target(*id))?;
            let previous = std::mem::replace(&mut task.notes, notes.clone());
            Ok(EditCommand::SetNotes {
                id: *id,
                notes: previous,
            })
        }

        EditCommand::SetDone { id, done } => {
            let task = plan.task_mut(*id).ok_or_else(|| bad_target(*id))?;
            let previous = std::mem::replace(&mut task.done, *done);
            Ok(EditCommand::SetDone {
                id: *id,
                done: previous,
            })
        }

        EditCommand::AddDependency {
            predecessor,
            successor,
        } => {
            require_task(plan, *predecessor)?;
            require_task(plan, *successor)?;
            let dep = Dependency::new(*predecessor, *successor);
            if !plan.dependencies.contains(&dep) {
                plan.dependencies.push(dep);
            }
            Ok(EditCommand::RemoveDependency {
                predecessor: *predecessor,
                successor: *successor,
            })
        }

        EditCommand::RemoveDependency {
            predecessor,
            successor,
        } => {
            let dep = Dependency::new(*predecessor, *successor);
            plan.dependencies.retain(|candidate| *candidate != dep);
            Ok(EditCommand::AddDependency {
                predecessor: *predecessor,
                successor: *successor,
            })
        }

        EditCommand::AddMilestone {
            name,
            date,
            linked_tasks,
            id,
        } => {
            let parsed = parse_date(date).ok_or_else(|| EditError::BadDate(date.clone()))?;
            let new_id = id.unwrap_or_else(next_milestone_id);
            plan.milestones.push(Milestone {
                id: new_id,
                name: name.clone(),
                date: parsed,
                linked_tasks: linked_tasks.clone(),
            });
            Ok(EditCommand::RemoveMilestone { id: new_id })
        }

        EditCommand::UpdateMilestone {
            id,
            name,
            date,
            linked_tasks,
        } => {
            let parsed = parse_date(date).ok_or_else(|| EditError::BadDate(date.clone()))?;
            let milestone = plan
                .milestones
                .iter_mut()
                .find(|m| m.id == *id)
                .ok_or_else(|| EditError::BadTarget(id.as_token()))?;
            let inverse = EditCommand::UpdateMilestone {
                id: *id,
                name: milestone.name.clone(),
                date: crate::model::format_date(milestone.date),
                linked_tasks: milestone.linked_tasks.clone(),
            };
            milestone.name = name.clone();
            milestone.date = parsed;
            milestone.linked_tasks = linked_tasks.clone();
            Ok(inverse)
        }

        EditCommand::RemoveMilestone { id } => {
            let position = plan
                .milestones
                .iter()
                .position(|m| m.id == *id)
                .ok_or_else(|| EditError::BadTarget(id.as_token()))?;
            let removed = plan.milestones.remove(position);
            Ok(EditCommand::AddMilestone {
                name: removed.name,
                date: crate::model::format_date(removed.date),
                linked_tasks: removed.linked_tasks,
                id: Some(removed.id),
            })
        }

        EditCommand::SetPlanMeta {
            title,
            description,
            project_start,
        } => {
            let parsed_start = match project_start {
                Some(text) => {
                    Some(parse_date(text).ok_or_else(|| EditError::BadDate(text.clone()))?)
                }
                None => None,
            };
            let inverse = EditCommand::SetPlanMeta {
                title: plan.title.clone(),
                description: plan.description.clone(),
                project_start: plan.project_start.map(crate::model::format_date),
            };
            plan.title = title.clone();
            plan.description = description.clone();
            plan.project_start = parsed_start;
            Ok(inverse)
        }

        EditCommand::ReplaceFromOutline { text } => {
            // The caller captures the previous text; parsing happens in Session.
            let previous = crate::outline::serialize(plan);
            let parsed = crate::outline::parse(text);
            *plan = parsed.plan;
            Ok(EditCommand::ReplaceFromOutline { text: previous })
        }
    }
}

fn bad_target(id: TaskId) -> EditError {
    EditError::BadTarget(id.as_token())
}

fn require_task(plan: &Plan, id: TaskId) -> Result<(), EditError> {
    if plan.has_task(id) {
        Ok(())
    } else {
        Err(bad_target(id))
    }
}

/// Every id in the subtree rooted at `root`, parents before children.
fn subtree_ids(plan: &Plan, root: TaskId) -> Vec<TaskId> {
    let mut ids = vec![root];
    let mut cursor = 0usize;
    while cursor < ids.len() {
        let current = ids[cursor];
        cursor += 1;
        for task in &plan.tasks {
            if task.parent == Some(current) && !ids.contains(&task.id) {
                ids.push(task.id);
            }
        }
    }
    ids
}

/// Inserts `task` at `index` among its siblings and renumbers that sibling set.
fn insert_at(plan: &mut Plan, mut task: Task, parent: Option<TaskId>, index: u32) {
    let mut siblings: Vec<TaskId> = plan
        .tasks
        .iter()
        .filter(|candidate| candidate.parent == parent)
        .map(|candidate| candidate.id)
        .collect();
    siblings.sort_by_key(|id| {
        plan.task(*id)
            .map(|task| (task.order, task.id.0))
            .unwrap_or((u32::MAX, 0))
    });

    let position = (index as usize).min(siblings.len());
    task.order = position as u32;
    plan.tasks.push(task);

    // Shift the siblings at or after the insertion point.
    for (offset, sibling_id) in siblings.iter().enumerate() {
        let new_order = if offset >= position {
            offset + 1
        } else {
            offset
        };
        if let Some(sibling) = plan.task_mut(*sibling_id) {
            sibling.order = new_order as u32;
        }
    }
}

/// Renumbers a sibling set to a dense 0..n sequence.
fn reindex_siblings(plan: &mut Plan, parent: Option<TaskId>) {
    let mut siblings: Vec<TaskId> = plan
        .tasks
        .iter()
        .filter(|candidate| candidate.parent == parent)
        .map(|candidate| candidate.id)
        .collect();
    siblings.sort_by_key(|id| {
        plan.task(*id)
            .map(|task| (task.order, task.id.0))
            .unwrap_or((u32::MAX, 0))
    });
    for (offset, sibling_id) in siblings.iter().enumerate() {
        if let Some(sibling) = plan.task_mut(*sibling_id) {
            sibling.order = offset as u32;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::outline::{parse, serialize};

    struct Ids {
        task: u32,
        milestone: u32,
    }

    fn harness(source: &str) -> (Plan, Ids) {
        let plan = parse(source).plan;
        let next_task = plan.tasks.iter().map(|t| t.id.0).max().unwrap_or(0) + 1;
        let next_milestone = plan.milestones.iter().map(|m| m.id.0).max().unwrap_or(0) + 1;
        (
            plan,
            Ids {
                task: next_task,
                milestone: next_milestone,
            },
        )
    }

    /// Applies a command and returns its inverse.
    fn run(plan: &mut Plan, ids: &mut Ids, command: &EditCommand) -> EditCommand {
        let mut next_task = || {
            let id = TaskId(ids.task);
            ids.task += 1;
            id
        };
        let mut next_milestone = || {
            let id = MilestoneId(ids.milestone);
            ids.milestone += 1;
            id
        };
        apply(plan, &mut next_task, &mut next_milestone, command).expect("command applies")
    }

    /// Asserts apply∘undo restores the exact starting document.
    fn assert_round_trip(source: &str, command: EditCommand) {
        let (mut plan, mut ids) = harness(source);
        let before = serialize(&plan);
        let inverse = run(&mut plan, &mut ids, &command);
        assert_ne!(
            serialize(&plan),
            before,
            "command had no effect: {command:?}"
        );
        run(&mut plan, &mut ids, &inverse);
        assert_eq!(
            serialize(&plan),
            before,
            "undo did not restore state: {command:?}"
        );
    }

    const SAMPLE: &str = "%mcm 1\n%title 演示\n\n- 甲 #t1\n  - 甲一 #t2\n  - 甲二 #t3\n- 乙 #t4 <-t2\n! 冻结 #m1 [2026-09-10] <-t4\n";

    #[test]
    fn add_task_inserts_and_undoes() {
        assert_round_trip(
            SAMPLE,
            EditCommand::AddTask {
                parent: Some(TaskId(1)),
                index: 1,
                title: "新任务".into(),
                id: None,
            },
        );
    }

    #[test]
    fn add_task_respects_the_requested_index() {
        let (mut plan, mut ids) = harness(SAMPLE);
        run(
            &mut plan,
            &mut ids,
            &EditCommand::AddTask {
                parent: Some(TaskId(1)),
                index: 0,
                title: "插到最前".into(),
                id: None,
            },
        );
        let children = plan.children_of(Some(TaskId(1)));
        assert_eq!(children[0].title, "插到最前");
        assert_eq!(children[1].title, "甲一");
    }

    #[test]
    fn rename_task_round_trips() {
        assert_round_trip(
            SAMPLE,
            EditCommand::RenameTask {
                id: TaskId(1),
                title: "改名".into(),
            },
        );
    }

    #[test]
    fn delete_task_removes_subtree_and_restores_everything() {
        let (mut plan, mut ids) = harness(SAMPLE);
        let before = serialize(&plan);
        let inverse = run(
            &mut plan,
            &mut ids,
            &EditCommand::DeleteTask { id: TaskId(1) },
        );
        // Subtree gone, and the dependency that referenced it too.
        assert!(plan.task(TaskId(1)).is_none());
        assert!(plan.task(TaskId(2)).is_none());
        assert!(plan.dependencies.is_empty());
        run(&mut plan, &mut ids, &inverse);
        assert_eq!(serialize(&plan), before);
    }

    #[test]
    fn delete_task_restores_milestone_links() {
        let (mut plan, mut ids) = harness(SAMPLE);
        let inverse = run(
            &mut plan,
            &mut ids,
            &EditCommand::DeleteTask { id: TaskId(4) },
        );
        assert!(plan.milestones[0].linked_tasks.is_empty());
        run(&mut plan, &mut ids, &inverse);
        assert_eq!(plan.milestones[0].linked_tasks, vec![TaskId(4)]);
    }

    #[test]
    fn move_task_round_trips() {
        assert_round_trip(
            SAMPLE,
            EditCommand::MoveTask {
                id: TaskId(3),
                new_parent: None,
                index: 0,
            },
        );
    }

    #[test]
    fn move_task_into_own_descendant_is_rejected() {
        let (mut plan, mut ids) = harness(SAMPLE);
        let mut next_task = || {
            let id = TaskId(ids.task);
            ids.task += 1;
            id
        };
        let mut next_milestone = || {
            let id = MilestoneId(ids.milestone);
            ids.milestone += 1;
            id
        };
        let result = apply(
            &mut plan,
            &mut next_task,
            &mut next_milestone,
            &EditCommand::MoveTask {
                id: TaskId(1),
                new_parent: Some(TaskId(2)),
                index: 0,
            },
        );
        assert_eq!(result, Err(EditError::CyclicMove));
    }

    #[test]
    fn schedule_assignee_notes_done_round_trip() {
        assert_round_trip(
            SAMPLE,
            EditCommand::SetSchedule {
                id: TaskId(2),
                schedule: Schedule::Duration { days: 3 },
            },
        );
        assert_round_trip(
            SAMPLE,
            EditCommand::SetAssignee {
                id: TaskId(2),
                assignee: Some("王芳".into()),
            },
        );
        assert_round_trip(
            SAMPLE,
            EditCommand::SetNotes {
                id: TaskId(2),
                notes: Some("备注".into()),
            },
        );
        assert_round_trip(
            SAMPLE,
            EditCommand::SetDone {
                id: TaskId(2),
                done: true,
            },
        );
    }

    #[test]
    fn dependency_commands_round_trip() {
        assert_round_trip(
            SAMPLE,
            EditCommand::AddDependency {
                predecessor: TaskId(3),
                successor: TaskId(4),
            },
        );
        assert_round_trip(
            SAMPLE,
            EditCommand::RemoveDependency {
                predecessor: TaskId(2),
                successor: TaskId(4),
            },
        );
    }

    #[test]
    fn adding_a_duplicate_dependency_is_idempotent() {
        let (mut plan, mut ids) = harness(SAMPLE);
        let before = plan.dependencies.len();
        run(
            &mut plan,
            &mut ids,
            &EditCommand::AddDependency {
                predecessor: TaskId(2),
                successor: TaskId(4),
            },
        );
        assert_eq!(plan.dependencies.len(), before);
    }

    #[test]
    fn milestone_commands_round_trip() {
        assert_round_trip(
            SAMPLE,
            EditCommand::AddMilestone {
                name: "新里程碑".into(),
                date: "2026-10-01".into(),
                linked_tasks: vec![TaskId(4)],
                id: None,
            },
        );
        assert_round_trip(SAMPLE, EditCommand::RemoveMilestone { id: MilestoneId(1) });
        assert_round_trip(
            SAMPLE,
            EditCommand::UpdateMilestone {
                id: MilestoneId(1),
                name: "改名".into(),
                date: "2026-11-11".into(),
                linked_tasks: vec![],
            },
        );
    }

    #[test]
    fn plan_meta_round_trips() {
        assert_round_trip(
            SAMPLE,
            EditCommand::SetPlanMeta {
                title: "新标题".into(),
                description: Some("说明".into()),
                project_start: Some("2026-09-01".into()),
            },
        );
    }

    #[test]
    fn replace_from_outline_round_trips() {
        assert_round_trip(
            SAMPLE,
            EditCommand::ReplaceFromOutline {
                text: "%mcm 1\n%title 全新\n\n- 唯一 #t1\n".into(),
            },
        );
    }

    #[test]
    fn bad_targets_are_rejected() {
        let (mut plan, mut ids) = harness(SAMPLE);
        let mut next_task = || {
            let id = TaskId(ids.task);
            ids.task += 1;
            id
        };
        let mut next_milestone = || {
            let id = MilestoneId(ids.milestone);
            ids.milestone += 1;
            id
        };
        let result = apply(
            &mut plan,
            &mut next_task,
            &mut next_milestone,
            &EditCommand::RenameTask {
                id: TaskId(99),
                title: "x".into(),
            },
        );
        assert!(matches!(result, Err(EditError::BadTarget(_))));
    }

    #[test]
    fn invalid_dates_are_rejected() {
        let (mut plan, mut ids) = harness(SAMPLE);
        let mut next_task = || {
            let id = TaskId(ids.task);
            ids.task += 1;
            id
        };
        let mut next_milestone = || {
            let id = MilestoneId(ids.milestone);
            ids.milestone += 1;
            id
        };
        let result = apply(
            &mut plan,
            &mut next_task,
            &mut next_milestone,
            &EditCommand::AddMilestone {
                name: "坏日期".into(),
                date: "2026-9-1".into(),
                linked_tasks: vec![],
                id: None,
            },
        );
        assert!(matches!(result, Err(EditError::BadDate(_))));
    }

    #[test]
    fn stale_views_cover_the_affected_projections() {
        assert_eq!(
            stale_views(&EditCommand::DeleteTask { id: TaskId(1) }).len(),
            4
        );
        let dependency_views = stale_views(&EditCommand::AddDependency {
            predecessor: TaskId(1),
            successor: TaskId(4),
        });
        assert!(dependency_views.contains(&ViewKind::DepGraph));
        let milestone_views = stale_views(&EditCommand::RemoveMilestone { id: MilestoneId(1) });
        assert!(milestone_views.contains(&ViewKind::Milestones));
        assert!(!milestone_views.contains(&ViewKind::DepGraph));
    }
}
