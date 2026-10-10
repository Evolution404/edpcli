pub fn usage_text() -> String {
    use std::fmt::Write as _;

    let mut out = format!(
        "edpcli — EDP/cems U 盘管理 CLI v{}\n\n用法: edpcli [命令] [选项]\n\n",
        env!("CARGO_PKG_VERSION")
    );
    for spec in crate::command_spec::top_level_specs() {
        let _ = writeln!(out, "  {:<10} {}", spec.name, spec.summary);
    }
    out.push_str(
        "\n交互式终端中无参数 edpcli 默认进入 TUI；管道/重定向等非 TTY 环境仍等价于 edpcli list。\n",
    );
    out
}

pub fn print_usage() {
    print!("{}", usage_text());
}

fn print_topic_help(topic: &str) {
    use crate::ui::bold;

    let Some(spec) = crate::command_spec::command(topic) else {
        print_usage();
        return;
    };
    println!("{}", bold(&format!("用法: {}", spec.usage)));
    if !spec.actions.is_empty() {
        for action in spec.actions {
            println!("  {:<10} {}", action.name, action.summary);
        }
    }

    match topic {
        "list" | "tui" => {
            println!("--include-virtual：显示已验证的外部虚拟 Disk Image 设备；虚拟盘写入另需经过独立身份验证与原生块事务。");
        }
        "info" => {
            println!("未指定来源且只有一个可用目标盘时自动选择；多盘时交互选择。");
        }
        "inspect" => {
            println!("raw=物理原始字节；decode=按已验证区域算法解码；meta=结构化区域与字段语义。");
            println!("--lba 支持 7,12,240250283 或 240250283-240250288；--count 仅能与单个起始 LBA 同用。");
        }
        "backup" => {
            println!("create 创建元数据备份。backup 无动作时等价于 list。");
        }
        "provision" => {
            println!("正式设备规划/镜像/写入命令：--include-virtual 将已确认的虚拟Disk Image纳入可选设备；选中后使用相同原生块WAL事务链。");
            println!("目标: --target mode0|mode1|mode2|mode3|plain");
            println!("    Plain 不是 mode4；它是独立的普通盘目标。");
            println!("    Plain 分区: 可重复 --partition START:SIZE:fat16|fat32|exfat[:LABEL]；SIZE 支持 sectors/MiB/GiB/fill。");
            println!("    Plain 未指定 --partition 时默认 P1 从 LBA2048 占满至盘尾。");
            println!("    离线原生镜像：provision image --target plain --total-sectors N --sector-bytes 512|4096 --out FILE [--partition ...]；仅创建新普通文件，不连接U盘。");
            println!("    4Kn EDP虚拟演示：provision image --target mode0|mode1|mode2|mode3 --synthetic-demo --sector-bytes 4096 --total-sectors 262144 --algorithm sms4|aes|aes-cross --out FILE；仅固定测试身份及测试密钥，绝不可存真实数据。");
            println!("    通用来源认证计划：provision plan --target mode0|mode1|mode2|mode3|plain --disk N --source-backup FILE.edpb [普通plan参数]；先逐块核对来源EDPB v4，再走相同原生计划器，不写盘。");
            println!("    4Kn来源Mode1离线增量写集镜像：provision image --source-backup FILE.edpb --target mode1 --out FILE；重建明文二合一exFAT、保留原加密区元数据，不连接实体U盘；稀疏镜像不包含原保密区用户数据，不可整体dd写盘。");
            println!("    4Kn 实盘只读备份比对：provision verify-source --disk N --backup FILE.edpb（完整原生协议、LCE和分区首块；不卸载、不写盘、不解禁）。");
            println!("    v4同几何恢复只读预览：provision restore-preview --disk N --backup FILE.edpb [--include-virtual]；只核对设备身份/原生几何与计划元数据写集，evidence-only未授权恢复。");

            println!("    正式 plan / image / write 默认保留兼容的原分区数据和 FileKey；仅对新建或几何/文件系统不兼容分区规划格式化重建。");
            println!("    使用 --format-boot / --format-share / --format-encrypt 可主动格式化指定分区。--preserve-unformatted 禁止隐式格式化任何未指定分区（不兼容则拒绝）。");
            println!("    文件系统: --boot-fs fat16|fat32|exfat --share-fs fat16|fat32|exfat --encrypt-fs fat16|fat32|exfat");
            println!("    各区卷标: --boot-label LABEL --share-label LABEL --encrypt-label LABEL");
            println!("新盘身份参数: [--label-id ID] --user USER --dept DEPT [--label LABEL]");
            println!("    --prompt-passwords：提权后依次读取共享原/目标、加密原/目标密码；原密码可留空，目标密码必填。支持交互或四行标准输入。");
            println!(
                "密码域: [--share-source-password PASSWORD] [--share-target-password PASSWORD]"
            );
            println!(
                "        [--encrypt-source-password PASSWORD] [--encrypt-target-password PASSWORD]"
            );
            println!(
                "标签默认值: {}；可通过 --label 自定义。",
                crate::provision::DEFAULT_SAFE6_LABEL
            );
            println!(
                "来源密码未指定先尝试已证实的默认密码，否则可保持严格同构的原密钥封装；未指定目标密码表示不要求改密，新建分区的密码默认为 {}。",
                crate::provision::DEFAULT_KEY_DOMAIN_PASSWORD_TEXT
            );
            println!("标签标识未指定时继承已确认来源OnlyID，普通盘随机生成合法候选；可通过 --label-id 手动覆盖。");
            println!("密码策略: 未指定时继承注册盘可靠 PassInfo；普通盘默认 强制改密=否、取消复杂性验证=否、两区最大错误次数=255。");
            println!("    --force-change-password / --no-force-change-password");
            println!(
                "    --cancel-password-complexity-check / --enforce-password-complexity-check"
            );
            println!(
                "    --share-max-password-errors N --encrypt-max-password-errors N   (0..255)"
            );
            println!("分区参数: --boot-mib N / --boot-sectors N、--share-mib N、--encrypt-mib N；mode0 未指定启动区时默认 20417 扇区。");
            println!(
                "当前可写文件系统为 FAT16/FAT32/exFAT；加密分区使用已验证的 SM4(mode2) 扇区变换。"
            );
        }
        "completion" => {
            println!("zsh : eval \"$(edpcli completion zsh)\"");
            println!("bash: eval \"$(edpcli completion bash)\"");
            println!("fish: edpcli completion fish | source");
        }
        _ => {}
    }
}

pub(crate) fn print_help(topic: Option<&str>) {
    match topic {
        Some(topic) => print_topic_help(topic),
        None => print_usage(),
    }
}
