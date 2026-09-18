//! Shell Tab 补全脚本与动态候选提供器。
//!
//! v2 只动态提供物理盘、全局备份编号、备份文件名与 LBA0-13。用户级 onlyid/index
//! 已退出 CLI grammar，补全层不得重新暴露。

use std::collections::BTreeSet;

use crate::backup_catalog::BackupCatalog;
use crate::diskio;
use crate::selectors::BackupSelector;
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
            let count = BackupSelector::load(&dir).numbered().len();
            (1..=count).map(|n| n.to_string()).collect()
        }
        "backup-file" => {
            let dir = diskio::resolve_backup_dir(backup_dir_flag);
            let mut names = BTreeSet::new();
            for entry in BackupCatalog::load(&dir).entries() {
                if let Some(name) = entry.path.file_name().and_then(|n| n.to_str()) {
                    names.insert(name.to_string());
                }
            }
            names.into_iter().collect()
        }
        "disk" => sysinfo::list_usb_disks(runner)
            .into_iter()
            .map(|disk| crate::platform::disk_selector_value(disk.n))
            .collect(),
        "lba" => (0..14).map(|n| n.to_string()).collect(),
        _ => vec![],
    }
}

pub fn script(shell: Shell) -> &'static str {
    match shell {
        Shell::Zsh => ZSH,
        Shell::Bash => BASH,
        Shell::Fish => FISH,
    }
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
    compadd -- list info apply backup inspect convert completion version help
    return
  fi

  case "$prev" in
    --disk) _edpcli_dynamic disk; return ;;
    --lba) _edpcli_dynamic lba; return ;;
    --backup-dir|--export|--dir|--out) _files -/; return ;;
  esac

  case "$cmd" in
    backup)
      if (( CURRENT == 3 )); then
        compadd -- create list restore verify delete prune
        return
      fi
      if [[ "$cur" == -* ]]; then
        case "$action" in
          create)  compadd -- --disk --backup-dir --help ;;
          restore) compadd -- --disk --yes --backup-dir --help ;;
          verify)  compadd -- --backup-dir --help ;;
          delete)  compadd -- --yes --backup-dir --help ;;
          prune)   compadd -- --keep --yes --backup-dir --help ;;
          list)    compadd -- --backup-dir --help ;;
        esac
      elif [[ "$action" == restore || "$action" == verify || "$action" == delete ]]; then
        _edpcli_backup_targets "$bak"
      fi
      ;;
    info)
      if [[ "$cur" == -* ]]; then
        compadd -- --disk --id --backup-dir --help
      else
        _edpcli_backup_targets "$bak"
      fi
      ;;
    inspect)
      if [[ "$cur" == -* ]]; then
        compadd -- --disk --lba --raw --hex --export --id --backup-dir --help
      else
        _edpcli_backup_targets "$bak"
      fi
      ;;
    apply)   [[ "$cur" == -* ]] && compadd -- --dry-run --disk --size --force --yes --backup-dir --help ;;
    convert) [[ "$cur" == -* ]] && compadd -- --dir --id --size --out --help ;;
    list)    [[ "$cur" == -* ]] && compadd -- --backup-dir --help ;;
    completion) (( CURRENT == 3 )) && compadd -- zsh bash fish ;;
    help) (( CURRENT == 3 )) && compadd -- list info apply backup inspect convert completion version ;;
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
    COMPREPLY=( $(compgen -W 'list info apply backup inspect convert completion version help' -- "$cur") )
    return
  fi

  case "$prev" in
    --disk)
      vals="$(edpcli __complete disk 2>/dev/null)"
      COMPREPLY=( $(compgen -W "$vals" -- "$cur") ); return ;;
    --lba)
      vals="$(edpcli __complete lba 2>/dev/null)"
      COMPREPLY=( $(compgen -W "$vals" -- "$cur") ); return ;;
    --backup-dir|--export|--dir|--out)
      COMPREPLY=( $(compgen -d -- "$cur") ); return ;;
  esac

  case "$cmd" in
    backup)
      if (( COMP_CWORD == 2 )); then
        COMPREPLY=( $(compgen -W 'create list restore verify delete prune' -- "$cur") )
      elif [[ "$cur" == -* ]]; then
        case "$action" in
          create)  vals='--disk --backup-dir --help' ;;
          restore) vals='--disk --yes --backup-dir --help' ;;
          verify)  vals='--backup-dir --help' ;;
          delete)  vals='--yes --backup-dir --help' ;;
          prune)   vals='--keep --yes --backup-dir --help' ;;
          list)    vals='--backup-dir --help' ;;
        esac
        COMPREPLY=( $(compgen -W "$vals" -- "$cur") )
      elif [[ "$action" == restore || "$action" == verify || "$action" == delete ]]; then
        _edpcli_backup_targets "$bak" "$cur"
      fi ;;
    info)
      if [[ "$cur" == -* ]]; then
        vals='--disk --id --backup-dir --help'
        COMPREPLY=( $(compgen -W "$vals" -- "$cur") )
      else
        _edpcli_backup_targets "$bak" "$cur"
      fi ;;
    inspect)
      if [[ "$cur" == -* ]]; then
        vals='--disk --lba --raw --hex --export --id --backup-dir --help'
        COMPREPLY=( $(compgen -W "$vals" -- "$cur") )
      else
        _edpcli_backup_targets "$bak" "$cur"
      fi ;;
    apply) vals='--dry-run --disk --size --force --yes --backup-dir --help'; COMPREPLY=( $(compgen -W "$vals" -- "$cur") ) ;;
    convert) vals='--dir --id --size --out --help'; COMPREPLY=( $(compgen -W "$vals" -- "$cur") ) ;;
    list) vals='--backup-dir --help'; COMPREPLY=( $(compgen -W "$vals" -- "$cur") ) ;;
    completion) COMPREPLY=( $(compgen -W 'zsh bash fish' -- "$cur") ) ;;
    help) COMPREPLY=( $(compgen -W 'list info apply backup inspect convert completion version' -- "$cur") ) ;;
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
complete -c edpcli -n '__fish_use_subcommand' -a 'list info apply backup inspect convert completion version help'
complete -c edpcli -n '__fish_seen_subcommand_from backup' -a 'create list restore verify delete prune'
complete -c edpcli -n '__edpcli_wants_backup_target' -a '(__edpcli_backup_numbers) (__edpcli_backup_files)'
complete -c edpcli -n '__fish_seen_subcommand_from inspect info apply backup' -l disk -r -a '(edpcli __complete disk 2>/dev/null)'
complete -c edpcli -n '__fish_seen_subcommand_from inspect' -l lba -r -a '(edpcli __complete lba 2>/dev/null)'
complete -c edpcli -n '__fish_seen_subcommand_from inspect' -l raw
complete -c edpcli -n '__fish_seen_subcommand_from inspect' -l hex
complete -c edpcli -n '__fish_seen_subcommand_from inspect' -l export -r
complete -c edpcli -n '__fish_seen_subcommand_from inspect info convert' -l id -r
complete -c edpcli -n '__fish_seen_subcommand_from backup inspect info list apply' -l backup-dir -r
complete -c edpcli -n '__fish_seen_subcommand_from backup apply' -l yes
complete -c edpcli -n '__fish_seen_subcommand_from backup' -l keep -r
complete -c edpcli -n '__fish_seen_subcommand_from apply convert' -l size -r
complete -c edpcli -n '__fish_seen_subcommand_from apply' -l dry-run
complete -c edpcli -n '__fish_seen_subcommand_from apply' -l force
complete -c edpcli -n '__fish_seen_subcommand_from convert' -l dir -r
complete -c edpcli -n '__fish_seen_subcommand_from convert' -l out -r
complete -c edpcli -n '__fish_seen_subcommand_from completion' -a 'zsh bash fish'

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
                "apply",
                "backup",
                "inspect",
                "backup create",
                "backup restore",
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
