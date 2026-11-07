use super::*;

impl App {
    pub(super) fn align(&mut self, mode: usize) {
        let before = self.project.clone();
        arrange_widgets(
            &mut self.project.pages[self.page],
            &self.selected,
            self.native.as_ref(),
            mode,
        );
        if before != self.project {
            self.status = [
                "已左对齐",
                "已水平居中",
                "已右对齐",
                "已顶对齐",
                "已垂直居中",
                "已底对齐",
                "已水平分布",
                "已垂直分布",
                "已置于顶层",
                "已上移一层",
                "已下移一层",
                "已置于底层",
                "已逆时针旋转 90°",
                "已顺时针旋转 90°",
            ][mode]
                .into();
            self.remember(before);
        }
    }
}

pub(super) fn arrange_widgets(
    page: &mut Page,
    selected: &BTreeSet<u64>,
    native: Option<&ingenaro::rp_format::NativeDocument>,
    mode: usize,
) {
    let editable_ids: BTreeSet<u64> = page
        .widgets
        .iter()
        .filter(|widget| {
            selected.contains(&widget.id)
                && !widget.locked
                && native.is_none_or(|document| document.is_editable_widget(widget.id))
        })
        .map(|widget| widget.id)
        .collect();
    if editable_ids.is_empty() || mode > 13 {
        return;
    }
    match mode {
        0..=7 => {
            let widgets = &mut page.widgets;
            use std::collections::BTreeMap;
            let owner = |id| native.map_or(id, |native| native.selection_owner(id));
            // Read-only master contents belong to their selected instance;
            // their synthetic locks must not freeze its enclosing group.
            let mut covered = selected.clone();
            for widget in widgets.iter() {
                if owner(widget.id) != widget.id && selected.contains(&owner(widget.id)) {
                    covered.insert(widget.id);
                }
            }
            let mut group_counts: BTreeMap<u64, (usize, usize)> = BTreeMap::new();
            for widget in widgets.iter() {
                for group in &widget.groups {
                    let counts = group_counts.entry(*group).or_default();
                    counts.0 += 1;
                    counts.1 += usize::from(covered.contains(&widget.id));
                }
            }
            let complete_groups = group_counts
                .into_iter()
                .filter_map(|(id, (total, chosen))| (total == chosen).then_some(id))
                .collect::<BTreeSet<_>>();
            let mut unit_order = Vec::new();
            let mut unit_members: BTreeMap<u64, Vec<usize>> = BTreeMap::new();
            let mut widget_units = BTreeMap::new();
            for (index, widget) in widgets.iter().enumerate() {
                if !selected.contains(&widget.id) || owner(widget.id) != widget.id {
                    continue;
                }
                let unit = widget
                    .groups
                    .iter()
                    .find(|group| complete_groups.contains(group))
                    .copied()
                    .unwrap_or(widget.id);
                if !unit_members.contains_key(&unit) {
                    unit_order.push(unit);
                }
                unit_members.entry(unit).or_default().push(index);
                widget_units.insert(widget.id, unit);
            }
            let mut unit_bounds: BTreeMap<u64, [f32; 4]> = BTreeMap::new();
            for widget in widgets.iter() {
                let Some(unit) = widget_units.get(&owner(widget.id)) else {
                    continue;
                };
                if !complete_groups.contains(unit) {
                    continue;
                }
                let next = ingenaro::manipulation::rotated_bounds(widget);
                let bounds = unit_bounds.entry(*unit).or_insert(next);
                bounds[0] = bounds[0].min(next[0]);
                bounds[1] = bounds[1].min(next[1]);
                bounds[2] = bounds[2].max(next[2]);
                bounds[3] = bounds[3].max(next[3]);
            }
            let mut proxies = Vec::with_capacity(unit_order.len());
            for unit in &unit_order {
                let members = &unit_members[unit];
                let mut proxy = widgets[members[0]].clone();
                proxy.id = *unit;
                proxy.locked = members
                    .iter()
                    .any(|index| !editable_ids.contains(&widgets[*index].id));
                if let Some(bounds) = unit_bounds.get(unit) {
                    proxy.x = bounds[0];
                    proxy.y = bounds[1];
                    proxy.width = bounds[2] - bounds[0];
                    proxy.height = bounds[3] - bounds[1];
                    proxy.rotation = 0.0;
                }
                proxies.push(proxy);
            }
            let initial_positions = proxies
                .iter()
                .map(|proxy| (proxy.id, [proxy.x, proxy.y]))
                .collect::<BTreeMap<_, _>>();
            let proxy_ids = unit_order.into_iter().collect::<BTreeSet<_>>();
            if mode < 6 {
                let modes = [
                    ingenaro::manipulation::Align::Left,
                    ingenaro::manipulation::Align::Center,
                    ingenaro::manipulation::Align::Right,
                    ingenaro::manipulation::Align::Top,
                    ingenaro::manipulation::Align::Middle,
                    ingenaro::manipulation::Align::Bottom,
                ];
                ingenaro::manipulation::align(&mut proxies, &proxy_ids, modes[mode]);
            } else {
                ingenaro::manipulation::distribute(&mut proxies, &proxy_ids, mode == 6);
            }
            for proxy in proxies {
                let initial = initial_positions[&proxy.id];
                let delta = [proxy.x - initial[0], proxy.y - initial[1]];
                for index in &unit_members[&proxy.id] {
                    let widget = &mut widgets[*index];
                    if editable_ids.contains(&widget.id) {
                        widget.x += delta[0];
                        widget.y += delta[1];
                    }
                }
            }
        }
        8..=11 => {
            let mut staged = page.clone();
            let widgets = &mut page.widgets;
            for widget in &mut staged.widgets {
                if !selected.contains(&widget.id) {
                    continue;
                }
                if let Some(native) = native {
                    let owner = native.selection_owner(widget.id);
                    if owner != widget.id
                        && selected.contains(&owner)
                        && editable_ids.contains(&owner)
                    {
                        widget.locked = false;
                    } else if !native.is_editable_widget(widget.id) {
                        widget.locked = true;
                    }
                }
            }
            let movement = match mode {
                8 => ingenaro::selection::LayerMove::ToFront,
                9 => ingenaro::selection::LayerMove::Forward,
                10 => ingenaro::selection::LayerMove::Backward,
                _ => ingenaro::selection::LayerMove::ToBack,
            };
            if ingenaro::grouping::reorder(&mut staged, selected, movement) {
                let locks = widgets
                    .iter()
                    .map(|widget| (widget.id, widget.locked))
                    .collect::<std::collections::BTreeMap<_, _>>();
                for widget in &mut staged.widgets {
                    widget.locked = locks[&widget.id];
                }
                *widgets = staged.widgets;
            }
        }
        12 | 13 => {
            let widgets = &mut page.widgets;
            let bounds = widgets
                .iter()
                .filter(|widget| selected.contains(&widget.id))
                .fold(
                    [
                        f32::INFINITY,
                        f32::INFINITY,
                        f32::NEG_INFINITY,
                        f32::NEG_INFINITY,
                    ],
                    |mut bounds, widget| {
                        let next = ingenaro::manipulation::rotated_bounds(widget);
                        bounds[0] = bounds[0].min(next[0]);
                        bounds[1] = bounds[1].min(next[1]);
                        bounds[2] = bounds[2].max(next[2]);
                        bounds[3] = bounds[3].max(next[3]);
                        bounds
                    },
                );
            let pivot = [(bounds[0] + bounds[2]) / 2.0, (bounds[1] + bounds[3]) / 2.0];
            for widget in widgets {
                if editable_ids.contains(&widget.id) {
                    *widget = ingenaro::manipulation::rotate(
                        widget,
                        if mode == 12 { -90.0 } else { 90.0 },
                        pivot,
                    );
                }
            }
        }
        _ => {}
    }
}
