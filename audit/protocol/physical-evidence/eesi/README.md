# Netac EESI-enabled physical capture provenance

Artifact: `netac_onlydisk_20260804_lba0_12.bin`

- Length: 6656 bytes (exactly LBA0-LBA12).
- SHA-256: `3c7e795b1b7110e9866dd31f44ba6e7c5e02ff77a1f70a8b11fcdcaf181fbf39`.
- Original workstation source: `/Users/zhangyuxi/Desktop/u_disk/utils/backup/disk4_20260804_080927.bin`.
- Original companion metadata is preserved as `netac_onlydisk_20260804_lba0_12.meta.json`; it records `device_id=disk&ven_netac&prod_onlydisk&rev_0000`, CRC32 `5088ee37`, capture time `20260804_080927`, size 6656, and MD5 `db17edf8246ad55e9800b36701afd8e4`.
- Capture tool: `/Users/zhangyuxi/Desktop/u_disk/utils/make_big_boot.py`, SHA-256 `d72f6fcd192e92e6423e3b078cffa46725d8e781cdbd48dfcdaa470b72d208bd` at provenance audit time.

The capture tool's `read_lba()` opens `/dev/rdiskN` with `O_RDONLY` and uses `pread`. Its `backup()` reads LBA0-LBA12 first and writes the `.bin`, `.md5`, and `.meta.json`. In the `--apply` path, `backup()` is called before the second `YES` confirmation, before `unmountDisk`, and before either `write_lba()` / `pwrite`. Therefore this file is the pre-write physical state seen by that run, not output synthesized by the modification code.

For LBA10, CRC32(device_id)=`0x5088EE37`; decrypting the first 0x80 bytes yields `EESI`, DWORD flag `1`, GBK `交换区`, GBK `保密区`, followed by 88 zero bytes. The physical tail `+0x80..+0x1FF` is zero in this capture.

Scope: this artifact is a purpose-specific positive physical reference for the EESI-enabled profile. It is deliberately not added to the 19 strict-encrypted + 1 authentic-no-password general census and does not identify the executable that originally manufactured the disk.
