# nopwd_tool — 常用命令
# 备份目录固定为仓库 backup/(sudo 会清环境变量, 故在命令行显式传入)
PY   ?= python3
BK   ?= $(CURDIR)/backup

# 常用参数直接用变量:
#   make run DISK=4                预览指定盘
#   make apply DISK=4 SIZE=100     写入指定盘 + Share 100GB
#   make apply FORCE=1             盘已是免密盘仍强制重写(默认拒绝)
#   make restore RESTORE=<备份.bin> APPLY=1   还原写入
DISK    ?=
SIZE    ?=
RESTORE ?=
FLAGS   := $(if $(DISK),--disk $(DISK)) $(if $(SIZE),--size $(SIZE)) $(if $(APPLY),--apply) \
           $(if $(FORCE),--force)

# 任意参数透传: make run -- --disk 4 --size 100
# ('--' 让 make 停止解析自身选项, 之后的内容原样传给 nopwd; 直接写 --disk 会被 make 吞掉)
EXTRA := $(wordlist 2,$(words $(MAKECMDGOALS)),$(MAKECMDGOALS))
ifneq ($(filter $(firstword $(MAKECMDGOALS)),run apply restore list),)
  ifneq ($(EXTRA),)
    $(eval $(EXTRA): ; @:)
  endif
endif

.PHONY: test check list run apply restore clean help

test:            ## 运行测试套件(不碰真盘, 文件镜像 + 真实备份金标)
	$(PY) -m unittest discover -s tests -v

check: test      ## 编译检查 + 测试
	$(PY) -m compileall -q nopwd tests

list:            ## 列出外接盘(编号/容量/接口/cems识别/备份份数)
	sudo NOPWD_BACKUP_DIR="$(BK)" $(PY) -m nopwd --list $(EXTRA)

run:             ## 预览改造(dry-run): make run [DISK=4] 或 make run -- --disk 4
	sudo NOPWD_BACKUP_DIR="$(BK)" $(PY) -m nopwd $(FLAGS) $(EXTRA)

apply:           ## 实际写入(自动备份 → 原子写入 → 读回校验): make apply [DISK=4] [SIZE=100]
	sudo NOPWD_BACKUP_DIR="$(BK)" $(PY) -m nopwd --apply $(FLAGS) $(EXTRA)

restore:         ## 备份: make restore [RESTORE=<备份.bin>] [APPLY=1 写入]
	sudo NOPWD_BACKUP_DIR="$(BK)" $(PY) -m nopwd --restore $(if $(RESTORE),"$(RESTORE)") $(FLAGS) $(EXTRA)

clean:
	rm -rf nopwd/__pycache__ tests/__pycache__

help:            ## 显示目标列表
	@grep -E '^[a-z-]+:.*?##' $(MAKEFILE_LIST) | awk 'BEGIN{FS=":.*?## "} {printf "  make %-10s %s\n", $$1, $$2}'

.DEFAULT_GOAL := help
