//! 自动提权: 需要裸盘读写的子命令在非 root 时以 sudo 重新执行自身。
//! 状态经 argv 传递(不依赖环境变量, sudo 会清环境); 继承 stdio 使
//! sudo 密码与 YES 确认提示都能交互; `--_elevated` 哨兵防异常 sudoers 配置下的重入循环。

use std::process::Command;

/// 内部哨兵旗标(追加到重执行 argv 末尾; 参数解析层识别并剥离)。
pub const ELEVATED_FLAG: &str = "--_elevated";

pub fn is_root() -> bool {
    // std 无 geteuid; 一次廉价子进程判定
    Command::new("id")
        .arg("-u")
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim() == "0")
        .unwrap_or(false)
}

/// 非 root 时: 打印提示并以 sudo 重执行自身(继承 stdio), 以子进程退出码结束进程。
/// 已带哨兵却仍非 root → 拒绝(sudo 未生效, 避免循环)。
pub fn ensure_elevated(argv: &[String]) {
    if is_root() {
        return;
    }
    if argv.contains(&ELEVATED_FLAG.to_string()) {
        eprintln!("错误: sudo 未授予 root 权限, 终止以避免循环。请检查 sudoers 配置。");
        std::process::exit(crate::common::EXIT_IO);
    }
    let exe = match std::env::current_exe() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("错误: 无法定位自身可执行文件: {}", e);
            std::process::exit(crate::common::EXIT_IO);
        }
    };
    let mut cmd = Command::new("sudo");
    cmd.arg(&exe);
    for a in argv {
        if a != ELEVATED_FLAG {
            cmd.arg(a);
        }
    }
    cmd.arg(ELEVATED_FLAG);
    println!("需要管理员权限(裸盘读写): 以 sudo 重运行 …");
    let status = match cmd.status() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("错误: 无法启动 sudo: {}", e);
            std::process::exit(crate::common::EXIT_IO);
        }
    };
    std::process::exit(status.code().unwrap_or(crate::common::EXIT_IO));
}

/// 构建重执行 argv(测试用): 原参数 + 哨兵, 不含已在其中的哨兵重复。
pub fn elevated_argv(argv: &[String]) -> Vec<String> {
    let mut v: Vec<String> = argv.iter().filter(|a| a.as_str() != ELEVATED_FLAG).cloned().collect();
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
