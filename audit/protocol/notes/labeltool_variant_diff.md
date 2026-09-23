# Safe U-disk label tool local variant audit

Status: **verified local integrity boundary** (2026-09-21).

The three executables currently present under
`~/Desktop/u_disk/VRV/cems/ydcc/` are not three historical product
generations. They share the same PE layout, compile timestamp and version
resource:

- PE compile timestamp: 2024-01-07 03:22:07
- File/Product version: `8198.2104.17.2157`
- size: 1,258,312 bytes
- `cemssafeudisklabeltool_orig.exe`
  SHA-256 `b530a82b29bbc43be8d415225392ca135ab7df8a8d9f69c6598493c4942e9e11`
- `cemssafeudisklabeltool_2ndbackup.exe`
  SHA-256 `8ce3f107e13df8dac1a752d2875d1f6089e586c90309aca3b6c2ce02d6ec6415`
- `cemssafeudisklabeltool.exe`
  SHA-256 `1b1ddfb92298f2860dfa82952557139d57e87daa6e387507f4bbd9ab18c27427`

## Patch lineage

`_orig.exe` is the only unmodified baseline among these three files.

`_2ndbackup.exe` differs from `_orig.exe` at exactly one file byte:

- file offset `0x48F76`, VA `0x00449B76`
- original `75 76` = `jne 0x00449BEE`
- patched `EB 76` = unconditional `jmp 0x00449BEE`

The skipped branch displays
`"No terminal tool policy, label tool prohibited from starting"`; therefore
this is a local policy-bypass patch, not a protocol-generation change.

The current `cemssafeudisklabeltool.exe` contains that same bypass plus
additional patches. Relative to `_orig.exe` it changes 83 bytes. The
important executable changes are:

1. `0x0042DDC0` changes the normal-policy getter tail into a jump to a code
   cave at `0x004289FB`.
2. The code cave writes fixed values into the returned policy object at
   offsets `+0x20/+0x3C/+0x4C/+0x50/+0x7C/+0x148`, then returns. The same
   object is consumed by `writeLabel.cpp`; the executable's own
   `PrintPolicy` format names this policy family `normalDetail`.
3. `0x00471C84` changes the all-`"0000"` branch from `xor al,al`
   (false) to `mov al,1` (true).
4. Remaining changed bytes are the corresponding PE header/relocation
   bookkeeping for the injected code.

These changes are useful for reconstructing the local test environment, but
**must not be cited as official producer evidence** for any LBA field.

## Official front-end boundary from the original binary

The unmodified `_orig.exe` dynamically loads
`/usbtoolBusManage.dll` in `fcn.0042FDB0`, resolves
`CreateBusManageImp`, creates the BusManage object, and initializes its
callback through the virtual interface.

The current paired `usbtoolBusManage.dll` has
`BusManageImp` vtable base `0x100E14FC`. The vtable slot at byte offset
`+0x20` is `BusManageImp::WriteLabel@0x100A28E0`. At entry it copies the
entire caller request as `0x265` DWORDs plus one WORD
(`0x996 = 2454` bytes) into its local request object before calling the
internal WriteLabel implementation.

This is the verified front-end -> business-layer boundary for the
2024/current generation. It does not identify the missing CEMS2.0/join59
writer and does not connect the historical Netac formatter to a legacy EDP
profile selector.

