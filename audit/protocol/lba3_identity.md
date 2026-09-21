# LBA3 target identity gate

Status: **PARTIAL / identity not locked** (2026-09-21).

## Local target that actually carries the nonzero LBA3 profile

The designated strict gold sample is:

`disk4_121110528_vid0951_pid1666_disk&ven_kingston&prod_datatraveler_3.0_onlyid2135149925_20260903_121319.bin`

Host-visible identity:

- VID:PID = `0951:1666`
- product = `Kingston DataTraveler 3.0`
- capacity = `121110528 * 512 = 62008590336` bytes
- LBA3 `+0x000..0x003 = 00 01 00 00`
- LBA3 `+0x020..0x027 = b5 7e 9c 45 00 80 00 14`
- LBA3 `+0x1F0..0x1FF = "this is mp mark\0"`

The checked-in baseline harness independently reproduces that shape from the
current gold set.  No controller model, chip firmware version, ID_BLK version,
MPALL version, NAND ID, or device serial was captured with this 2026-09-03
sample.  The target device is not currently present in the macOS USB device
tree, so no new device-specific controller probe can be made from this host at
this point.

## Why VID/PID/product/capacity cannot select a controller generation

Two public device reports have the **same** host-visible identity and the exact
`62008590336`-byte physical capacity, but identify different controller
generations:

1. Psychson issue #173 reports `0951:1666`, `Kingston DataTraveler 3.0`,
   physical capacity `62008590336`, **PS2307**, chip F/W `01.02.55`, ID_BLK
   `1.3.0.0`, MPALL `v3.34.07`:
   https://github.com/brandonlw/Psychson/issues/173
2. Psychson issue #213 reports the same VID/PID/product and physical capacity,
   but **PS2309**, chip F/W `08.05.5D`, ID_BLK `1.4.33.0`, MPALL `v5.35.35`:
   https://github.com/brandonlw/Psychson/issues/213

This is a direct counterexample to controller inference from the currently
recorded host identity.  MPALL 3.34/PS2307 and MPALL 5.35/PS2309 must therefore
remain separate hypotheses until device-specific evidence exists.

## Existing manufacturing evidence and its boundary

The locally archived `MPALL_F1_9000_v372_0B.exe` establishes a real Phison
F2-mark/F2-INFO manufacturing path.  Its `CBaseController::WriteF2Mark` writes
and reads back a 512-byte object buffer with vendor commands, but the staging
page requires a `12 01 00 02` prefix.  That is structurally incompatible with
the target LBA3 `00 01 00 00` prefix.  `GetInfo.exe` also decodes SampleMark and
MPF1F2 at offsets unrelated to target LBA3 `+0x020..0x027`.

The package FW/BN binaries contain `"this is mp mark"` in their final 512-byte
block at offset `+0x000`; the host-visible target LBA3 contains the same text at
`+0x1F0`.  This proves ecosystem/marker ancestry only.  It does **not** prove a
copy, projection, rotation, checksum transform, or LBA mapping.

## Exact evidence required before any LBA3 byte is promoted

At least one of the following must tie the actual target device to a controller
generation:

- a device-specific USBFlashInfo/ChipGenius/MPALL GetInfo capture containing
  controller part number, chip F/W, ID_BLK and preferably NAND ID; or
- a read-only vendor-command capture from that exact physical device that
  returns equivalent controller/firmware identity.

After identity is locked, the manufacturing proof must still show both sides
of the host-visible projection:

1. the exact producer/copy/transform that constructs
   `00 01 00 00 ... b5 7e 9c 45 00 80 00 14 ... this is mp mark`; and
2. the firmware path that exposes or consumes that structure as host LBA3 (or
   proves the intermediate mapping if LBA3 is a projection of another page).

Until both are present, LBA3 remains `512B PARTIAL`; string similarity and the
nearby F2 INFO protocol do not count as completion evidence.

