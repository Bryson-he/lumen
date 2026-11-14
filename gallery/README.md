# Lumen gallery

Looks, component styles and widgets to share. Lumen's Gallery screen reads `gallery.json` and lists what is in it; "Add to Lumen" downloads the item's file and adds it the same way as a file you pick yourself.

This folder is the whole gallery. Put it online as it is (a GitHub repository works: point Lumen at the raw address of `gallery.json`), or point Lumen at the folder on your PC to try it.

## What is in it

| Folder | Holds | Made in Lumen with |
|---|---|---|
| `widgets/` | `.lumenwidget` packages | Widgets → a widget's Parts tab → "Share it as one file…" |
| `styles/` | `.lumenstyle` component styles | Theme & colours → Component styles → Export |
| `presets/` | `.lumen` looks | Presets → a preset's menu → Export |

## Adding something

1. Make the file in Lumen (see the table) and put it in the matching folder. File names use letters, numbers and dashes.
2. Add an entry to `gallery.json`:

```json
{
  "id": "my-widget",
  "kind": "widget",
  "name": "My widget",
  "author": "Your name",
  "description": "One or two sentences on what it does.",
  "file": "widgets/my-widget.lumenwidget",
  "script": true,
  "permissions": ["network"]
}
```

`kind` is `widget`, `style` or `preset`. `script` and `permissions` are only for widgets that have a script, and only for showing in the list: Lumen reads what a script really asks for from the file itself, shows that before adding the widget, and never runs a script until the person has allowed it.

3. Open a pull request.

## Rules for what is accepted

- A script asks only for the permissions it needs, and its description says why.
- Colours are theme roles, never fixed colours, so everything follows the user's theme.
- Nothing that copies another product's name, artwork or look.
