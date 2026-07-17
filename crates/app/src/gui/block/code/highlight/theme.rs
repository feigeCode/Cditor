use crate::api::SyntaxHighlightPalette;

pub(crate) const DEFAULT_CODE_HIGHLIGHT_THEME: &str = "catppuccin_latte";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CodeThemeItem {
    pub id: &'static str,
    pub label: &'static str,
    pub background: u32,
    pub foreground: u32,
    pub preview: [u32; 4],
}

pub(crate) const CODE_THEME_ITEMS: [CodeThemeItem; 8] = [
    item(
        "github_light",
        "GitHub Light",
        0xf6f8fa,
        0x1f2328,
        [0xcf222e, 0x0550ae, 0x0a3069, 0x57606a],
    ),
    item(
        "github_dark",
        "GitHub Dark",
        0x0d1117,
        0xe6edf3,
        [0xff7b72, 0x79c0ff, 0xa5d6ff, 0x8b949e],
    ),
    item(
        "dracula",
        "Dracula",
        0x282a36,
        0xf8f8f2,
        [0xff79c6, 0xbd93f9, 0xf1fa8c, 0x6272a4],
    ),
    item(
        "catppuccin_latte",
        "Catppuccin Latte",
        0xeff1f5,
        0x4c4f69,
        [0x8839ef, 0x1e66f5, 0x40a02b, 0x9ca0b0],
    ),
    item(
        "catppuccin_mocha",
        "Catppuccin Mocha",
        0x1e1e2e,
        0xcdd6f4,
        [0xcba6f7, 0x89b4fa, 0xa6e3a1, 0x6c7086],
    ),
    item(
        "gruvbox_light",
        "Gruvbox Light",
        0xfbf1c7,
        0x3c3836,
        [0x9d0006, 0x076678, 0x79740e, 0x928374],
    ),
    item(
        "gruvbox_dark",
        "Gruvbox Dark",
        0x282828,
        0xebdbb2,
        [0xfb4934, 0x83a598, 0xb8bb26, 0x928374],
    ),
    item(
        "kanagawa_wave",
        "Kanagawa Wave",
        0x1f1f28,
        0xdcd7ba,
        [0x957fb8, 0x7e9cd8, 0x98bb6c, 0x727169],
    ),
];

const fn item(
    id: &'static str,
    label: &'static str,
    background: u32,
    foreground: u32,
    preview: [u32; 4],
) -> CodeThemeItem {
    CodeThemeItem {
        id,
        label,
        background,
        foreground,
        preview,
    }
}

pub(crate) fn code_theme_item(theme_name: &str) -> CodeThemeItem {
    CODE_THEME_ITEMS
        .iter()
        .copied()
        .find(|item| item.id == theme_name)
        .or_else(|| {
            CODE_THEME_ITEMS
                .iter()
                .copied()
                .find(|item| item.id == DEFAULT_CODE_HIGHLIGHT_THEME)
        })
        .expect("default code highlight theme is in the menu")
}

pub(crate) fn external_theme_item(palette: SyntaxHighlightPalette) -> CodeThemeItem {
    item(
        "host",
        "Host Theme",
        palette.background,
        palette.foreground,
        [palette.foreground; 4],
    )
}
