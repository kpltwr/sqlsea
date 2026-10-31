//! Every colour, font and size in the app goes through this file.
//!
//! Colours come in a light and a dark set; style functions pick the set that
//! matches the active `Theme`, so views never name a colour directly.

use iced::border::{self, Radius};
use iced::font::{Family, Weight};
use iced::widget::{button, container, scrollable, text, text_editor, text_input};
use iced::{Background, Border, Color, Font, Shadow, Theme, Vector};

const fn rgb(r: u8, g: u8, b: u8) -> Color {
    Color::from_rgb8(r, g, b)
}

pub struct Colors {
    // Ground and text: cool neutrals, blue-grey hairlines.
    pub paper: Color,
    pub paper2: Color,
    pub paper3: Color,
    pub ink: Color,
    pub ink2: Color,
    pub muted: Color,
    pub rule: Color,
    pub rule2: Color,
    // The SQL editor.
    pub graphite: Color,
    pub graphite_ink: Color,
    pub graphite_muted: Color,
    pub graphite_select: Color,
    // The one signal.
    pub accent: Color,
    pub accent2: Color,
    pub accent_soft: Color,
    pub accent_wash: Color,
    /// Text on the wash.
    pub on_wash: Color,
    pub accent_press: Color,
    pub accent_edge: Color,
    pub accent_ink: Color,
    // Keycaps.
    pub key_face: Color,
    pub key_hover: Color,
    pub key_edge: Color,
    // State.
    pub error: Color,
    pub error_solid: Color,
    pub error_wash: Color,
    pub error_edge: Color,
    pub ok: Color,
    pub shadow: Color,
}

pub const LIGHT: Colors = Colors {
    paper: rgb(0xf8, 0xfb, 0xfe),
    paper2: rgb(0xef, 0xf3, 0xf8),
    paper3: rgb(0xe3, 0xe8, 0xef),
    ink: rgb(0x11, 0x18, 0x25),
    ink2: rgb(0x36, 0x3e, 0x49),
    muted: rgb(0x5d, 0x64, 0x6e),
    rule: rgb(0xda, 0xde, 0xe4),
    rule2: rgb(0xc2, 0xc8, 0xcf),
    graphite: rgb(0x12, 0x18, 0x23),
    graphite_ink: rgb(0xe3, 0xe8, 0xee),
    graphite_muted: rgb(0x93, 0x9c, 0xa9),
    graphite_select: rgb(0x2a, 0x3d, 0x5e),
    accent: rgb(0x04, 0x62, 0xd3),
    accent2: rgb(0x75, 0xb4, 0xff),
    accent_soft: rgb(0xb4, 0xcc, 0xee),
    accent_wash: rgb(0xe3, 0xf0, 0xff),
    on_wash: rgb(0x00, 0x40, 0xbb),
    accent_press: rgb(0x00, 0x40, 0xbb),
    accent_edge: rgb(0x00, 0x3d, 0x98),
    accent_ink: rgb(0xf8, 0xfb, 0xfe),
    key_face: rgb(0xfc, 0xfd, 0xff),
    key_hover: rgb(0xff, 0xff, 0xff),
    key_edge: rgb(0xb7, 0xbe, 0xc8),
    error: rgb(0xc9, 0x30, 0x2d),
    error_solid: rgb(0xc9, 0x30, 0x2d),
    error_wash: rgb(0xff, 0xec, 0xe9),
    error_edge: rgb(0x9a, 0x1f, 0x1d),
    ok: rgb(0x18, 0x7c, 0x49),
    shadow: Color::from_rgba8(0x11, 0x18, 0x25, 0.06),
};

pub const DARK: Colors = Colors {
    paper: rgb(0x10, 0x15, 0x1d),
    paper2: rgb(0x15, 0x1b, 0x25),
    paper3: rgb(0x1e, 0x25, 0x31),
    ink: rgb(0xe6, 0xeb, 0xf1),
    ink2: rgb(0xc0, 0xc7, 0xd1),
    muted: rgb(0x8e, 0x97, 0xa4),
    rule: rgb(0x25, 0x2d, 0x3a),
    rule2: rgb(0x35, 0x3f, 0x4e),
    graphite: rgb(0x0a, 0x0e, 0x14),
    graphite_ink: rgb(0xe3, 0xe8, 0xee),
    graphite_muted: rgb(0x80, 0x89, 0x97),
    graphite_select: rgb(0x24, 0x40, 0x6b),
    accent: rgb(0x2b, 0x6f, 0xdc),
    accent2: rgb(0x75, 0xb4, 0xff),
    accent_soft: rgb(0xb4, 0xcc, 0xee),
    accent_wash: rgb(0x17, 0x2a, 0x4b),
    on_wash: rgb(0x9c, 0xc6, 0xff),
    accent_press: rgb(0x22, 0x5c, 0xbd),
    accent_edge: rgb(0x14, 0x3a, 0x78),
    accent_ink: rgb(0xff, 0xff, 0xff),
    key_face: rgb(0x1b, 0x22, 0x2d),
    key_hover: rgb(0x22, 0x2a, 0x37),
    key_edge: rgb(0x05, 0x08, 0x0d),
    error: rgb(0xff, 0x7a, 0x70),
    error_solid: rgb(0xc7, 0x3a, 0x31),
    error_wash: rgb(0x2c, 0x16, 0x18),
    error_edge: rgb(0x7e, 0x24, 0x1f),
    ok: rgb(0x4c, 0xc3, 0x8a),
    shadow: Color::from_rgba8(0x00, 0x00, 0x00, 0.35),
};

/// The colour set for the active theme.
pub fn colors(theme: &Theme) -> &'static Colors {
    if theme.extended_palette().is_dark {
        &DARK
    } else {
        &LIGHT
    }
}

pub fn app_theme(dark: bool) -> Theme {
    let c = if dark { &DARK } else { &LIGHT };
    Theme::custom(
        if dark { "Cobalt dark" } else { "Cobalt" }.to_string(),
        iced::theme::Palette {
            background: c.paper,
            text: c.ink,
            primary: c.accent,
            success: c.ok,
            warning: c.error,
            danger: c.error,
        },
    )
}

// Faces: Familjen Grotesk for heads and labels, IBM Plex Sans for UI text,
// JetBrains Mono for SQL and machine values.
pub const BODY: Font = Font::with_name("IBM Plex Sans");
pub const BODY_MEDIUM: Font = Font {
    weight: Weight::Medium,
    ..BODY
};
pub const DISPLAY: Font = Font {
    family: Family::Name("Familjen Grotesk"),
    weight: Weight::Semibold,
    ..Font::DEFAULT
};
pub const LABEL: Font = Font {
    weight: Weight::Medium,
    ..DISPLAY
};
pub const MONO: Font = Font::with_name("JetBrains Mono");

pub const FONTS: [&[u8]; 5] = [
    include_bytes!("../assets/fonts/IBMPlexSans-400.ttf"),
    include_bytes!("../assets/fonts/IBMPlexSans-500.ttf"),
    include_bytes!("../assets/fonts/FamiljenGrotesk-500.ttf"),
    include_bytes!("../assets/fonts/FamiljenGrotesk-600.ttf"),
    include_bytes!("../assets/fonts/JetBrainsMono-Regular.ttf"),
];

/// The app icon, drawn in the top bar.
pub const ICON_SVG: &[u8] = include_bytes!("../brand/svg/icon.svg");

// Type scale, ratio about 1.125.
pub const TEXT_XS: f32 = 12.0;
pub const TEXT_SM: f32 = 13.0;
pub const TEXT_MD: f32 = 14.0;
pub const TEXT_LG: f32 = 16.0;
pub const TEXT_XL: f32 = 20.0;

// Mono advance at TEXT_SM, for sizing grid columns.
pub const MONO_ADVANCE: f32 = 7.85;

pub const RADIUS_INPUT: f32 = 6.0;
pub const RADIUS_PANEL: f32 = 8.0;
pub const CONTROL_H: f32 = 32.0;

fn hair(color: Color, radius: impl Into<Radius>) -> Border {
    Border {
        color,
        width: 1.0,
        radius: radius.into(),
    }
}

fn faded(c: Color) -> Color {
    Color { a: c.a * 0.55, ..c }
}

fn edge(color: Color, y: f32) -> Shadow {
    Shadow {
        color,
        offset: Vector::new(0.0, y),
        blur_radius: 0.0,
    }
}

// ---------- text ----------

fn tinted(color: Color) -> text::Style {
    text::Style { color: Some(color) }
}

pub fn text_ink(theme: &Theme) -> text::Style {
    tinted(colors(theme).ink)
}

pub fn text_ink2(theme: &Theme) -> text::Style {
    tinted(colors(theme).ink2)
}

pub fn text_muted(theme: &Theme) -> text::Style {
    tinted(colors(theme).muted)
}

/// Cobalt text: the solid accent on light ground, the lighter one on dark.
pub fn text_accent(theme: &Theme) -> text::Style {
    let c = colors(theme);
    tinted(if theme.extended_palette().is_dark {
        c.accent2
    } else {
        c.accent
    })
}

pub fn text_ok(theme: &Theme) -> text::Style {
    tinted(colors(theme).ok)
}

pub fn text_error(theme: &Theme) -> text::Style {
    tinted(colors(theme).error)
}

// ---------- containers ----------

fn fill(color: Color) -> container::Style {
    container::Style {
        background: Some(color.into()),
        ..Default::default()
    }
}

pub fn bar(theme: &Theme) -> container::Style {
    fill(colors(theme).paper)
}

pub fn sidebar(theme: &Theme) -> container::Style {
    let c = colors(theme);
    container::Style {
        text_color: Some(c.ink),
        ..fill(c.paper2)
    }
}

pub fn panel(theme: &Theme) -> container::Style {
    let c = colors(theme);
    container::Style {
        border: hair(c.rule, RADIUS_PANEL),
        shadow: Shadow {
            color: c.shadow,
            offset: Vector::new(0.0, 1.0),
            blur_radius: 2.0,
        },
        ..fill(c.paper)
    }
}

pub fn rule_h(theme: &Theme) -> container::Style {
    fill(colors(theme).rule)
}

pub fn accent_bar(theme: &Theme) -> container::Style {
    fill(colors(theme).accent)
}

pub fn tray(theme: &Theme) -> container::Style {
    let c = colors(theme);
    container::Style {
        border: hair(c.rule, RADIUS_INPUT + 2.0),
        ..fill(c.paper3)
    }
}

pub fn graphite(theme: &Theme) -> container::Style {
    let c = colors(theme);
    container::Style {
        text_color: Some(c.graphite_ink),
        ..fill(c.graphite)
    }
}

pub fn grid_head(theme: &Theme) -> container::Style {
    let c = colors(theme);
    container::Style {
        text_color: Some(c.muted),
        ..fill(c.paper2)
    }
}

pub fn error_note(theme: &Theme) -> container::Style {
    let c = colors(theme);
    container::Style {
        text_color: Some(c.error),
        border: hair(Color { a: 0.3, ..c.error }, RADIUS_INPUT),
        ..fill(c.error_wash)
    }
}

pub fn skeleton(theme: &Theme) -> container::Style {
    container::Style {
        border: border::rounded(3),
        ..fill(colors(theme).paper3)
    }
}

// ---------- buttons ----------

/// A keycap: key face on a hard bottom edge that sinks when pressed.
pub fn key(theme: &Theme, status: button::Status) -> button::Style {
    let c = colors(theme);
    let base = button::Style {
        background: Some(c.key_face.into()),
        text_color: c.ink,
        border: hair(c.rule2, RADIUS_INPUT),
        shadow: edge(c.key_edge, 2.0),
        snap: true,
    };
    match status {
        button::Status::Active => base,
        button::Status::Hovered => button::Style {
            background: Some(c.key_hover.into()),
            border: hair(c.key_edge, RADIUS_INPUT),
            shadow: edge(c.key_edge, 3.0),
            ..base
        },
        button::Status::Pressed => button::Style {
            background: Some(c.paper2.into()),
            shadow: Shadow::default(),
            ..base
        },
        button::Status::Disabled => button::Style {
            background: Some(faded(c.key_face).into()),
            text_color: faded(c.ink),
            border: hair(faded(c.rule2), RADIUS_INPUT),
            shadow: Shadow::default(),
            ..base
        },
    }
}

/// The one solid cobalt key per view.
pub fn key_primary(theme: &Theme, status: button::Status) -> button::Style {
    let c = colors(theme);
    let base = button::Style {
        background: Some(c.accent.into()),
        text_color: c.accent_ink,
        border: hair(c.accent_edge, RADIUS_INPUT),
        shadow: edge(c.accent_edge, 2.0),
        snap: true,
    };
    match status {
        button::Status::Active => base,
        button::Status::Hovered => button::Style {
            shadow: edge(c.accent_edge, 3.0),
            ..base
        },
        button::Status::Pressed => button::Style {
            background: Some(c.accent_press.into()),
            shadow: Shadow::default(),
            ..base
        },
        // Cobalt is spent only on what can be pressed; a dormant primary is a plain key.
        button::Status::Disabled => key(theme, status),
    }
}

/// The armed second step of a destructive action.
pub fn key_danger_armed(theme: &Theme, status: button::Status) -> button::Style {
    let c = colors(theme);
    let base = button::Style {
        background: Some(c.error_solid.into()),
        text_color: c.accent_ink,
        border: hair(c.error_edge, RADIUS_INPUT),
        shadow: edge(c.error_edge, 2.0),
        snap: true,
    };
    match status {
        button::Status::Hovered => button::Style {
            shadow: edge(c.error_edge, 3.0),
            ..base
        },
        button::Status::Pressed => button::Style {
            shadow: Shadow::default(),
            ..base
        },
        _ => base,
    }
}

/// A destructive key: same keycap, error ink.
pub fn key_danger(theme: &Theme, status: button::Status) -> button::Style {
    let c = colors(theme);
    button::Style {
        text_color: if status == button::Status::Disabled {
            faded(c.error)
        } else {
            c.error
        },
        ..key(theme, status)
    }
}

/// A key on the sunken tray; the chosen one is pressed in.
pub fn tray_key(chosen: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |theme, status| {
        let c = colors(theme);
        if chosen {
            button::Style {
                background: Some(c.key_face.into()),
                text_color: c.ink,
                border: hair(c.rule2, RADIUS_INPUT),
                shadow: Shadow {
                    color: c.shadow,
                    offset: Vector::new(0.0, 1.0),
                    blur_radius: 2.0,
                },
                snap: true,
            }
        } else {
            let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
            button::Style {
                background: None,
                text_color: if hovered { c.ink } else { c.muted },
                border: hair(Color::TRANSPARENT, RADIUS_INPUT),
                shadow: Shadow::default(),
                snap: true,
            }
        }
    }
}

/// Quiet text-only control: tabs, list rows, inline actions.
pub fn ghost(theme: &Theme, status: button::Status) -> button::Style {
    let c = colors(theme);
    let base = button::Style {
        background: None,
        text_color: c.ink2,
        border: hair(Color::TRANSPARENT, RADIUS_INPUT),
        shadow: Shadow::default(),
        snap: true,
    };
    match status {
        button::Status::Active => base,
        button::Status::Hovered => button::Style {
            background: Some(c.paper3.into()),
            text_color: c.ink,
            ..base
        },
        button::Status::Pressed => button::Style {
            background: Some(c.rule.into()),
            text_color: c.ink,
            ..base
        },
        button::Status::Disabled => button::Style {
            text_color: faded(c.muted),
            ..base
        },
    }
}

/// A row in a list (sidebar table, saved connection). Selection is the cobalt wash.
pub fn list_row(selected: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |theme, status| {
        let c = colors(theme);
        if selected {
            button::Style {
                background: Some(c.accent_wash.into()),
                text_color: c.on_wash,
                border: hair(Color::TRANSPARENT, RADIUS_INPUT),
                shadow: Shadow::default(),
                snap: true,
            }
        } else {
            ghost(theme, status)
        }
    }
}

/// A data row in the grid: square, full-bleed, wash when chosen.
pub fn grid_row(selected: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |theme, status| {
        let c = colors(theme);
        let bg = if selected {
            c.accent_wash
        } else {
            match status {
                button::Status::Hovered | button::Status::Pressed => c.paper2,
                _ => c.paper,
            }
        };
        button::Style {
            background: Some(bg.into()),
            text_color: c.ink,
            border: Border::default(),
            shadow: Shadow::default(),
            snap: true,
        }
    }
}

/// Column head in the grid: clickable to sort.
pub fn grid_head_button(theme: &Theme, status: button::Status) -> button::Style {
    let c = colors(theme);
    button::Style {
        background: Some(
            match status {
                button::Status::Hovered | button::Status::Pressed => c.paper3,
                _ => c.paper2,
            }
            .into(),
        ),
        text_color: c.muted,
        border: Border::default(),
        shadow: Shadow::default(),
        snap: true,
    }
}

/// The small NULL toggle beside a field.
pub fn null_toggle(on: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |theme, status| {
        let c = colors(theme);
        let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
        button::Style {
            background: Some(
                if on {
                    c.accent_wash
                } else if hovered {
                    c.paper3
                } else {
                    Color::TRANSPARENT
                }
                .into(),
            ),
            text_color: if on { c.on_wash } else { c.muted },
            border: hair(if on { c.accent2 } else { c.rule }, 4.0),
            shadow: Shadow::default(),
            snap: true,
        }
    }
}

// ---------- inputs ----------

/// Fields keep a constant 1px border; focus turns it cobalt.
pub fn field(theme: &Theme, status: text_input::Status) -> text_input::Style {
    let c = colors(theme);
    let base = text_input::Style {
        background: Background::Color(c.key_face),
        border: hair(c.rule2, RADIUS_INPUT),
        icon: c.muted,
        placeholder: c.muted,
        value: c.ink,
        selection: Color { a: 0.3, ..c.accent },
    };
    match status {
        text_input::Status::Active => base,
        text_input::Status::Hovered => text_input::Style {
            border: hair(c.key_edge, RADIUS_INPUT),
            ..base
        },
        text_input::Status::Focused { .. } => text_input::Style {
            border: hair(c.accent, RADIUS_INPUT),
            ..base
        },
        text_input::Status::Disabled => text_input::Style {
            background: Background::Color(c.paper2),
            value: faded(c.ink),
            border: hair(faded(c.rule2), RADIUS_INPUT),
            ..base
        },
    }
}

/// A field whose value differs from what the database holds.
pub fn field_changed(theme: &Theme, status: text_input::Status) -> text_input::Style {
    let c = colors(theme);
    let s = field(theme, status);
    match status {
        text_input::Status::Focused { .. } => s,
        _ => text_input::Style {
            border: hair(c.accent2, RADIUS_INPUT),
            background: Background::Color(c.accent_wash),
            ..s
        },
    }
}

/// A multi-line field styled like its single-line siblings.
pub fn as_editor(
    field: fn(&Theme, text_input::Status) -> text_input::Style,
    theme: &Theme,
    status: text_editor::Status,
) -> text_editor::Style {
    let s = field(
        theme,
        match status {
            text_editor::Status::Active => text_input::Status::Active,
            text_editor::Status::Hovered => text_input::Status::Hovered,
            text_editor::Status::Focused { is_hovered } => {
                text_input::Status::Focused { is_hovered }
            }
            text_editor::Status::Disabled => text_input::Status::Disabled,
        },
    );
    text_editor::Style {
        background: s.background,
        border: s.border,
        placeholder: s.placeholder,
        value: s.value,
        selection: s.selection,
    }
}

pub fn sql_editor(theme: &Theme, status: text_editor::Status) -> text_editor::Style {
    let c = colors(theme);
    text_editor::Style {
        background: Background::Color(c.graphite),
        border: Border {
            color: match status {
                text_editor::Status::Focused { .. } => c.graphite_select,
                _ => c.graphite,
            },
            width: 1.0,
            radius: 0.0.into(),
        },
        placeholder: c.graphite_muted,
        value: c.graphite_ink,
        selection: c.graphite_select,
    }
}

pub fn scroller(theme: &Theme, status: scrollable::Status) -> scrollable::Style {
    let c = colors(theme);
    let dragged = matches!(status, scrollable::Status::Dragged { .. });
    let rail = scrollable::Rail {
        background: None,
        border: Border::default(),
        scroller: scrollable::Scroller {
            background: Background::Color(if dragged { c.muted } else { c.rule2 }),
            border: border::rounded(4),
        },
    };
    scrollable::Style {
        container: container::Style::default(),
        vertical_rail: rail,
        horizontal_rail: rail,
        gap: None,
        auto_scroll: scrollable::default(theme, status).auto_scroll,
    }
}
