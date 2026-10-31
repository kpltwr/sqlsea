//! The window's own title bar controls and resize edges, used on Linux.
//!
//! Wayland desktops such as GNOME leave the title bar to the app, and the frame
//! winit draws can only be stock light or dark, so it never matches sqlsea's
//! palette. On Linux the app's top bar becomes the title bar instead. macOS and
//! Windows keep their native title bars, which follow the app's theme.

use iced::widget::{button, column, container, mouse_area, row, space, stack, svg};
use iced::window::{self, Direction};
use iced::{Element, Fill, Length, Task, mouse};

use crate::theme;

/// Whether the app draws its own title bar.
pub const CUSTOM: bool = cfg!(target_os = "linux");

/// Width of the invisible grips along the window's edges.
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

/// Makes a title bar draggable, and maximizable with a double click.
pub fn title_bar<'a, M: Clone + 'a>(
    bar: Element<'a, M>,
    on_action: fn(Action) -> M,
) -> Element<'a, M> {
    if !CUSTOM {
        return bar;
    }
    mouse_area(bar)
        .on_press(on_action(Action::Drag))
        .on_double_click(on_action(Action::ToggleMaximize))
        .into()
}

/// Minimize, maximize and close.
pub fn controls<'a, M: Clone + 'a>(on_action: fn(Action) -> M) -> Option<Element<'a, M>> {
    if !CUSTOM {
        return None;
    }
    let control = |icon: &'static [u8], action: Action, close: bool| -> Element<'a, M> {
        button(
            svg(svg::Handle::from_memory(icon))
                .width(16)
                .height(16)
                .style(theme::window_icon),
        )
        .width(28)
        .height(28)
        .padding(6)
        .style(theme::window_button(close))
        .on_press(on_action(action))
        .into()
    };
    Some(
        row![
            control(MINIMIZE, Action::Minimize, false),
            control(MAXIMIZE, Action::ToggleMaximize, false),
            control(CLOSE, Action::Close, true),
        ]
        .spacing(6)
        .into(),
    )
}

/// Draws a hairline around the window and adds resize grips at its edges.
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

const MINIMIZE: &[u8] = br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16"><path d="M4 8.5h8" stroke="black" stroke-width="1.3" stroke-linecap="round"/></svg>"#;
const MAXIMIZE: &[u8] = br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16"><rect x="4.5" y="4.5" width="7" height="7" rx="1" fill="none" stroke="black" stroke-width="1.3"/></svg>"#;
pub const CLOSE: &[u8] = br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16"><path d="M5 5l6 6M11 5l-6 6" stroke="black" stroke-width="1.3" stroke-linecap="round"/></svg>"#;
