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
            println!("目标: --target mode0|mode1|mode2|mode3|plain");
            println!("    Plain 不是 mode4；它是独立的普通盘目标。");
            println!("    Plain 分区: 可重复 --partition START:SIZE:fat16|fat32|exfat[:LABEL]；SIZE 支持 sectors/MiB/GiB/fill。");
            println!("    Plain 未指定 --partition 时默认 P1 从 LBA2048 占满至盘尾。");
            println!("    mode1 若识别到现有 mode0，将保留原 type4 位置/密钥并让 type2 扩满前部。");
            println!("    可选格式化: --format-boot --format-share --format-encrypt");
            println!("    文件系统: --boot-fs fat16|fat32|exfat --share-fs fat16|fat32|exfat --encrypt-fs fat16|fat32|exfat");
            println!("    各区卷标: --boot-label LABEL --share-label LABEL --encrypt-label LABEL");
            println!("新盘身份参数: [--label-id ID] --user USER --dept DEPT [--label LABEL]");
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
                "来源密码未指定表示 Unknown；存在的目标密码域默认 {}；卷标默认值: 启动区。",
                crate::provision::DEFAULT_KEY_DOMAIN_PASSWORD_TEXT
            );
            println!("标签标识未指定时自动生成一个合法 onlyid 候选；可通过 --label-id 手动覆盖。");
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
