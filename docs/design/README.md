# Lumen — approved design files

These are the approved screen designs for Lumen, in "Direction D: clean modern with pencil accents".
They were made in a design tool, so they are **reference mock-ups, not app code**.

| File | Screen |
| --- | --- |
| `home-light.dc.html` | Home, light mode |
| `home-dark.dc.html` | Home, dark mode |
| `theme-dark.dc.html` | Theme & colours, dark mode (variant buttons swap the palette — see the script at the bottom) |
| `wallpapers-dark.dc.html` | Wallpapers library, dark mode |
| `widgets-dark.dc.html` | Widgets screen with desktop edit mode, dark mode |

How to read them:

- All layout and colours are inline `style="…"` attributes: copy sizes, colours, radii and spacing from there.
- `{{accent}}` means the **pencil accent colour** token (`#2F5BEA` light, `#8EA6FF` dark).
- Other `{{…}}` holes and `<sc-for>` / `<sc-if>` tags are the mock-up tool's templating; the values they use are in the `<script type="text/x-dc">` block at the bottom of each file.
- `support.js` is the design tool's runtime and is not included, so the files will not render on their own. Read the source.
- Light versions of Theme, Wallpapers and Widgets were not drawn: build them by swapping the dark tokens for the light ones in `docs/PLAN.md` → "Visual design".

Live canvas (needs a claude.ai login): https://claude.ai/artifact/FyPxse9HxhhQYqfqXS6xub
