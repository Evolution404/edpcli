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

The final-512-byte relationship has now been verified byte-for-byte against all
four BIN files in the archived v3.72 package:

- `BN67V1292KM.BIN`: marker page starts at file offset `0x8200`;
- `BN67V132M.BIN`: marker page starts at file offset `0x8200`;
- `FW67FF01V60424M.BIN`: marker page starts at file offset `0x16200`;
- `FW67FF01V61110M.BIN`: marker page starts at file offset `0x1C200`.

Each page starts with the 16 bytes `"this is mp mark\0"`; version/controller
material follows at page `+0x10`.  For example the two FW pages contain
`67 01 01 10 06 04 24 46 ff 01 ff ...` and
`67 01 01 10 06 11 10 46 ff 01 ff ...`.

The MPALL executable consumes this layout directly rather than treating the
marker as a decorative string.  In `CBaseController::virtual_464` the recovered
machine-code path:

1. opens a candidate FW/BN file;
2. seeks to `file_size - 0x200`;
3. reads exactly `0x200` bytes;
4. compares the first 15 bytes with `"this is mp mark"`; and
5. uses the adjacent marker-page bytes in controller/version compatibility
   checks (including the `"controller : %x"` /
   `"controller ver: %x %x %x"` diagnostic paths).

MSVC RTTI/vtable recovery now fixes those previously name-only `virtual_464/468`
paths to concrete code addresses: Base=`0x55FC90/0x55F570`, Base30=
`0x541290/0x541780`, C2250=`0x535C40/0x5354A0`, C2260=
`0x5333A0/0x532B80`, and C2261=`0x5316B0/0x532B80`.  Direct disassembly of the
controller implementations confirms the same lifecycle: open candidate FW/BN,
seek `file_size-0x200`, read 512B, compare the first15 bytes against
`"this is mp mark"`, then consume the adjacent controller/version bytes.  These
are FW/BN **marker-page readers/compatibility checkers**, not host-sector
serializers.

The F2 writer names are now independently fixed to code as well:
`CBaseController::WriteF2Mark=0x581B00` and
`CU32SSBaseContoller::WriteF2Mark=0x487FA0`.  Base `WriteF2Mark` passes
`this+0x1C00C` into the F2 vendor-write helper, reads back an `INFO` response,
and compares the first `0x200` bytes.  CU32SS uses a different `F3 00 83...`
wrapper and a `0x1C0` payload.  Therefore same-name F2 operations are not one
universal 512B host-LBA3 writer, and neither recovered function constructs the
sparse `marker@+0x1F0` record.

A new offline audit adds a stronger structural invariant without crossing that
boundary.  Two independent physical nonzero LBA3 profiles have distinct first
40 bytes but exactly the same `+0x028..+0x1FF` 472-byte tail, SHA-256
`5f88797f7273191052e7a9300316e1a4f0f31563db07110a86fa4e648379198f`.
That tail is 456 zero bytes at `+0x028..+0x1EF` followed by
`"this is mp mark\0"` at `+0x1F0..+0x1FF`.  All four pinned v3.72 FW/BN final
marker pages, after moving their first 16 marker bytes to the end, match this
physical tail 472/472; the BN pages match the full physical sector 500/512 and
the FW pages 496/512.  `scripts/protocol/audit_lba3_phison_marker_page.py`
replays the archive/member/page hashes and this comparison without opening any
device.

This is evidence of a shared marker-page-family tail, not evidence that MPALL
performs that 16-byte reordering.  A scan of the v3.72 PC executable found no
`496B+16B` rotation path, and the exact physical 8-byte values
`b5 7e 9c 45 00 80 00 14` / `a8 82 a4 22 00 20 02 16` occur locally only in
the corresponding physical captures.  The manufacturer record builder and the
controller-firmware mapping that exposes it as host LBA3 are still missing.

Simple checksum projection was also tested and rejected.  Neither observed
LBA3 DWORD (`0x459C7EB5` in the strict sample,
`0x22A482A8` in the 2026-08-03 historical profile) matches standard CRC32 of
the v3.72 FW/BN whole file, its final 512 bytes, marker 16 bytes, marker metadata
16 bytes, or the remaining 496 bytes.  The strict DWORD also does not match the
EDP `crc32_bare` of any LBA0-LBA12 sector, the 6656-byte image, the image with
LBA3 zeroed, the LBA3 prefix, device_id, capacity, sector count or onlyid.
Thus `+0x020..0x023` is not supported as a simple EDP-side or FW-file checksum.

## Local capture inventory is insufficient to lock PS2307 versus PS2309

The older 2026-08-03 profile is preserved in two back-to-back 6656-byte
captures:

- `utils/backup/disk4_20260803_105045.bin`;
- `utils/backup/disk4_20260803_105053.bin`.

They are byte-identical and carry
`+0x020..0x027 = a8 82 a4 22 00 20 02 16`.  Their sidecar JSON files record
only the EDP device_id, EDP CRC, LBA range, image hash/time and a few partition
facts.  The later 2026-08-27 Kingston sidecars add VID/PID/capacity, but still
do **not** contain USB serial, bcdDevice/SCSI revision, controller model,
firmware, ID_BLK or NAND ID.  The saved macOS ioreg snapshots do not contain
this Kingston device either, and the local CEMS `usb_info.xml` is a generic
VID/PID dictionary rather than a device capture.

This is a reproducible negative boundary: the existing local archive cannot
select PS2307 or PS2309 for either nonzero LBA3 profile.

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

