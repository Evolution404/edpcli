# Phison F2 / LBA3 producer trace — 2026-09-21

This note preserves evidence gathered while tracing the external manufacturer-owned LBA3 profile.
It does **not** change the strict LBA0–LBA12 COMPLETE byte count.

## Confirmed sample family

Public static-analysis records for the SHA-256
`96614750c61e0ad6b05d19e74848c1679f6318dd21de6faee46c92fb05152142`
(`MPALL_F1_9000_v372_0B.exe`) expose all of the following strings in one binary:

- `CBaseController::DoF2`
- `CBaseController::read_write_f2`
- `CBaseController::U3_DoF2`
- `CBaseController::WriteF2Mark`
- `CU32SSBaseContoller::U3_DoF2`
- `CU32SSBaseContoller::WriteF2Mark`
- `F1-F2 MARK`
- `F2 Merged`

A later independent MPALL generation,
`mpall_f1_7f00_dl07_v503_0a.exe`
(SHA-256 `2cfd1c3ea9d6bec17d8237f0be79ae96f78fe77a40032987c52d3a30996e29bf`),
retains the same F2 lifecycle function-name family, including
`DoF2`, `read_write_f2`, `U3_DoF2`, `WriteF2Mark`, and `F2 Merged`.

Therefore the F2 lifecycle is a cross-generation Phison MP implementation family rather than a
single-build string residue. This strengthens the producer-family attribution already used by the
main protocol audit, but does not by itself identify the exact PS2307/PS2309 wire profile that
produced the committed Kingston LBA3 sectors.

## Machine-code facts recovered in the previous local analysis session

The v3.72.0B executable was obtained and inspected offline without executing the MP utility.

- `CBaseController::WriteF2Mark` was located near `0x00581B00`.
- It passes the object-owned buffer at approximately `this+0x1C00C` into the F2 write path.
- It then issues an `"INFO"` readback and compares the first 512 bytes with the same buffer.
  A mismatch follows an explicit error path.
- `CU32SSBaseContoller::WriteF2Mark` was located near `0x00487FA0`.
  This controller family uses a different target-command wrapper and passes 0x1C0 (448) bytes
  from the same object-owned area.
- Consequently, the shared `WriteF2Mark` name must not be interpreted as one universal
  512-byte transport layout across controller classes.
- Several references to `this+0x1C00C` were recovered inside class-specific virtual methods
  such as `C2273Controller::virtual_308`, `C2267Controller::virtual_308`, and
  `C2261Controller::virtual_308`.
- One class path tests the buffer prefix against `12 01 00 02`; this is **not** the committed
  Kingston LBA3 wire profile, whose observed prefix is `00 01 00 00`.
  Therefore that branch is a useful negative discriminator and must not be promoted as the
  LBA3 template.

## Current strict interpretation

The committed LBA3 evidence remains:

- EDP itself preserves and ignores this sector.
- Real devices show a zero profile and at least two non-zero
  `"this is mp mark\0"` profiles.
- The manufacturer family is Phison MP/FW and the producer family includes F2-mark writers.
- The exact writer that constructs the committed `00 01 00 00 ... this is mp mark\0` profile,
  the meaning of `+0x020..+0x027`, and the controller-firmware consumer remain unresolved.

Therefore LBA3 stays 512B PARTIAL and contributes 0B to COMPLETE.

## Next concrete reverse-engineering targets

1. Recover write xrefs into the object-owned F2 buffer around `this+0x1C00C`, rather than
   following marker-string xrefs.
2. Classify those writers by controller class and identify which family can generate prefix
   `00 01 00 00`.
3. Trace the `F2 Merged` and `U3_DoF2` paths to determine whether the 512-byte host-visible
   LBA3 sector is a projection of a larger controller F2 structure.
4. Only after an exact producer/store layout and a real consumer are recovered should any
   LBA3 subrange move from PARTIAL to COMPLETE.
