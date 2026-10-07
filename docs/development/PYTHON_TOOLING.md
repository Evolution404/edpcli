# Python 工具链

edpcli 是 Rust 主项目；仓库内 Python 只承担工程、CI、审计、回放和协议复现脚本。
Python 环境由 `uv` 统一管理，不依赖系统 Python、全局 `pip`、Homebrew Python 或 Conda。

## 版本合同

- `.python-version` 固定日常开发解释器为 Python 3.14.5。
- `pyproject.toml` 声明最低兼容版本 `>=3.11`；CI 同时验证 3.11.15 和 3.14.5。
- `[tool.uv] python-preference = "only-managed"`，仓库命令只使用 uv 管理的解释器。
- `uv.lock` 必须提交；正式门禁使用 `--locked`，依赖声明和锁文件不一致时直接失败。
- 仓库显式使用官方 `https://pypi.org/simple` 生成锁文件，避免个人或地区镜像改变可复现结果。

## 依赖分层

核心工程脚本只使用 Python 标准库，默认环境没有第三方运行依赖。
协议逆向/机器码复现脚本使用 `protocol` dependency group：

```text
capstone
pefile
unicorn
```

普通工程脚本：

```bash
uv run --locked python scripts/test-full.py --profile full
```

协议工具：

```bash
uv run --locked --group protocol python scripts/protocol/probe_lba4_reader.py --help
```

不要在仓库环境中执行全局 `pip install`。新增普通 Python 依赖前应先确认是否可以继续使用标准库；
只有协议复现专用依赖才进入 `protocol` 组。修改依赖后运行 `uv lock` 并提交 `uv.lock`。

## CI

GitHub Actions 通过 `.github/actions/setup-python-tooling/action.yml` 统一安装 uv。
该复合 action 对 `astral-sh/setup-uv` 使用精确 commit pin，并启用 uv/Python 缓存。
workflow 不允许直接调用裸 `python`/`python3` 执行仓库脚本，统一使用 `uv run --locked python`。

`ci.yml` 的 Python tooling contract 会在 Python 3.11.15 和 3.14.5 上执行：

1. 全部 `scripts/**/*.py` 语法编译；
2. `scripts/tests` 单元测试；
3. `protocol` 依赖组解析与导入验证。

这些约束由 `scripts/tests/test_engineering.py` 回归测试锁定，防止后续 CI 或本地门禁重新退化为系统 Python。
