# Home Assistant

Lumen can show anything Home Assistant knows about on a widget, and switch or run things with a click.

## Connecting

1. In Lumen, open **Settings** and find **Home Assistant**.
2. Type the address you use to open Home Assistant in a browser, such as `http://homeassistant.local:8123`.
3. Choose "Open that page". In Home Assistant, under "Long-lived access tokens", create a token and copy all of it straight away (it is shown only once).
4. Paste it into "Access token" and choose **Connect**. Lumen says "Connected to …" when it works.

If Lumen says Home Assistant did not accept the token, the token is wrong, incomplete, or has been deleted: make a new one. Lumen tries a refused token once and then stops until you change it, because Home Assistant logs every refused attempt.

The token is kept in a file in your Lumen folder on this PC. It is never put in a widget, a preset, a shared package or anything sent to GitHub.

## Adding something

On the **Widgets** screen choose **Home Assistant…**. Search the list and pick a thing:

- something you can switch (a light, plug, fan, lock, cover…) becomes a small switch widget: click it to switch it;
- a scene, script or button becomes a "click to run" widget;
- anything else (a sensor, a person, the weather) becomes a reading.

The new widget is an ordinary widget file, so you can restyle it, move its parts, or add more parts like any other.

## Using Home Assistant in any widget

Any part of any widget can use Home Assistant. Replace `light.desk` with the entity id Home Assistant lists.

| Write | To get |
|---|---|
| `{ha.light.desk}` | Its state as you would read it: "On", "21.5 °C", "Not home" |
| `{ha.light.desk.name}` | Its name |
| `{ha.light.desk.state}` | Its raw state: `on`, `21.5` |
| `{ha.light.desk.unit}` | Its unit, if it has one |
| `{ha.light.desk.brightness}` | Any attribute Home Assistant reports for it |
| `when: ha.light.desk.on` | Show a part only while it is on (or open, unlocked, home, playing) |
| `alert-when: ha.sensor.office_temperature > 26` | Change a part's colour past a number |
| `click: ha.toggle light.desk` | Switch it, or run it if it is a scene, script or button |

A bar, ring, dial or blocks part whose source is a percentage (a battery, a humidity sensor: `source: ha.sensor.phone_battery`) fills by that much.

In the app, a part's "When clicked" has "Switch or run something in Home Assistant".

## What Lumen asks, and when

Lumen asks Home Assistant only about entities that a widget on screen mentions, every 10 seconds, and not at all while a fullscreen game or app is in front. A click is sent straight away, and the widget shows the likely result at once.

If Home Assistant cannot be reached, Lumen tries again less and less often, up to every five minutes.

## Doing more

For anything this does not cover (a service with extra settings, several calls in a row), a widget script can call Home Assistant's API directly: see [SCRIPTS.md](SCRIPTS.md).
