use crate::provision::DiskProvisionKind;
use crate::tui::{
    state::{AppState, InputMode},
    theme,
    ui::{OverviewMetric, OverviewSearch},
};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ProvisionKindCounts {
    pub total: usize,
    pub plain: usize,
    pub mode0: usize,
    pub mode1: usize,
    pub mode2: usize,
    pub mode3: usize,
    pub unknown: usize,
}

impl ProvisionKindCounts {
    pub fn from_kinds(kinds: impl IntoIterator<Item = Option<DiskProvisionKind>>) -> Self {
        let mut counts = Self::default();
        for kind in kinds {
            counts.total += 1;
            match kind {
                Some(DiskProvisionKind::Plain) => counts.plain += 1,
                Some(DiskProvisionKind::Mode0) => counts.mode0 += 1,
                Some(DiskProvisionKind::Mode1) => counts.mode1 += 1,
                Some(DiskProvisionKind::Mode2) => counts.mode2 += 1,
                Some(DiskProvisionKind::Mode3) => counts.mode3 += 1,
                None => counts.unknown += 1,
            }
        }
        counts
    }

    pub fn nonzero_kinds(self) -> Vec<(Option<DiskProvisionKind>, &'static str, usize)> {
        let mut values = Vec::with_capacity(6);
        for (kind, label, count) in [
            (Some(DiskProvisionKind::Plain), "普通盘", self.plain),
            (Some(DiskProvisionKind::Mode0), "mode0", self.mode0),
            (Some(DiskProvisionKind::Mode1), "mode1", self.mode1),
            (Some(DiskProvisionKind::Mode2), "mode2", self.mode2),
            (Some(DiskProvisionKind::Mode3), "mode3", self.mode3),
            (None, "未知", self.unknown),
        ] {
            if count > 0 {
                values.push((kind, label, count));
            }
        }
        values
    }
}

pub fn overview_metrics(counts: ProvisionKindCounts) -> Vec<OverviewMetric> {
    let mut metrics = vec![OverviewMetric::new(
        "总计",
        counts.total,
        theme::current().accent(),
    )];
    metrics.extend(
        counts
            .nonzero_kinds()
            .into_iter()
            .map(|(kind, label, count)| {
                let style = kind
                    .map(|kind| theme::current().provision_kind_emphasis(kind))
                    .unwrap_or_else(|| theme::current().warning());
                OverviewMetric::new(label, count, style)
            }),
    );
    metrics
}

pub fn overview_search(state: &AppState, placeholder: &str) -> OverviewSearch {
    let active = state.input_mode() == InputMode::Search;
    let filtered = state.workspace_filter_active();
    let text = if active {
        format!(
            "/{}▌",
            crate::ui::sanitize_terminal_text(state.input_buffer())
        )
    } else if let Some(status) = state.search_status() {
        status
    } else {
        placeholder.to_string()
    };
    OverviewSearch {
        text,
        active,
        filtered,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_count_kinds_are_omitted_and_order_is_stable() {
        let counts = ProvisionKindCounts::from_kinds([
            Some(DiskProvisionKind::Plain),
            Some(DiskProvisionKind::Mode1),
            Some(DiskProvisionKind::Mode1),
            None,
        ]);
        assert_eq!(counts.total, 4);
        assert_eq!(
            counts
                .nonzero_kinds()
                .into_iter()
                .map(|(_, label, count)| (label, count))
                .collect::<Vec<_>>(),
            vec![("普通盘", 1), ("mode1", 2), ("未知", 1)]
        );
    }
}
