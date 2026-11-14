# How deep customisation works elsewhere, and what Lumen takes from it

Written 2026-11-14, after M13, at Bryson's request: look over every feature against the apps Lumen draws on, see how they let people make fully custom looks, and decide how Lumen should (a visual way for everyone, and a text way such as YAML for people who want full control).

Sources are listed at the end. Where a tool's current details could not be confirmed, that is said.

## How the reference apps do it

### Rainmeter (desktop skins)

- **A skin is a folder of plain text files.** Edit the file, press refresh, see the change. Everything a skin is can be read, copied and changed by hand. Skins are shared as one `.rmskin` package.
- **Measures and meters are separate.** A *measure* fetches a value (CPU use, the time, a web page) and is never visible. A *meter* draws something (text, an image, a bar, a line graph, a ring, a shape) and can be bound to a measure. One measure can feed many meters.
- **Variables.** A `[Variables]` section holds values (a font name, a size, a colour) that the rest of the skin refers to as `#name#`. Changing one line restyles the whole skin. Many skins put their variables in one shared file so a whole suite follows it.
- **Shared styles.** `MeterStyle` lets several meters share a set of properties ("all labels look like this") and still override one or two.
- **Includes.** `@Include` pulls another file in, mostly used for those shared variables and styles.
- **Actions and conditions.** Any meter can do something when clicked, scrolled or hovered. Measures can change things when a value crosses a threshold (turn the bar red above 90%).
- **Scripting.** Lua, for anything the built-in measures cannot do.
- **What hurts.** The INI syntax is unforgiving and has no editor to speak of; positions are typed as numbers; a careless skin or plugin can leak memory; there is no theme, so every skin has its own hard-coded colours.

### YASB and Zebar (status bars)

- **One YAML file** describes the bar: which monitors, and which widgets sit in the left, centre and right.
- **Looks come from CSS.** YASB reads a `styles.css` beside the config; it can also write the Windows accent colours into a CSS file to import.
- **Text is a template with live values in it.** A label is written like `CPU {cpu.usage}%` rather than chosen from a list. Zebar's templates also allow if/else and loops.
- **Providers with settings.** Zebar's data comes from named providers that take their own options (a weather provider takes a latitude and longitude). Zebar's newer widget-pack format could not be confirmed from the sources found.
- **What hurts.** The config file is the only editor (YASB has since gained a separate GUI app); every change is a save and reload; CSS is powerful but assumes you know CSS.

### Seelen UI (whole desktop)

- **A theme is a folder with a `metadata.yml`** plus CSS/SCSS files, pulled in with an `!include` tag. Themes can also be one file or one compressed package.
- **Themes declare their own settings.** A theme lists options in YAML (a name, a label, a type, a starting value); the app shows them as controls, and the CSS reads them as variables. So the *author* decides what is adjustable, and the *user* adjusts it without touching code.
- **Widgets are small web apps**; themes restyle them by targeting class names.
- **What hurts.** A browser view per surface costs memory; restyling means knowing each widget's class names.

### Wallpaper Engine and Lively

- **Author-defined options** again: a wallpaper exposes sliders, colours and switches in the app.
- **A gallery with one-click install**, and pausing when a fullscreen app is in front.

### matugen, pywal, Material You

- **A palette from an image, written out to other apps** through templates (a terminal, an editor, a browser theme). The colours are the product; everything else reads them.

## Where Lumen stands against each idea

| Idea | Who does it | Lumen today | Gap |
| --- | --- | --- | --- |
| One palette every surface reads | matugen, Material You | Yes: theme roles, used by widgets, bar, flyout, launcher, styles and parts | Not yet written out to *other* apps |
| Data separate from what draws it | Rainmeter (measures / meters), Zebar (providers) | Yes since M13: parts bind to sources (`clock.time`, `stats.cpu`) | Sources are fixed and take no settings; none come from the internet |
| Many kinds of piece | Rainmeter (text, image, bar, line, histogram, ring, shape) | Text, level bar, picture | No shape, divider, ring, graph or icon |
| Live values inside text | YASB, Zebar, Rainmeter | No: a text part is either one live value or fixed text | `CPU {stats.cpu}` cannot be written |
| Shared variables | Rainmeter, Seelen | Partly: theme roles and component styles | No values of the user's own ("my display font", "my big size") shared across widgets |
| Shared text styles | Rainmeter (MeterStyle) | No | Every part repeats its font and size |
| Plain text files, edit and reload | Rainmeter, YASB, Zebar, Seelen | No: settings are JSON written by the app, not meant for hand editing | No readable widget file, no reload on save, no code view |
| A visual editor | (almost nobody: this is the gap in the field) | Property panels (M12, M13); no drag and drop yet | M14 |
| Click actions, hover, scroll | Rainmeter | Only on the bar's built-in modules | Parts cannot do anything |
| Conditions and thresholds | Rainmeter, Zebar templates | No | A bar cannot change colour when a value is high; a part cannot hide when empty |
| Author-defined options | Seelen, Wallpaper Engine | No | A shared widget cannot say "these three things are adjustable" |
| Scripting | Rainmeter (Lua) | Luau scripts with permissions the user agrees to (`docs/SCRIPTS.md`) | Built (M21) |
| Packages and a gallery | Rainmeter (.rmskin), Wallpaper Engine | `.lumen` presets and `.lumenstyle` files | No widget package; no gallery |
| Stays light, safe with games | (the weak point of most) | Yes: governor, game mode, no injection | Keep it that way as power is added |

## What Lumen will do

Lumen's edge over these tools is the combination none of them has: **a real visual editor, and a readable text file, both editing the same thing, all drawn in one theme.** Rainmeter has the files without the editor or the theme; the bars have YAML and CSS without an editor; Seelen has themes but needs a browser per surface.

1. **The visual way first (M14).** Drag parts around a canvas, drop new ones in, restyle from a panel. No file ever needs opening.
2. **Then the text way, on the same data (M15).** Each widget can be saved as a **YAML file**: readable, commentable, and reloaded the moment it is saved. YAML because the bars and Seelen have made it the familiar choice on Windows, it allows comments, and it is far kinder than Rainmeter's INI. The designer writes this same file, so the two never drift, and a "code view" in the app shows it beside the canvas. This replaces the made-up syntax in the plan's "Example widget file".
3. **Borrow Rainmeter's three power tools, in that file:** *variables* (name a value once, use it everywhere), *shared text styles* (a "label" look many parts use), and *includes* (one file of variables and styles a whole set of widgets shares).
4. **Text as a template.** `CPU {stats.cpu}` in any text part. Small, and it lands in M14 because the designer needs it to make new text parts useful.
5. **More kinds of piece**: shapes and dividers in M14; ring, graph and icon with behaviour in M17.
6. **Data sources with settings, including online ones (M16)**, starting with weather.
7. **Behaviour (M17)**: click actions on parts, conditions (`when:`) and thresholds.
8. **Author-defined options and packages**, with the scripted SDK and gallery that follow: a shared widget lists what is adjustable, and Lumen shows controls for it.
9. **Keep colours as roles in every one of these.** A YAML file may name `primary` or `accent`, never `#ff0000`. That rule is what the other tools lack and what makes a shared widget fit anyone's desktop.

What Lumen deliberately does **not** copy: CSS as the styling language (it needs a browser engine per surface, which is the memory cost Lumen exists to avoid); INI; unrestricted native plugins in the main process; hard-coded colours.

### What a widget file looks like (built in M15; the full format is in `crates/lumen-ipc/src/widget_file.rs`)

```yaml
lumen-widget: 1
name: Big clock
size: [260, 120]

variables:
  display: Space Grotesk      # change once, every part using $display follows
  big: 56

styles:                        # shared looks, like Rainmeter's MeterStyle
  caption: { font: Inter, size: 13, color: muted }

parts:
  - id: time
    text: "{clock.time}"
    font: $display
    size: $big
    weight: bold
    color: accent              # a theme role, never a hex colour
    at: [20, 28]
    box: [220, 64]
    stretch: true
  - id: date
    text: "{clock.date}"
    style: caption
    at: [20, 14]
    box: [220, 18]
```

### Options (built in M19)

A file can let its user adjust a few things without opening it. Each option is used like a variable, and each widget on the desktop keeps its own answers:

```yaml
options:
  big: { label: Time size, type: number, default: 40, min: 20, max: 80 }
  show-date: { label: Show the date, type: toggle, default: true }
  tint: { label: Colour, type: color, default: accent }
parts:
  - id: time
    text: "{clock.time}"
    size: $big
    color: $tint
  - id: date
    text: "{clock.date}"
    shown: $show-date
```

## Sources

- Rainmeter forum and community guides on measures, meters, variables, MeterStyle and @Include: https://forum.rainmeter.net/viewtopic.php?p=212506 , https://forum.rainmeter.net/viewtopic.php?p=215974 , https://lewe.gitbook.io/rainmeter-skin-grayhound/grayhound-basics
- YASB (Reborn) configuration and styling: https://github.com/amnweb/yasb/wiki/Configuration , https://github.com/amnweb/yasb/wiki/Styling
- Zebar: https://github.com/LGUG2Z/zebar
- Seelen UI theme, resource and widget guidelines: https://seelen.io/docs/theme-guidelines , https://seelen.io/docs/resource-guidelines , https://seelen.io/docs/widget-guidelines
