# sqlsea brand kit

![Brand sheet](png/sheet-1600.png)

## The mark

The mark is a wave above two table rows. The wave is the sea and the rows are the data. The mark is on a blue tile with two cut corners and a darker bottom edge, like the buttons in the app.

There are two drawings of the app icon. Use `icon.svg` at 48 px and larger. Use `icon-small.svg` from 16 px to 32 px. It has one row less and thicker lines, so that it stays clear at small sizes.

## Files

Edit the files in `source/`. Then run this command to make all files again:

```sh
cargo run --example brand
```

The command writes the SVG files to `svg/`, with the text changed to outlines. These files look correct on computers that do not have the fonts. The command also writes the PNG files to `png/`, and the icon files `icon.ico` (Windows) and `icon.icns` (macOS).

| Asset | Use | Files |
| --- | --- | --- |
| App icon | Launchers, the window, the docs | `svg/icon.svg`, `png/icon-16.png` to `png/icon-1024.png`, `icon.ico`, `icon.icns` |
| Lockup | The README, websites, slides | `svg/lockup.svg`, `png/lockup-528.png`, `png/lockup-1056.png` |
| Lockup for dark backgrounds | Dark backgrounds | `svg/lockup-dark.svg` and the PNG files |
| One-color lockup | Print with one ink | `svg/lockup-mono.svg` and the PNG files |
| Wordmark | Places where the icon is near | `svg/wordmark.svg` and the PNG files |
| Mark | The mark without its tile, on light backgrounds | `svg/mark.svg`, `svg/mark-mono.svg` and the PNG files |
| Social preview | The repository card and link previews | `png/social-1280.png` |
| Brand sheet | All of the brand on one page | `png/sheet-1600.png` |

## Rules for use

Keep a clear space around the mark on all sides. The clear space is the height of one table row of the mark, about one seventh of the icon.

Do not make the lockup narrower than 96 px. Do not make the icon smaller than 16 px, and use the small drawing below 48 px.

Write the name in lower case, `sqlsea`, in Familjen Grotesk 600. The letter spacing is −0.028 em.

The tile is always blue, on light and on dark backgrounds. Use the one-color lockup only if you cannot print in color.

Do not do these things:

1. Do not change the color of the tile. Do not put the blue tile on a blue background.
2. Do not add gradients, shadows or outlines to the mark.
3. Do not stretch or turn the mark, and do not round the cut corners.
4. Do not set the name in a different font, in capital letters, or below the icon.

## Colors

The blue accent marks only the primary action, the selection and state. The other colors are neutral, with a little blue in them.

| Name | Hex | Use |
| --- | --- | --- |
| Cobalt | `#0462d3` | The primary action, the selection, the tile |
| Cobalt edge | `#003d98` | The bottom edge of buttons |
| Wash | `#e3f0ff` | The selected row, changed fields |
| Sky | `#75b4ff` | SQL keywords in the editor |
| Graphite | `#121823` | The SQL editor, dark backgrounds |
| Graphite ink | `#e3e8ee` | Text on graphite |
| Error | `#c9302d` | Errors only |
| Paper | `#f8fbfe` | The background |
| Paper 2 | `#eff3f8` | The sidebar, table heads |
| Paper 3 | `#e3e8ef` | Option groups, hover |
| Rule | `#dadee4` | Thin lines |
| Ink | `#111825` | Text |
| Ink 2 | `#363e49` | Labels |
| Muted | `#5d646e` | Less important text |

These are the light colors. The app also has a dark set. Both sets are in `src/theme.rs`. If you change a color, change it in `src/theme.rs` and in `source/`.

## Fonts

Familjen Grotesk 600 is for the name and headings, and 500 is for labels. IBM Plex Sans 400 and 500 are for other text. JetBrains Mono is for SQL and data, never for labels.

The three fonts have the SIL Open Font License. The font files and the licenses are in `assets/fonts`.
