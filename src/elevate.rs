//! 自动提权：需要裸盘读写的子命令在权限不足时由平台层重新执行自身。
//! 状态经 argv 传递，不依赖提权后的环境继承；`--_elevated` 哨兵防异常重入循环。

/// 内部哨兵旗标(追加到重执行 argv 末尾; 参数解析层识别并剥离)。
pub const ELEVATED_FLAG: &str = "--_elevated";

pub fn is_root() -> bool {
    crate::platform::is_elevated()
}

/// 权限不足时：打印提示并通过平台层重执行自身，以子进程退出码结束进程。
/// 已带哨兵却仍未提权 → 拒绝，避免循环。
pub fn ensure_elevated(argv: &[String]) {
    if is_root() {
        return;
    }
    if argv.contains(&ELEVATED_FLAG.to_string()) {
        eprintln!("错误: 管理员权限未生效，终止以避免循环。请检查本平台的提权配置。");
        std::process::exit(crate::common::EXIT_IO);
    }
    let exe = match std::env::current_exe() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("错误: 无法定位自身可执行文件: {}", e);
            std::process::exit(crate::common::EXIT_IO);
        }
    };
    println!(
        "需要管理员权限(裸盘访问): 通过 {} 重新启动 …",
        crate::platform::elevation_label()
    );
    let code = match crate::platform::run_elevated(&exe, argv, ELEVATED_FLAG) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("错误: 无法获取管理员权限: {}", e);
            std::process::exit(crate::common::EXIT_IO);
        }
    };
    std::process::exit(code);
}

/// 构建重执行 argv(测试用): 原参数 + 哨兵, 不含已在其中的哨兵重复。
pub fn elevated_argv(argv: &[String]) -> Vec<String> {
    let mut v: Vec<String> = argv
        .iter()
        .filter(|a| a.as_str() != ELEVATED_FLAG)
        .cloned()
        .collect();
    v.push(ELEVATED_FLAG.to_string());
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn elevated_argv_appends_sentinel_once() {
        let argv: Vec<String> = vec!["apply".into(), "--disk".into(), "6".into()];
        let v = elevated_argv(&argv);
        assert_eq!(v, vec!["apply", "--disk", "6", ELEVATED_FLAG]);
        // 幂等: 已带哨兵不重复
        let argv2: Vec<String> = vec!["apply".into(), ELEVATED_FLAG.into()];
        assert_eq!(elevated_argv(&argv2), vec!["apply", ELEVATED_FLAG]);
    }
}
