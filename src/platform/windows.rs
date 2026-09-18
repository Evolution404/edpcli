use std::collections::BTreeMap;
use std::ffi::{c_void, OsStr};
use std::fs::File;
use std::io;
use std::mem::size_of;
use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::ptr::{null, null_mut};

use super::{ExtDisk, HardwareProbe, InquiryInfo, NativeTransport, PlatformKind};
use crate::common::SECTOR;
use crate::sysinfo::CmdRunner;
use windows_sys::Win32::Devices::DeviceAndDriverInstallation::{
    CM_Get_Device_IDW, CM_Get_Parent, SetupDiDestroyDeviceInfoList, SetupDiEnumDeviceInterfaces,
    SetupDiGetClassDevsW, SetupDiGetDeviceInterfaceDetailW, CR_SUCCESS, DIGCF_DEVICEINTERFACE,
    DIGCF_PRESENT, HDEVINFO, SP_DEVICE_INTERFACE_DATA, SP_DEVICE_INTERFACE_DETAIL_DATA_W,
    SP_DEVINFO_DATA,
};
use windows_sys::Win32::Foundation::{
    CloseHandle, GetLastError, ERROR_NO_MORE_FILES, GENERIC_READ, GENERIC_WRITE, HANDLE,
    INVALID_HANDLE_VALUE,
};
use windows_sys::Win32::Security::{
    GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY,
};
#[cfg(feature = "ci-virtual-disk")]
use windows_sys::Win32::Storage::FileSystem::{BusTypeFileBackedVirtual, BusTypeVirtual};
use windows_sys::Win32::Storage::FileSystem::{
    BusTypeUsb, CreateFileW, FindFirstVolumeW, FindNextVolumeW, FindVolumeClose,
    GetVolumeNameForVolumeMountPointW, GetVolumePathNameW, FILE_ATTRIBUTE_NORMAL, FILE_SHARE_READ,
    FILE_SHARE_WRITE, IOCTL_VOLUME_GET_VOLUME_DISK_EXTENTS, OPEN_EXISTING,
};
use windows_sys::Win32::System::Ioctl::{
    PropertyStandardQuery, StorageDeviceProperty, DISK_GEOMETRY_EX, FSCTL_DISMOUNT_VOLUME,
    FSCTL_LOCK_VOLUME, FSCTL_UNLOCK_VOLUME, GUID_DEVINTERFACE_DISK,
    IOCTL_DISK_GET_DRIVE_GEOMETRY_EX, IOCTL_STORAGE_GET_DEVICE_NUMBER,
    IOCTL_STORAGE_QUERY_PROPERTY, STORAGE_DEVICE_DESCRIPTOR, STORAGE_DEVICE_NUMBER,
    STORAGE_PROPERTY_QUERY, VOLUME_DISK_EXTENTS,
};
use windows_sys::Win32::System::SystemInformation::GetWindowsDirectoryW;
use windows_sys::Win32::System::Threading::{
    GetCurrentProcess, GetExitCodeProcess, OpenProcessToken, WaitForSingleObject, INFINITE,
};
use windows_sys::Win32::System::IO::DeviceIoControl;
use windows_sys::Win32::UI::Shell::{ShellExecuteExW, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW};

pub(super) const fn kind() -> PlatformKind {
    PlatformKind::Windows
}

fn wide(value: &str) -> Vec<u16> {
    OsStr::new(value)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

struct OwnedHandle(HANDLE);

impl OwnedHandle {
    fn get(&self) -> HANDLE {
        self.0
    }
}

impl Drop for OwnedHandle {
    fn drop(&mut self) {
        if !self.0.is_null() && self.0 != INVALID_HANDLE_VALUE {
            unsafe { CloseHandle(self.0) };
        }
    }
}

fn open_handle(path: &str, access: u32, share: u32) -> io::Result<OwnedHandle> {
    let path = wide(path);
    let handle = unsafe {
        CreateFileW(
            path.as_ptr(),
            access,
            share,
            null(),
            OPEN_EXISTING,
            FILE_ATTRIBUTE_NORMAL,
            null_mut(),
        )
    };
    if handle == INVALID_HANDLE_VALUE {
        Err(io::Error::last_os_error())
    } else {
        Ok(OwnedHandle(handle))
    }
}

fn device_io(
    handle: HANDLE,
    code: u32,
    input: Option<(*const c_void, u32)>,
    output: Option<(*mut c_void, u32)>,
) -> io::Result<u32> {
    let (input_ptr, input_len) = input.unwrap_or((null(), 0));
    let (output_ptr, output_len) = output.unwrap_or((null_mut(), 0));
    let mut returned = 0u32;
    let ok = unsafe {
        DeviceIoControl(
            handle,
            code,
            input_ptr,
            input_len,
            output_ptr,
            output_len,
            &mut returned,
            null_mut(),
        )
    };
    if ok == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(returned)
    }
}

fn physical_path(disk: u32) -> String {
    format!(r"\\.\PhysicalDrive{disk}")
}

pub(super) fn raw_disk_path(disk: u32) -> String {
    physical_path(disk)
}

pub(super) fn parse_disk_selector(value: &str) -> Result<u32, String> {
    if !value.is_empty() && value.bytes().all(|b| b.is_ascii_digit()) {
        return value
            .parse::<u32>()
            .map_err(|_| format!("磁盘编号超出范围: {value}"));
    }
    let lower = value.to_ascii_lowercase();
    for prefix in [r"\\.\physicaldrive", "physicaldrive"] {
        if lower.starts_with(prefix) {
            let suffix = &value[prefix.len()..];
            if !suffix.is_empty() && suffix.bytes().all(|b| b.is_ascii_digit()) {
                return suffix
                    .parse::<u32>()
                    .map_err(|_| format!("磁盘编号超出范围: {value}"));
            }
        }
    }
    Err(format!("无法解析 Windows PhysicalDrive 选择器: {value}"))
}

pub(super) const fn disk_selector_syntax() -> &'static str {
    r"--disk <N|PhysicalDriveN|\\.\PhysicalDriveN>"
}

pub(super) fn disk_selector_value(disk: u32) -> String {
    physical_path(disk)
}

#[derive(Debug, Clone)]
struct WinDiskProbe {
    size: u64,
    removable: bool,
    usb: bool,
    #[cfg(feature = "ci-virtual-disk")]
    bus_type: i32,
    vendor: String,
    product: String,
    revision: String,
}

struct DevInfoSet(HDEVINFO);

impl Drop for DevInfoSet {
    fn drop(&mut self) {
        unsafe { SetupDiDestroyDeviceInfoList(self.0) };
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct UsbIdentity {
    vid: Option<u16>,
    pid: Option<u16>,
    transport: NativeTransport,
}

fn parse_hex_tag(text: &str, tag: &str) -> Option<u16> {
    let upper = text.to_ascii_uppercase();
    let pos = upper.find(tag)? + tag.len();
    u16::from_str_radix(upper.get(pos..pos + 4)?, 16).ok()
}

fn usb_identity_from_instance_chain(ids: &[String]) -> Option<UsbIdentity> {
    let mut vid = None;
    let mut pid = None;
    let mut transport = NativeTransport::Unknown;
    for id in ids {
        let upper = id.to_ascii_uppercase();
        vid = vid.or_else(|| parse_hex_tag(&upper, "VID_"));
        pid = pid.or_else(|| parse_hex_tag(&upper, "PID_"));
        if upper.contains("UASPSTOR") {
            transport = NativeTransport::Uas;
        } else if transport == NativeTransport::Unknown && upper.contains("USBSTOR") {
            transport = NativeTransport::Bot;
        }
    }
    (vid.is_some() || pid.is_some() || transport != NativeTransport::Unknown).then_some(
        UsbIdentity {
            vid,
            pid,
            transport,
        },
    )
}

fn devinst_id(devinst: u32) -> Option<String> {
    let mut buffer = vec![0u16; 1024];
    let rc = unsafe { CM_Get_Device_IDW(devinst, buffer.as_mut_ptr(), buffer.len() as u32, 0) };
    if rc != CR_SUCCESS {
        return None;
    }
    let len = buffer.iter().position(|&c| c == 0).unwrap_or(buffer.len());
    Some(String::from_utf16_lossy(&buffer[..len]))
}

fn devinst_chain(mut devinst: u32) -> Vec<String> {
    let mut ids = Vec::new();
    for _ in 0..16 {
        if let Some(id) = devinst_id(devinst) {
            ids.push(id);
        }
        let mut parent = 0u32;
        let rc = unsafe { CM_Get_Parent(&mut parent, devinst, 0) };
        if rc != CR_SUCCESS || parent == devinst {
            break;
        }
        devinst = parent;
    }
    ids
}

fn interface_device_number(path: &str) -> Option<u32> {
    let handle = open_handle(path, 0, FILE_SHARE_READ | FILE_SHARE_WRITE).ok()?;
    let mut number = STORAGE_DEVICE_NUMBER::default();
    device_io(
        handle.get(),
        IOCTL_STORAGE_GET_DEVICE_NUMBER,
        None,
        Some((
            (&mut number as *mut STORAGE_DEVICE_NUMBER).cast(),
            size_of::<STORAGE_DEVICE_NUMBER>() as u32,
        )),
    )
    .ok()?;
    Some(number.DeviceNumber)
}

fn setupapi_disk_map() -> Vec<(u32, Option<UsbIdentity>)> {
    let set = unsafe {
        SetupDiGetClassDevsW(
            &GUID_DEVINTERFACE_DISK,
            null(),
            null_mut(),
            DIGCF_PRESENT | DIGCF_DEVICEINTERFACE,
        )
    };
    if set == -1isize {
        return vec![];
    }
    let set = DevInfoSet(set);
    let mut disks = BTreeMap::<u32, Option<UsbIdentity>>::new();

    for index in 0..256u32 {
        let mut interface = SP_DEVICE_INTERFACE_DATA {
            cbSize: size_of::<SP_DEVICE_INTERFACE_DATA>() as u32,
            ..SP_DEVICE_INTERFACE_DATA::default()
        };
        let ok = unsafe {
            SetupDiEnumDeviceInterfaces(
                set.0,
                null(),
                &GUID_DEVINTERFACE_DISK,
                index,
                &mut interface,
            )
        };
        if ok == 0 {
            break;
        }

        let mut required = 0u32;
        unsafe {
            SetupDiGetDeviceInterfaceDetailW(
                set.0,
                &interface,
                null_mut(),
                0,
                &mut required,
                null_mut(),
            )
        };
        if required < size_of::<SP_DEVICE_INTERFACE_DETAIL_DATA_W>() as u32 {
            continue;
        }
        let mut storage = vec![0u64; (required as usize).div_ceil(size_of::<u64>())];
        let detail = storage
            .as_mut_ptr()
            .cast::<SP_DEVICE_INTERFACE_DETAIL_DATA_W>();
        unsafe {
            (*detail).cbSize = size_of::<SP_DEVICE_INTERFACE_DETAIL_DATA_W>() as u32;
        }
        let mut devinfo = SP_DEVINFO_DATA {
            cbSize: size_of::<SP_DEVINFO_DATA>() as u32,
            ..SP_DEVINFO_DATA::default()
        };
        let ok = unsafe {
            SetupDiGetDeviceInterfaceDetailW(
                set.0,
                &interface,
                detail,
                required,
                &mut required,
                &mut devinfo,
            )
        };
        if ok == 0 {
            continue;
        }
        let path_offset = std::mem::offset_of!(SP_DEVICE_INTERFACE_DETAIL_DATA_W, DevicePath);
        let detail_bytes = storage.len() * size_of::<u64>();
        if path_offset >= detail_bytes {
            continue;
        }
        let max_chars = (detail_bytes - path_offset) / size_of::<u16>();
        let path_ptr = unsafe { (*detail).DevicePath.as_ptr() };
        let path_slice = unsafe { std::slice::from_raw_parts(path_ptr, max_chars) };
        let path_len = path_slice.iter().position(|&c| c == 0).unwrap_or(max_chars);
        let path = String::from_utf16_lossy(&path_slice[..path_len]);
        let Some(disk) = interface_device_number(&path) else {
            continue;
        };
        let identity = usb_identity_from_instance_chain(&devinst_chain(devinfo.DevInst));
        match disks.get_mut(&disk) {
            Some(existing) if existing.is_none() && identity.is_some() => *existing = identity,
            Some(_) => {}
            None => {
                disks.insert(disk, identity);
            }
        }
    }
    disks.into_iter().collect()
}

fn setupapi_usb_identity(disk: u32) -> Option<UsbIdentity> {
    setupapi_disk_map()
        .into_iter()
        .find_map(|(number, identity)| (number == disk).then_some(identity).flatten())
}

fn descriptor_string(bytes: &[u8], offset: u32) -> String {
    let offset = offset as usize;
    if offset == 0 || offset >= bytes.len() {
        return String::new();
    }
    let tail = &bytes[offset..];
    let end = tail.iter().position(|&b| b == 0).unwrap_or(tail.len());
    String::from_utf8_lossy(&tail[..end]).trim().to_string()
}

fn query_disk(disk: u32) -> io::Result<WinDiskProbe> {
    // desired_access=0 足够做 storage property/geometry 查询，并允许普通用户先枚举选盘。
    let handle = open_handle(&physical_path(disk), 0, FILE_SHARE_READ | FILE_SHARE_WRITE)?;

    let query = STORAGE_PROPERTY_QUERY {
        PropertyId: StorageDeviceProperty,
        QueryType: PropertyStandardQuery,
        AdditionalParameters: [0],
    };
    // u64 backing 保证 STORAGE_DEVICE_DESCRIPTOR 所需对齐。
    let mut descriptor_words = vec![0u64; 512];
    let descriptor_bytes_len = descriptor_words.len() * size_of::<u64>();
    let returned = device_io(
        handle.get(),
        IOCTL_STORAGE_QUERY_PROPERTY,
        Some((
            (&query as *const STORAGE_PROPERTY_QUERY).cast(),
            size_of::<STORAGE_PROPERTY_QUERY>() as u32,
        )),
        Some((
            descriptor_words.as_mut_ptr().cast(),
            descriptor_bytes_len as u32,
        )),
    )? as usize;
    if returned < size_of::<STORAGE_DEVICE_DESCRIPTOR>() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "STORAGE_DEVICE_DESCRIPTOR 返回过短",
        ));
    }
    let descriptor = unsafe {
        &*(descriptor_words
            .as_ptr()
            .cast::<STORAGE_DEVICE_DESCRIPTOR>())
    };
    let bytes = unsafe {
        std::slice::from_raw_parts(
            descriptor_words.as_ptr().cast::<u8>(),
            returned.min(descriptor_bytes_len),
        )
    };

    let mut geometry_words = [0u64; 32];
    let geometry_returned = device_io(
        handle.get(),
        IOCTL_DISK_GET_DRIVE_GEOMETRY_EX,
        None,
        Some((
            geometry_words.as_mut_ptr().cast(),
            (geometry_words.len() * size_of::<u64>()) as u32,
        )),
    )? as usize;
    if geometry_returned < size_of::<DISK_GEOMETRY_EX>() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "DISK_GEOMETRY_EX 返回过短",
        ));
    }
    let geometry = unsafe { &*(geometry_words.as_ptr().cast::<DISK_GEOMETRY_EX>()) };
    if geometry.DiskSize <= 0 {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "磁盘容量无效"));
    }

    Ok(WinDiskProbe {
        size: geometry.DiskSize as u64,
        removable: descriptor.RemovableMedia,
        usb: descriptor.BusType == BusTypeUsb,
        #[cfg(feature = "ci-virtual-disk")]
        bus_type: descriptor.BusType,
        vendor: descriptor_string(bytes, descriptor.VendorIdOffset),
        product: descriptor_string(bytes, descriptor.ProductIdOffset),
        revision: descriptor_string(bytes, descriptor.ProductRevisionOffset),
    })
}

pub(super) fn hardware_probe(disk: u32) -> Option<HardwareProbe> {
    let probe = query_disk(disk).ok()?;
    let usb = setupapi_usb_identity(disk);
    Some(HardwareProbe {
        vid: usb.and_then(|id| id.vid),
        pid: usb.and_then(|id| id.pid),
        transport: usb
            .map(|id| id.transport)
            .unwrap_or(NativeTransport::Unknown),
        inquiry: (!probe.vendor.is_empty()).then_some(InquiryInfo {
            vendor: probe.vendor,
            product: probe.product,
            revision: probe.revision,
        }),
    })
}

pub(super) fn fallback_hardware_probe(
    _runner: &dyn CmdRunner,
    _disk: u32,
) -> Option<HardwareProbe> {
    None
}

pub(super) fn list_external_disks(_runner: &dyn CmdRunner) -> Vec<ExtDisk> {
    setupapi_disk_map()
        .into_iter()
        .filter_map(|(disk, usb)| {
            let probe = query_disk(disk).ok()?;
            if !probe.usb && !probe.removable {
                return None;
            }
            Some(ExtDisk {
                n: disk,
                size: probe.size,
                vid: usb
                    .and_then(|id| id.vid)
                    .map(|vid| format!("{vid:04x}"))
                    .unwrap_or_else(|| "xxxx".into()),
                pid: usb
                    .and_then(|id| id.pid)
                    .map(|pid| format!("{pid:04x}"))
                    .unwrap_or_else(|| "xxxx".into()),
                proto: if probe.usb {
                    "USB".into()
                } else {
                    "Removable".into()
                },
            })
        })
        .collect()
}

pub(super) fn disk_total_sectors(_runner: &dyn CmdRunner, disk: u32) -> Option<u64> {
    query_disk(disk)
        .ok()
        .map(|probe| probe.size / SECTOR as u64)
}

pub(super) fn usb_vid_pid(_runner: &dyn CmdRunner, disk: u32) -> (String, String) {
    let Some(identity) = setupapi_usb_identity(disk) else {
        return ("xxxx".into(), "xxxx".into());
    };
    let vid = identity
        .vid
        .map(|value| format!("{value:04x}"))
        .unwrap_or_else(|| "xxxx".into());
    let pid = identity
        .pid
        .map(|value| format!("{value:04x}"))
        .unwrap_or_else(|| "xxxx".into());
    (vid, pid)
}

fn volume_extents(handle: HANDLE) -> io::Result<Vec<u32>> {
    let mut words = vec![0u64; 512];
    let returned = device_io(
        handle,
        IOCTL_VOLUME_GET_VOLUME_DISK_EXTENTS,
        None,
        Some((
            words.as_mut_ptr().cast(),
            (words.len() * size_of::<u64>()) as u32,
        )),
    )? as usize;
    if returned < size_of::<VOLUME_DISK_EXTENTS>() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "卷 extent 返回过短",
        ));
    }
    let extents = unsafe { &*(words.as_ptr().cast::<VOLUME_DISK_EXTENTS>()) };
    let count = extents.NumberOfDiskExtents as usize;
    let required = size_of::<u32>()
        .saturating_add(
            count.saturating_mul(size_of::<windows_sys::Win32::System::Ioctl::DISK_EXTENT>()),
        )
        .saturating_add(8);
    if count == 0 || required > returned + 8 || required > words.len() * size_of::<u64>() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "卷 extent 数量异常",
        ));
    }
    let slice = unsafe { std::slice::from_raw_parts(extents.Extents.as_ptr(), count) };
    Ok(slice.iter().map(|extent| extent.DiskNumber).collect())
}

fn enumerate_volumes() -> io::Result<Vec<String>> {
    let mut buffer = vec![0u16; 1024];
    let find = unsafe { FindFirstVolumeW(buffer.as_mut_ptr(), buffer.len() as u32) };
    if find == INVALID_HANDLE_VALUE {
        return Err(io::Error::last_os_error());
    }
    let mut names = Vec::new();
    loop {
        let len = buffer.iter().position(|&c| c == 0).unwrap_or(buffer.len());
        if len > 0 {
            names.push(String::from_utf16_lossy(&buffer[..len]));
        }
        let ok = unsafe { FindNextVolumeW(find, buffer.as_mut_ptr(), buffer.len() as u32) };
        if ok == 0 {
            let error = unsafe { GetLastError() };
            if error == ERROR_NO_MORE_FILES {
                break;
            }
            unsafe { FindVolumeClose(find) };
            return Err(io::Error::from_raw_os_error(error as i32));
        }
    }
    unsafe { FindVolumeClose(find) };
    Ok(names)
}

fn volume_open_path(volume_name: &str) -> String {
    volume_name.trim_end_matches('\\').to_string()
}

fn windows_volume_name() -> Option<String> {
    let mut windows_dir = vec![0u16; 32768];
    let n = unsafe { GetWindowsDirectoryW(windows_dir.as_mut_ptr(), windows_dir.len() as u32) };
    if n == 0 || n as usize >= windows_dir.len() {
        return None;
    }
    windows_dir.truncate(n as usize);
    windows_dir.push(0);

    let mut mount = vec![0u16; 32768];
    let ok =
        unsafe { GetVolumePathNameW(windows_dir.as_ptr(), mount.as_mut_ptr(), mount.len() as u32) };
    if ok == 0 {
        return None;
    }
    let mount_len = mount.iter().position(|&c| c == 0)?;
    mount.truncate(mount_len + 1);

    let mut volume = vec![0u16; 32768];
    let ok = unsafe {
        GetVolumeNameForVolumeMountPointW(mount.as_ptr(), volume.as_mut_ptr(), volume.len() as u32)
    };
    if ok == 0 {
        return None;
    }
    let len = volume.iter().position(|&c| c == 0)?;
    Some(String::from_utf16_lossy(&volume[..len]))
}

fn system_disk_numbers() -> Vec<u32> {
    let Some(volume) = windows_volume_name() else {
        return vec![];
    };
    let Ok(handle) = open_handle(
        &volume_open_path(&volume),
        0,
        FILE_SHARE_READ | FILE_SHARE_WRITE,
    ) else {
        return vec![];
    };
    volume_extents(handle.get()).unwrap_or_default()
}

fn is_system_disk_number(disk: u32) -> bool {
    let system = system_disk_numbers();
    // 无法确定系统卷映射时 fail-closed，不能把未知盘当成安全目标。
    system.is_empty() || system.contains(&disk)
}

pub(super) fn is_system_disk(_runner: &dyn CmdRunner, disk: u32) -> bool {
    is_system_disk_number(disk)
}

pub(super) struct WriteGuard {
    locked_volumes: Vec<OwnedHandle>,
}

impl Drop for WriteGuard {
    fn drop(&mut self) {
        for handle in &self.locked_volumes {
            let _ = device_io(handle.get(), FSCTL_UNLOCK_VOLUME, None, None);
        }
    }
}

fn prepare_write_disk(disk: u32) -> io::Result<WriteGuard> {
    if is_system_disk_number(disk) {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "目标磁盘承载 Windows 系统卷或系统卷映射不可确认，拒绝写盘",
        ));
    }
    let mut locked = Vec::new();
    for volume in enumerate_volumes()? {
        let path = volume_open_path(&volume);
        // 先用 access=0 查询归属，避免“打不开读写就跳过”造成目标卷漏锁。
        let query = open_handle(&path, 0, FILE_SHARE_READ | FILE_SHARE_WRITE).map_err(|error| {
            io::Error::new(
                error.kind(),
                format!("无法查询卷 {volume} 的磁盘归属: {error}"),
            )
        })?;
        let extents = volume_extents(query.get()).map_err(|error| {
            io::Error::new(
                error.kind(),
                format!("无法读取卷 {volume} 的 disk extents: {error}"),
            )
        })?;
        if !extents.contains(&disk) {
            continue;
        }
        drop(query);
        let handle = open_handle(
            &path,
            GENERIC_READ | GENERIC_WRITE,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
        )
        .map_err(|error| {
            io::Error::new(
                error.kind(),
                format!("目标卷 {volume} 无法以读写方式打开: {error}"),
            )
        })?;
        device_io(handle.get(), FSCTL_LOCK_VOLUME, None, None).map_err(|error| {
            io::Error::new(error.kind(), format!("无法锁定卷 {volume}: {error}"))
        })?;
        if let Err(error) = device_io(handle.get(), FSCTL_DISMOUNT_VOLUME, None, None) {
            let _ = device_io(handle.get(), FSCTL_UNLOCK_VOLUME, None, None);
            return Err(io::Error::new(
                error.kind(),
                format!("无法卸载卷 {volume}: {error}"),
            ));
        }
        locked.push(handle);
    }
    Ok(WriteGuard {
        locked_volumes: locked,
    })
}

pub(super) fn prepare_write(_runner: &dyn CmdRunner, disk: u32) -> io::Result<WriteGuard> {
    prepare_write_disk(disk)
}

#[cfg(feature = "ci-virtual-disk")]
pub(super) fn ci_prepare_virtual_write(path: &str) -> io::Result<WriteGuard> {
    let disk = parse_disk_selector(path)
        .map_err(|message| io::Error::new(io::ErrorKind::InvalidInput, message))?;
    let probe = query_disk(disk)?;
    if probe.bus_type != BusTypeVirtual && probe.bus_type != BusTypeFileBackedVirtual {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            format!(
                "CI 虚拟磁盘拒绝非 Virtual/FileBackedVirtual 目标: bus_type={}",
                probe.bus_type
            ),
        ));
    }
    prepare_write_disk(disk)
}

pub(super) fn is_elevated() -> bool {
    let mut token: HANDLE = null_mut();
    let opened = unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) };
    if opened == 0 || token.is_null() {
        return false;
    }
    let token = OwnedHandle(token);
    let mut elevation = TOKEN_ELEVATION { TokenIsElevated: 0 };
    let mut returned = 0u32;
    let ok = unsafe {
        GetTokenInformation(
            token.get(),
            TokenElevation,
            (&mut elevation as *mut TOKEN_ELEVATION).cast(),
            size_of::<TOKEN_ELEVATION>() as u32,
            &mut returned,
        )
    };
    ok != 0 && elevation.TokenIsElevated != 0
}

pub(super) fn invoking_user_home() -> Option<PathBuf> {
    std::env::var_os("USERPROFILE").map(PathBuf::from)
}

pub(super) fn has_elevation_origin() -> bool {
    false
}

pub(super) fn probe_command_cacheable(_cmd: &[&str]) -> bool {
    false
}

pub(super) fn is_raw_device_path(path: &str) -> bool {
    path.to_ascii_lowercase().starts_with(r"\\.\physicaldrive")
}

pub(super) fn raw_busy_error(error: &io::Error) -> bool {
    // ERROR_SHARING_VIOLATION：卷锁释放/设备重枚举期间可短暂出现。
    error.raw_os_error() == Some(32)
}

pub(super) fn sync_raw_device(file: &File) -> io::Result<()> {
    file.sync_all()
}

pub(super) fn sync_directory(_path: &Path) -> io::Result<()> {
    // 备份文件与 sidecar 已分别 FlushFileBuffers(sync_all)。Windows std::fs::File
    // 不支持像 Unix 一样直接打开目录句柄，因此目录项由 NTFS/系统缓存负责。
    Ok(())
}

pub(super) const fn elevation_label() -> &'static str {
    "Windows UAC"
}

fn quote_windows_arg(arg: &str) -> String {
    if !arg.is_empty() && !arg.chars().any(|c| c.is_whitespace() || c == '"') {
        return arg.to_string();
    }
    let mut out = String::from("\"");
    let mut backslashes = 0usize;
    for ch in arg.chars() {
        match ch {
            '\\' => backslashes += 1,
            '"' => {
                out.push_str(&"\\".repeat(backslashes * 2 + 1));
                out.push('"');
                backslashes = 0;
            }
            _ => {
                out.push_str(&"\\".repeat(backslashes));
                backslashes = 0;
                out.push(ch);
            }
        }
    }
    out.push_str(&"\\".repeat(backslashes * 2));
    out.push('"');
    out
}

pub(super) fn run_elevated(exe: &Path, argv: &[String], sentinel: &str) -> io::Result<i32> {
    let exe = wide(&exe.to_string_lossy());
    let verb = wide("runas");
    let mut args: Vec<String> = argv
        .iter()
        .filter(|arg| arg.as_str() != sentinel)
        .cloned()
        .collect();
    args.push(sentinel.to_string());
    let params = wide(
        &args
            .iter()
            .map(|arg| quote_windows_arg(arg))
            .collect::<Vec<_>>()
            .join(" "),
    );

    let mut info = SHELLEXECUTEINFOW {
        cbSize: size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_NOCLOSEPROCESS,
        lpVerb: verb.as_ptr(),
        lpFile: exe.as_ptr(),
        lpParameters: params.as_ptr(),
        nShow: 1, // SW_SHOWNORMAL
        ..SHELLEXECUTEINFOW::default()
    };
    let ok = unsafe { ShellExecuteExW(&mut info) };
    if ok == 0 || info.hProcess.is_null() {
        return Err(io::Error::last_os_error());
    }
    let process = OwnedHandle(info.hProcess);
    unsafe { WaitForSingleObject(process.get(), INFINITE) };
    let mut exit_code = crate::common::EXIT_IO as u32;
    let ok = unsafe { GetExitCodeProcess(process.get(), &mut exit_code) };
    if ok == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(exit_code as i32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn descriptor_string_is_bounds_checked() {
        let bytes = b"\0\0Netac\0OnlyDisk\0";
        assert_eq!(descriptor_string(bytes, 2), "Netac");
        assert_eq!(descriptor_string(bytes, 999), "");
        assert_eq!(descriptor_string(bytes, 0), "");
    }

    #[test]
    fn windows_argument_quoting_handles_spaces_quotes_and_trailing_slashes() {
        assert_eq!(quote_windows_arg("plain"), "plain");
        assert_eq!(quote_windows_arg("a b"), "\"a b\"");
        assert_eq!(quote_windows_arg(""), "\"\"");
        assert!(quote_windows_arg("a\\\"b").starts_with('"'));
    }

    #[test]
    fn parses_usb_identity_and_transport_from_instance_chain() {
        let ids = vec![
            r"SCSI\Disk&Ven_aigo&Prod_U335&Rev_PMAP".to_string(),
            r"UASPSTOR\Disk&Ven_aigo&Prod_U335&Rev_PMAP".to_string(),
            r"USB\VID_3535&PID_6300\123".to_string(),
        ];
        assert_eq!(
            usb_identity_from_instance_chain(&ids),
            Some(UsbIdentity {
                vid: Some(0x3535),
                pid: Some(0x6300),
                transport: NativeTransport::Uas,
            })
        );
    }

    #[test]
    fn transport_is_preserved_when_vid_pid_are_unavailable() {
        let ids = vec![r"UASPSTOR\Disk&Ven_aigo&Prod_U335&Rev_PMAP".to_string()];
        assert_eq!(
            usb_identity_from_instance_chain(&ids),
            Some(UsbIdentity {
                vid: None,
                pid: None,
                transport: NativeTransport::Uas,
            })
        );
    }

    #[test]
    fn bot_transport_is_detected_from_usbstor_chain() {
        let ids = vec![
            r"SCSI\Disk&Ven_Netac&Prod_OnlyDisk".to_string(),
            r"USBSTOR\Disk&Ven_Netac&Prod_OnlyDisk".to_string(),
            r"USB\VID_0D18&PID_2005\ABC".to_string(),
        ];
        assert_eq!(
            usb_identity_from_instance_chain(&ids),
            Some(UsbIdentity {
                vid: Some(0x0d18),
                pid: Some(0x2005),
                transport: NativeTransport::Bot,
            })
        );
    }

    #[test]
    fn uas_wins_over_bot_when_both_appear_in_parent_chain() {
        let ids = vec![
            r"USBSTOR\Disk&Ven_aigo&Prod_U335".to_string(),
            r"UASPSTOR\Disk&Ven_aigo&Prod_U335".to_string(),
            r"USB\VID_3535&PID_6300\123".to_string(),
        ];
        assert_eq!(
            usb_identity_from_instance_chain(&ids)
                .expect("USB identity")
                .transport,
            NativeTransport::Uas
        );
    }

    #[test]
    fn malformed_vid_pid_do_not_destroy_transport_signal() {
        let ids = vec![
            r"UASPSTOR\Disk&Ven_aigo&Prod_U335".to_string(),
            r"USB\VID_ZZZZ&PID_12\123".to_string(),
        ];
        assert_eq!(
            usb_identity_from_instance_chain(&ids),
            Some(UsbIdentity {
                vid: None,
                pid: None,
                transport: NativeTransport::Uas,
            })
        );
    }

    #[test]
    fn parses_windows_disk_selectors_case_insensitively() {
        assert_eq!(parse_disk_selector("7").unwrap(), 7);
        assert_eq!(parse_disk_selector("PhysicalDrive12").unwrap(), 12);
        assert_eq!(parse_disk_selector(r"\\.\physicaldrive3").unwrap(), 3);
        assert!(parse_disk_selector("C:").is_err());
        assert!(parse_disk_selector("PhysicalDrive").is_err());
    }
}
