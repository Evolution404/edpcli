# DeviceNumber.dll host-identity algorithm

Status: **verified static producer dependency** (2026-09-21).

This note records the host-identity component used by the historical
`CEMSUsbRegsiter.dll` path that populates `MyHardinfo` / LBA8
`HDSerialInfo`.  It deliberately does **not** claim that this component
produces the five `HDOnlySerial[5]` / `HSerialCRC[5]` values.

## Binary identity

- Path:
  `~/Desktop/u_disk/VRV/cems/ydcc/devicenumber.dll`
- SHA-256:
  `0ef94c3679da6f27eac75959cf299bbad19676c251d88f7554fbc305407d6041`
- PE COFF timestamp reported by the binary:
  `2008-12-03 06:51:10`
- Architecture: x86 / i386.
- Relevant exports:
  - `EDP_DeviceNumber @ 0x10011E00`
  - `EDP_LicenseNumber @ 0x10012A90`
  - `EDP_DiskNumber @ 0x10012C90`
- Embedded acquisition strings include `\\\\.\\PhysicalDrive%d`,
  `MACCount`, and `MACAddress`.

The old timestamp is useful provenance but is not, by itself, proof that a
particular strict-legacy U-disk was created in 2008.  The evidence below is
the recovered machine-code algorithm, not the timestamp.

## CRC implementation

`EDP_DiskNumber@0x10012C90` enumerates physical-drive identity material and
then reduces the assembled byte string to one DWORD.

At `0x10013051` it calls `fcn.10013170`, which initializes the CRC lookup
table.  The table generator repeatedly shifts right and conditionally XORs
`0xEDB88320`, i.e. the reflected IEEE CRC-32 polynomial.

At `0x100130A2` it calls `fcn.10013210` with:

- initial CRC = `0`;
- pointer to the assembled identity bytes;
- byte length of that identity;
- the table initialized above.

`fcn.10013210` performs:

```text
crc = ~initial
for each byte:
    crc = (crc >> 8) ^ table[(crc ^ byte) & 0xff]
return ~crc
```

Therefore `EDP_DiskNumber` returns the standard reflected IEEE CRC-32 of
the assembled host physical-disk identity material. `EDP_DeviceNumber`
also calls the same `fcn.10013170 / fcn.10013210` pair for its composite
host identity path.

## Protocol consequence

Historical `CEMSUsbRegsiter.dll v19.11.4.1` calls the companion
`UsbTools.dll` disk-number entry first and falls back to the device-number
entry.  It stores that single DWORD in both the restore-node
`MyHardinfo` family and LBA8 `HDSerialInfo` family.  This establishes the
meaning of those fields as a host-identity CRC family more precisely than
the earlier generic “hardware serial” label.

This still does **not** close the strict-legacy profile:

1. the v19.11.4.1 writer is a transitional generation whose
   `UsbOnlyInfo` behavior differs from the strict-legacy gold samples; and
2. `DeviceNumber.dll` returns one DWORD, while
   `UsbLabelParam::HDOnlySerial[5]` is five DWORDs.  No machine-code path
   has yet been found that expands this single result into those five
   values or otherwise generates the strict nonzero five-slot sequence.

Accordingly LBA4 `MyHardinfo`, LBA8 `HDSerialInfo`, and especially LBA4
`HSerialCRC[5]` keep their existing strict status until the exact
generation/profile producer is recovered.

