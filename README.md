# TRCC-LCD

Open-source Windows app for the LCD screen on Thermalright AIO coolers. Put your own pictures on the screen without the vendor software.

**Status:** early development. There is nothing to install yet.

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
