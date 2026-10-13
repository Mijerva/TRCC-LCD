# USB protocol

Notes for the `87AD:70DB` screen. All integers are little-endian.

## Transport

- One interface with two bulk endpoints: OUT `0x01` and IN `0x81`, 512-byte packets.
- Windows binds the WinUSB driver to it automatically.

## Handshake

Send 64 bytes:

| Offset | Value |
|---|---|
| 0..3 | `12 34 56 78` |
| 56 | `01` |
| rest | `00` |

The screen replies with 64 bytes. It starts with the same magic followed by an ASCII model string (`SSCRM-V1` on the tested unit). Two bytes select the panel profile:

| Offset | Name | Tested value |
|---|---|---|
| 24 | PM | 3 |
| 36 | SUB | 1 |

A PM of 0 means the handshake failed. Other bytes in the reply can identify a single unit, so do not paste full replies in public issues.

## Sending a frame

Send a 64-byte header followed by the image data:

| Offset | Size | Value |
|---|---|---|
| 0 | 4 | magic `12 34 56 78` |
| 4 | 4 | command: 2 = JPEG, 3 = RGB565 |
| 8 | 4 | width |
| 12 | 4 | height |
| 56 | 4 | mode, always 2 |
| 60 | 4 | payload length in bytes |

Other header bytes are zero.

- Write header and payload as one stream, in chunks of up to 16 KiB.
- If the total length is a multiple of 512, finish with a zero-length packet.
- PM 3 uses JPEG at 480x480.

## Sources

- [thermalright-trcc-linux](https://github.com/Lexonight1/thermalright-trcc-linux)
- [thermalwriter](https://github.com/mgaruccio/thermalwriter)
