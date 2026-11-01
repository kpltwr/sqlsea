//! The Query tab: SQL editors in tabs, each with its own result, and the
//! query history of the connection.

use std::time::Instant;

use iced::keyboard::{Key, key::Named};
use iced::widget::{
    Column, Row, button, column, container, row, scrollable, space, svg, text, text_editor,
    text_input,
};
use iced::{Center, Element, Fill, Length, Padding};

use crate::db::ResultSet;
use crate::fit::fit;
use crate::store::{self, HistoryEntry};
use crate::{
    Density, Load, Message, QUERY_GRID, RUN_KEY, count, empty_state, error_note, grid, muted,
    small_key, sql, theme, thousands,
};

pub struct QueryTab {
    pub id: u64,
    pub editor: text_editor::Content,
    pub result: Load<ResultSet>,
    pub selected: Option<usize>,
    /// Goes up on each run, so that a late answer to an earlier run is ignored.
    pub run: u64,
    /// The SQL of the current or the last run, for the history.
    pub sql: String,
    /// The start time of the current run, while it runs.
    pub started: Option<Instant>,
    /// True after the user asks to stop the current run.
    pub cancelling: bool,
    /// The reasons that the last run waits for the user to confirm it.
    pub risks: Option<Vec<String>>,
}

impl QueryTab {
    fn new(id: u64) -> Self {
        QueryTab {
            id,
            editor: text_editor::Content::new(),
            result: Load::Idle,
            selected: None,
            run: 0,
            sql: String::new(),
            started: None,
            cancelling: false,
            risks: None,
        }
    }

    pub fn title(&self) -> String {
        format!("Query {}", self.id)
    }

    pub fn running(&self) -> bool {
        self.started.is_some()
    }

    /// The selected text, or all text if nothing is selected.
    pub fn sql_to_run(&self) -> String {
        self.editor
            .selection()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| self.editor.text())
    }

    /// Suggestions for the word at the cursor.
    pub fn completion(
        &self,
        catalog: &crate::complete::Catalog,
    ) -> Option<crate::complete::Completion> {
        let cursor = self.editor.cursor();
        if cursor.selection.is_some() {
            return None;
        }
        let line = self.editor.line(cursor.position.line)?;
        crate::complete::complete(
            &self.editor.text(),
            &line.text,
            cursor.position.column,
            catalog,
        )
    }

    /// Replaces the word at the cursor with suggestion `index`.
    pub fn accept(&mut self, index: usize, catalog: &crate::complete::Catalog) {
        let Some(completion) = self.completion(catalog) else {
            return;
        };
        let Some(item) = completion.items.get(index) else {
            return;
        };
        for _ in completion.prefix.chars() {
            self.editor
                .perform(text_editor::Action::Edit(text_editor::Edit::Backspace));
        }
        self.editor
            .perform(text_editor::Action::Edit(text_editor::Edit::Paste(
                std::sync::Arc::new(item.clone()),
            )));
    }

    /// Replaces the text of the editor. Undo still works.
    pub fn set_sql(&mut self, sql: String) {
        self.editor.perform(text_editor::Action::SelectAll);
        self.editor
            .perform(text_editor::Action::Edit(text_editor::Edit::Paste(
                std::sync::Arc::new(sql),
            )));
    }

    /// Records the end of a run. Returns its history entry, or `None` if the
    /// answer is for an earlier run.
    pub fn finish(&mut self, run: u64, result: Result<ResultSet, String>) -> Option<HistoryEntry> {
        if run != self.run {
            return None;
        }
        let elapsed = self.started.take().map(|t| t.elapsed()).unwrap_or_default();
        let cancelled = std::mem::take(&mut self.cancelling);
        let entry = HistoryEntry {
            sql: self.sql.clone(),
            at: store::now(),
            ok: result.is_ok(),
            rows: match &result {
                Ok(rs) if rs.columns.is_empty() => rs.affected,
                Ok(rs) => Some((rs.rows.len() + rs.dropped) as u64),
                Err(_) => None,
            },
            millis: elapsed.as_millis() as u64,
        };
        self.result = match result {
            Ok(rs) => Load::Ready(rs),
            Err(e) if cancelled && crate::db::is_cancelled(&e) => {
                Load::Failed("The query was cancelled.".into())
            }
            Err(e) => Load::Failed(e),
        };
        Some(entry)
    }
}

pub struct Queries {
    pub tabs: Vec<QueryTab>,
    pub active: usize,
    next_id: u64,
    pub history: Vec<HistoryEntry>,
    pub show_history: bool,
    pub history_filter: String,
}

impl Queries {
    pub fn new(history: Vec<HistoryEntry>) -> Self {
        Queries {
            tabs: vec![QueryTab::new(1)],
            active: 0,
            next_id: 2,
            history,
            show_history: false,
            history_filter: String::new(),
        }
    }

    pub fn active(&self) -> &QueryTab {
        &self.tabs[self.active]
    }

    /// The tab that a suggestion goes into: the active tab.
    pub fn tab_mut_for_completion(&mut self) -> &mut QueryTab {
        self.active_mut()
    }

    pub fn active_mut(&mut self) -> &mut QueryTab {
        &mut self.tabs[self.active]
    }

    pub fn find_mut(&mut self, id: u64) -> Option<&mut QueryTab> {
        self.tabs.iter_mut().find(|t| t.id == id)
    }

    pub fn any_running(&self) -> bool {
        self.tabs.iter().any(QueryTab::running)
    }

    pub fn add(&mut self) {
        self.tabs.push(QueryTab::new(self.next_id));
        self.next_id += 1;
        self.active = self.tabs.len() - 1;
    }

    pub fn select(&mut self, id: u64) {
        if let Some(i) = self.tabs.iter().position(|t| t.id == id) {
            self.active = i;
        }
    }

    /// Closes a tab. The last tab does not close. It becomes empty.
    pub fn close(&mut self, id: u64) {
        let Some(i) = self.tabs.iter().position(|t| t.id == id) else {
            return;
        };
        if self.tabs.len() == 1 {
            self.tabs[0] = QueryTab::new(self.next_id);
            self.next_id += 1;
            return;
        }
        self.tabs.remove(i);
        if self.active > i || self.active == self.tabs.len() {
            self.active = self.active.saturating_sub(1);
        }
    }

    // ---------- view ----------

    /// The toolbar: History, Export, and Run or Cancel.
    pub fn tools(&self, density: Density) -> Element<'_, Message> {
        let tab = self.active();
        let exportable = matches!(&tab.result, Load::Ready(rs) if !rs.columns.is_empty());
        let run: Element<_> = match tab.started {
            Some(started) => row![
                text(if density == Density::Full {
                    format!("Running {:.1} s", started.elapsed().as_secs_f32())
                } else {
                    format!("{:.1} s", started.elapsed().as_secs_f32())
                })
                .size(theme::TEXT_SM)
                .style(theme::text_muted)
                .wrapping(text::Wrapping::None),
                small_key(
                    if tab.cancelling {
                        "Cancelling"
                    } else {
                        "Cancel"
                    },
                    theme::key_danger,
                    (!tab.cancelling).then_some(Message::Cancel),
                ),
            ]
            .spacing(12)
            .align_y(Center)
            .into(),
            None if density == Density::Full => row![
                text(format!("{RUN_KEY}+Enter"))
                    .size(theme::TEXT_SM)
                    .style(theme::text_muted)
                    .wrapping(text::Wrapping::None),
                small_key("Run", theme::key_primary, Some(Message::Run)),
            ]
            .spacing(12)
            .align_y(Center)
            .into(),
            None => small_key("Run", theme::key_primary, Some(Message::Run)),
        };
        row![
            toggle_key("History", self.show_history, Message::ToggleHistory),
            small_key(
                "Export",
                theme::key,
                exportable.then_some(Message::ExportQuery)
            ),
            run,
        ]
        .spacing(8)
        .align_y(Center)
        .into()
    }

    /// `width` is the width of the main area. `panel` is the width of the history panel, if it is open.
    pub fn view(
        &self,
        width: f32,
        panel: f32,
        catalog: &crate::complete::Catalog,
    ) -> Element<'_, Message> {
        let tab = self.active();
        let running = tab.running();
        let completion = tab.completion(catalog);
        let can_complete = completion.is_some();
        let editor = text_editor(&tab.editor)
            .id("sql")
            .placeholder("select * from …")
            .on_action(Message::Edit)
            .font(theme::MONO)
            .size(theme::TEXT_MD)
            .padding(16)
            .height(Fill)
            .highlight_with::<sql::Sql>((), sql::format)
            .key_binding(move |press| {
                let key = press.key.as_ref();
                if matches!(key, Key::Named(Named::Enter)) && press.modifiers.command() {
                    return Some(text_editor::Binding::Custom(Message::Run));
                }
                if matches!(key, Key::Named(Named::Escape)) && running {
                    return Some(text_editor::Binding::Custom(Message::Cancel));
                }
                // Tab takes the first suggestion, if there is one.
                if matches!(key, Key::Named(Named::Tab)) && !press.modifiers.shift() && can_complete
                {
                    return Some(text_editor::Binding::Custom(Message::Complete(0)));
                }
                if matches!(key, Key::Named(Named::Tab)) && !press.modifiers.shift() {
                    return Some(text_editor::Binding::Sequence(vec![
                        text_editor::Binding::Insert(' '),
                        text_editor::Binding::Insert(' '),
                    ]));
                }
                text_editor::Binding::from_key_press(press)
            })
            .style(theme::sql_editor);

        let results: Element<_> = if let Some(risks) = &tab.risks {
            confirm(risks)
        } else {
            match &tab.result {
                Load::Idle => empty_state(
                    format!(
                        "Results show here. Write SQL above and press {RUN_KEY}+Enter. \
                     If you select text, only that text runs."
                    ),
                    None,
                ),
                Load::Loading => grid::skeleton(),
                Load::Failed(e) => container(error_note(e)).padding(16).width(Fill).into(),
                Load::Ready(rs) if rs.columns.is_empty() => {
                    let mut note = column![
                        text(match rs.affected {
                            Some(n) => format!("{} affected", count(n, "row")),
                            None => "The statement ran.".to_string(),
                        })
                        .font(theme::DISPLAY)
                        .size(theme::TEXT_LG)
                    ]
                    .spacing(4);
                    if rs.affected == Some(0) {
                        note = note.push(muted("Nothing matched the statement's conditions."));
                    }
                    container(note).padding(16).width(Fill).into()
                }
                Load::Ready(rs) => grid::view(grid::Grid {
                    result: rs,
                    selected: tab.selected,
                    on_select: Some(Message::SelectQueryRow),
                    on_sort: None,
                    sort: None,
                    ids: QUERY_GRID,
                    on_scroll: Message::GridScrolled,
                }),
            }
        };

        // The suggestion bar keeps its height when empty, so the editor does not jump.
        let mut suggestions = Row::new().spacing(4).align_y(Center);
        if let Some(completion) = &completion {
            for (i, item) in completion.items.iter().enumerate() {
                suggestions = suggestions.push(
                    button(text(item.clone()).font(theme::MONO).size(theme::TEXT_XS))
                        .padding(Padding::from([3.0, 8.0]))
                        .style(theme::list_row(i == 0))
                        .on_press(Message::Complete(i)),
                );
            }
            suggestions = suggestions.push(
                text("Tab")
                    .size(theme::TEXT_XS)
                    .style(theme::text_muted)
                    .wrapping(text::Wrapping::None),
            );
        }
        let suggestions = container(scrollable(suggestions).direction(
            scrollable::Direction::Horizontal(scrollable::Scrollbar::hidden()),
        ))
        .padding(Padding::from([0.0, 12.0]))
        .height(32)
        .center_y(32)
        .width(Fill)
        .style(theme::query_strip);

        let work = column![
            self.strip(width),
            container(editor)
                .height(Length::FillPortion(2))
                .style(theme::graphite),
            suggestions,
            container(results).height(Length::FillPortion(3)),
        ];

        if self.show_history {
            row![
                container(work).width(Fill),
                container(space().width(1))
                    .height(Fill)
                    .style(theme::rule_h),
                self.history_view(panel),
            ]
            .into()
        } else {
            work.into()
        }
    }

    /// The query tabs above the editor. If there are too many tabs, they scroll
    /// sideways. New always stays visible.
    fn strip(&self, width: f32) -> Element<'_, Message> {
        let tabs = Row::with_children(self.tabs.iter().enumerate().map(|(i, tab)| {
            let active = i == self.active;
            let close = button(
                svg(svg::Handle::from_memory(crate::chrome::CLOSE))
                    .width(14)
                    .height(14)
                    .style(theme::query_tab_icon(active)),
            )
            .padding(2)
            .style(theme::query_tab_close(active))
            .on_press(Message::CloseQuery(tab.id));
            button(
                row![
                    text(tab.title())
                        .font(theme::BODY_MEDIUM)
                        .size(theme::TEXT_SM),
                    close,
                ]
                .spacing(6)
                .align_y(Center),
            )
            .padding(Padding::from([5.0, 8.0]).left(12.0))
            .style(theme::query_tab(active))
            .on_press(Message::SelectQuery(tab.id))
            .into()
        }))
        .spacing(2);

        let add = button(text("New").font(theme::BODY_MEDIUM).size(theme::TEXT_SM))
            .padding(Padding::from([5.0, 10.0]))
            .style(theme::ghost)
            .on_press(Message::NewQuery);

        let tabs = container(
            scrollable(tabs)
                .direction(scrollable::Direction::Horizontal(
                    scrollable::Scrollbar::hidden(),
                ))
                .id("query-tabs"),
        )
        .max_width((width - 96.0).max(120.0));

        container(row![tabs, add].spacing(6).align_y(Center))
            .padding(Padding::from([4.0, 8.0]).bottom(0.0))
            .width(Fill)
            .style(theme::query_strip)
            .into()
    }

    fn history_view(&self, width: f32) -> Element<'_, Message> {
        let head = row![
            text("History").font(theme::DISPLAY).size(theme::TEXT_LG),
            space::horizontal(),
            small_key("Close", theme::ghost, Some(Message::ToggleHistory)),
        ]
        .align_y(Center);

        let filter = text_input("Filter history", &self.history_filter)
            .on_input(Message::HistoryFilter)
            .padding(Padding::from([6.0, 10.0]))
            .size(theme::TEXT_MD)
            .style(theme::field);

        let needle = self.history_filter.trim().to_lowercase();
        let shown: Vec<(usize, &HistoryEntry)> = self
            .history
            .iter()
            .enumerate()
            .filter(|(_, e)| needle.is_empty() || e.sql.to_lowercase().contains(&needle))
            .collect();

        let list: Element<_> =
            if self.history.is_empty() {
                muted("Queries you run on this connection show here.")
            } else if shown.is_empty() {
                muted(format!(
                    "No query matches “{}”.",
                    self.history_filter.trim()
                ))
            } else {
                let now = store::now();
                scrollable(
                    Column::with_children(shown.into_iter().map(|(i, entry)| {
                        let preview =
                            Column::with_children(entry.sql.trim().lines().take(3).map(|line| {
                                fit(line).font(theme::MONO).size(theme::TEXT_XS).into()
                            }));
                        let outcome = match (entry.ok, entry.rows) {
                            (false, _) => "Failed".to_string(),
                            (true, Some(n)) => count(n, "row"),
                            (true, None) => "Ran".to_string(),
                        };
                        button(
                            column![
                                preview,
                                row![
                                    text(ago(now, entry.at))
                                        .size(theme::TEXT_XS)
                                        .style(theme::text_muted),
                                    text(outcome).size(theme::TEXT_XS).style(if entry.ok {
                                        theme::text_muted
                                    } else {
                                        theme::text_error
                                    }),
                                    text(format!("{} ms", thousands(entry.millis)))
                                        .size(theme::TEXT_XS)
                                        .style(theme::text_muted),
                                ]
                                .spacing(12),
                            ]
                            .spacing(6),
                        )
                        .width(Fill)
                        .padding(Padding::from([8.0, 10.0]))
                        .style(theme::list_row(false))
                        .on_press(Message::UseHistory(i))
                        .into()
                    }))
                    .spacing(2)
                    .padding(Padding::ZERO.right(10.0)),
                )
                .style(theme::scroller)
                .height(Fill)
                .into()
            };

        container(column![head, filter, list].spacing(12))
            .width(width)
            .height(Fill)
            .padding(16)
            .style(theme::bar)
            .into()
    }
}

/// Asks the user to confirm statements that change or remove every row.
fn confirm(risks: &[String]) -> Element<'_, Message> {
    let reasons = Column::with_children(risks.iter().map(|risk| {
        text(risk.as_str())
            .size(theme::TEXT_SM)
            .wrapping(text::Wrapping::WordOrGlyph)
            .into()
    }))
    .spacing(4);
    container(
        container(
            column![
                text("Run this query?")
                    .font(theme::DISPLAY)
                    .size(theme::TEXT_LG),
                reasons,
                row![
                    space::horizontal(),
                    small_key("Cancel", theme::key, Some(Message::DismissRisks)),
                    small_key(
                        "Run anyway",
                        theme::key_danger_armed,
                        Some(Message::RunAnyway)
                    ),
                ]
                .spacing(8),
            ]
            .spacing(12),
        )
        .padding(16)
        .width(Fill)
        .style(theme::error_note),
    )
    .padding(16)
    .width(Fill)
    .into()
}

/// A key that shows whether its panel is open.
fn toggle_key(label: &str, on: bool, message: Message) -> Element<'_, Message> {
    button(
        container(
            text(label)
                .font(theme::BODY_MEDIUM)
                .size(theme::TEXT_SM)
                .wrapping(text::Wrapping::None),
        )
        .center_y(Length::Fill),
    )
    .height(theme::CONTROL_H)
    .padding(Padding::from([0.0, 12.0]))
    .style(theme::key_toggle(on))
    .on_press(message)
    .into()
}

/// About how long ago, in words.
fn ago(now: u64, at: u64) -> String {
    let s = now.saturating_sub(at);
    match s {
        0..60 => "Just now".into(),
        60..3_600 => format!("{} min ago", s / 60),
        3_600..86_400 => format!("{} h ago", s / 3_600),
        86_400..172_800 => "Yesterday".into(),
        _ => format!("{} days ago", s / 86_400),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn closing_tabs_keeps_one_and_a_sane_selection() {
        let mut q = Queries::new(Vec::new());
        q.add();
        q.add();
        assert_eq!(q.active().id, 3);
        q.close(3);
        assert_eq!(q.active().id, 2);
        q.select(1);
        q.close(2);
        assert_eq!(q.active().id, 1);
        q.close(1);
        assert_eq!(q.tabs.len(), 1);
        assert_ne!(q.active().id, 1);
    }

    #[test]
    fn stale_runs_are_dropped() {
        let mut tab = QueryTab::new(1);
        tab.run = 2;
        tab.started = Some(Instant::now());
        assert!(tab.finish(1, Ok(ResultSet::default())).is_none());
        assert!(tab.running());
        assert!(tab.finish(2, Ok(ResultSet::default())).is_some());
        assert!(!tab.running());
    }

    #[test]
    fn ago_reads_naturally() {
        assert_eq!(ago(100, 90), "Just now");
        assert_eq!(ago(10_000, 10_000 - 7_200), "2 h ago");
        assert_eq!(ago(1_000_000, 1_000_000 - 100_000), "Yesterday");
    }
}
