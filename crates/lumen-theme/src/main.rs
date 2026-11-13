//! `lumen-theme` command line: generate a theme JSON from a wallpaper or a colour.

use std::{process::ExitCode, time::Instant};

use lumen_theme::{Color, Mode, Theme, Variant, extract};

const USAGE: &str = "\
lumen-theme: generate a Lumen theme

Usage:
  lumen-theme extract <image> [options]     theme from a wallpaper (jpg, png, webp, bmp)
  lumen-theme from-color <#RRGGBB> [options] theme from one colour

Options:
  --variant <tonal|vibrant|expressive|monochrome>   default: tonal
  --mode <light|dark>                               default: dark
  --contrast <-1|0|1>                               reduced, standard, high; default: 0
  --name <text>                                     theme name; default: from the file name
  --timing                                          print how long each step took (to stderr)
  -h, --help                                        show this help

The theme JSON is printed to stdout.";

fn main() -> ExitCode {
    match run(std::env::args().skip(1).collect()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("error: {message}\n\nRun `lumen-theme --help` for usage.");
            ExitCode::from(2)
        }
    }
}

struct Options {
    variant: Variant,
    mode: Mode,
    contrast: i8,
    name: Option<String>,
    timing: bool,
}

fn run(args: Vec<String>) -> Result<(), String> {
    if args.is_empty() || args.iter().any(|a| a == "-h" || a == "--help") {
        println!("{USAGE}");
        return Ok(());
    }

    let command = args[0].as_str();
    let (target, options) = parse(&args[1..])?;
    let Options { variant, mode, contrast, name, timing } = options;

    let started = Instant::now();
    let mut theme = match command {
        "extract" => {
            let colors = extract(&target).map_err(|e| e.to_string())?;
            let extracted = started.elapsed();
            let theme = Theme::from_wallpaper_colors(&target, &colors, variant, mode, contrast).map_err(|e| e.to_string())?;
            if timing {
                eprintln!("extract:  {:>7.2} ms (decode + sample + quantise)", extracted.as_secs_f64() * 1000.0);
                eprintln!("generate: {:>7.2} ms", (started.elapsed() - extracted).as_secs_f64() * 1000.0);
            }
            theme
        }
        "from-color" => {
            let color: Color = target.parse().map_err(|e: lumen_theme::Error| e.to_string())?;
            Theme::from_color(color, variant, mode, contrast).map_err(|e| e.to_string())?
        }
        other => return Err(format!("unknown command \"{other}\"; expected extract or from-color")),
    };
    if timing {
        eprintln!("total:    {:>7.2} ms", started.elapsed().as_secs_f64() * 1000.0);
    }

    if let Some(name) = name {
        theme.name = name;
    }
    println!("{}", serde_json::to_string_pretty(&theme).map_err(|e| e.to_string())?);
    Ok(())
}

/// Splits the arguments after the command into the one positional target and the options.
fn parse(args: &[String]) -> Result<(String, Options), String> {
    let mut options = Options { variant: Variant::Tonal, mode: Mode::Dark, contrast: 0, name: None, timing: false };
    let mut target = None;
    let mut args = args.iter();

    while let Some(arg) = args.next() {
        let mut value = |flag: &str| args.next().cloned().ok_or_else(|| format!("{flag} needs a value"));
        match arg.as_str() {
            "--variant" => options.variant = value("--variant")?.parse().map_err(|e: lumen_theme::Error| e.to_string())?,
            "--mode" => options.mode = value("--mode")?.parse().map_err(|e: lumen_theme::Error| e.to_string())?,
            "--contrast" => {
                let raw = value("--contrast")?;
                options.contrast = match raw.parse::<i8>() {
                    Ok(n @ -1..=1) => n,
                    _ => return Err(format!("--contrast must be -1, 0 or 1, got \"{raw}\"")),
                };
            }
            "--name" => options.name = Some(value("--name")?),
            "--timing" => options.timing = true,
            // "-1" style values are consumed above, so anything else starting with '-' is a typo.
            flag if flag.starts_with('-') => return Err(format!("unknown option \"{flag}\"")),
            _ if target.is_none() => target = Some(arg.clone()),
            extra => return Err(format!("unexpected extra argument \"{extra}\"")),
        }
    }

    let target = target.ok_or("missing the image path or colour")?;
    Ok((target, options))
}
