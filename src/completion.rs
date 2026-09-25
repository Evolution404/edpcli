//! Shell Tab 补全脚本与动态候选提供器。
//!
//! v2 只动态提供物理盘、全局备份编号、备份文件名与 LBA0-12。用户级 onlyid/index
//! 已退出 CLI grammar，补全层不得重新暴露。

use crate::diskio;
use crate::sysinfo::{self, CmdRunner};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shell {
    Zsh,
    Bash,
    Fish,
}

impl Shell {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "zsh" => Some(Self::Zsh),
            "bash" => Some(Self::Bash),
            "fish" => Some(Self::Fish),
            _ => None,
        }
    }
}

/// 动态候选。输出值保持“每行一个纯值”，让三种 shell 直接消费。
pub fn dynamic_values(
    kind: &str,
    backup_dir_flag: Option<&str>,
    runner: &dyn CmdRunner,
) -> Vec<String> {
    match kind {
        "backup-number" => {
            let dir = diskio::resolve_backup_dir(backup_dir_flag);
            let count = diskio::scan_backup_names(&dir).len();
            (1..=count).map(|n| n.to_string()).collect()
        }
        "backup-file" => {
            let dir = diskio::resolve_backup_dir(backup_dir_flag);
            diskio::scan_backup_names(&dir)
                .into_iter()
                .filter_map(|path| path.file_name()?.to_str().map(str::to_string))
                .collect()
        }
        "disk" => sysinfo::list_usb_disks(runner)
            .into_iter()
            .map(|disk| crate::platform::disk_selector_value(disk.n))
            .collect(),
        "lba" => (0..crate::common::METADATA_SECTOR_COUNT)
            .map(|n| n.to_string())
            .collect(),
        _ => vec![],
    }
}

fn action_words(command: &str) -> String {
    crate::command_spec::command(command)
        .map(|spec| crate::command_spec::words(spec.actions.iter().map(|action| action.name)))
        .unwrap_or_default()
}

fn option_words(command: &str, action: Option<&str>) -> String {
    crate::command_spec::command(command)
        .map(|spec| crate::command_spec::words(spec.option_names(action)))
        .unwrap_or_default()
}

fn fish_option_lines() -> String {
    use std::collections::BTreeMap;

    let mut lines = Vec::new();
    for spec in crate::command_spec::top_level_specs() {
        let mut options = BTreeMap::new();
        for option in spec.options {
            options.insert(option.name, option.takes_value);
        }
        for action in spec.actions {
            for option in action.options {
                options.insert(option.name, option.takes_value);
            }
        }
        for (name, takes_value) in options {
            let long = name.trim_start_matches("--");
            let value = if takes_value { " -r" } else { "" };
            lines.push(format!(
                "complete -c edpcli -n '__fish_seen_subcommand_from {}' -l {}{}",
                spec.name, long, value
            ));
        }
    }
    lines.join("\n")
}

fn render_schema(template: &str) -> String {
    let top = crate::command_spec::words(
        crate::command_spec::top_level_specs()
            .iter()
            .map(|command| command.name),
    );
    template
        .replace("__TOP__", &top)
        .replace("__HELP_TOPICS__", &top)
        .replace("__BACKUP_ACTIONS__", &action_words("backup"))
        .replace("__PROVISION_ACTIONS__", &action_words("provision"))
        .replace("__INSPECT_ACTIONS__", &action_words("inspect"))
        .replace("__COMPLETION_ACTIONS__", &action_words("completion"))
        .replace(
            "__BACKUP_CREATE_OPTIONS__",
            &option_words("backup", Some("create")),
        )
        .replace(
            "__BACKUP_LIST_OPTIONS__",
            &option_words("backup", Some("list")),
        )
        .replace(
            "__BACKUP_RESTORE_OPTIONS__",
            &option_words("backup", Some("restore")),
        )
        .replace(
            "__BACKUP_VERIFY_OPTIONS__",
            &option_words("backup", Some("verify")),
        )
        .replace(
            "__BACKUP_DELETE_OPTIONS__",
            &option_words("backup", Some("delete")),
        )
        .replace(
            "__BACKUP_PRUNE_OPTIONS__",
            &option_words("backup", Some("prune")),
        )
        .replace(
            "__PROVISION_PLAN_OPTIONS__",
            &option_words("provision", Some("plan")),
        )
        .replace(
            "__PROVISION_IMAGE_OPTIONS__",
            &option_words("provision", Some("image")),
        )
        .replace(
            "__PROVISION_WRITE_OPTIONS__",
            &option_words("provision", Some("write")),
        )
        .replace("__INFO_OPTIONS__", &option_words("info", None))
        .replace("__INSPECT_OPTIONS__", &option_words("inspect", None))
        .replace("__LIST_OPTIONS__", &option_words("list", None))
        .replace("__FISH_OPTIONS__", &fish_option_lines())
}

pub fn script(shell: Shell) -> String {
    render_schema(match shell {
        Shell::Zsh => ZSH,
        Shell::Bash => BASH,
        Shell::Fish => FISH,
    })
}

const ZSH: &str = r#"#compdef edpcli

autoload -Uz compinit
(( $+functions[compdef] )) || compinit

_edpcli_flag_value() {
  local flag="$1" i
  REPLY=""
  for (( i=2; i <= ${#words}; i++ )); do
    if [[ "${words[i]}" == "$flag" && $((i+1)) -le ${#words} ]]; then
      REPLY="${words[i+1]}"
    elif [[ "${words[i]}" == ${flag}=* ]]; then
      REPLY="${words[i]#${flag}=}"
    fi
  done
}

_edpcli_dynamic() {
  local kind="$1"; shift
  local -a vals
  vals=("${(@f)$(command edpcli __complete "$kind" "$@" 2>/dev/null)}")
  (( ${#vals} )) && compadd -- $vals
}

_edpcli_backup_targets() {
  local bak="$1"
  if [[ -n "$bak" ]]; then
    _edpcli_dynamic backup-number --backup-dir "$bak"
    _edpcli_dynamic backup-file --backup-dir "$bak"
  else
    _edpcli_dynamic backup-number
    _edpcli_dynamic backup-file
  fi
  _files
}

_edpcli() {
  local cur prev cmd action bak
  cur="${words[CURRENT]}"
  prev="${words[CURRENT-1]}"
  cmd="${words[2]}"
  action="${words[3]}"
  _edpcli_flag_value --backup-dir; bak="$REPLY"

  if (( CURRENT == 2 )); then
    compadd -- __TOP__
    return
  fi

  case "$prev" in
    --disk) _edpcli_dynamic disk; return ;;
    --lba) _edpcli_dynamic lba; return ;;
    --backup-dir|--export|--out) _files -/; return ;;
  esac

  case "$cmd" in
    backup)
      if (( CURRENT == 3 )); then
        compadd -- __BACKUP_ACTIONS__
        return
      fi
      if [[ "$cur" == -* ]]; then
        case "$action" in
          create)  compadd -- __BACKUP_CREATE_OPTIONS__ ;;
          restore) compadd -- __BACKUP_RESTORE_OPTIONS__ ;;
          verify)  compadd -- __BACKUP_VERIFY_OPTIONS__ ;;
          delete)  compadd -- __BACKUP_DELETE_OPTIONS__ ;;
          prune)   compadd -- __BACKUP_PRUNE_OPTIONS__ ;;
          list)    compadd -- __BACKUP_LIST_OPTIONS__ ;;
        esac
      elif [[ "$action" == restore || "$action" == verify || "$action" == delete ]]; then
        _edpcli_backup_targets "$bak"
      fi
      ;;
    provision)
      if (( CURRENT == 3 )); then
        compadd -- __PROVISION_ACTIONS__
        return
      fi
      if [[ "$cur" == -* ]]; then
        case "$action" in
          plan)    compadd -- __PROVISION_PLAN_OPTIONS__ ;;
          image)   compadd -- __PROVISION_IMAGE_OPTIONS__ ;;
          write)   compadd -- __PROVISION_WRITE_OPTIONS__ ;;
        esac
      fi
      ;;
    info)
      if [[ "$cur" == -* ]]; then
        compadd -- __INFO_OPTIONS__
      else
        _edpcli_backup_targets "$bak"
      fi
      ;;
    inspect)
      if (( CURRENT == 3 )); then
        compadd -- __INSPECT_ACTIONS__
      elif [[ "$cur" == -* ]]; then
        compadd -- __INSPECT_OPTIONS__
      else
        _edpcli_backup_targets "$bak"
      fi
      ;;
    list)    [[ "$cur" == -* ]] && compadd -- __LIST_OPTIONS__ ;;
    completion) (( CURRENT == 3 )) && compadd -- __COMPLETION_ACTIONS__ ;;
    help) (( CURRENT == 3 )) && compadd -- __HELP_TOPICS__ ;;
  esac
}

# v2 backup create / backup restore / backup verify / backup delete / backup prune
compdef _edpcli edpcli
"#;

const BASH: &str = r#"_edpcli_flag_value() {
  local flag="$1" i
  EDPCLI_VALUE=""
  for (( i=1; i<${#COMP_WORDS[@]}; i++ )); do
    if [[ "${COMP_WORDS[i]}" == "$flag" && $((i+1)) -lt ${#COMP_WORDS[@]} ]]; then
      EDPCLI_VALUE="${COMP_WORDS[i+1]}"
    elif [[ "${COMP_WORDS[i]}" == ${flag}=* ]]; then
      EDPCLI_VALUE="${COMP_WORDS[i]#${flag}=}"
    fi
  done
}

_edpcli_backup_targets() {
  local bak="$1" cur="$2" vals
  if [[ -n "$bak" ]]; then
    vals="$(edpcli __complete backup-number --backup-dir "$bak" 2>/dev/null)
$(edpcli __complete backup-file --backup-dir "$bak" 2>/dev/null)"
  else
    vals="$(edpcli __complete backup-number 2>/dev/null)
$(edpcli __complete backup-file 2>/dev/null)"
  fi
  COMPREPLY=( $(compgen -W "$vals" -- "$cur") $(compgen -f -- "$cur") )
}

_edpcli() {
  local cur prev cmd action bak vals
  COMPREPLY=()
  cur="${COMP_WORDS[COMP_CWORD]}"
  prev="${COMP_WORDS[COMP_CWORD-1]}"
  cmd="${COMP_WORDS[1]}"
  action="${COMP_WORDS[2]}"
  _edpcli_flag_value --backup-dir; bak="$EDPCLI_VALUE"

  if (( COMP_CWORD == 1 )); then
    COMPREPLY=( $(compgen -W '__TOP__' -- "$cur") )
    return
  fi

  case "$prev" in
    --disk)
      vals="$(edpcli __complete disk 2>/dev/null)"
      COMPREPLY=( $(compgen -W "$vals" -- "$cur") ); return ;;
    --lba)
      vals="$(edpcli __complete lba 2>/dev/null)"
      COMPREPLY=( $(compgen -W "$vals" -- "$cur") ); return ;;
    --backup-dir|--export|--out)
      COMPREPLY=( $(compgen -d -- "$cur") ); return ;;
  esac

  case "$cmd" in
    backup)
      if (( COMP_CWORD == 2 )); then
        COMPREPLY=( $(compgen -W '__BACKUP_ACTIONS__' -- "$cur") )
      elif [[ "$cur" == -* ]]; then
        case "$action" in
          create)  vals='__BACKUP_CREATE_OPTIONS__' ;;
          restore) vals='__BACKUP_RESTORE_OPTIONS__' ;;
          verify)  vals='__BACKUP_VERIFY_OPTIONS__' ;;
          delete)  vals='__BACKUP_DELETE_OPTIONS__' ;;
          prune)   vals='__BACKUP_PRUNE_OPTIONS__' ;;
          list)    vals='__BACKUP_LIST_OPTIONS__' ;;
        esac
        COMPREPLY=( $(compgen -W "$vals" -- "$cur") )
      elif [[ "$action" == restore || "$action" == verify || "$action" == delete ]]; then
        _edpcli_backup_targets "$bak" "$cur"
      fi ;;
    provision)
      if (( COMP_CWORD == 2 )); then
        COMPREPLY=( $(compgen -W '__PROVISION_ACTIONS__' -- "$cur") )
      elif [[ "$cur" == -* ]]; then
        case "$action" in
          plan)    vals='__PROVISION_PLAN_OPTIONS__' ;;
          image)   vals='__PROVISION_IMAGE_OPTIONS__' ;;
          write)   vals='__PROVISION_WRITE_OPTIONS__' ;;
        esac
        COMPREPLY=( $(compgen -W "$vals" -- "$cur") )
      fi ;;
    info)
      if [[ "$cur" == -* ]]; then
        vals='__INFO_OPTIONS__'
        COMPREPLY=( $(compgen -W "$vals" -- "$cur") )
      else
        _edpcli_backup_targets "$bak" "$cur"
      fi ;;
    inspect)
      if (( COMP_CWORD == 2 )); then
        COMPREPLY=( $(compgen -W '__INSPECT_ACTIONS__' -- "$cur") )
      elif [[ "$cur" == -* ]]; then
        vals='__INSPECT_OPTIONS__'
        COMPREPLY=( $(compgen -W "$vals" -- "$cur") )
      else
        _edpcli_backup_targets "$bak" "$cur"
      fi ;;
    list) vals='__LIST_OPTIONS__'; COMPREPLY=( $(compgen -W "$vals" -- "$cur") ) ;;
    completion) COMPREPLY=( $(compgen -W '__COMPLETION_ACTIONS__' -- "$cur") ) ;;
    help) COMPREPLY=( $(compgen -W '__HELP_TOPICS__' -- "$cur") ) ;;
  esac
}

# v2 backup create / backup restore / backup verify / backup delete / backup prune
complete -F _edpcli edpcli
"#;

const FISH: &str = r#"function __edpcli_flag_value
    set -l flag $argv[1]
    set -l tokens (commandline -opc)
    for i in (seq (count $tokens))
        if test "$tokens[$i]" = "$flag"
            set -l j (math $i + 1)
            if test $j -le (count $tokens)
                echo $tokens[$j]
            end
        else if string match -q "$flag=*" -- $tokens[$i]
            string replace "$flag=" '' -- $tokens[$i]
        end
    end
end

function __edpcli_backup_numbers
    set -l bak (__edpcli_flag_value --backup-dir)
    if test -n "$bak"
        edpcli __complete backup-number --backup-dir "$bak" 2>/dev/null
    else
        edpcli __complete backup-number 2>/dev/null
    end
end

function __edpcli_backup_files
    set -l bak (__edpcli_flag_value --backup-dir)
    if test -n "$bak"
        edpcli __complete backup-file --backup-dir "$bak" 2>/dev/null
    else
        edpcli __complete backup-file 2>/dev/null
    end
end

function __edpcli_wants_backup_target
    set -l tokens (commandline -opc)
    if test (count $tokens) -lt 2
        return 1
    end
    if contains -- $tokens[2] info inspect
        return 0
    end
    if test "$tokens[2]" = backup; and test (count $tokens) -ge 3
        if contains -- $tokens[3] restore verify delete
            return 0
        end
    end
    return 1
end

complete -c edpcli -f
complete -c edpcli -n '__fish_use_subcommand' -a '__TOP__'
complete -c edpcli -n '__fish_seen_subcommand_from backup' -a '__BACKUP_ACTIONS__'
complete -c edpcli -n '__fish_seen_subcommand_from provision' -a '__PROVISION_ACTIONS__'
__FISH_OPTIONS__
complete -c edpcli -n '__edpcli_wants_backup_target' -a '(__edpcli_backup_numbers) (__edpcli_backup_files)'
complete -c edpcli -n '__fish_seen_subcommand_from inspect info backup provision' -l disk -r -a '(edpcli __complete disk 2>/dev/null)'
complete -c edpcli -n '__fish_seen_subcommand_from inspect' -l lba -r -a '(edpcli __complete lba 2>/dev/null)'
complete -c edpcli -n '__fish_seen_subcommand_from completion' -a '__COMPLETION_ACTIONS__'

# v2 backup create / backup restore / backup verify / backup delete / backup prune
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_scripts_match_v2_grammar() {
        for shell in [Shell::Zsh, Shell::Bash, Shell::Fish] {
            let script = script(shell);
            for required in [
                "__complete",
                "list",
                "info",
                "backup",
                "provision",
                "inspect",
                "backup create",
                "backup restore",
                "no-force-change-password",
                "cancel-password-complexity-check",
                "enforce-password-complexity-check",
                "share-max-password-errors",
                "encrypt-max-password-errors",
            ] {
                assert!(script.contains(required), "{shell:?} missing {required}");
            }
            for removed in ["--onlyid", "--index", "metainfo", "backup rm"] {
                assert!(
                    !script.contains(removed),
                    "{shell:?} leaked removed grammar {removed}"
                );
            }
        }
    }
}
