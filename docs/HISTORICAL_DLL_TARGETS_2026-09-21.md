# Historical SAFE6 DLL retrieval targets — 2026-09-21

This note records exact public archive identities needed to close remaining legacy producer/profile
branches. It does not change the strict protocol COMPLETE byte count.

## Same-era 2020 artifacts

### cemsusbregsiter.dll

- Version: 19.11.4.1
- 32-bit PE
- Archive date: 2020-12-17
- MD5: `783d01f19e998a514834bc5e5f4249ad`
- Public archive detail page:
  `https://www.ijinshan.com/filerepair/cemsusbregsiter.dll.shtml`

This binary has already been used by the main audit to recover the 2020 HDSerialInfo producer
family.

### safeusbregsitercems.dll

- Version: 19.4.4.2
- 32-bit PE
- Size: 158.11KB
- Archive date: 2020-12-16
- MD5: `e516454e5b37da8a853702aca7d4261c`
- Public archive detail page:
  `https://www.ijinshan.com/filerepair/safeusbregsitercems.dll.shtml`

This is now the preferred exact historical target for closing the legacy upper-level label-format
selection path around `CCEMSSafeUsbRegsiter::UsbFormat`.

### EdpEDiskCtrl.dll

- Version: 3.6.10.18
- 32-bit PE
- Archive date: 2020-05-19
- MD5: `95a06e0d466ba40a7d5c0e6a409e2114`
- Public archive detail page:
  `https://www.ijinshan.com/filerepair/edpediskctrl.dll.shtml`

This binary has already been used by the main audit as an independent 2020 runtime reader.

## Why the safeusbregsitercems target matters

The unresolved LBA0 bootstrap-body profile selection currently sits above the already-recovered
low-level Netac formatting exports. The current product chain reaches a
`SafeUsbRegsiterCems.dll!GetUsbTegsiterObj` object and a
`CCEMSSafeUsbRegsiter::UsbFormat` method, but the exact historical condition that selected the
legacy/Netac MBR bootstrap profile is not yet closed.

The exact 2020 `safeusbregsitercems.dll` identity above provides a reproducible target for that
missing selection logic. Until the binary is actually obtained and its call graph verified, it
must not be used as evidence to promote LBA0 bytes from PARTIAL to COMPLETE.
