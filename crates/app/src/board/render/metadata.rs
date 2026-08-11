use super::*;
use std::cmp::Ordering;

impl BoardView {
    pub(in crate::board) fn selected_entry(&self) -> Option<(&str, &EntryDTO)> {
        let entry_id = self.entry_dialog.entry_id?;

        self.cards.iter().find_map(|card| {
            card.entries
                .iter()
                .find(|entry| entry.id == entry_id)
                .map(|entry| (card.title.as_ref(), entry))
        })
    }

    pub(super) fn entry_matches_filters(&self, entry: &EntryDTO) -> bool {
        storage::board_projection::entry_matches_view(
            entry,
            &self.active_view_config,
            &self.property_values,
            &self.board_properties.definitions,
            Local::now().date_naive(),
        )
    }

    pub(super) fn compare_entries_for_active_sort(
        &self,
        left: &EntryDTO,
        right: &EntryDTO,
    ) -> Ordering {
        self.active_view_config
            .sort
            .as_ref()
            .map_or(Ordering::Equal, |sort| {
                storage::board_projection::compare_entries_for_view(
                    left,
                    right,
                    sort,
                    &self.property_values,
                    &self.board_properties.definitions,
                )
            })
    }

    pub(super) fn label_marker_color(&self, color: &str, cx: &Context<Self>) -> Hsla {
        match color {
            "green" => cx.theme().green,
            "amber" => cx.theme().yellow,
            "red" => cx.theme().red,
            "purple" => cx.theme().magenta,
            "slate" => cx.theme().muted_foreground,
            _ => cx.theme().blue,
        }
    }

    pub(super) fn render_label_chip(
        &self,
        label: &crate::board::dto::BoardLabelDTO,
        cx: &Context<Self>,
    ) -> impl IntoElement {
        let marker = self.label_marker_color(label.color.as_ref(), cx);
        let colors = accessible_text_colors(cx.theme().secondary, cx.theme().secondary_foreground);

        h_flex()
            .flex_shrink_0()
            .h_5()
            .min_w_0()
            .max_w(px(128.))
            .gap_1()
            .items_center()
            .rounded_full()
            .px_1p5()
            .bg(colors.background)
            .text_color(colors.foreground)
            .text_xs()
            .line_height(relative(1.2))
            .font_weight(FontWeight::MEDIUM)
            .child(div().size(px(6.)).rounded_full().bg(marker))
            .child(div().truncate().child(label.name.clone()))
    }

    pub(super) fn render_card_label_chips(
        &self,
        entry: &EntryDTO,
        cx: &Context<Self>,
    ) -> impl IntoElement {
        h_flex()
            .w_full()
            .h_6()
            .min_w_0()
            .gap_1p5()
            .items_center()
            .flex_wrap()
            .overflow_hidden()
            .children(
                entry
                    .labels
                    .iter()
                    .map(|label| self.render_label_chip(label, cx)),
            )
    }

    pub(super) fn render_card_metadata(
        &self,
        entry: &EntryDTO,
        show_due_date: bool,
        cx: &Context<Self>,
    ) -> impl IntoElement {
        h_flex()
            .w_full()
            .min_w_0()
            .h_5()
            .gap_3()
            .items_center()
            .when_some(
                show_due_date.then_some(entry.due_on.as_ref()).flatten(),
                |this, due_on| this.child(self.render_card_due_date(due_on, cx)),
            )
            .when(!entry.checklist_items.is_empty(), |this| {
                this.child(self.render_card_checklist_progress(entry, cx))
            })
            .when(!entry.attachments.is_empty(), |this| {
                this.child(
                    h_flex()
                        .gap_1()
                        .items_center()
                        .text_xs()
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(cx.theme().primary_foreground.opacity(0.76))
                        .child(Icon::new(IconName::Folder).xsmall())
                        .child(entry.attachments.len().to_string()),
                )
            })
    }

    pub(super) fn render_card_due_date(
        &self,
        due_on: &SharedString,
        cx: &Context<Self>,
    ) -> impl IntoElement {
        let status = due_date_status(due_on.as_ref(), Local::now().date_naive());
        let background = match status {
            DueDateStatus::Overdue => cx.theme().danger,
            DueDateStatus::Today => cx.theme().warning,
            DueDateStatus::Future => cx.theme().secondary,
            DueDateStatus::Invalid => cx.theme().warning,
        };
        let label = NaiveDate::parse_from_str(due_on.as_ref(), "%Y-%m-%d")
            .map(|date| date.format("%b %-d").to_string())
            .unwrap_or_else(|_| due_on.to_string());

        self.render_card_date_pill(label, background, cx)
    }

    pub(super) fn render_card_date_pill(
        &self,
        label: impl Into<SharedString>,
        background: Hsla,
        cx: &Context<Self>,
    ) -> impl IntoElement {
        let colors = accessible_text_colors(background, cx.theme().secondary_foreground);

        h_flex()
            .flex_shrink_0()
            .h_5()
            .min_w_0()
            .max_w(px(160.))
            .gap_1()
            .items_center()
            .rounded_full()
            .px_1p5()
            .bg(colors.background)
            .text_xs()
            .line_height(relative(1.2))
            .font_weight(FontWeight::MEDIUM)
            .text_color(colors.foreground)
            .child(
                Icon::new(IconName::Calendar)
                    .xsmall()
                    .text_color(colors.foreground),
            )
            .child(div().truncate().child(label.into()))
    }

    pub(super) fn render_card_checklist_progress(
        &self,
        entry: &EntryDTO,
        cx: &Context<Self>,
    ) -> impl IntoElement {
        let completed = entry
            .checklist_items
            .iter()
            .filter(|item| item.checked)
            .count();
        let is_complete = completed == entry.checklist_items.len();

        h_flex()
            .gap_1()
            .items_center()
            .text_xs()
            .font_weight(FontWeight::MEDIUM)
            .text_color(if is_complete {
                cx.theme().success
            } else {
                cx.theme().primary_foreground.opacity(0.76)
            })
            .child(Icon::new(IconName::CircleCheck).xsmall())
            .child(format!("{completed}/{}", entry.checklist_items.len()))
    }
}
