use super::super::*;

pub(in crate::cli) fn list_needs_elevation(
    rows: &[Row],
    elevated: bool,
    has_sentinel: bool,
) -> bool {
    crate::application::device_scan_needs_elevation(rows, elevated, has_sentinel)
}

pub(in crate::cli) fn list_flow(runner: &SysRunner, backup_dir_flag: Option<String>) -> i32 {
    let bak = crate::application::resolve_backup_dir(backup_dir_flag.as_deref());
    let rows = crate::application::scan_device_dashboard(runner, &bak);

    // 先无特权只读探测；只有真实遇到 PermissionDenied 才自动请求平台管理员授权。
    // 无外接盘时不会无意义弹授权提示；需要权限时由平台层负责交互并重执行自身。
    let has_sentinel = std::env::args().any(|arg| arg == ELEVATED_FLAG);
    if list_needs_elevation(&rows, elevate::is_root(), has_sentinel) {
        println!("检测到外接盘，但读取身份、姓名和部门等裸盘信息需要管理员权限。");
        let argv = argv_with_backup_dir_for_elevation(backup_dir_flag.as_deref());
        return elevate::ensure_elevated(&argv);
    }

    print!("{}", print_disk_table(&rows));
    EXIT_OK
}
