use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::thread;

use anyhow::{Context, Result};
use clap::builder::{PossibleValuesParser, TypedValueParser};
use clap::{Parser, Subcommand, ValueEnum};
use trcc_core::player;
use trcc_core::usb::UsbTransport;
use trcc_core::{Fit, Lcd, Rotation, render};

const SCREEN: u32 = 480;

#[derive(Parser)]
#[command(
    name = "trcc-lcd",
    version,
    about = "Put pictures on Thermalright AIO LCD screens"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Show which screen is connected.
    Info,
    /// Show an image on the screen until Ctrl+C.
    Send {
        image: PathBuf,
        /// Send one frame and exit. The screen goes back to its default after about 2 seconds.
        #[arg(long)]
        once: bool,
        #[command(flatten)]
        opts: FrameOpts,
    },
    /// Render an image to a JPEG file exactly as the screen would show it.
    Render {
        image: PathBuf,
        /// Output JPEG file.
        #[arg(short, long)]
        output: PathBuf,
        /// Overwrite the output file if it exists.
        #[arg(long)]
        force: bool,
        #[command(flatten)]
        opts: FrameOpts,
    },
}

#[derive(clap::Args)]
struct FrameOpts {
    /// How the image is fitted to the square screen.
    #[arg(long, value_enum, default_value_t = FitArg::Fill)]
    fit: FitArg,
    /// Rotation in degrees, for screens mounted sideways.
    #[arg(long, default_value = "0", value_parser = PossibleValuesParser::new(["0", "90", "180", "270"]).map(|s| s.parse::<u16>().unwrap_or_default()))]
    rotate: u16,
    /// JPEG quality, 1 to 100.
    #[arg(long, default_value_t = 90, value_parser = clap::value_parser!(u8).range(1..=100))]
    quality: u8,
}

#[derive(Clone, Copy, ValueEnum)]
enum FitArg {
    Fill,
    Fit,
    Stretch,
}

impl From<FitArg> for Fit {
    fn from(f: FitArg) -> Self {
        match f {
            FitArg::Fill => Fit::Fill,
            FitArg::Fit => Fit::Fit,
            FitArg::Stretch => Fit::Stretch,
        }
    }
}

fn frame(image: &Path, opts: &FrameOpts, width: u32, height: u32) -> Result<Vec<u8>> {
    let img = render::load(image).with_context(|| format!("could not load {}", image.display()))?;
    let rotation = Rotation::from_degrees(opts.rotate)?;
    let prepared = render::prepare(&img, width, height, opts.fit.into(), rotation);
    Ok(render::encode_jpeg(&prepared, opts.quality)?)
}

fn run(cli: Cli) -> Result<()> {
    match cli.command {
        Command::Info => {
            let lcd = Lcd::connect(UsbTransport::open()?)?;
            let (info, panel) = (lcd.info(), lcd.panel());
            println!("Model:      {}", info.model);
            println!("Profile:    PM {} SUB {}", info.pm, info.sub);
            println!("Resolution: {} x {}", panel.width, panel.height);
        }
        Command::Send { image, once, opts } => {
            let mut lcd = Lcd::connect(UsbTransport::open()?)?;
            let panel = lcd.panel();
            let jpeg = frame(&image, &opts, panel.width, panel.height)?;
            lcd.send_jpeg(&jpeg)?;
            println!("Sent {} ({:.1} KB)", image.display(), kb(jpeg.len()));
            if !once {
                println!("Showing it until you press Ctrl+C.");
                loop {
                    thread::sleep(player::REFRESH);
                    lcd.send_jpeg(&jpeg)?;
                }
            }
        }
        Command::Render {
            image,
            output,
            force,
            opts,
        } => {
            let jpeg = frame(&image, &opts, SCREEN, SCREEN)?;
            let mut file = OpenOptions::new()
                .write(true)
                .create(force)
                .truncate(force)
                .create_new(!force)
                .open(&output)
                .with_context(|| {
                    format!(
                        "could not create {} (use --force to overwrite)",
                        output.display()
                    )
                })?;
            file.write_all(&jpeg)?;
            println!("Wrote {} ({:.1} KB)", output.display(), kb(jpeg.len()));
        }
    }
    Ok(())
}

#[allow(clippy::cast_precision_loss)]
fn kb(bytes: usize) -> f64 {
    bytes as f64 / 1024.0
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("Error: {e:#}");
            ExitCode::FAILURE
        }
    }
}
