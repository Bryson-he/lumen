//! `lumen-win` test command: exercises each Windows integration by hand.

use std::{process::ExitCode, thread::sleep, time::Duration};

use lumen_theme::{Color, Mode};
use lumen_win::{
    accent,
    appearance::{self, Appearance},
    fullscreen,
    wallpaper::Wallpapers,
};

const USAGE: &str = "\
lumen-win: try Lumen's Windows integration

Usage:
  lumen-win monitors                              list monitors and their wallpapers
  lumen-win wallpaper <image> [--monitor <n>]     set the wallpaper (all monitors by default)
  lumen-win mode [light|dark]                     show or switch Windows light/dark mode
  lumen-win accent [#RRGGBB]                      show or set the Windows accent colour
  lumen-win fullscreen [--watch <seconds>]        show the fullscreen app, or log changes

Options for wallpaper, mode and accent:
  --revert-after <seconds>    put the previous setting back after this long";

fn main() -> ExitCode {
    let result = run(std::env::args().skip(1).collect());
    // Let apps finish hearing about any colour change before exiting.
    lumen_win::flush_broadcasts();
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("error: {message}\n\nRun `lumen-win --help` for usage.");
            ExitCode::from(2)
        }
    }
}

fn run(mut args: Vec<String>) -> Result<(), String> {
    if args.is_empty() || args.iter().any(|a| a == "-h" || a == "--help") {
        println!("{USAGE}");
        return Ok(());
    }
    let revert_after = take_option(&mut args, "--revert-after")?.map(seconds).transpose()?;
    let monitor = take_option(&mut args, "--monitor")?.map(|n| n.parse::<usize>().map_err(|_| format!("--monitor needs a number, got \"{n}\""))).transpose()?;
    let watch = take_option(&mut args, "--watch")?.map(seconds).transpose()?;
    let value = args.get(1).map(String::as_str);
    if let Some(extra) = args.get(2) {
        return Err(format!("unexpected extra argument \"{extra}\""));
    }

    match (args[0].as_str(), value) {
        ("monitors", None) => print_monitors(&Wallpapers::new().map_err(text)?),

        ("wallpaper", Some(image)) => {
            let wallpapers = Wallpapers::new().map_err(text)?;
            let before = wallpapers.monitors().map_err(text)?;
            wallpapers.set(monitor, image).map_err(text)?;
            println!("Wallpaper set on {}.", monitor.map_or("all monitors".to_owned(), |n| format!("monitor {n}")));
            print_monitors(&wallpapers)?;
            if let Some(wait) = revert_after {
                sleep(wait);
                for m in before.iter().filter(|m| monitor.is_none_or(|n| n == m.number)) {
                    match &m.wallpaper {
                        Some(previous) => wallpapers.set(Some(m.number), previous).map_err(text)?,
                        None => println!("Monitor {} had no picture wallpaper before; left as is.", m.number),
                    }
                }
                println!("Reverted.");
                print_monitors(&wallpapers)?;
            }
            Ok(())
        }

        ("mode", None) => {
            let now = appearance::get().map_err(text)?;
            println!("apps: {}, taskbar and Start: {}", now.apps, now.system);
            Ok(())
        }
        ("mode", Some(mode)) => {
            let mode: Mode = mode.parse().map_err(text)?;
            let before = appearance::get().map_err(text)?;
            appearance::set(Appearance::both(mode)).map_err(text)?;
            println!("Windows is now {mode} (was apps: {}, taskbar and Start: {}).", before.apps, before.system);
            if let Some(wait) = revert_after {
                sleep(wait);
                appearance::set(before).map_err(text)?;
                println!("Reverted to apps: {}, taskbar and Start: {}.", before.apps, before.system);
            }
            Ok(())
        }

        ("accent", None) => {
            match accent::get().map_err(text)? {
                Some(color) => println!("{color}"),
                None => println!("Windows has no accent colour recorded."),
            }
            Ok(())
        }
        ("accent", Some(color)) => {
            let color: Color = color.parse().map_err(text)?;
            let before = accent::snapshot().map_err(text)?;
            let was = accent::get().map_err(text)?;
            accent::set(color).map_err(text)?;
            println!("Accent is now {color} (was {}).", was.map_or("unset".to_owned(), |c| c.to_string()));
            if let Some(wait) = revert_after {
                sleep(wait);
                accent::restore(&before).map_err(text)?;
                println!("Reverted to {}.", accent::get().map_err(text)?.map_or("unset".to_owned(), |c| c.to_string()));
            }
            Ok(())
        }

        ("fullscreen", None) => match watch {
            None => {
                println!("{}", describe(fullscreen::current().as_ref()));
                Ok(())
            }
            Some(duration) => {
                println!("Watching for {} s. Make something fullscreen (a game, a video, F11 in a browser).", duration.as_secs());
                let started = std::time::Instant::now();
                fullscreen::watch(Some(duration), move |app| {
                    println!("[{:>6.1}s] {}", started.elapsed().as_secs_f64(), describe(app));
                })
                .map_err(text)
            }
        },

        (command, _) => Err(format!("\"{command}\" is not a command, or has the wrong arguments")),
    }
}

fn print_monitors(wallpapers: &Wallpapers) -> Result<(), String> {
    for m in wallpapers.monitors().map_err(text)? {
        println!(
            "Monitor {}{}: {}x{} at ({}, {})  wallpaper: {}",
            m.number,
            if m.primary { " (primary)" } else { "" },
            m.width,
            m.height,
            m.x,
            m.y,
            m.wallpaper.map_or("none (solid colour)".to_owned(), |p| p.display().to_string()),
        );
    }
    Ok(())
}

fn describe(app: Option<&fullscreen::FullscreenApp>) -> String {
    match app {
        None => "fullscreen OFF: normal desktop".to_owned(),
        Some(app) => format!(
            "fullscreen ON:  {:?} \"{}\" (class {}) on the monitor at ({}, {})",
            app.kind, app.title, app.window_class, app.monitor_x, app.monitor_y
        ),
    }
}

/// Removes `--flag value` from the arguments and returns the value.
fn take_option(args: &mut Vec<String>, flag: &str) -> Result<Option<String>, String> {
    let Some(index) = args.iter().position(|a| a == flag) else { return Ok(None) };
    if index + 1 >= args.len() {
        return Err(format!("{flag} needs a value"));
    }
    args.remove(index);
    Ok(Some(args.remove(index)))
}

fn seconds(text: String) -> Result<Duration, String> {
    match text.parse::<f64>() {
        Ok(s) if s > 0.0 && s <= 3600.0 => Ok(Duration::from_secs_f64(s)),
        _ => Err(format!("expected a number of seconds between 0 and 3600, got \"{text}\"")),
    }
}

fn text(error: impl std::fmt::Display) -> String {
    error.to_string()
}
