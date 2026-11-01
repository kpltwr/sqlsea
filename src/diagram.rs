//! The relationship diagram: each table is a box with its columns, and a line
//! goes from each foreign key to the table that it points to.
//!
//! The layout is made once, when the keys load or the schema changes. Tables
//! that other tables point to go on the left. A table goes one place to the
//! right of each table that it points to. Tables without keys go last.
//!
//! You can drag a table to a new place. The places are saved for each
//! connection and schema, and they replace the places of the layout.

use std::collections::HashMap;

use iced::alignment;
use iced::keyboard;
use iced::mouse::{self, ScrollDelta};
use iced::widget::canvas::{self, Frame, LineCap, Path, Stroke, Text};
use iced::widget::text::Alignment;
use iced::{Element, Event, Fill, Point, Rectangle, Renderer, Size, Theme, Vector};

use crate::db::{Relations, Table};
use crate::fit;
use crate::store::Places;
use crate::theme;

const HEADER: f32 = 32.0;
const ROW: f32 = 22.0;
const PAD: f32 = 12.0;
/// The space on the left of a column for its key mark.
const GUTTER: f32 = 22.0;
const MIN_WIDTH: f32 = 170.0;
const MAX_WIDTH: f32 = 340.0;
const MAX_TYPE: f32 = 110.0;
/// A table with more columns shows this many, then a line with the count of the others.
const MAX_ROWS: usize = 24;
const GAP_X: f32 = 96.0;
const GAP_Y: f32 = 28.0;
/// A stack of tables wraps to a new stack at this height.
const MAX_STACK: f32 = 1800.0;
const MARGIN: f32 = 32.0;
const NAME_SIZE: f32 = 13.0;
const COLUMN_SIZE: f32 = 12.0;

pub struct Diagram {
    relations: Relations,
    /// The schemas that have tables. Empty for SQLite.
    pub schemas: Vec<String>,
    pub schema: Option<String>,
    nodes: Vec<Node>,
    edges: Vec<Edge>,
    /// The part of the plane that the tables cover, with a margin.
    area: Rectangle,
    /// Goes up with each layout, so that the canvas fits the new layout in its view.
    layout: u64,
    /// The places of the tables that were moved by hand.
    places: Places,
}

/// A table on the diagram.
struct Node {
    table: Table,
    rect: Rectangle,
    title: String,
    /// The name, the type and the kind of key of each row that shows.
    rows: Vec<(String, String, Mark)>,
    /// The columns that do not show.
    hidden: usize,
    /// The row of each column by name, for the ends of the lines.
    row_of: HashMap<String, usize>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mark {
    None,
    Primary,
    Foreign,
}

/// A foreign key between two nodes.
struct Edge {
    from: usize,
    from_column: String,
    to: usize,
    to_column: String,
}

impl Diagram {
    /// Shows `prefer` if it has tables, else "public", else the first schema.
    pub fn new(relations: Relations, prefer: Option<&str>, places: Places) -> Self {
        let mut schemas: Vec<String> = relations
            .tables
            .iter()
            .filter_map(|e| e.table.schema.clone())
            .collect();
        schemas.sort();
        schemas.dedup();
        let schema = [prefer, Some("public")]
            .into_iter()
            .flatten()
            .find(|s| schemas.iter().any(|x| x == s))
            .map(str::to_string)
            .or_else(|| schemas.first().cloned());
        let mut diagram = Diagram {
            relations,
            schemas,
            schema,
            nodes: Vec::new(),
            edges: Vec::new(),
            area: Rectangle::default(),
            layout: 0,
            places,
        };
        diagram.arrange();
        diagram
    }

    pub fn set_schema(&mut self, schema: String) {
        self.schema = Some(schema);
        self.arrange();
    }

    /// The tables and the keys that show.
    pub fn counts(&self) -> (usize, usize) {
        (self.nodes.len(), self.edges.len())
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    pub fn places(&self) -> &Places {
        &self.places
    }

    /// True if a table of the chosen schema was moved by hand.
    pub fn moved_by_hand(&self) -> bool {
        self.places
            .get(&self.schema_key())
            .is_some_and(|p| !p.is_empty())
    }

    /// Forgets the places of the chosen schema that were moved by hand.
    pub fn arrange_again(&mut self) {
        self.places.remove(&self.schema_key());
        self.arrange();
    }

    /// Moves a table to `to`, if the layout is still `layout`.
    pub fn move_table(&mut self, layout: u64, node: usize, to: Point) {
        if layout != self.layout || node >= self.nodes.len() {
            return;
        }
        let to = Point::new(to.x.round(), to.y.round());
        self.nodes[node].rect.x = to.x;
        self.nodes[node].rect.y = to.y;
        let key = table_key(&self.nodes[node].table);
        self.places
            .entry(self.schema_key())
            .or_default()
            .insert(key, (to.x, to.y));
        self.area = area(&self.nodes);
    }

    fn schema_key(&self) -> String {
        self.schema.clone().unwrap_or_default()
    }

    pub fn view<'a, M: 'a>(
        &'a self,
        on_open: fn(Table) -> M,
        on_move: fn(u64, usize, Point) -> M,
    ) -> Element<'a, M> {
        canvas::Canvas::new(View {
            diagram: self,
            on_open,
            on_move,
        })
        .width(Fill)
        .height(Fill)
        .into()
    }

    /// The tables of the chosen schema, and the tables in other schemas that
    /// they have keys with. Then their places.
    fn arrange(&mut self) {
        let r = &self.relations;
        let in_schema = |i: usize| self.schema.is_none() || r.tables[i].table.schema == self.schema;
        let mut shown: Vec<usize> = (0..r.tables.len()).filter(|&i| in_schema(i)).collect();
        for k in &r.keys {
            if in_schema(k.from) != in_schema(k.to) {
                shown.push(if in_schema(k.from) { k.to } else { k.from });
            }
        }
        shown.sort();
        shown.dedup();
        let node_of: HashMap<usize, usize> =
            shown.iter().enumerate().map(|(n, &t)| (t, n)).collect();

        let mut nodes: Vec<Node> = shown
            .iter()
            .map(|&t| node(r, t, !in_schema(t), &self.schema))
            .collect();
        let mut edges = Vec::new();
        for k in &r.keys {
            let (Some(&from), Some(&to)) = (node_of.get(&k.from), node_of.get(&k.to)) else {
                continue;
            };
            for column in &k.from_columns {
                if let Some(&row) = nodes[from].row_of.get(column)
                    && nodes[from].rows[row].2 == Mark::None
                {
                    nodes[from].rows[row].2 = Mark::Foreign;
                }
            }
            edges.push(Edge {
                from,
                from_column: k.from_columns.first().cloned().unwrap_or_default(),
                to,
                to_column: k.to_columns.first().cloned().unwrap_or_default(),
            });
        }

        place(&mut nodes, &edges);
        // The tables that were moved by hand go to their places. The others
        // keep the layout, to the right of the moved ones, so that none overlap.
        if let Some(saved) = self.places.get(&self.schema_key()) {
            let (moved, rest): (Vec<usize>, Vec<usize>) =
                (0..nodes.len()).partition(|&i| saved.contains_key(&table_key(&nodes[i].table)));
            if !moved.is_empty() {
                for &i in &moved {
                    let (x, y) = saved[&table_key(&nodes[i].table)];
                    nodes[i].rect.x = x;
                    nodes[i].rect.y = y;
                }
                let right = moved
                    .iter()
                    .map(|&i| nodes[i].rect.x + nodes[i].rect.width)
                    .fold(f32::MIN, f32::max);
                let left = rest
                    .iter()
                    .map(|&i| nodes[i].rect.x)
                    .fold(f32::MAX, f32::min);
                for &i in &rest {
                    nodes[i].rect.x += right + GAP_X - left;
                }
            }
        }
        self.area = area(&nodes);
        self.nodes = nodes;
        self.edges = edges;
        self.layout += 1;
    }
}

/// The name of a table in the saved places.
fn table_key(table: &Table) -> String {
    match &table.schema {
        Some(schema) => format!("{schema}.{}", table.name),
        None => table.name.clone(),
    }
}

/// The part of the plane that the nodes cover, with a margin.
fn area(nodes: &[Node]) -> Rectangle {
    let Some(first) = nodes.first() else {
        return Rectangle::new(Point::ORIGIN, Size::new(1.0, 1.0));
    };
    let mut r = first.rect;
    for n in &nodes[1..] {
        r = r.union(&n.rect);
    }
    r.expand(MARGIN)
}

/// Measures a table and cuts its names to fit. The place comes later.
fn node(r: &Relations, t: usize, outside: bool, schema: &Option<String>) -> Node {
    let entity = &r.tables[t];
    let title = match (&entity.table.schema, outside || schema.is_none()) {
        (Some(s), true) => format!("{s}.{}", entity.table.name),
        _ => entity.table.name.clone(),
    };
    let shown = entity.columns.len().min(MAX_ROWS);
    let type_width = |t: &str| fit::measure(t, COLUMN_SIZE, theme::MONO).min(MAX_TYPE);
    let mut width = fit::measure(&title, NAME_SIZE, theme::LABEL) + 2.0 * PAD + 1.0;
    for c in &entity.columns[..shown] {
        let w = GUTTER
            + fit::measure(&c.name, COLUMN_SIZE, theme::MONO)
            + 16.0
            + type_width(&c.data_type)
            + PAD
            // Rounding in the measure can cut a name that fits exactly.
            + 1.0;
        width = width.max(w);
    }
    let width = width.clamp(MIN_WIDTH, MAX_WIDTH);
    let rows = entity.columns[..shown]
        .iter()
        .map(|c| {
            let type_shown = fit::cut_to(&c.data_type, COLUMN_SIZE, theme::MONO, MAX_TYPE);
            let room = width - GUTTER - PAD - 16.0 - type_width(&c.data_type);
            (
                fit::cut_to(&c.name, COLUMN_SIZE, theme::MONO, room),
                type_shown,
                if c.primary_key {
                    Mark::Primary
                } else {
                    Mark::None
                },
            )
        })
        .collect::<Vec<_>>();
    let hidden = entity.columns.len() - shown;
    let height = HEADER + ROW * (rows.len() + usize::from(hidden > 0)) as f32 + 6.0;
    Node {
        table: entity.table.clone(),
        title: fit::cut_to(&title, NAME_SIZE, theme::LABEL, width - 2.0 * PAD),
        row_of: entity.columns[..shown]
            .iter()
            .enumerate()
            .map(|(i, c)| (c.name.clone(), i))
            .collect(),
        rows,
        hidden,
        rect: Rectangle::new(Point::ORIGIN, Size::new(width, height)),
    }
}

/// Puts each node in a column by its rank.
fn place(nodes: &mut [Node], edges: &[Edge]) {
    let n = nodes.len();
    let linked: Vec<bool> = (0..n)
        .map(|i| {
            edges
                .iter()
                .any(|e| e.from != e.to && (e.from == i || e.to == i))
        })
        .collect();

    // A table is one rank to the right of each table that it points to. The
    // passes stop after `n` rounds, so a cycle of keys cannot loop forever.
    let mut rank = vec![0usize; n];
    for _ in 0..n {
        let mut changed = false;
        for e in edges.iter().filter(|e| e.from != e.to) {
            if rank[e.from] < rank[e.to] + 1 && rank[e.to] + 1 < n {
                rank[e.from] = rank[e.to] + 1;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    let mut ranks: Vec<Vec<usize>> = Vec::new();
    for i in (0..n).filter(|&i| linked[i]) {
        if ranks.len() <= rank[i] {
            ranks.resize(rank[i] + 1, Vec::new());
        }
        ranks[rank[i]].push(i);
    }
    ranks.retain(|r| !r.is_empty());

    // Orders each rank by the mean place of its neighbors, so that lines cross less.
    let mut order = vec![0.0f32; n];
    let set_order = |ranks: &Vec<Vec<usize>>, order: &mut Vec<f32>| {
        for rank in ranks {
            for (p, &i) in rank.iter().enumerate() {
                order[i] = (p as f32 + 0.5) / rank.len() as f32;
            }
        }
    };
    set_order(&ranks, &mut order);
    for _ in 0..4 {
        for rank in ranks.iter_mut() {
            let key = |i: usize| {
                let near: Vec<f32> = edges
                    .iter()
                    .filter(|e| e.from != e.to)
                    .filter_map(|e| match (e.from == i, e.to == i) {
                        (true, _) => Some(order[e.to]),
                        (_, true) => Some(order[e.from]),
                        _ => None,
                    })
                    .collect();
                if near.is_empty() {
                    order[i]
                } else {
                    near.iter().sum::<f32>() / near.len() as f32
                }
            };
            let mut keyed: Vec<(f32, usize)> = rank.iter().map(|&i| (key(i), i)).collect();
            keyed.sort_by(|a, b| a.0.total_cmp(&b.0));
            *rank = keyed.into_iter().map(|(_, i)| i).collect();
        }
        set_order(&ranks, &mut order);
    }

    // The linked tables in their columns. A tall column wraps to a new column.
    let mut x = MARGIN;
    let mut bottom: f32 = 0.0;
    for rank in &ranks {
        x = stack(nodes, rank, x, MAX_STACK, &mut bottom);
    }
    // Then the tables without keys, in columns as tall as the rest.
    let rest: Vec<usize> = (0..n).filter(|&i| !linked[i]).collect();
    if !rest.is_empty() {
        let limit = if ranks.is_empty() {
            // Only tables without keys: about as tall as wide.
            let area: f32 = rest
                .iter()
                .map(|&i| (nodes[i].rect.width + GAP_X) * (nodes[i].rect.height + GAP_Y))
                .sum();
            area.sqrt().max(400.0)
        } else {
            bottom.max(600.0)
        };
        stack(nodes, &rest, x, limit, &mut bottom);
    }
}

/// Stacks `items` from the top at `x`, in columns not taller than `limit`.
/// Returns the x of the next column.
fn stack(nodes: &mut [Node], items: &[usize], mut x: f32, limit: f32, bottom: &mut f32) -> f32 {
    let mut y = MARGIN;
    let mut width: f32 = 0.0;
    for &i in items {
        let h = nodes[i].rect.height;
        if y > MARGIN && y + h > MARGIN + limit {
            x += width + GAP_X;
            y = MARGIN;
            width = 0.0;
        }
        nodes[i].rect.x = x;
        nodes[i].rect.y = y;
        y += h + GAP_Y;
        width = width.max(nodes[i].rect.width);
        *bottom = bottom.max(y - GAP_Y);
    }
    x + width + GAP_X
}

// ---------- canvas ----------

struct View<'a, M> {
    diagram: &'a Diagram,
    on_open: fn(Table) -> M,
    on_move: fn(u64, usize, Point) -> M,
}

/// What a press of the mouse drags.
#[derive(Clone, Copy)]
enum Drag {
    /// The whole diagram. The press point and the offset at the press.
    View(Point, Vector),
    /// One table. The press point.
    Table(usize, Point),
}

/// The view of the canvas: screen = offset + zoom × diagram.
#[derive(Default)]
pub struct State {
    offset: Vector,
    zoom: f32,
    /// The layout that the view is for. A new layout fits in the view again.
    layout: u64,
    drag: Option<Drag>,
    moved: bool,
    /// How far the dragged table moved on the screen.
    shift: Vector,
    zoom_key: bool,
    hover: Option<usize>,
}

const MIN_ZOOM: f32 = 0.25;
const MAX_ZOOM: f32 = 2.0;

impl<M> View<'_, M> {
    /// The view that shows the whole diagram, or its top left at 75% if it is too large.
    fn fit(&self, bounds: Rectangle) -> (Vector, f32) {
        let area = self.diagram.area;
        let zoom = (bounds.width / area.width)
            .min(bounds.height / area.height)
            .clamp(0.75, 1.0);
        let left = ((bounds.width - area.width * zoom) / 2.0).max(0.0);
        let top = ((bounds.height - area.height * zoom) / 2.0).max(0.0);
        (Vector::new(left - area.x * zoom, top - area.y * zoom), zoom)
    }

    fn view_of(&self, state: &State, bounds: Rectangle) -> (Vector, f32) {
        if state.layout == self.diagram.layout {
            (state.offset, state.zoom)
        } else {
            self.fit(bounds)
        }
    }

    /// The node under a point of the canvas. The last one drawn is on top.
    fn node_at(&self, at: Point, offset: Vector, zoom: f32) -> Option<usize> {
        let p = Point::new((at.x - offset.x) / zoom, (at.y - offset.y) / zoom);
        self.diagram.nodes.iter().rposition(|n| n.rect.contains(p))
    }
}

impl<M> canvas::Program<M> for View<'_, M> {
    type State = State;

    fn update(
        &self,
        state: &mut State,
        event: &Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<canvas::Action<M>> {
        if state.layout != self.diagram.layout {
            (state.offset, state.zoom) = self.fit(bounds);
            state.layout = self.diagram.layout;
            state.hover = None;
        }
        let at = cursor.position_in(bounds);
        match event {
            Event::Keyboard(keyboard::Event::ModifiersChanged(m)) => {
                state.zoom_key = m.control() || m.command();
                None
            }
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                let at = at?;
                state.drag = Some(match self.node_at(at, state.offset, state.zoom) {
                    Some(node) => Drag::Table(node, at),
                    None => Drag::View(at, state.offset),
                });
                state.moved = false;
                state.shift = Vector::ZERO;
                Some(canvas::Action::capture())
            }
            Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                if let Some(drag) = state.drag {
                    let now = cursor.position_from(bounds.position())?;
                    let start = match drag {
                        Drag::View(start, _) | Drag::Table(_, start) => start,
                    };
                    let delta = now - start;
                    if delta.x.abs() + delta.y.abs() > 3.0 {
                        state.moved = true;
                    }
                    if !state.moved {
                        return None;
                    }
                    match drag {
                        Drag::View(_, offset) => state.offset = offset + delta,
                        Drag::Table(..) => state.shift = delta,
                    }
                    return Some(canvas::Action::request_redraw().and_capture());
                }
                let hover = at.and_then(|p| self.node_at(p, state.offset, state.zoom));
                if hover != state.hover {
                    state.hover = hover;
                    return Some(canvas::Action::request_redraw());
                }
                None
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                let drag = state.drag.take()?;
                let shift = std::mem::take(&mut state.shift);
                match (drag, state.moved) {
                    (Drag::View(..), true) => Some(canvas::Action::capture()),
                    (Drag::View(..), false) => None,
                    (Drag::Table(node, _), true) => {
                        let to =
                            self.diagram.nodes[node].rect.position() + shift * (1.0 / state.zoom);
                        let message = (self.on_move)(self.diagram.layout, node, to);
                        Some(canvas::Action::publish(message).and_capture())
                    }
                    (Drag::Table(node, _), false) => {
                        let table = self.diagram.nodes[node].table.clone();
                        Some(canvas::Action::publish((self.on_open)(table)).and_capture())
                    }
                }
            }
            Event::Mouse(mouse::Event::WheelScrolled { delta }) => {
                let at = at?;
                let (x, y) = match *delta {
                    ScrollDelta::Lines { x, y } => (x * 40.0, y * 40.0),
                    ScrollDelta::Pixels { x, y } => (x, y),
                };
                if state.zoom_key {
                    let zoom = (state.zoom * 1.0015f32.powf(y)).clamp(MIN_ZOOM, MAX_ZOOM);
                    // The point under the cursor stays under the cursor.
                    let p = (at - Point::ORIGIN - state.offset) * (1.0 / state.zoom);
                    state.offset = at - Point::ORIGIN - p * zoom;
                    state.zoom = zoom;
                } else {
                    state.offset += Vector::new(x, y);
                }
                state.hover = self.node_at(at, state.offset, state.zoom);
                Some(canvas::Action::request_redraw().and_capture())
            }
            _ => None,
        }
    }

    fn mouse_interaction(
        &self,
        state: &State,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> mouse::Interaction {
        if state.drag.is_some() && state.moved {
            mouse::Interaction::Grabbing
        } else if matches!(state.drag, Some(Drag::Table(..)))
            || (cursor.is_over(bounds) && state.hover.is_some())
        {
            mouse::Interaction::Pointer
        } else if cursor.is_over(bounds) {
            mouse::Interaction::Grab
        } else {
            mouse::Interaction::default()
        }
    }

    fn draw(
        &self,
        state: &State,
        renderer: &Renderer,
        theme: &Theme,
        bounds: Rectangle,
        _: mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let colors = theme::diagram(theme);
        let (offset, zoom) = self.view_of(state, bounds);
        let hover = state.hover.filter(|_| state.layout == self.diagram.layout);
        let mut frame = Frame::new(renderer, bounds.size());
        // The part of the diagram in view, to skip what does not show.
        let seen = Rectangle::new(
            Point::new(-offset.x / zoom, -offset.y / zoom),
            Size::new(bounds.width / zoom, bounds.height / zoom),
        )
        .expand(MAX_WIDTH);
        frame.translate(offset);
        frame.scale(zoom);

        let nodes = &self.diagram.nodes;
        // The table that is dragged shows at its new place, on top of the others.
        let dragged = match state.drag {
            Some(Drag::Table(node, _)) if state.moved => Some(node),
            _ => None,
        };
        let hover = dragged.or(hover);
        let rects: Vec<Rectangle> = nodes
            .iter()
            .enumerate()
            .map(|(i, n)| {
                if dragged == Some(i) {
                    n.rect + state.shift * (1.0 / zoom)
                } else {
                    n.rect
                }
            })
            .collect();
        // Lines first, so that the tables cover their ends. The lines of the
        // hovered table come last, on top of the others.
        let mut edges: Vec<&Edge> = self.diagram.edges.iter().collect();
        edges.sort_by_key(|e| hover.is_some_and(|h| e.from == h || e.to == h));
        for e in edges {
            let (ra, rb) = (rects[e.from], rects[e.to]);
            if !seen.intersects(&ra) && !seen.intersects(&rb) {
                continue;
            }
            let focus = hover.is_some_and(|h| e.from == h || e.to == h);
            let color = if focus { colors.focus } else { colors.line };
            let a = (&nodes[e.from], ra, e.from_column.as_str());
            let b = (&nodes[e.to], rb, e.to_column.as_str());
            draw_edge(&mut frame, a, b, color, focus);
        }
        let order = (0..nodes.len())
            .filter(|&i| dragged != Some(i))
            .chain(dragged);
        for i in order {
            if seen.intersects(&rects[i]) {
                draw_node(&mut frame, &nodes[i], rects[i], &colors, hover == Some(i));
            }
        }
        vec![frame.into_geometry()]
    }
}

/// The y of the middle of a column row, or of the header if the column does not show.
fn row_y(node: &Node, rect: Rectangle, column: &str) -> f32 {
    match node.row_of.get(column) {
        Some(&row) => rect.y + HEADER + ROW * (row as f32 + 0.5),
        None => rect.y + HEADER / 2.0,
    }
}

/// A line from `from` to `to`. Each end is a node, its place and a column.
fn draw_edge(
    frame: &mut Frame,
    (from, fa, from_column): (&Node, Rectangle, &str),
    (to, fb, to_column): (&Node, Rectangle, &str),
    color: iced::Color,
    focus: bool,
) {
    let y0 = row_y(from, fa, from_column);
    let y1 = row_y(to, fb, to_column);
    let stroke = Stroke::default()
        .with_width(if focus { 1.75 } else { 1.25 })
        .with_color(color)
        .with_line_cap(LineCap::Round);
    // Each end leaves its table on the side that faces the other table. A key
    // to its own table, or between tables in one column, loops on the right.
    let same_column = fa.x < fb.x + fb.width && fb.x < fa.x + fa.width;
    let (x0, d0, x1, d1) = if same_column {
        (fa.x + fa.width, 1.0, fb.x + fb.width, 1.0)
    } else if fa.center_x() > fb.center_x() {
        (fa.x, -1.0, fb.x + fb.width, 1.0)
    } else {
        (fa.x + fa.width, 1.0, fb.x, -1.0)
    };
    let reach = if same_column {
        40.0 + (y1 - y0).abs() * 0.15
    } else {
        ((x1 - x0).abs() / 2.0).max(30.0)
    };
    // The marks take the first 10 units at each end.
    let (p0, p3) = (
        Point::new(x0 + 10.0 * d0, y0),
        Point::new(x1 + 10.0 * d1, y1),
    );
    let path = Path::new(|b| {
        b.move_to(Point::new(x0, y0));
        b.line_to(p0);
        b.bezier_curve_to(
            Point::new(p0.x + reach * d0, y0),
            Point::new(p3.x + reach * d1, y1),
            p3,
        );
        b.line_to(Point::new(x1, y1));
    });
    frame.stroke(&path, stroke);
    // Many rows on the key side: a fork. One row on the other side: a bar.
    let fork = Path::new(|b| {
        b.move_to(Point::new(x0 + 10.0 * d0, y0));
        b.line_to(Point::new(x0, y0 - 5.0));
        b.move_to(Point::new(x0 + 10.0 * d0, y0));
        b.line_to(Point::new(x0, y0 + 5.0));
    });
    frame.stroke(&fork, stroke);
    let bar = Path::line(
        Point::new(x1 + 6.0 * d1, y1 - 5.0),
        Point::new(x1 + 6.0 * d1, y1 + 5.0),
    );
    frame.stroke(&bar, stroke);
}

fn draw_node(frame: &mut Frame, node: &Node, r: Rectangle, c: &theme::Diagram, focus: bool) {
    let card = Path::rounded_rectangle(r.position(), r.size(), 6.0.into());
    frame.fill(&card, c.card);
    let header = Path::rounded_rectangle(
        r.position(),
        Size::new(r.width, HEADER),
        iced::border::Radius::default().top(6.0),
    );
    frame.fill(&header, if focus { c.focus_header } else { c.header });
    frame.stroke(
        &card,
        Stroke::default()
            .with_width(if focus { 1.5 } else { 1.0 })
            .with_color(if focus { c.focus } else { c.border }),
    );

    let text = |content: &str, at: Point, size: f32, font, color, align_x| Text {
        content: content.to_string(),
        position: at,
        color,
        size: size.into(),
        font,
        align_x,
        align_y: alignment::Vertical::Center,
        ..Text::default()
    };
    frame.fill_text(text(
        &node.title,
        Point::new(r.x + PAD, r.y + HEADER / 2.0),
        NAME_SIZE,
        theme::LABEL,
        c.title,
        Alignment::Left,
    ));
    for (i, (name, data_type, mark)) in node.rows.iter().enumerate() {
        let y = r.y + HEADER + ROW * (i as f32 + 0.5);
        let dot = Point::new(r.x + PAD + 3.0, y);
        match mark {
            Mark::Primary => frame.fill(&Path::circle(dot, 3.0), c.key),
            Mark::Foreign => frame.stroke(
                &Path::circle(dot, 2.75),
                Stroke::default().with_width(1.25).with_color(c.muted),
            ),
            Mark::None => {}
        }
        frame.fill_text(text(
            name,
            Point::new(r.x + GUTTER, y),
            COLUMN_SIZE,
            theme::MONO,
            if *mark == Mark::Primary {
                c.title
            } else {
                c.column
            },
            Alignment::Left,
        ));
        frame.fill_text(text(
            data_type,
            Point::new(r.x + r.width - PAD, y),
            COLUMN_SIZE,
            theme::MONO,
            c.muted,
            Alignment::Right,
        ));
    }
    if node.hidden > 0 {
        let y = r.y + HEADER + ROW * (node.rows.len() as f32 + 0.5);
        frame.fill_text(text(
            &format!("{} more columns", node.hidden),
            Point::new(r.x + GUTTER, y),
            COLUMN_SIZE,
            theme::BODY,
            c.muted,
            Alignment::Left,
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{Entity, EntityColumn, Key};
    use canvas::Program;

    fn entity(schema: Option<&str>, name: &str, columns: &[&str]) -> Entity {
        Entity {
            table: Table {
                schema: schema.map(str::to_string),
                name: name.into(),
                is_view: false,
            },
            columns: columns
                .iter()
                .enumerate()
                .map(|(i, c)| EntityColumn {
                    name: c.to_string(),
                    data_type: "integer".into(),
                    primary_key: i == 0,
                })
                .collect(),
        }
    }

    fn relations(tables: Vec<Entity>, keys: &[(usize, &str, usize)]) -> Relations {
        Relations {
            tables,
            keys: keys
                .iter()
                .map(|&(from, column, to)| Key {
                    from,
                    from_columns: vec![column.into()],
                    to,
                    to_columns: vec!["id".into()],
                })
                .collect(),
        }
    }

    fn rect(d: &Diagram, title: &str) -> Rectangle {
        d.nodes.iter().find(|n| n.title == title).unwrap().rect
    }

    #[test]
    fn tables_go_right_of_the_tables_they_point_to() {
        let d = Diagram::new(
            relations(
                vec![
                    entity(None, "a", &["id"]),
                    entity(None, "b", &["id", "a_id"]),
                    entity(None, "c", &["id", "b_id", "a_id"]),
                    entity(None, "d", &["id"]),
                ],
                &[(1, "a_id", 0), (2, "b_id", 1), (2, "a_id", 0)],
            ),
            None,
            Places::default(),
        );
        let x = |t| rect(&d, t).x;
        assert!(x("a") < x("b") && x("b") < x("c"));
        // A table without keys goes last.
        assert!(x("d") > x("c"));
        for (i, a) in d.nodes.iter().enumerate() {
            for b in &d.nodes[i + 1..] {
                let overlap = a.rect.x < b.rect.x + b.rect.width
                    && b.rect.x < a.rect.x + a.rect.width
                    && a.rect.y < b.rect.y + b.rect.height
                    && b.rect.y < a.rect.y + a.rect.height;
                assert!(!overlap, "{} and {} overlap", a.title, b.title);
            }
        }
        assert_eq!(d.counts(), (4, 3));
    }

    #[test]
    fn a_cycle_of_keys_and_a_key_to_itself_still_lay_out() {
        let d = Diagram::new(
            relations(
                vec![
                    entity(None, "a", &["id", "b_id", "parent_id"]),
                    entity(None, "b", &["id", "a_id"]),
                ],
                &[(0, "b_id", 1), (1, "a_id", 0), (0, "parent_id", 0)],
            ),
            None,
            Places::default(),
        );
        assert_eq!(d.counts(), (2, 3));
        assert!(d.area.width.is_finite() && d.area.width < 2000.0);
    }

    #[test]
    fn one_schema_shows_with_the_tables_it_has_keys_with() {
        let r = relations(
            vec![
                entity(Some("billing"), "invoices", &["id", "order_id"]),
                entity(Some("billing"), "payments", &["id", "invoice_id"]),
                entity(Some("public"), "notes", &["id"]),
                entity(Some("public"), "orders", &["id"]),
            ],
            &[(0, "order_id", 3), (1, "invoice_id", 0)],
        );
        let mut d = Diagram::new(r, None, Places::default());
        assert_eq!(d.schemas, ["billing", "public"]);
        // "public" is the default.
        let titles = |d: &Diagram| d.nodes.iter().map(|n| n.title.clone()).collect::<Vec<_>>();
        assert_eq!(titles(&d), ["billing.invoices", "notes", "orders"]);
        d.set_schema("billing".into());
        assert_eq!(titles(&d), ["invoices", "payments", "public.orders"]);
    }

    #[test]
    fn a_click_opens_a_table_a_drag_moves_and_ctrl_scroll_zooms() {
        let d = Diagram::new(
            relations(
                vec![
                    entity(None, "a", &["id"]),
                    entity(None, "b", &["id", "a_id"]),
                ],
                &[(1, "a_id", 0)],
            ),
            None,
            Places::default(),
        );
        let view = View {
            diagram: &d,
            on_open: |t: Table| t.name,
            on_move: |_, n, p| format!("move {n} to {},{}", p.x, p.y),
        };
        let bounds = Rectangle::new(Point::ORIGIN, Size::new(800.0, 600.0));
        let mut state = State::default();
        let send = |state: &mut State, event: Event, at: Point| {
            view.update(state, &event, bounds, mouse::Cursor::Available(at))
                .and_then(|a| a.into_inner().0)
        };
        let press = || Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left));
        let release = || Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left));
        let moved = |p| Event::Mouse(mouse::Event::CursorMoved { position: p });
        let on_screen = |state: &State, r: Rectangle| {
            Point::new(
                state.offset.x + state.zoom * r.center_x(),
                state.offset.y + state.zoom * r.center_y(),
            )
        };

        send(&mut state, moved(Point::ORIGIN), Point::ORIGIN);
        let b = on_screen(&state, rect(&d, "b"));
        send(&mut state, press(), b);
        assert_eq!(send(&mut state, release(), b).as_deref(), Some("b"));

        // A drag on a table moves the table, by the distance in the diagram.
        let before = state.offset;
        let to = b + Vector::new(60.0, 40.0);
        send(&mut state, press(), b);
        send(&mut state, moved(to), to);
        let moved_to = rect(&d, "b").position() + Vector::new(60.0, 40.0) * (1.0 / state.zoom);
        assert_eq!(
            send(&mut state, release(), to),
            Some(format!("move 1 to {},{}", moved_to.x, moved_to.y))
        );
        assert_eq!(state.offset, before);

        // A drag on the background moves the view.
        let empty = Point::new(5.0, 5.0);
        let to = empty + Vector::new(60.0, 40.0);
        send(&mut state, press(), empty);
        send(&mut state, moved(to), to);
        assert_eq!(send(&mut state, release(), to), None);
        assert_eq!(state.offset, before + Vector::new(60.0, 40.0));

        let at = Point::new(300.0, 200.0);
        let under = |s: &State| (at - Point::ORIGIN - s.offset) * (1.0 / s.zoom);
        let world = under(&state);
        let zoom = state.zoom;
        send(
            &mut state,
            Event::Keyboard(keyboard::Event::ModifiersChanged(keyboard::Modifiers::CTRL)),
            at,
        );
        let wheel = mouse::Event::WheelScrolled {
            delta: ScrollDelta::Lines { x: 0.0, y: 2.0 },
        };
        send(&mut state, Event::Mouse(wheel), at);
        assert!(state.zoom > zoom);
        let after = under(&state);
        assert!((after.x - world.x).abs() < 0.01 && (after.y - world.y).abs() < 0.01);
    }

    #[test]
    fn moved_tables_keep_their_places_and_new_tables_do_not_cover_them() {
        let r = || {
            relations(
                vec![
                    entity(Some("public"), "a", &["id"]),
                    entity(Some("public"), "b", &["id", "a_id"]),
                ],
                &[(1, "a_id", 0)],
            )
        };
        let mut d = Diagram::new(r(), None, Places::default());
        assert!(!d.moved_by_hand());
        let layout = d.layout;
        d.move_table(layout, 0, Point::new(500.4, 300.6));
        assert!(d.moved_by_hand());
        assert_eq!(d.places()["public"]["public.a"], (500.0, 301.0));
        // A move from an older layout does nothing.
        d.move_table(layout - 1, 1, Point::new(0.0, 0.0));
        assert_ne!(rect(&d, "b").position(), Point::ORIGIN);

        // The place comes back with the saved places. "b" has no saved place,
        // so it goes to the right of "a".
        let mut places = d.places().clone();
        places.get_mut("public").unwrap().remove("public.b");
        let mut d = Diagram::new(r(), None, places);
        assert_eq!(rect(&d, "a").position(), Point::new(500.0, 301.0));
        let (a, b) = (rect(&d, "a"), rect(&d, "b"));
        assert!(b.x >= a.x + a.width);
        assert!(d.area.contains(Point::new(a.x, a.y)));

        d.arrange_again();
        assert!(!d.moved_by_hand());
        assert!(rect(&d, "a").x < rect(&d, "b").x);
        assert_ne!(rect(&d, "a").position(), Point::new(500.0, 301.0));
    }
}
