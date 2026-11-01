//! The title bar controls and resize edges that sqlsea draws on Linux.
//!
//! On Wayland, the app must draw its own title bar. The title bar that winit
//! draws does not use the colors of sqlsea, so on Linux the top bar of the app
//! is the title bar. macOS and Windows keep their own title bars.

use iced::advanced::layout::{self, Layout};
use iced::advanced::mouse::{Click, click};
use iced::advanced::widget::{Operation, Tree, Widget, tree};
use iced::advanced::{Clipboard, Shell, overlay, renderer};
use iced::widget::{button, column, container, mouse_area, row, space, stack};
use iced::window::{self, Direction};
use iced::{Element, Event, Fill, Length, Point, Rectangle, Renderer, Size, Task, Vector, mouse};

use crate::theme;

/// Whether the app draws its own title bar.
pub const CUSTOM: bool = cfg!(target_os = "linux");

/// The width of the invisible resize edges, in pixels.
const GRIP: f32 = 5.0;

#[derive(Debug, Clone, Copy)]
pub enum Action {
    Drag,
    ToggleMaximize,
    Minimize,
    Close,
    Resize(Direction),
}

pub fn perform<T: Send + 'static>(action: Action) -> Task<T> {
    window::latest().and_then(move |id| match action {
        Action::Drag => window::drag(id),
        Action::ToggleMaximize => window::toggle_maximize(id),
        Action::Minimize => window::minimize(id, true),
        Action::Close => window::close(id),
        Action::Resize(direction) => window::drag_resize(id, direction),
    })
}

/// Lets the user drag the title bar, and double-click it to maximize.
pub fn title_bar<'a, M: Clone + 'a>(
    bar: Element<'a, M>,
    on_action: fn(Action) -> M,
) -> Element<'a, M> {
    if !CUSTOM {
        return bar;
    }
    Element::new(TitleBar {
        content: bar,
        on_action,
    })
}

/// The mouse distance that turns a press on the title bar into a window move.
const DRAG_START: f32 = 4.0;

/// Moves the window when the mouse moves with the button down, and toggles
/// maximize on a double click.
///
/// The move starts only when the mouse moves. While the window moves, the
/// system has the mouse, so a move that starts at the press hides the second
/// click of a double click.
struct TitleBar<'a, M, Theme = iced::Theme, R = Renderer> {
    content: Element<'a, M, Theme, R>,
    on_action: fn(Action) -> M,
}

#[derive(Default)]
struct BarState {
    /// Where the button went down, until the mouse moves or the button goes up.
    pressed_at: Option<Point>,
    previous_click: Option<Click>,
}

impl<M, Theme, R: renderer::Renderer> Widget<M, Theme, R> for TitleBar<'_, M, Theme, R> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<BarState>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(BarState::default())
    }

    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.content)]
    }

    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(std::slice::from_ref(&self.content));
    }

    fn size(&self) -> Size<Length> {
        self.content.as_widget().size()
    }

    fn layout(&mut self, tree: &mut Tree, renderer: &R, limits: &layout::Limits) -> layout::Node {
        self.content
            .as_widget_mut()
            .layout(&mut tree.children[0], renderer, limits)
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &R,
        operation: &mut dyn Operation,
    ) {
        self.content
            .as_widget_mut()
            .operate(&mut tree.children[0], layout, renderer, operation);
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &R,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, M>,
        viewport: &Rectangle,
    ) {
        // The buttons in the bar get the event first.
        self.content.as_widget_mut().update(
            &mut tree.children[0],
            event,
            layout,
            cursor,
            renderer,
            clipboard,
            shell,
            viewport,
        );
        let state: &mut BarState = tree.state.downcast_mut();
        if shell.is_event_captured() {
            state.pressed_at = None;
            return;
        }
        match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                let Some(at) = cursor.position_over(layout.bounds()) else {
                    return;
                };
                let click = Click::new(at, mouse::Button::Left, state.previous_click);
                if click.kind() == click::Kind::Double {
                    state.pressed_at = None;
                    state.previous_click = None;
                    shell.publish((self.on_action)(Action::ToggleMaximize));
                } else {
                    state.pressed_at = Some(at);
                    state.previous_click = Some(click);
                }
                shell.capture_event();
            }
            Event::Mouse(mouse::Event::CursorMoved { position }) => {
                if let Some(at) = state.pressed_at
                    && at.distance(*position) > DRAG_START
                {
                    state.pressed_at = None;
                    // A move is not the first click of a double click.
                    state.previous_click = None;
                    shell.publish((self.on_action)(Action::Drag));
                }
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left))
            | Event::Mouse(mouse::Event::CursorLeft) => state.pressed_at = None,
            _ => {}
        }
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &R,
    ) -> mouse::Interaction {
        self.content.as_widget().mouse_interaction(
            &tree.children[0],
            layout,
            cursor,
            viewport,
            renderer,
        )
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut R,
        theme: &Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        self.content.as_widget().draw(
            &tree.children[0],
            renderer,
            theme,
            style,
            layout,
            cursor,
            viewport,
        );
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'b>,
        renderer: &R,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'b, M, Theme, R>> {
        self.content.as_widget_mut().overlay(
            &mut tree.children[0],
            layout,
            renderer,
            viewport,
            translation,
        )
    }
}

/// Minimize, maximize and close.
pub fn controls<'a, M: Clone + 'a>(on_action: fn(Action) -> M) -> Option<Element<'a, M>> {
    if !CUSTOM {
        return None;
    }
    // The marks are drawn shapes, so no image or SVG code is needed.
    let control = |mark: Element<'a, M>, action: Action, close: bool| -> Element<'a, M> {
        button(container(mark).center(16))
            .width(28)
            .height(28)
            .padding(6)
            .style(theme::window_button(close))
            .on_press(on_action(action))
            .into()
    };
    let minimize = container(space().width(10).height(1.5)).style(theme::window_mark_line);
    let maximize = container(space().width(9).height(9)).style(theme::window_mark_box);
    let close = crate::mark::cross(9.0, theme::mark_ink2);
    Some(
        row![
            control(minimize.into(), Action::Minimize, false),
            control(maximize.into(), Action::ToggleMaximize, false),
            control(close, Action::Close, true),
        ]
        .spacing(6)
        .into(),
    )
}

/// Draws a thin border around the window and adds the resize edges.
pub fn frame<'a, M: Clone + 'a>(
    content: Element<'a, M>,
    on_action: fn(Action) -> M,
) -> Element<'a, M> {
    if !CUSTOM {
        return content;
    }
    let grip = |width: Length, height: Length, direction, cursor| -> Element<'a, M> {
        mouse_area(space().width(width).height(height))
            .on_press(on_action(Action::Resize(direction)))
            .interaction(cursor)
            .into()
    };
    use mouse::Interaction::{
        ResizingDiagonallyDown as Down, ResizingDiagonallyUp as Up, ResizingHorizontally as Across,
        ResizingVertically as Along,
    };
    let edge = Length::Fixed(GRIP);
    let grips = column![
        row![
            grip(edge, edge, Direction::NorthWest, Down),
            grip(Fill, edge, Direction::North, Along),
            grip(edge, edge, Direction::NorthEast, Up),
        ],
        row![
            grip(edge, Fill, Direction::West, Across),
            space().width(Fill).height(Fill),
            grip(edge, Fill, Direction::East, Across),
        ]
        .height(Fill),
        row![
            grip(edge, edge, Direction::SouthWest, Up),
            grip(Fill, edge, Direction::South, Along),
            grip(edge, edge, Direction::SouthEast, Down),
        ],
    ];
    stack![container(content).style(theme::window_frame), grips].into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use iced::advanced::clipboard;

    /// Sends `events` to a title bar, and returns the actions that it asks for.
    fn actions(events: &[(Event, Point)], waits: &[u64]) -> Vec<String> {
        let mut bar: TitleBar<'_, Action, (), ()> = TitleBar {
            content: Element::new(iced::widget::Space::new().width(400).height(48)),
            on_action: |a| a,
        };
        let mut tree = Tree::new(&bar as &dyn Widget<Action, (), ()>);
        let node = bar.layout(
            &mut tree,
            &(),
            &layout::Limits::new(Size::ZERO, Size::new(400.0, 48.0)),
        );
        let mut out = Vec::new();
        for (i, (event, at)) in events.iter().enumerate() {
            if let Some(ms) = waits.get(i).filter(|ms| **ms > 0) {
                std::thread::sleep(std::time::Duration::from_millis(*ms));
            }
            let mut messages = Vec::new();
            let mut shell = Shell::new(&mut messages);
            bar.update(
                &mut tree,
                event,
                Layout::new(&node),
                mouse::Cursor::Available(*at),
                &(),
                &mut clipboard::Null,
                &mut shell,
                &Rectangle::new(Point::ORIGIN, Size::new(400.0, 48.0)),
            );
            out.extend(messages.into_iter().map(|a| format!("{a:?}")));
        }
        out
    }

    fn press(at: Point) -> (Event, Point) {
        (
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
            at,
        )
    }
    fn release(at: Point) -> (Event, Point) {
        (
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
            at,
        )
    }
    fn moved(at: Point) -> (Event, Point) {
        (Event::Mouse(mouse::Event::CursorMoved { position: at }), at)
    }

    #[test]
    fn a_double_click_maximizes_and_does_not_move_the_window() {
        let p = Point::new(100.0, 20.0);
        let got = actions(&[press(p), release(p), press(p), release(p)], &[]);
        assert_eq!(got, ["ToggleMaximize"]);
    }

    #[test]
    fn a_press_and_a_move_drag_the_window() {
        let p = Point::new(100.0, 20.0);
        let near = Point::new(102.0, 21.0);
        let far = Point::new(120.0, 24.0);
        let got = actions(&[press(p), moved(near), moved(far), moved(far)], &[]);
        assert_eq!(got, ["Drag"]);
    }

    #[test]
    fn two_slow_clicks_do_nothing() {
        let p = Point::new(100.0, 20.0);
        let got = actions(
            &[press(p), release(p), press(p), release(p)],
            &[0, 0, 350, 0],
        );
        assert!(got.is_empty(), "{got:?}");
    }
}
