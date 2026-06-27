//! Shared utility functions for the lstr application.

// This entire module will only be compiled on Unix-like systems.

/// Formats a size in bytes into a human-readable string using binary prefixes (KiB, MiB).
pub fn format_size(bytes: u64) -> String {
    const KIB: f64 = 1024.0;
    const MIB: f64 = KIB * 1024.0;
    const GIB: f64 = MIB * 1024.0;
    const TIB: f64 = GIB * 1024.0;

    let bytes = bytes as f64;

    if bytes < KIB {
        format!("{bytes} B")
    } else if bytes < MIB {
        format!("{:.1} KiB", bytes / KIB)
    } else if bytes < GIB {
        format!("{:.1} MiB", bytes / MIB)
    } else if bytes < TIB {
        format!("{:.1} GiB", bytes / GIB)
    } else {
        format!("{:.1} TiB", bytes / TIB)
    }
}

/// Formats a Unix file mode into a human-readable string (e.g., "rwxr-xr-x").
#[cfg(unix)]
pub fn format_permissions(mode: u32) -> String {
    let user_r = if mode & 0o400 != 0 { 'r' } else { '-' };
    let user_w = if mode & 0o200 != 0 { 'w' } else { '-' };
    let user_x = if mode & 0o100 != 0 { 'x' } else { '-' };
    let group_r = if mode & 0o040 != 0 { 'r' } else { '-' };
    let group_w = if mode & 0o020 != 0 { 'w' } else { '-' };
    let group_x = if mode & 0o010 != 0 { 'x' } else { '-' };
    let other_r = if mode & 0o004 != 0 { 'r' } else { '-' };
    let other_w = if mode & 0o002 != 0 { 'w' } else { '-' };
    let other_x = if mode & 0o001 != 0 { 'x' } else { '-' };
    format!("{user_r}{user_w}{user_x}{group_r}{group_w}{group_x}{other_r}{other_w}{other_x}")
}

/// Renders a filename with an LsColors style applied (foreground color +
/// bold/italic/underline). Goes through the `colored` crate, so it honors the
/// global color override / TTY detection: when color is disabled the result is
/// the plain name.
pub fn style_name(name: &str, style: &lscolors::Style) -> String {
    use colored::Colorize;
    let mut styled = name.normal();
    if let Some(fg) = &style.foreground {
        use lscolors::Color as LsColor;
        let color = match fg {
            LsColor::Black => colored::Color::Black,
            LsColor::Red => colored::Color::Red,
            LsColor::Green => colored::Color::Green,
            LsColor::Yellow => colored::Color::Yellow,
            LsColor::Blue => colored::Color::Blue,
            LsColor::Magenta => colored::Color::Magenta,
            LsColor::Cyan => colored::Color::Cyan,
            LsColor::White => colored::Color::White,
            LsColor::BrightBlack => colored::Color::BrightBlack,
            LsColor::BrightRed => colored::Color::BrightRed,
            LsColor::BrightGreen => colored::Color::BrightGreen,
            LsColor::BrightYellow => colored::Color::BrightYellow,
            LsColor::BrightBlue => colored::Color::BrightBlue,
            LsColor::BrightMagenta => colored::Color::BrightMagenta,
            LsColor::BrightCyan => colored::Color::BrightCyan,
            LsColor::BrightWhite => colored::Color::BrightWhite,
            LsColor::Fixed(_) => colored::Color::White,
            LsColor::RGB(r, g, b) => colored::Color::TrueColor { r: *r, g: *g, b: *b },
        };
        styled = styled.color(color);
    }
    if style.font_style.bold {
        styled = styled.bold();
    }
    if style.font_style.italic {
        styled = styled.italic();
    }
    if style.font_style.underline {
        styled = styled.underline();
    }
    styled.to_string()
}

// Unit tests for utility functions
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn style_name_applies_foreground_when_color_on() {
        let _c = crate::test_color::guard();
        colored::control::set_override(true);
        let style = lscolors::Style {
            foreground: Some(lscolors::Color::Green),
            ..Default::default()
        };
        let out = style_name("file.rs", &style);
        assert!(out.contains("\x1b[32m"), "green ANSI present when color on: {out:?}");
        assert!(out.contains("file.rs"));
        colored::control::unset_override();
    }

    #[test]
    fn style_name_plain_when_color_off() {
        let _c = crate::test_color::guard();
        colored::control::set_override(false);
        let style = lscolors::Style {
            foreground: Some(lscolors::Color::Green),
            ..Default::default()
        };
        let out = style_name("file.rs", &style);
        assert_eq!(out, "file.rs", "plain when color off");
        colored::control::unset_override();
    }

    #[test]
    fn test_format_size() {
        assert_eq!(format_size(500), "500 B");
        assert_eq!(format_size(1024), "1.0 KiB");
        assert_eq!(format_size(1536), "1.5 KiB");
        let mib = 1024 * 1024;
        assert_eq!(format_size(mib), "1.0 MiB");
        assert_eq!(format_size(mib + mib / 2), "1.5 MiB");
        let gib = mib * 1024;
        assert_eq!(format_size(gib), "1.0 GiB");
    }

    #[test]
    #[cfg(unix)]
    fn test_format_permissions() {
        // -rwxr-xr-x
        let mode = 0o755;
        assert_eq!(format_permissions(mode), "rwxr-xr-x");
        // -rw-r--r--
        let mode_read = 0o644;
        assert_eq!(format_permissions(mode_read), "rw-r--r--");
        // -rwx------
        let mode_user_only = 0o700;
        assert_eq!(format_permissions(mode_user_only), "rwx------");
    }
}
