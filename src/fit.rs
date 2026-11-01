//! One line of text that ends in "…" if it does not fit.
//!
//! The text widget of iced wraps or goes past its box. Names, paths and SQL in
//! lists and bars must stay on one line and must not cover other controls.
//! This widget measures the text with the real font and cuts it to fit.

use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer;
use iced::advanced::text::{self, Paragraph as _};
use iced::advanced::widget::{Tree, Widget, tree};
use iced::alignment;
use iced::widget::text::{LineHeight, Shaping, Style, Wrapping};
use iced::{Element, Font, Length, Pixels, Rectangle, Size, Theme, mouse};

pub struct Fit {
    content: String,
    size: f32,
    font: Option<Font>,
    style: Option<fn(&Theme) -> Style>,
    width: Length,
    max_width: f32,
}

/// One line of `content`, cut with "…" to the width available.
pub fn fit(content: impl Into<String>) -> Fit {
    Fit {
        content: content.into(),
        size: crate::theme::TEXT_MD,
        font: None,
        style: None,
        width: Length::Shrink,
        max_width: f32::INFINITY,
    }
}

impl Fit {
    pub fn size(mut self, size: f32) -> Self {
        self.size = size;
        self
    }

    pub fn font(mut self, font: Font) -> Self {
        self.font = Some(font);
        self
    }

    pub fn style(mut self, style: fn(&Theme) -> Style) -> Self {
        self.style = Some(style);
        self
    }

    /// `Fill` takes the space that the other widgets leave. `Shrink`, the
    /// default, takes only the space that the text needs.
    pub fn width(mut self, width: impl Into<Length>) -> Self {
        self.width = width.into();
        self
    }

    /// The widest that the text can be, even if there is more space.
    pub fn max_width(mut self, max_width: f32) -> Self {
        self.max_width = max_width;
        self
    }
}

struct State<P> {
    paragraph: P,
    /// The inputs of the last layout, so that unchanged text is not measured again.
    built: Option<(String, f32, f32, Font)>,
}

fn line(content: &str, size: f32, font: Font) -> text::Text<&str, Font> {
    text::Text {
        content,
        bounds: Size::INFINITE,
        size: Pixels(size),
        line_height: LineHeight::default(),
        font,
        align_x: text::Alignment::Default,
        align_y: alignment::Vertical::Top,
        shaping: Shaping::Advanced,
        wrapping: Wrapping::None,
    }
}

/// The width of one line of text, measured with the real font.
pub fn measure(content: &str, size: f32, font: Font) -> f32 {
    use iced::advanced::graphics::text::Paragraph;
    Paragraph::with_text(line(content, size, font)).min_width()
}

/// `content` cut with "…" to `width`, for text that is not in a `Fit` widget.
pub fn cut_to(content: &str, size: f32, font: Font, width: f32) -> String {
    cut::<iced::advanced::graphics::text::Paragraph>(content, size, font, width)
}

/// The text to show in `width`. This is the first line, with " …" if more
/// lines follow. If the line is too wide, it is cut and ends with "…".
fn cut<P: text::Paragraph<Font = Font>>(
    content: &str,
    size: f32,
    font: Font,
    width: f32,
) -> String {
    let fits = |s: &str| P::with_text(line(s, size, font)).min_width() <= width;
    let first = content.lines().next().unwrap_or("");
    let whole = if first.len() < content.trim_end().len() {
        format!("{first} …")
    } else {
        first.to_string()
    };
    if fits(&whole) {
        return whole;
    }
    // The byte offset after each character.
    let ends: Vec<usize> = first
        .char_indices()
        .map(|(i, c)| i + c.len_utf8())
        .collect();
    let candidate = |n: usize| format!("{}…", first[..ends[n - 1]].trim_end());
    // Binary search. `lo` characters fit and `hi` characters do not fit.
    let (mut lo, mut hi) = (0, ends.len());
    while hi - lo > 1 {
        let mid = (lo + hi) / 2;
        if fits(&candidate(mid)) {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    // If not one character fits, show nothing. A "…" alone is confusing.
    if lo == 0 {
        String::new()
    } else {
        candidate(lo)
    }
}

impl<Message, Renderer> Widget<Message, Theme, Renderer> for Fit
where
    Renderer: text::Renderer<Font = Font>,
{
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State<Renderer::Paragraph>>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State::<Renderer::Paragraph> {
            paragraph: Renderer::Paragraph::default(),
            built: None,
        })
    }

    fn size(&self) -> Size<Length> {
        Size::new(self.width, Length::Shrink)
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        let state = tree.state.downcast_mut::<State<Renderer::Paragraph>>();
        let font = self.font.unwrap_or_else(|| renderer.default_font());
        let width = limits.max().width.min(self.max_width);
        let key = (self.content.clone(), width, self.size, font);
        if state.built.as_ref() != Some(&key) {
            let shown = cut::<Renderer::Paragraph>(&self.content, self.size, font, width);
            state.paragraph = Renderer::Paragraph::with_text(line(&shown, self.size, font));
            state.built = Some(key);
        }
        let intrinsic = state.paragraph.min_bounds();
        let size = limits
            .max_width(self.max_width)
            .resolve(self.width, Length::Shrink, intrinsic);
        layout::Node::new(size)
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        defaults: &renderer::Style,
        layout: Layout<'_>,
        _cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        let state = tree.state.downcast_ref::<State<Renderer::Paragraph>>();
        let color = self
            .style
            .and_then(|style| style(theme).color)
            .unwrap_or(defaults.text_color);
        let bounds = layout.bounds();
        renderer.fill_paragraph(&state.paragraph, bounds.position(), color, *viewport);
    }
}

impl<'a, Message: 'a> From<Fit> for Element<'a, Message> {
    fn from(fit: Fit) -> Self {
        Element::new(fit)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use iced::advanced::graphics::text::Paragraph;

    fn width(s: &str) -> f32 {
        Paragraph::with_text(line(s, 14.0, Font::MONOSPACE)).min_width()
    }

    fn cut_to(s: &str, w: f32) -> String {
        cut::<Paragraph>(s, 14.0, Font::MONOSPACE, w)
    }

    #[test]
    fn short_text_is_left_alone() {
        assert_eq!(cut_to("orders", 500.0), "orders");
    }

    #[test]
    fn long_text_ends_in_an_ellipsis_and_fits() {
        let long = "warehouse_inventory_adjustments_by_supplier_and_region";
        let limit = width(long) / 2.0;
        let shown = cut_to(long, limit);
        assert!(shown.ends_with('…'), "{shown}");
        assert!(width(&shown) <= limit);
        assert!(long.starts_with(shown.trim_end_matches('…')));
    }

    #[test]
    fn later_lines_are_marked_not_shown() {
        assert_eq!(cut_to("select 1\nfrom t", 500.0), "select 1 …");
    }

    #[test]
    fn no_room_means_no_text() {
        assert_eq!(cut_to("anything", 1.0), "");
    }

    #[test]
    fn multibyte_text_is_cut_on_character_boundaries() {
        let shown = cut_to("Zürich–Genève–Ålesund–Kraków–Łódź", width("Zürich–Ge"));
        assert!(shown.ends_with('…'), "{shown}");
    }
}
