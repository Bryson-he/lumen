//! Component styles: how the built-in pieces (widget cards, the bar,
//! flyouts, the launcher) are shaped and which theme role each of their
//! colours comes from. The shape of `styles.json` and of the style messages
//! between the core and the settings app.
//!
//! A style never holds a colour, only the *name of a theme role*, so a style
//! made by the user recolours with whatever theme is active.

use serde::{Deserialize, Serialize};

/// The most styles of the user's own that are kept.
pub const MAX_STYLES: usize = 40;
pub const RADIUS_MAX: u8 = 60;
pub const BORDER_MAX: u8 = 4;
/// Text size and spacing multipliers, in percent.
pub const TEXT_SCALE_RANGE: (u8, u8) = (85, 125);
pub const SPACING_RANGE: (u8, u8) = (70, 150);
pub const OPACITY_RANGE: (u8, u8) = (20, 100);
/// The fonts shipped with Lumen that a style may ask for.
pub const FONTS: [&str; 2] = ["Inter", "Space Grotesk"];
/// The id of the style that reproduces the theme's own look.
pub const DEFAULT_STYLE: &str = "default";

/// A colour of the active theme.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Role {
    Primary,
    Secondary,
    Tertiary,
    Neutral,
    Surface,
    OnSurface,
    Outline,
    Accent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum StyleBackground {
    /// See-through, as much as the theme's "blur" setting says.
    #[default]
    Glass,
    Solid,
    /// From the background role to the gradient role, top-left to bottom-right.
    Gradient,
    None,
}

/// One named look.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ComponentStyle {
    pub id: String,
    pub name: String,
    pub background: StyleBackground,
    pub background_role: Role,
    pub gradient_role: Role,
    /// How solid the background is, in percent (not used by glass).
    pub opacity: u8,
    /// Use the theme's corner radius instead of `radius`.
    pub corners_follow_theme: bool,
    /// Corner radius in pixels; large values make small pieces fully round.
    pub radius: u8,
    /// Use the theme's border width instead of `border_width`.
    pub border_follows_theme: bool,
    pub border_width: u8,
    pub border_role: Role,
    pub text_role: Role,
    /// Level bars, the caret, "playing" dots and other highlights.
    pub accent_role: Role,
    /// One of [`FONTS`]; `None` uses the theme's font.
    pub font: Option<String>,
    /// Text size, in percent of the normal size.
    pub text_scale: u8,
    /// Padding and gaps, in percent of the normal spacing.
    pub spacing: u8,
}

impl Default for ComponentStyle {
    /// The theme's own look.
    fn default() -> Self {
        Self {
            id: DEFAULT_STYLE.to_owned(),
            name: "Theme default".to_owned(),
            background: StyleBackground::Glass,
            background_role: Role::Surface,
            gradient_role: Role::Neutral,
            opacity: 100,
            corners_follow_theme: true,
            radius: 12,
            border_follows_theme: true,
            border_width: 1,
            border_role: Role::OnSurface,
            text_role: Role::OnSurface,
            accent_role: Role::Primary,
            font: None,
            text_scale: 100,
            spacing: 100,
        }
    }
}

impl ComponentStyle {
    fn sanitised(mut self) -> Self {
        self.name = self.name.trim().chars().take(40).collect();
        if self.name.is_empty() {
            self.name = "My style".to_owned();
        }
        self.opacity = self.opacity.clamp(OPACITY_RANGE.0, OPACITY_RANGE.1);
        self.radius = self.radius.min(RADIUS_MAX);
        self.border_width = self.border_width.min(BORDER_MAX);
        self.text_scale = self.text_scale.clamp(TEXT_SCALE_RANGE.0, TEXT_SCALE_RANGE.1);
        self.spacing = self.spacing.clamp(SPACING_RANGE.0, SPACING_RANGE.1);
        if self.font.as_deref().is_some_and(|font| !FONTS.contains(&font)) {
            self.font = None;
        }
        self
    }
}

/// The styles that come with Lumen. They cannot be edited, only duplicated.
pub fn builtin_styles() -> Vec<ComponentStyle> {
    let base = ComponentStyle::default;
    let named = |id: &str, name: &str| ComponentStyle { id: id.to_owned(), name: name.to_owned(), ..base() };
    vec![
        base(),
        ComponentStyle { background: StyleBackground::Solid, ..named("solid", "Solid") },
        ComponentStyle { background: StyleBackground::None, border_follows_theme: false, border_width: 2, border_role: Role::Primary, ..named("outline", "Outline") },
        ComponentStyle { background: StyleBackground::Solid, background_role: Role::Neutral, corners_follow_theme: false, radius: 28, border_follows_theme: false, border_width: 0, spacing: 115, ..named("soft", "Soft") },
        ComponentStyle {
            background: StyleBackground::Gradient,
            background_role: Role::Surface,
            gradient_role: Role::Primary,
            opacity: 92,
            border_follows_theme: false,
            border_width: 0,
            accent_role: Role::Accent,
            ..named("dusk", "Dusk")
        },
        ComponentStyle {
            background: StyleBackground::Solid,
            corners_follow_theme: false,
            radius: 0,
            border_follows_theme: false,
            border_width: 1,
            border_role: Role::Outline,
            font: Some("Space Grotesk".to_owned()),
            spacing: 90,
            ..named("sharp", "Sharp")
        },
    ]
}

/// Which style each kind of piece uses.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct StyleAssignments {
    pub widgets: String,
    pub bar: String,
    pub flyout: String,
    pub launcher: String,
}

impl Default for StyleAssignments {
    fn default() -> Self {
        let default = || DEFAULT_STYLE.to_owned();
        Self { widgets: default(), bar: default(), flyout: default(), launcher: default() }
    }
}

/// `styles.json`: the user's own styles and what uses which.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct StyleSheet {
    pub styles: Vec<ComponentStyle>,
    pub assigned: StyleAssignments,
}

impl StyleSheet {
    /// The style with this id: one of the user's, else a built-in one, else the default.
    pub fn resolve(&self, id: &str) -> ComponentStyle {
        self.styles.iter().find(|style| style.id == id).cloned().or_else(|| builtin_styles().into_iter().find(|style| style.id == id)).unwrap_or_default()
    }

    pub fn exists(&self, id: &str) -> bool {
        self.styles.iter().any(|style| style.id == id) || builtin_styles().iter().any(|style| style.id == id)
    }

    /// Keeps hand-edited or imported files within sensible limits: values in
    /// range, ids unique and not clashing with the built-in ones, and nothing
    /// assigned to a style that is not there.
    pub fn sanitised(mut self) -> Self {
        let reserved: Vec<String> = builtin_styles().into_iter().map(|style| style.id).collect();
        let mut seen: Vec<String> = Vec::new();
        self.styles.retain(|style| {
            let id_ok = !style.id.is_empty() && style.id.len() <= 64 && style.id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-');
            let fresh = id_ok && !reserved.contains(&style.id) && !seen.contains(&style.id);
            if fresh {
                seen.push(style.id.clone());
            }
            fresh
        });
        self.styles.truncate(MAX_STYLES);
        self.styles = self.styles.into_iter().map(ComponentStyle::sanitised).collect();
        let assigned = self.assigned.clone();
        let checked = |id: String| if self.exists(&id) { id } else { DEFAULT_STYLE.to_owned() };
        self.assigned = StyleAssignments { widgets: checked(assigned.widgets), bar: checked(assigned.bar), flyout: checked(assigned.flyout), launcher: checked(assigned.launcher) };
        self
    }

    /// An id for a new style called `name` that nothing else uses.
    pub fn new_id(&self, name: &str) -> String {
        let mut slug: String = name.to_lowercase().chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).collect();
        slug = slug.split('-').filter(|part| !part.is_empty()).collect::<Vec<_>>().join("-");
        if slug.is_empty() {
            slug = "style".to_owned();
        }
        slug.truncate(48);
        let mut id = slug.clone();
        let mut number = 2;
        while self.exists(&id) {
            id = format!("{slug}-{number}");
            number += 1;
        }
        id
    }
}

/// What `getStyles` returns and `stylesChanged` carries.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StylesState {
    /// The styles that come with Lumen (read-only).
    pub builtin: Vec<ComponentStyle>,
    /// The user's own.
    pub styles: Vec<ComponentStyle>,
    pub assigned: StyleAssignments,
}

impl From<StyleSheet> for StylesState {
    fn from(sheet: StyleSheet) -> Self {
        Self { builtin: builtin_styles(), styles: sheet.styles, assigned: sheet.assigned }
    }
}

/// Params of `exportStyle`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportStyleParams {
    pub id: String,
    /// Where to write the `.lumenstyle` file.
    pub path: String,
}

/// Params of `importStyle`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportStyleParams {
    pub path: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn custom(id: &str) -> ComponentStyle {
        ComponentStyle { id: id.to_owned(), name: format!("Style {id}"), ..Default::default() }
    }

    #[test]
    fn a_style_file_holds_role_names_never_colours() {
        let json = serde_json::to_string(&builtin_styles()).unwrap();
        assert!(!json.contains('#'), "no hex colours in a style");
        assert!(json.contains(r#""accentRole":"primary""#) && json.contains(r#""textRole":"onSurface""#));
    }

    #[test]
    fn built_in_styles_have_unique_ids_and_start_with_the_default() {
        let styles = builtin_styles();
        assert_eq!(styles[0], ComponentStyle::default());
        let mut ids: Vec<&str> = styles.iter().map(|style| style.id.as_str()).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), styles.len());
    }

    #[test]
    fn wild_values_are_tamed_and_bad_ids_dropped() {
        let wild = ComponentStyle { name: "   ".into(), opacity: 3, radius: 200, border_width: 30, text_scale: 250, spacing: 10, font: Some("Comic Sans".into()), ..custom("mine") };
        let sheet = StyleSheet { styles: vec![wild, custom("mine"), custom("solid"), custom("has space"), custom("ok-2")], assigned: StyleAssignments { bar: "gone".into(), flyout: "ok-2".into(), ..Default::default() } }.sanitised();
        assert_eq!(sheet.styles.iter().map(|style| style.id.as_str()).collect::<Vec<_>>(), ["mine", "ok-2"], "duplicates, built-in ids and odd ids are dropped");
        let tamed = &sheet.styles[0];
        assert_eq!((tamed.name.as_str(), tamed.opacity, tamed.radius, tamed.border_width, tamed.text_scale, tamed.spacing), ("My style", 20, 60, 4, 125, 70));
        assert_eq!(tamed.font, None);
        assert_eq!(sheet.assigned.bar, DEFAULT_STYLE, "a missing style falls back to the default");
        assert_eq!(sheet.assigned.flyout, "ok-2");
    }

    #[test]
    fn resolving_prefers_the_users_styles_then_built_ins_then_the_default() {
        let sheet = StyleSheet { styles: vec![ComponentStyle { radius: 33, ..custom("mine") }], ..Default::default() };
        assert_eq!(sheet.resolve("mine").radius, 33);
        assert_eq!(sheet.resolve("soft").name, "Soft");
        assert_eq!(sheet.resolve("nope"), ComponentStyle::default());
    }

    #[test]
    fn new_ids_come_from_the_name_and_never_clash() {
        let sheet = StyleSheet { styles: vec![custom("my-bar")], ..Default::default() };
        assert_eq!(sheet.new_id("My Bar!"), "my-bar-2");
        assert_eq!(sheet.new_id("Solid"), "solid-2");
        assert_eq!(sheet.new_id("  ✨  "), "style");
    }

    #[test]
    fn an_old_or_partial_file_fills_in_defaults() {
        let sheet: StyleSheet = serde_json::from_str(r#"{ "styles": [{ "id": "a", "name": "A", "background": "solid" }], "assigned": { "bar": "a" } }"#).unwrap();
        assert_eq!(sheet.styles[0].accent_role, Role::Primary);
        assert_eq!(sheet.styles[0].background, StyleBackground::Solid);
        assert_eq!((sheet.assigned.bar.as_str(), sheet.assigned.widgets.as_str()), ("a", DEFAULT_STYLE));
    }
}
