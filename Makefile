# nopwd_tool — 常用命令
# 备份目录固定为仓库 backup/(sudo 会清环境变量, 故在命令行显式传入)
PY      ?= python3
BK      ?= $(CURDIR)/backup

.PHONY: test check run apply restore clean

test:            ## 运行测试套件(不碰真盘, 文件镜像 + 真实备份金标)
	$(PY) -m unittest discover -s tests -v

check: test      ## 编译检查 + 测试
	$(PY) -m compileall -q nopwd tests

run:             ## 预览改造(dry-run, 自动检测 USB 盘); ARGS="--disk 4" 可指定
	sudo NOPWD_BACKUP_DIR="$(BK)" $(PY) -m nopwd $(ARGS)

apply:           ## 实际写入(自动备份 → 原子写入 → 读回校验)
	sudo NOPWD_BACKUP_DIR="$(BK)" $(PY) -m nopwd --apply $(ARGS)

restore:         ## 列出本盘备份; make restore RESTORE=<备份.bin> 预检, 加 APPLY=1 写入
	sudo NOPWD_BACKUP_DIR="$(BK)" $(PY) -m nopwd --restore $(RESTORE) $(if $(APPLY),--apply) $(ARGS)

clean:
	rm -rf nopwd/__pycache__ tests/__pycache__

help:            ## 显示目标列表
	@grep -E '^[a-z-]+:.*?##' $(MAKEFILE_LIST) | awk 'BEGIN{FS=":.*?## "} {printf "  make %-10s %s\n", $$1, $$2}'

.PHONY: help
.DEFAULT_GOAL := help
