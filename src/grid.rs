//! The result grid: a pinned head over hairline-ruled rows, values in mono.

use iced::widget::{
    Column, Row, button, column, container, responsive, row, scrollable, space, text,
};
use iced::{Element, Fill, Length, Padding};

use crate::db::ResultSet;
use crate::theme;

const PAD_X: f32 = 12.0;
const ROW_H: f32 = 30.0;
const MIN_W: f32 = 72.0;
const MAX_W: f32 = 360.0;
/// Rows sampled when sizing columns.
const SAMPLE: usize = 200;
const MAX_CHARS: usize = 160;

pub struct Grid<'a, M> {
    pub result: &'a ResultSet,
    pub selected: Option<usize>,
    pub on_select: Option<fn(usize) -> M>,
    pub on_sort: Option<fn(usize) -> M>,
    /// Column index and whether it sorts ascending.
    pub sort: Option<(usize, bool)>,
    /// Widget ids of the body and of the head that follows it sideways.
    pub ids: (&'static str, &'static str),
    /// Reports the body's horizontal offset so the head can follow.
    pub on_scroll: fn(&'static str, f32) -> M,
}

fn widths(rs: &ResultSet) -> Vec<f32> {
    rs.columns
        .iter()
        .enumerate()
        .map(|(i, name)| {
            let head = name.chars().count() as f32 * 7.2 + 18.0;
            let body = rs
                .rows
                .iter()
                .take(SAMPLE)
                .map(|r| {
                    r[i].as_deref()
                        .map_or(4, |v| first_line(v).0.chars().count())
                })
                .max()
                .unwrap_or(0) as f32
                * theme::MONO_ADVANCE;
            (head.max(body) + PAD_X * 2.0).clamp(MIN_W, MAX_W)
        })
        .collect()
}

/// The first line of a value, clipped, and whether anything was cut.
fn first_line(value: &str) -> (&str, bool) {
    let line = value.lines().next().unwrap_or("");
    let cut = line.char_indices().nth(MAX_CHARS).map(|(i, _)| i);
    match cut {
        Some(i) => (&line[..i], true),
        None => (line, line.len() < value.len()),
    }
}

pub fn view<'a, M: Clone + 'a>(grid: Grid<'a, M>) -> Element<'a, M> {
    let Grid {
        result,
        selected,
        on_select,
        on_sort,
        sort,
        ids: (body_id, head_id),
        on_scroll,
    } = grid;
    let base = widths(result);

    responsive(move |size| {
        let mut widths = base.clone();
        let total: f32 = widths.iter().sum();
        if total < size.width
            && let Some(last) = widths.last_mut()
        {
            *last += size.width - total;
        }
        let total: f32 = widths.iter().sum();

        let head = Row::with_children(result.columns.iter().enumerate().map(|(i, name)| {
            let mark = match sort {
                Some((c, true)) if c == i => "asc",
                Some((c, false)) if c == i => "desc",
                _ => "",
            };
            let label = row![
                text(name.as_str()).font(theme::LABEL).size(theme::TEXT_XS),
                space::horizontal(),
                text(mark)
                    .font(theme::LABEL)
                    .size(theme::TEXT_XS)
                    .style(theme::text_accent),
            ]
            .spacing(6);
            button(label)
                .width(widths[i])
                .padding(Padding::from([8.0, PAD_X]))
                .style(theme::grid_head_button)
                .on_press_maybe(on_sort.map(|f| f(i)))
                .into()
        }));

        let rows = Column::with_children(result.rows.iter().enumerate().map(|(r, values)| {
            let cells = Row::with_children(values.iter().enumerate().map(|(i, v)| {
                let cell: Element<'a, M> = match v {
                    Some(v) => {
                        let (shown, cut) = first_line(v);
                        let shown = if cut {
                            format!("{shown}…")
                        } else {
                            shown.to_string()
                        };
                        text(shown)
                            .font(theme::MONO)
                            .size(theme::TEXT_SM)
                            .wrapping(text::Wrapping::None)
                            .into()
                    }
                    None => text("NULL")
                        .font(theme::MONO)
                        .size(theme::TEXT_SM)
                        .style(theme::text_muted)
                        .into(),
                };
                container(cell)
                    .width(widths[i])
                    .height(ROW_H)
                    .padding(Padding::from([0.0, PAD_X]))
                    .center_y(ROW_H)
                    .clip(true)
                    .into()
            }));
            column![
                button(cells)
                    .padding(0)
                    .width(total)
                    .style(theme::grid_row(selected == Some(r)))
                    .on_press_maybe(on_select.map(|f| f(r))),
                container(space().height(1))
                    .width(total)
                    .style(theme::rule_h),
            ]
            .into()
        }));

        let bar = scrollable::Scrollbar::new()
            .width(8)
            .scroller_width(6)
            .margin(1);
        let body = scrollable(rows)
            .id(body_id)
            .direction(scrollable::Direction::Both {
                vertical: bar,
                horizontal: bar,
            })
            .on_scroll(move |viewport| on_scroll(head_id, viewport.absolute_offset().x))
            .style(theme::scroller)
            .width(Fill)
            .height(Fill);

        let head = scrollable(head)
            .id(head_id)
            .direction(scrollable::Direction::Horizontal(
                scrollable::Scrollbar::hidden(),
            ))
            .width(Fill);

        column![
            head,
            container(space().height(1))
                .width(Fill)
                .style(theme::rule_h),
            body
        ]
        .height(Fill)
        .into()
    })
    .into()
}

/// Loading state: bars in the shape of the rows that are coming.
pub fn skeleton<'a, M: 'a>() -> Element<'a, M> {
    const SHAPES: [[f32; 4]; 4] = [
        [48.0, 140.0, 96.0, 180.0],
        [40.0, 110.0, 120.0, 150.0],
        [52.0, 160.0, 80.0, 200.0],
        [44.0, 120.0, 104.0, 130.0],
    ];
    let bar = |w: f32| -> Element<'a, M> {
        container(space().width(w).height(10))
            .style(theme::skeleton)
            .into()
    };
    let rows = (0..10).map(|r| {
        let shape = SHAPES[r % SHAPES.len()];
        container(Row::with_children(
            shape
                .into_iter()
                .map(|w| container(bar(w)).width(Length::Fixed(w + 64.0)).into()),
        ))
        .padding(Padding::from([10.0, PAD_X]))
        .into()
    });
    column![
        container(space().height(31))
            .width(Fill)
            .style(theme::grid_head),
        Column::with_children(rows)
    ]
    .into()
}
