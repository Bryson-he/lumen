# Scripted widgets

A widget file can name a script. The script works values out (from a file, a web page, a program, or just sums) and the widget's parts show them. Scripts are written in [Luau](https://luau.org), the small language Roblox maintains (Lua with a few additions).

## A first script

Two files in the widgets folder (Widgets → "Open the folder").

`dice.yaml`:

```yaml
lumen-widget: 1
name: Dice
size: [200, 90]
script: dice.luau        # a .luau file in this same folder
every: 10                # seconds between runs (5 if left out; 1 to 3600)
parts:
  - id: roll
    text: "You rolled {script.roll}"
    at: [16, 16]
    box: [168, 24]
  - id: again
    text: "Roll again"
    color: muted
    click: script        # a click on this part is passed to the script
    at: [16, 48]
    box: [168, 20]
```

`dice.luau`:

```lua
local roll = math.random(1, 6)

function update()            -- called every `every` seconds
  return { roll = roll }     -- shown by parts as {script.roll}
end

function click(part)         -- `part` is the id of the part that was clicked
  roll = math.random(1, 6)
end
```

- `update()` returns a table. Each named value in it (text, a number, or true/false) can be shown as `{script.name}`, used as a part's `source`, or tested in `when:` and `alert-when:` (`script.roll > 4`).
- `click(part)` is optional. After it, `update()` is called again straight away, so the widget shows what the click did.
- The script keeps its own variables between calls for as long as it runs.
- A value the script has not handed back shows as nothing, not as the braces.

## Allowing a script

A script does not run until you allow it in Lumen (Widgets screen; a card appears at the top, and under the widget's Parts tab). The card shows what the script asks for and lets you read it. Until then its widget says "Script waiting for your OK in Lumen".

Allowing is for that exact script text with those exact permissions. If either changes, by you or by anyone else, the script stops and Lumen asks again. "Stop allowing" stops it.

A script that arrives in a shared `.lumenwidget` file is never allowed by the import.

## Asking for more: permissions

By default a script can only work with Lumen's own live values. To do more, the widget file lists what it needs:

```yaml
permissions: [files, network, run]
```

| Permission | What the script can then do |
|---|---|
| `files` | Read and write files, list folders, anywhere you can |
| `network` | Fetch `https://` addresses |
| `run` | Run programs and read what they print; open programs, files and web pages |

With these a script can do anything you can do on the PC, which is the point, and also why Lumen asks first. Only allow scripts from people you trust.

## The `lumen` library

Always available:

| Call | Gives |
|---|---|
| `lumen.value("stats.cpu")` | One of Lumen's live values as text (`clock.*`, `stats.*`, `media.*`, `weather.*`), or `nil` |
| `lumen.time()` | Seconds since 1970, with fractions |
| `lumen.log("text")` | Writes a line to Lumen's log |

With `files`:

| Call | Gives |
|---|---|
| `lumen.read_file(path)` | The file's text (up to 4 MB) |
| `lumen.write_file(path, text)` | Writes the file, replacing it |
| `lumen.list_folder(path)` | The names in a folder |

With `network`:

| Call | Gives |
|---|---|
| `lumen.http_get("https://…")` | The page's text. Waits up to 10 seconds |

With `run`:

| Call | Gives |
|---|---|
| `lumen.run("program", { "argument", … })` | What the program printed. No window is shown; stopped after 15 seconds |
| `lumen.open("path or https://…")` | Opens it as a double-click would; `true` if it worked |

A call the widget has no permission for fails with a message saying which permission to add. Any failure (a missing file, no connection) stops that run with an error; use `pcall` to carry on instead:

```lua
local ok, page = pcall(lumen.http_get, "https://example.com/")
if not ok then return { status = "offline" } end
```

## Limits

- Each call into a script may compute for 2 seconds; an endless loop is stopped and reported. Waiting on a download or a program has its own limits above.
- A script's machine may hold 32 MB.
- `update()` may hand back up to 64 values of up to 400 characters each.
- A script file may be up to 256 KB.
- A script that fails is tried again less and less often (up to every 5 minutes) until it works or its text changes.
- Luau's own sandbox is on: there is no `io`, no `os.execute`, no `loadstring`, no `require`. Everything outside goes through `lumen`.

## When scripts run

Each allowed script has a thread of its own, so a slow script never holds up drawing. Scripts only run while a widget that uses them is on screen, and not while a fullscreen app or game is in front. What a script costs is counted against its widget like any other work.

What went wrong, if anything, is shown at the foot of the widget and on the script's card in Lumen.
