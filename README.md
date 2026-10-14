# TRCC-LCD

Open-source Windows app for the LCD screen on Thermalright AIO coolers. Put your own pictures on the screen without the vendor software.

**Status:** early development. A command-line tool works; the desktop app is next.

## Command-line tool

Build it with [Rust](https://rustup.rs/):

```
cargo build --release
```

Then:

```
trcc-lcd info                              Show the connected screen
trcc-lcd send photo.png                    Show a picture until Ctrl+C
trcc-lcd send photo.png --fit fit          Show the whole picture with bars
trcc-lcd send photo.png --rotate 90        For a screen mounted sideways
trcc-lcd render photo.png -o preview.jpg   Save what the screen would show
```

`--fit` is `fill` (default, crops the edges), `fit` or `stretch`. PNG, JPEG, GIF and WebP files are supported.

## Tray app

`trcc-lcd-tray` runs in the background and keeps your picture on the screen. Animated GIFs play. It uses about 2 MB of memory. The screen only keeps a picture for about 2 seconds, so the app resends it once a second, which takes almost no CPU.

Right-click the tray icon to pause, reload, open the settings folder, turn on "Start with Windows" or quit.

Settings live in `%APPDATA%\TRCC-LCD\config.toml` and are picked up within a few seconds of saving:

```toml
image = 'C:\Pictures\cooler.gif'
fit = "fill"          # fill, fit or stretch
rotation = 0          # 0, 90, 180 or 270
quality = 90          # JPEG quality, 1 to 100
```

Only one copy runs at a time. While it runs, the command-line tool cannot reach the screen.

## Supported hardware

| USB ID | Resolution | Status |
|---|---|---|
| `87AD:70DB` | 480x480 | Tested |

Other models may work. If you have one, open a [device report](../../issues/new?template=device_report.yml).

## How it works

The app talks to the screen directly over USB using the WinUSB driver Windows already installs for it. No extra driver is needed. See [docs/PROTOCOL.md](docs/PROTOCOL.md).

## Credits

The USB protocol was documented by [thermalright-trcc-linux](https://github.com/Lexonight1/thermalright-trcc-linux) and [thermalwriter](https://github.com/mgaruccio/thermalwriter). This project shares no code with them.

## Disclaimer

Not affiliated with or endorsed by Thermalright. Use at your own risk.

## License

[MIT](LICENSE)
