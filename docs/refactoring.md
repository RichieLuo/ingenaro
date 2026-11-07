# 本轮结构调整

本轮保留原业务公共接口、数据格式和配置目录，拆开混合职责与测试。统计为物理行数，包含注释和空行；以下是原大文件与现在入口的对照，不代表删除了这些功能。

| 原文件 | 调整前 | 现在入口 | 调整后 |
| --- | ---: | --- | ---: |
| `src/ui.rs` | 3582 | `src/ui/mod.rs` | 851 |
| `src/rp_format/native.rs` | 3361 | `src/rp_format/native.rs` | 704 |
| `src/model.rs` | 1955 | `src/model.rs` | 743 |
| `src/rp_format/mod.rs` | 1562 | `src/rp_format/mod.rs` | 865 |
| `src/rp_format/binary.rs` | 1505 | `src/rp_format/binary.rs` | 516 |
| `src/widget_library.rs` | 1288 | `src/widget_library.rs` | 308 |
| `src/export.rs` | 1092 | `src/export.rs` | 674 |

- `src/ui/mod.rs`：UI 文件移入正常子模块目录，启动、文档保存、编辑、快捷键、预览、导航、画布和窗口各自分开。测试按画布、编辑、会话及原生文件恢复行为分组。

- `src/rp_format/native.rs`：原生文档上下文保留在入口，文本、组件、交互、顺序、分组及项目补丁分别实现；兼容测试与真实文件测试也分组，写回与冲突检测仍走原接口。

- `src/model.rs`：数据定义、项目校验和欢迎示例分离。demo.rs 只保存内置示例，validation.rs 负责输入边界。

- `src/rp_format/mod.rs`：容器与页面绑定留在入口，Converter 实现移入 converter.rs，保留已有原生读取职责。

- `src/rp_format/binary.rs`：图模型与公共入口、Reader、Writer 分离；字节布局与未知数据保留逻辑不变。

- `src/widget_library.rs`：组件类型/创建、目录数据及几何路径分开；目录使用的私有宏跟随目录数据，避免跨模块宏作用域依赖。

- `src/export.rs`：导出测试从实现中分离；HTML/SVG 共用的转义与模型校验仍集中维护。

所有新增子模块通过正常的 Rust 模块声明连接，不使用 include! 拼接源码。测试仍由 cargo test 自动发现；忽略的实库/显式样本测试保留原忽略条件。

运行 `python scripts/check-structure.py` 查看当前最大文件与 CI 检查线。vendor 驱动补丁不做纯风格重排，以保留上游差异可审阅性。

## 验证记录（2026-10-25，Windows x64）

- `cargo fmt --all --check`、`cargo clippy --locked --all-targets -- -D warnings`、`cargo test --locked` 通过；原有显式忽略的实库/调用方样本测试仍按原条件跳过。
- `python -m unittest discover -s scripts/tests -v` 的 7 个打包契约测试通过，覆盖失败构建、目标目录、归档权限、静态 CRT 参数、旧包保护和版本 tag 匹配。
- `python scripts/check-structure.py` 与 `git diff --check` 通过。
- 对照 Git 原版本核对函数声明，未发现原有函数遗漏；205 个 Rust 测试声明保持一致。SQLShop 的图标像素函数统一命名为 `icon_image`。
- Windows 发布包实际构建完成，ZIP 完整性、包内程序 SHA-256、使用说明和第三方许可清单核对通过；PE 版本资源与项目名称/版本一致，依赖列表不包含动态 MSVC CRT。
- 三仓库的通用打包脚本、资源环境脚本、文件规模检查、发布工作流、架构文档及格式配置保持一致。

本次生成的候选包为 `dist/architecture-unification/Ingenaro-0.6.0-windows-x64.zip`，SHA-256 为 `5b030ebdb8dc8c9f4e83aebdf80636354dddfeb00ea395e9de12bde672aee108`。此目录是验证输出；常规打包仍默认使用 `dist/`，已有同名产物不会被覆盖。

本轮未执行 GitHub Actions、macOS/Linux 构建或目标系统启动验证。跨平台 CI 和发布配置已统一，不能将本机 Windows 验证视为三系统实测。
