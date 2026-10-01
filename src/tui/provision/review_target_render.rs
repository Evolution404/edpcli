use super::*;
use crate::tui::state::ProvisionConfirmationViewModel;

pub(super) fn draw_target_line(
    frame: &mut Frame,
    area: ratatui::layout::Rect,
    view: &ProvisionConfirmationViewModel,
) {
    let vid_pid = match (view.target.vid, view.target.pid) {
        (Some(vid), Some(pid)) => format!("{vid:04X}:{pid:04X}"),
        _ => "VID:PID —".into(),
    };
    let onlyid = view
        .target
        .onlyid
        .as_deref()
        .map(|value| format!("onlyid {value}"))
        .unwrap_or_else(|| safe(&view.target.device_id));
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(format!("disk{}", view.target.disk), accent()),
            Span::raw(format!(
                " · {} · {} · {} · {}",
                AppState::format_sector_size(view.target.total_sectors),
                vid_pid,
                onlyid,
                view.target.target.full_name()
            )),
        ])),
        area,
    );
}
