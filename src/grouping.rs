//! Nested groups retain document coordinates and contiguous layer blocks.

use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Context, Result, ensure};

use crate::{
    model::{MAX_GROUP_DEPTH, Page, Project, Widget},
    selection::LayerMove,
};

/// A sibling in the layer panel. Group members retain back-to-front canvas order.
pub enum Layer<'a> {
    Group { id: u64, members: &'a [Widget] },
    Widget(&'a Widget),
}

/// Walk one level of a validated page's layer tree in front-to-back order.
/// Start with the whole page at depth zero, then pass each group's members at depth + 1.
pub fn layers(mut widgets: &[Widget], depth: usize) -> impl Iterator<Item = Layer<'_>> {
    std::iter::from_fn(move || {
        let widget = widgets.last()?;
        if let Some(&id) = widget.groups.get(depth) {
            let start = widgets
                .iter()
                .rposition(|widget| widget.groups.get(depth) != Some(&id))
                .map_or(0, |index| index + 1);
            let (rest, members) = widgets.split_at(start);
            widgets = rest;
            Some(Layer::Group { id, members })
        } else {
            widgets = &widgets[..widgets.len() - 1];
            Some(Layer::Widget(widget))
        }
    })
}

pub(crate) fn validate_group_name(name: &str) -> Result<()> {
    ensure!(
        !name.trim().is_empty() && name.len() <= 1024,
        "分组名称不能为空且不能超过 1024 字节"
    );
    Ok(())
}

pub fn rename_group(page: &mut Page, id: u64, name: &str) -> Result<()> {
    validate_group_name(name)?;
    ensure!(
        page.widgets.iter().any(|w| w.groups.contains(&id)),
        "当前页面不存在分组 {id}"
    );
    page.group_names.insert(id, name.to_owned());
    Ok(())
}

/// Normal selection expands the outermost group. An explicit inside-selection
/// keeps the requested member IDs, permitting operations within a group.
pub fn expanded_selection(page: &Page, ids: &BTreeSet<u64>, inside: bool) -> BTreeSet<u64> {
    let groups = page
        .widgets
        .iter()
        .filter(|widget| ids.contains(&widget.id))
        .filter_map(|widget| widget.groups.first().copied())
        .collect::<BTreeSet<_>>();
    page.widgets
        .iter()
        .filter(|widget| {
            ids.contains(&widget.id)
                || (!inside
                    && widget
                        .groups
                        .first()
                        .is_some_and(|group| groups.contains(group)))
        })
        .map(|widget| widget.id)
        .collect()
}

/// Wrap the complete selected groups in one new outer group. Members keep
/// their own order and geometry; the block ends at the old topmost member.
/// The returned IDs are widget IDs suitable for the editor's selection.
pub fn group(project: &mut Project, page: usize, ids: &BTreeSet<u64>) -> Result<BTreeSet<u64>> {
    group_named(project, page, ids, None).map(|(_, selected)| selected)
}

/// Create a group with an optional custom name in the same transaction.
pub fn group_named(
    project: &mut Project,
    page: usize,
    ids: &BTreeSet<u64>,
    name: Option<&str>,
) -> Result<(u64, BTreeSet<u64>)> {
    project.validate()?;
    if let Some(name) = name {
        validate_group_name(name)?;
    }
    let source = project.pages.get(page).context("组合所在页面不存在")?;
    ensure!(
        ids.iter()
            .all(|id| source.widgets.iter().any(|widget| widget.id == *id)),
        "组合选择中包含不属于当前页面的元件"
    );
    let selected = expanded_selection(source, ids, false);
    ensure!(selected.len() >= 2, "请至少选择两个元件进行组合");
    for widget in source
        .widgets
        .iter()
        .filter(|widget| selected.contains(&widget.id))
    {
        ensure!(!widget.locked, "组合中包含锁定元件，请先解锁");
        ensure!(
            widget.groups.len() < MAX_GROUP_DEPTH,
            "元件组合不能超过 32 层"
        );
    }
    let next_id = project
        .next_id
        .checked_add(1)
        .filter(|next| *next < u64::MAX)
        .context("项目 ID 已耗尽")?;
    let mut edited = project.clone();
    let id = edited.next_id;
    edited.next_id = next_id;
    let widgets = &mut edited.pages[page].widgets;
    let top = widgets
        .iter()
        .rposition(|widget| selected.contains(&widget.id))
        .unwrap();
    let insertion = top + 1 - selected.len();
    let mut members = Vec::with_capacity(selected.len());
    let mut others = Vec::with_capacity(widgets.len() - selected.len());
    for mut widget in std::mem::take(widgets) {
        if selected.contains(&widget.id) {
            widget.groups.insert(0, id);
            members.push(widget);
        } else {
            others.push(widget);
        }
    }
    let above = others.split_off(insertion);
    others.extend(members);
    others.extend(above);
    *widgets = others;
    if let Some(name) = name {
        edited.pages[page].group_names.insert(id, name.to_owned());
    }
    edited.validate()?;
    *project = edited;
    Ok((id, selected))
}

/// Remove one outer group from every selected group, preserving all deeper
/// groups. Partial selections expand before the change, so no group is split.
pub fn ungroup(project: &mut Project, page: usize, ids: &BTreeSet<u64>) -> Result<BTreeSet<u64>> {
    project.validate()?;
    let source = project.pages.get(page).context("解组所在页面不存在")?;
    ensure!(
        ids.iter()
            .all(|id| source.widgets.iter().any(|widget| widget.id == *id)),
        "解组选择中包含不属于当前页面的元件"
    );
    let selected = expanded_selection(source, ids, false);
    let groups = source
        .widgets
        .iter()
        .filter(|widget| selected.contains(&widget.id))
        .filter_map(|widget| widget.groups.first().copied())
        .collect::<BTreeSet<_>>();
    ensure!(!groups.is_empty(), "当前选择中没有可以解开的组合");
    ensure!(
        source
            .widgets
            .iter()
            .filter(|widget| widget
                .groups
                .first()
                .is_some_and(|group| groups.contains(group)))
            .all(|widget| !widget.locked),
        "组合中包含锁定元件，请先解锁"
    );
    let mut edited = project.clone();
    for widget in &mut edited.pages[page].widgets {
        if widget
            .groups
            .first()
            .is_some_and(|group| groups.contains(group))
        {
            widget.groups.remove(0);
        }
    }
    edited.pages[page]
        .group_names
        .retain(|id, _| !groups.contains(id));
    edited.validate()?;
    *project = edited;
    Ok(selected)
}

/// Freshen group IDs after assigning new widget IDs during paste. The caller
/// supplies the destination project's allocator. Failure changes neither data
/// nor the allocator; ancestry and member order remain identical.
pub fn remap_groups(widgets: &mut [Widget], next_id: &mut u64) -> Result<()> {
    remap_group_ids(widgets, next_id).map(|_| ())
}

/// Return the ID mapping so clipboard metadata can follow the copied groups.
pub fn remap_group_ids(widgets: &mut [Widget], next_id: &mut u64) -> Result<BTreeMap<u64, u64>> {
    let mut mapping = BTreeMap::new();
    let mut next = *next_id;
    let widget_ids = widgets
        .iter()
        .map(|widget| widget.id)
        .collect::<BTreeSet<_>>();
    for widget in widgets.iter() {
        ensure!(
            widget.groups.len() <= MAX_GROUP_DEPTH,
            "元件组合不能超过 32 层"
        );
        let mut path = BTreeSet::new();
        for id in &widget.groups {
            ensure!(
                *id > 0 && path.insert(*id),
                "组合 ID 不能为零或在同一路径中重复"
            );
            if let std::collections::btree_map::Entry::Vacant(entry) = mapping.entry(*id) {
                ensure!(
                    next > 0 && !widget_ids.contains(&next),
                    "组合 ID 分配与粘贴元件冲突"
                );
                let after = next
                    .checked_add(1)
                    .filter(|after| *after < u64::MAX)
                    .context("项目 ID 已耗尽")?;
                entry.insert(next);
                next = after;
            }
        }
    }
    for widget in widgets {
        for id in &mut widget.groups {
            *id = mapping[&*id];
        }
    }
    *next_id = next;
    Ok(mapping)
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Unit {
    Group(u64),
    Widget(u64),
}

/// Move complete sibling blocks. Fully selected groups move at their parent's
/// scope; partial selections may move only among siblings within the same
/// group. Cross-scope selections and locked selections leave the page intact.
pub fn reorder(page: &mut Page, ids: &BTreeSet<u64>, movement: LayerMove) -> bool {
    let selected = page
        .widgets
        .iter()
        .filter(|widget| ids.contains(&widget.id))
        .collect::<Vec<_>>();
    if selected.is_empty() || selected.iter().any(|widget| widget.locked) {
        return false;
    }
    let mut members: BTreeMap<u64, BTreeSet<u64>> = BTreeMap::new();
    for widget in &page.widgets {
        for id in &widget.groups {
            members.entry(*id).or_default().insert(widget.id);
        }
    }
    let mut scope = None;
    let mut units = BTreeSet::new();
    for widget in selected {
        let complete = widget
            .groups
            .iter()
            .position(|id| members[id].is_subset(ids));
        let (parent, unit) = if let Some(depth) = complete {
            (&widget.groups[..depth], Unit::Group(widget.groups[depth]))
        } else {
            (widget.groups.as_slice(), Unit::Widget(widget.id))
        };
        if scope
            .as_ref()
            .is_some_and(|scope: &Vec<u64>| scope.as_slice() != parent)
        {
            return false;
        }
        scope = Some(parent.to_vec());
        units.insert(unit);
    }
    let scope = scope.unwrap();
    let Some(start) = page
        .widgets
        .iter()
        .position(|widget| widget.groups.starts_with(&scope))
    else {
        return false;
    };
    let end = page
        .widgets
        .iter()
        .rposition(|widget| widget.groups.starts_with(&scope))
        .unwrap()
        + 1;
    if page.widgets[start..end]
        .iter()
        .any(|widget| !widget.groups.starts_with(&scope))
    {
        return false;
    }
    let mut blocks = Vec::new();
    let mut position = start;
    let mut seen = BTreeSet::new();
    while position < end {
        let first = position;
        let widget = &page.widgets[position];
        let unit = if let Some(id) = widget.groups.get(scope.len()) {
            let id = *id;
            position += 1;
            while position < end && page.widgets[position].groups.get(scope.len()) == Some(&id) {
                position += 1;
            }
            Unit::Group(id)
        } else {
            position += 1;
            Unit::Widget(widget.id)
        };
        if !seen.insert(unit) {
            return false;
        }
        blocks.push((first, position, units.contains(&unit)));
    }
    if !units.is_subset(&seen) {
        return false;
    }
    let original = blocks.clone();
    match movement {
        LayerMove::ToFront => blocks.sort_by_key(|(_, _, selected)| *selected),
        LayerMove::ToBack => blocks.sort_by_key(|(_, _, selected)| !selected),
        LayerMove::Forward => {
            for index in (0..blocks.len().saturating_sub(1)).rev() {
                if blocks[index].2 && !blocks[index + 1].2 {
                    blocks.swap(index, index + 1);
                }
            }
        }
        LayerMove::Backward => {
            for index in 1..blocks.len() {
                if blocks[index].2 && !blocks[index - 1].2 {
                    blocks.swap(index, index - 1);
                }
            }
        }
    }
    if blocks == original {
        return false;
    }
    let reordered = blocks
        .into_iter()
        .flat_map(|(start, end, _)| page.widgets[start..end].iter().cloned())
        .collect::<Vec<_>>();
    page.widgets.splice(start..end, reordered);
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::WidgetKind;

    fn fixture() -> Project {
        let mut project = Project::blank();
        for index in 0..6 {
            let id = project.allocate_id();
            let mut widget = Widget::new(
                id,
                WidgetKind::Rectangle,
                index as f32 * 25.0,
                index as f32 * 10.0,
            );
            widget.rotation = index as f32 * 15.0;
            project.pages[0].widgets.push(widget);
        }
        project
    }

    fn order(page: &Page) -> Vec<u64> {
        page.widgets.iter().map(|widget| widget.id).collect()
    }

    #[test]
    fn grouping_nonadjacent_members_preserves_geometry_and_stable_layer_order() {
        let mut project = fixture();
        let original = project.clone();
        let selected = group(&mut project, 0, &BTreeSet::from([2, 4, 6])).unwrap();
        assert_eq!(selected, BTreeSet::from([2, 4, 6]));
        assert_eq!(order(&project.pages[0]), [3, 5, 2, 4, 6, 7]);
        let id = original.next_id;
        for widget in &project.pages[0].widgets {
            let previous = original.pages[0]
                .widgets
                .iter()
                .find(|previous| previous.id == widget.id)
                .unwrap();
            let mut expected = previous.clone();
            if selected.contains(&widget.id) {
                expected.groups = vec![id];
            }
            assert_eq!(*widget, expected);
        }
        assert_eq!(project.next_id, id + 1);
        project.validate().unwrap();
    }

    #[test]
    fn partial_group_selection_expands_and_ungroup_removes_only_the_outer_level() {
        let mut project = fixture();
        group(&mut project, 0, &BTreeSet::from([2, 3])).unwrap();
        let inner = project.pages[0].widgets[0].groups[0];
        let expanded = group(&mut project, 0, &BTreeSet::from([2, 4])).unwrap();
        assert_eq!(expanded, BTreeSet::from([2, 3, 4]));
        assert_eq!(
            expanded_selection(&project.pages[0], &BTreeSet::from([3]), false),
            expanded
        );
        assert_eq!(
            expanded_selection(&project.pages[0], &BTreeSet::from([3]), true),
            BTreeSet::from([3])
        );
        let ungrouped = ungroup(&mut project, 0, &BTreeSet::from([3])).unwrap();
        assert_eq!(ungrouped, expanded);
        for widget in &project.pages[0].widgets {
            assert_eq!(
                widget.groups,
                if [2, 3].contains(&widget.id) {
                    vec![inner]
                } else {
                    vec![]
                }
            );
        }
        project.validate().unwrap();
    }

    #[test]
    fn invalid_grouping_is_atomic_for_locks_depth_and_allocator_exhaustion() {
        let mut project = fixture();
        project.pages[0].widgets[0].locked = true;
        let original = project.clone();
        assert!(group(&mut project, 0, &BTreeSet::from([2, 3])).is_err());
        assert_eq!(project, original);
        project.pages[0].widgets[0].locked = false;
        project.next_id = u64::MAX - 1;
        let original = project.clone();
        assert!(group(&mut project, 0, &BTreeSet::from([2, 3])).is_err());
        assert_eq!(project, original);
        let mut project = fixture();
        let path = (100..132).collect::<Vec<_>>();
        for widget in &mut project.pages[0].widgets[..2] {
            widget.groups = path.clone();
        }
        project.next_id = 132;
        project.validate().unwrap();
        let original = project.clone();
        assert!(group(&mut project, 0, &BTreeSet::from([2, 3])).is_err());
        assert_eq!(project, original);
    }

    #[test]
    fn pasted_groups_receive_fresh_ids_without_losing_nesting() {
        let mut project = fixture();
        group(&mut project, 0, &BTreeSet::from([2, 3])).unwrap();
        group(&mut project, 0, &BTreeSet::from([2, 4])).unwrap();
        let mut copied = project.pages[0].widgets[..3].to_vec();
        let previous = copied.clone();
        let mut next = 100;
        remap_groups(&mut copied, &mut next).unwrap();
        assert_eq!(next, 102);
        assert_eq!(copied[0].groups[0], copied[2].groups[0]);
        assert_eq!(copied[0].groups, copied[1].groups);
        assert_ne!(copied[0].groups[0], copied[0].groups[1]);
        for (after, before) in copied.iter().zip(previous) {
            assert_eq!(after.x, before.x);
            assert!(after.groups.iter().all(|id| *id >= 100));
        }
        let original = copied.clone();
        let mut exhausted = u64::MAX - 1;
        assert!(remap_groups(&mut copied, &mut exhausted).is_err());
        assert_eq!(copied, original);
        assert_eq!(exhausted, u64::MAX - 1);
    }

    #[test]
    fn sibling_layer_moves_keep_selected_and_unselected_groups_intact() {
        let mut project = fixture();
        group(&mut project, 0, &BTreeSet::from([3, 4])).unwrap();
        group(&mut project, 0, &BTreeSet::from([5, 6])).unwrap();
        assert!(reorder(
            &mut project.pages[0],
            &BTreeSet::from([2]),
            LayerMove::Forward
        ));
        assert_eq!(order(&project.pages[0]), [3, 4, 2, 5, 6, 7]);
        assert!(reorder(
            &mut project.pages[0],
            &BTreeSet::from([3, 4]),
            LayerMove::ToFront
        ));
        assert_eq!(order(&project.pages[0]), [2, 5, 6, 7, 3, 4]);
        project.validate().unwrap();
    }

    #[test]
    fn inside_layer_moves_stay_inside_the_parent_scope_and_cross_scope_is_noop() {
        let mut project = fixture();
        group(&mut project, 0, &BTreeSet::from([2, 3, 4])).unwrap();
        assert!(reorder(
            &mut project.pages[0],
            &BTreeSet::from([2]),
            LayerMove::ToFront
        ));
        assert_eq!(order(&project.pages[0]), [3, 4, 2, 5, 6, 7]);
        let original = project.pages[0].clone();
        assert!(!reorder(
            &mut project.pages[0],
            &BTreeSet::from([2, 5]),
            LayerMove::Forward
        ));
        assert_eq!(project.pages[0], original);
        project.validate().unwrap();
    }
}
