//! Shell Tab 补全脚本与动态候选提供器。
//!
//! 补全脚本自身不依赖外部补全框架：zsh/bash/fish 只负责上下文判断；onlyid、备份编号、
//! 备份文件名和物理盘号由隐藏的 `edpcli __complete ...` 实时提供。

use std::collections::BTreeSet;

use crate::backup_catalog::BackupCatalog;
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

/// 动态候选。输出值故意保持“每行一个纯值”，让三种 shell 都能直接消费。
pub fn dynamic_values(
    kind: &str,
    onlyid: Option<&str>,
    backup_dir_flag: Option<&str>,
    runner: &dyn CmdRunner,
) -> Vec<String> {
    match kind {
        "onlyid" => {
            let dir = diskio::resolve_backup_dir(backup_dir_flag);
            BackupCatalog::load(&dir).onlyid_values()
        }
        "index" => {
            let Some(id) = onlyid else { return vec![] };
            let dir = diskio::resolve_backup_dir(backup_dir_flag);
            let count = BackupCatalog::load(&dir)
                .onlyid_group(id)
                .map(|group| group.len())
                .unwrap_or(0);
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
            .map(|d| d.n.to_string())
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

# `eval "$(edpcli completion zsh)"` 在尚未初始化 completion system 的干净 zsh
# 里也应直接可用，而不是要求用户先知道 compinit。
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

_edpcli() {
  local cur prev cmd action id bak
  cur="${words[CURRENT]}"
  prev="${words[CURRENT-1]}"
  cmd="${words[2]}"
  action="${words[3]}"
  _edpcli_flag_value --onlyid; id="$REPLY"
  _edpcli_flag_value --backup-dir; bak="$REPLY"

  if (( CURRENT == 2 )); then
    compadd -- list run apply restore backup inspect metainfo meta convert completion version help
    return
  fi

  case "$prev" in
    --onlyid)
      if [[ -n "$bak" ]]; then _edpcli_dynamic onlyid --backup-dir "$bak"; else _edpcli_dynamic onlyid; fi
      return ;;
    --index)
      if [[ -n "$id" ]]; then
        if [[ -n "$bak" ]]; then _edpcli_dynamic index --onlyid "$id" --backup-dir "$bak"; else _edpcli_dynamic index --onlyid "$id"; fi
      fi
      return ;;
    --disk)
      _edpcli_dynamic disk; return ;;
    --backup|--image)
      if [[ -n "$bak" ]]; then _edpcli_dynamic backup-file --backup-dir "$bak"; else _edpcli_dynamic backup-file; fi
      _files
      return ;;
    --backup-dir|--export|--dir|--out)
      _files -/; return ;;
  esac

  case "$cmd" in
    backup)
      if (( CURRENT == 3 )); then
        compadd -- list verify prune rm --onlyid --backup-dir --help
        return
      fi
      if [[ "$cur" == -* ]]; then
        case "$action" in
          rm)     compadd -- --onlyid --yes --backup-dir --help ;;
          prune)  compadd -- --onlyid --keep --yes --backup-dir --help ;;
          verify) compadd -- --onlyid --index --backup-dir --help ;;
          list)   compadd -- --onlyid --backup-dir --help ;;
          *)      compadd -- --onlyid --backup-dir --help ;;
        esac
      fi
      ;;
    inspect)
      if [[ "$cur" == -* ]]; then
        compadd -- --disk --backup --onlyid --index --raw --hex --export --id --backup-dir --help
      else
        _edpcli_dynamic lba
        _files
      fi
      ;;
    metainfo|meta)
      if [[ "$cur" == -* ]]; then
        compadd -- --disk --backup --onlyid --index --id --backup-dir --help
      elif (( CURRENT == 3 )); then
        if [[ -n "$bak" ]]; then _edpcli_dynamic onlyid --backup-dir "$bak"; else _edpcli_dynamic onlyid; fi
        _files
      elif (( CURRENT == 4 )) && [[ "${words[3]}" == <-> || "${words[3]}" == -<-> ]]; then
        if [[ -n "$bak" ]]; then
          _edpcli_dynamic index --onlyid "${words[3]}" --backup-dir "$bak"
        else
          _edpcli_dynamic index --onlyid "${words[3]}"
        fi
      fi
      ;;
    run)     [[ "$cur" == -* ]] && compadd -- --disk --size --backup-dir --help ;;
    apply)   [[ "$cur" == -* ]] && compadd -- --disk --size --force --yes --backup-dir --help ;;
    restore) [[ "$cur" == -* ]] && compadd -- --disk --yes --backup-dir --help ;;
    convert) [[ "$cur" == -* ]] && compadd -- --dir --id --size --out --help ;;
    list)    [[ "$cur" == -* ]] && compadd -- --backup-dir --help ;;
    completion)
      (( CURRENT == 3 )) && compadd -- zsh bash fish
      ;;
    help)
      (( CURRENT == 3 )) && compadd -- list run apply restore backup inspect metainfo meta convert completion
      ;;
  esac
}

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

_edpcli() {
  local cur prev cmd action id bak vals
  COMPREPLY=()
  cur="${COMP_WORDS[COMP_CWORD]}"
  prev="${COMP_WORDS[COMP_CWORD-1]}"
  cmd="${COMP_WORDS[1]}"
  action="${COMP_WORDS[2]}"
  _edpcli_flag_value --onlyid; id="$EDPCLI_VALUE"
  _edpcli_flag_value --backup-dir; bak="$EDPCLI_VALUE"

  if (( COMP_CWORD == 1 )); then
    COMPREPLY=( $(compgen -W 'list run apply restore backup inspect metainfo meta convert completion version help' -- "$cur") )
    return
  fi
  case "$prev" in
    --onlyid)
      vals="$(edpcli __complete onlyid ${bak:+--backup-dir "$bak"} 2>/dev/null)"
      COMPREPLY=( $(compgen -W "$vals" -- "$cur") ); return ;;
    --index)
      vals="$(edpcli __complete index ${id:+--onlyid "$id"} ${bak:+--backup-dir "$bak"} 2>/dev/null)"
      COMPREPLY=( $(compgen -W "$vals" -- "$cur") ); return ;;
    --disk)
      vals="$(edpcli __complete disk 2>/dev/null)"
      COMPREPLY=( $(compgen -W "$vals" -- "$cur") ); return ;;
    --backup|--image)
      vals="$(edpcli __complete backup-file ${bak:+--backup-dir "$bak"} 2>/dev/null)"
      COMPREPLY=( $(compgen -W "$vals" -- "$cur") $(compgen -f -- "$cur") ); return ;;
    --backup-dir|--export|--dir|--out)
      COMPREPLY=( $(compgen -d -- "$cur") ); return ;;
  esac

  case "$cmd" in
    backup)
      if (( COMP_CWORD == 2 )); then
        COMPREPLY=( $(compgen -W 'list verify prune rm --onlyid --backup-dir --help' -- "$cur") )
      elif [[ "$cur" == -* ]]; then
        case "$action" in
          rm) vals='--onlyid --yes --backup-dir --help' ;;
          prune) vals='--onlyid --keep --yes --backup-dir --help' ;;
          verify) vals='--onlyid --index --backup-dir --help' ;;
          *) vals='--onlyid --backup-dir --help' ;;
        esac
        COMPREPLY=( $(compgen -W "$vals" -- "$cur") )
      fi ;;
    inspect)
      if [[ "$cur" == -* ]]; then
        vals='--disk --backup --onlyid --index --raw --hex --export --id --backup-dir --help'
      else
        vals="$(edpcli __complete lba 2>/dev/null)"
      fi
      COMPREPLY=( $(compgen -W "$vals" -- "$cur") $(compgen -f -- "$cur") ) ;;
    metainfo|meta)
      if [[ "$cur" == -* ]]; then
        vals='--disk --backup --onlyid --index --id --backup-dir --help'
        COMPREPLY=( $(compgen -W "$vals" -- "$cur") )
      elif (( COMP_CWORD == 2 )); then
        vals="$(edpcli __complete onlyid ${bak:+--backup-dir "$bak"} 2>/dev/null)"
        COMPREPLY=( $(compgen -W "$vals" -- "$cur") $(compgen -f -- "$cur") )
      elif (( COMP_CWORD == 3 )) && [[ "${COMP_WORDS[2]}" =~ ^-?[0-9]+$ ]]; then
        vals="$(edpcli __complete index --onlyid "${COMP_WORDS[2]}" ${bak:+--backup-dir "$bak"} 2>/dev/null)"
        COMPREPLY=( $(compgen -W "$vals" -- "$cur") )
      fi ;;
    run) vals='--disk --size --backup-dir --help'; COMPREPLY=( $(compgen -W "$vals" -- "$cur") ) ;;
    apply) vals='--disk --size --force --yes --backup-dir --help'; COMPREPLY=( $(compgen -W "$vals" -- "$cur") ) ;;
    restore) vals='--disk --yes --backup-dir --help'; COMPREPLY=( $(compgen -W "$vals" -- "$cur") $(compgen -f -- "$cur") ) ;;
    convert) vals='--dir --id --size --out --help'; COMPREPLY=( $(compgen -W "$vals" -- "$cur") ) ;;
    list) vals='--backup-dir --help'; COMPREPLY=( $(compgen -W "$vals" -- "$cur") ) ;;
    completion) COMPREPLY=( $(compgen -W 'zsh bash fish' -- "$cur") ) ;;
    help) COMPREPLY=( $(compgen -W 'list run apply restore backup inspect metainfo meta convert completion' -- "$cur") ) ;;
  esac
}

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

function __edpcli_onlyids
    set -l bak (__edpcli_flag_value --backup-dir)
    if test -n "$bak"
        edpcli __complete onlyid --backup-dir "$bak" 2>/dev/null
    else
        edpcli __complete onlyid 2>/dev/null
    end
end

function __edpcli_indices
    set -l id (__edpcli_flag_value --onlyid)
    set -l bak (__edpcli_flag_value --backup-dir)
    if test -n "$id"
        if test -n "$bak"
            edpcli __complete index --onlyid "$id" --backup-dir "$bak" 2>/dev/null
        else
            edpcli __complete index --onlyid "$id" 2>/dev/null
        end
    end
end

function __edpcli_meta_positionals
    set -l tokens (commandline -opc)
    set -l bak (__edpcli_flag_value --backup-dir)
    if test (count $tokens) -eq 2
        __edpcli_onlyids
        return
    end
    if test (count $tokens) -eq 3; and string match -qr '^-?[0-9]+$' -- $tokens[3]
        if test -n "$bak"
            edpcli __complete index --onlyid "$tokens[3]" --backup-dir "$bak" 2>/dev/null
        else
            edpcli __complete index --onlyid "$tokens[3]" 2>/dev/null
        end
    end
end

complete -c edpcli -f
complete -c edpcli -n '__fish_use_subcommand' -a 'list run apply restore backup inspect metainfo meta convert completion version help'
complete -c edpcli -n '__fish_seen_subcommand_from backup' -a 'list verify prune rm'
complete -c edpcli -n '__fish_seen_subcommand_from metainfo meta' -a '(__edpcli_meta_positionals)'
complete -c edpcli -n '__fish_seen_subcommand_from inspect backup metainfo meta' -l onlyid -r -a '(__edpcli_onlyids)'
complete -c edpcli -n '__fish_seen_subcommand_from inspect backup metainfo meta' -l index -r -a '(__edpcli_indices)'
complete -c edpcli -n '__fish_seen_subcommand_from inspect metainfo meta run apply restore' -l disk -r -a '(edpcli __complete disk 2>/dev/null)'
complete -c edpcli -n '__fish_seen_subcommand_from inspect metainfo meta' -l backup -r -a '(edpcli __complete backup-file 2>/dev/null)'
complete -c edpcli -n '__fish_seen_subcommand_from inspect' -l raw
complete -c edpcli -n '__fish_seen_subcommand_from inspect' -l hex
complete -c edpcli -n '__fish_seen_subcommand_from inspect' -l export -r
complete -c edpcli -n '__fish_seen_subcommand_from inspect metainfo meta convert' -l id -r
complete -c edpcli -n '__fish_seen_subcommand_from backup inspect metainfo meta list run apply restore' -l backup-dir -r
complete -c edpcli -n '__fish_seen_subcommand_from backup apply restore' -l yes
complete -c edpcli -n '__fish_seen_subcommand_from backup' -l keep -r
complete -c edpcli -n '__fish_seen_subcommand_from run apply convert' -l size -r
complete -c edpcli -n '__fish_seen_subcommand_from apply' -l force
complete -c edpcli -n '__fish_seen_subcommand_from convert' -l dir -r
complete -c edpcli -n '__fish_seen_subcommand_from convert' -l out -r
complete -c edpcli -n '__fish_seen_subcommand_from completion' -a 'zsh bash fish'
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_scripts_reference_dynamic_provider() {
        for shell in [Shell::Zsh, Shell::Bash, Shell::Fish] {
            let s = script(shell);
            assert!(s.contains("__complete"));
            assert!(s.contains("inspect"));
            assert!(s.contains("backup"));
        }
    }
}
