# 组件库与 RP 11 对照

调查日期：2026-10-24。目标是本机 RP 11 安装副本。按原厂库的实际目录和组件类型核对 Ingenaro；所有新增预设、图标和渲染实现均由 Ingenaro 独立编写。

## 证据与范围

本轮通过 REA 静态分析已安装播放器目录中的 `axQuery.std.js` 文件。分析父证据为 `ev_9f92fd9a43d6dabe660687186b528db78cc28b412e6445bbd9a5b49298b00ff6`，摘要为 `ev_265bd305e5c35459697e96531c8b06daa37c14f508b96fc545be136e58d1ee32`。目录 SHA-256 为 `bb3938daef4dcfec7bf4fe7821673c987367f8b7b8fd4c9df4919e8c9da42e8f`。选择的文件成功解析，另外 48 个相关文件未进入这次分析；关系与语义覆盖为 partial，不能据此宣称整个播放器已经复原。

直接阅读该文件第 6–34 行可观察到 table、menuObject、repeater、dynamicPanel、textBox、textArea、listBox、comboBox、checkbox、radioButton、imageMapRegion、treeNodeObject、inlineFrame 等不同类型声明。这些是播放器的静态类型声明；没有执行参考桌面应用或验证其全部交互。声明本身也不等于默认库中有一项一一对应的预设。

`.rplib` 清单另由 Ingenaro 的只读容器/对象图解析器读取，**不属于 REA 反编译结果**。检查脚本为 [inspect-library.rs](../scripts/inspect-library.rs)，读取 `DesignDocument` 的 `PackageInfo` 页面、sitemap 分类，以及对应对象包中的 `DiagramObject` 系列类型（此处省略命名空间前缀）。没有复制库中的图形、图片、样式或交互实现。完整目录在 [library-inventory.json](library-inventory.json)，图标库元数据在 [library-icons-inventory.json](library-icons-inventory.json)。

主程序集的既有观察仍有效：版本 `11.0.0.4137`，SHA-256 `1cc3801cb9ca79ef509b17be1deecc054419df8752bf0b52c808967e08ec9bd4`。REA managed members 检查受到 64 MiB 表示预算限制，不能给出完整桌面编辑器成员或行为；本轮没有把这个失败解释为组件不存在。

| 原厂库 | 实际页面条目数 | 范围说明 |
| --- | ---: | --- |
| Default | 40 | Common 16、Forms 6、Menus and table 4、Markup 8、Dynamic 6 |
| Flow | 37 | 流程图形、图片与 Snapshot |
| Icons | 920 | 仅名称、分类和 ID 清单；未提取图形 |
| Sample form patterns | 48 | 包括 4 项 Instructions；其余也是复合示例，不等于基础控件类型数 |
| Sample UI patterns | 15 | 包括 3 项 Instructions；含轮播、选项卡、分页、弹窗、可筛选表格等复合示例 |

这些数字是该安装副本中的库页面数，不是所有参考编辑器版本的全量能力数字。默认库与流程图库之间有重名项，不能简单相加后当作独立基础类型数。

## Default 的 40 项逐项对照

Ingenaro 的目录分为基本元件、表单与菜单、界面组合、标注、流程图、常用图标六组，支持中英文关键词搜索和分类筛选。当前目录包含 119 个可添加条目，复用 13 种底层 `WidgetKind`，提供样式预设、矢量造型和多组件组合。侧栏缩略图使用 Ingenaro 自绘线稿。

下表依据 `src/widget_library.rs` 的当前目录核对。标题、段落、便签等视觉预设复用基础类型；单选、文本域、下拉和列表使用各自的真实控件类型。组合预设可解组编辑，不具备原厂专用对象的数据模型。这里记录实现范围，运行与回归验证结果另见项目验证记录。

| 分类 | 参考库项 | Ingenaro 对应与边界 |
| --- | --- | --- |
| Common | Box 1 | 矩形；可编辑填充、边框、文字 |
| Common | Box 2 | “矩形 2”样式预设，使用独立设计的填充和边框 |
| Common | Box 3 | “矩形 3”样式预设，使用独立设计的填充和边框 |
| Common | Image | 图片组件，读取用户选取的本地图片 |
| Common | Placeholder | 带对角线的矢量占位符 |
| Common | Button | 次要按钮样式预设，可添加点击动作 |
| Common | Primary button | 主要按钮样式预设，可添加点击动作 |
| Common | Link button | 文本链接样式；用已有交互设置工程内页面跳转，尚非完整超链接富文本模型 |
| Common | Heading 1 | 一级标题预设 |
| Common | Heading 2 | 二级标题预设 |
| Common | Heading 3 | 三级标题预设 |
| Common | Label | 单行文本预设 |
| Common | Paragraph | 可换行段落预设 |
| Common | Horizontal line | 水平直线 |
| Common | Vertical line | “垂直线”预设，以实际竖向路径表示线段 |
| Common | Ellipse | 椭圆 |
| Forms | Text field | 单行输入框 |
| Forms | Text area | 真实多行文本域；与单行输入区分 |
| Forms | Droplist | 真实下拉选择；选项逐行编辑 |
| Forms | List box | 真实列表选择；选项逐行编辑 |
| Forms | Checkbox | 真实复选框；预览可勾选 |
| Forms | Radio button | 真实单选控件；属性可指定互斥组，默认按组件名称分组，显式空组表示各自独立 |
| Menus and table | Classic table | 可解组的表格单元格组合；没有专用插入行列、合并单元格模型 |
| Menus and table | Classic menu - horizontal | 横向菜单组合；子菜单展开和键盘导航未对齐 |
| Menus and table | Classic menu - vertical | 纵向菜单组合；子菜单展开和键盘导航未对齐 |
| Menus and table | Tree | 文本节点组合；尚非可折叠、可编辑层级的数据控件 |
| Markup | Sticky 1 | “便签 1”黄色矩形文本预设；不绑定原生说明数据 |
| Markup | Sticky 2 | “便签 2”绿色矩形文本预设；不绑定原生说明数据 |
| Markup | Sticky 3 | “便签 3”蓝色矩形文本预设；不绑定原生说明数据 |
| Markup | Sticky 4 | “便签 4”粉色矩形文本预设；不绑定原生说明数据 |
| Markup | Circle marker | “圆形标记”预设，椭圆和编号文本 |
| Markup | Drop marker | “水滴标记”预设，独立矢量轮廓和编号文本；轮廓为多边形近似 |
| Markup | Horizontal arrow | 矢量箭头；可缩放、旋转和导出 |
| Markup | Vertical arrow | “垂直箭头”预设，使用竖向矢量轮廓 |
| Dynamic | Dynamic panel | 现有基础容器不等价；原生导入预览保留多状态，新建与编辑状态仍缺失 |
| Dynamic | Hot spot | 透明点击区域预设；沿用已有点击事件，不等于完整 ImageMapRegion 编辑器 |
| Dynamic | Inline frame | 已导入原生框架的预览有部分支持；新建、框架属性编辑尚缺 |
| Dynamic | Repeater - card | 尚无新建数据驱动中继器、数据集与行事件编辑 |
| Dynamic | Repeater - table | 尚无新建数据驱动中继器、排序/筛选/分页编辑 |
| Dynamic | Snapshot | 尚无绑定其他页面/组件的快照刷新模型；普通图片不应冒充 |

## Flow 的 37 项

实际名称为：Actor、Beveled rectangle、Bracket、Collate、Database、Delay、Diamond、Display、Document、Ellipse、File、Hexagon、Image、Internal storage (RAM)、Keying、Logical or、Magnetic tape、Manual input、Multi-document、Off page connector、Offline storage、Parallelogram、Punched card、Punched tape、Rectangle、Rounded rectangle、Semicircle、Snapshot、Sort、Stacked file、Stacked rectangle、Stacked rounded、Stored data、Subroutine/Predefined process、Summing junction、Trapezoid、Triangle。

本轮提供其中除 Snapshot 之外各项的静态造型对应；图片、三角形等条目与基本元件组共用，不为每个原厂库名新增底层类型。各造型都是独立绘制的示意路径，尚未验证与原厂图形逐像素一致。

| Flow 库项目 | Ingenaro 对应静态预设 |
| --- | --- |
| Rectangle、Rounded rectangle、Ellipse、Hexagon、Triangle、Image | 基本元件中的矩形、圆角矩形、椭圆、六边形、三角形和图片；流程组另有流程、开始/结束、连接点、准备等常见用途入口 |
| Diamond、Parallelogram、Document、Database | 判断、数据、文档、数据库 |
| Actor、Beveled rectangle、Bracket、Collate | 人物、斜角矩形、括号、校对 |
| Delay、Display、File、Internal storage (RAM) | 延迟、显示、文件、内部存储 |
| Keying、Logical or、Magnetic tape、Manual input | 键控、逻辑或、磁带、手动输入 |
| Multi-document、Off page connector、Offline storage | 多文档、离页连接、离线存储 |
| Punched card、Punched tape、Semicircle、Sort | 穿孔卡、穿孔带、半圆、排序 |
| Stacked file、Stacked rectangle、Stacked rounded | 堆叠文件、堆叠矩形、堆叠圆角 |
| Stored data、Subroutine/Predefined process、Summing junction、Trapezoid | 存储数据、子流程、求和连接、梯形 |
| Snapshot | 关联快照尚未实现；普通图片不代替其引用与刷新模型 |

延迟、显示、半圆、数据库、磁带、人物头部等弧线使用采样后的折线；文档和穿孔带的波边也由折线近似，堆叠圆角的转角采用斜切近似。它们支持静态布局和导出，没有流程执行、数据存储或延迟动作语义。“连接点”“离页连接”和箭头也不会自动连接、吸附或跟随其他对象。

## 编辑、预览与原生保存边界

新表单保留真实类型；HTML 中分别使用文本输入、`textarea`、复选/单选输入和 `select`，SVG 仅导出其静态外观。下拉/列表的 `text` 逐行表示选项，当前为单选列表；单选互斥组在页面内生效，显式空组各自独立。原生文件保留其 `radio-group` 名称。

自有导出 HTML 在本机 Edge 无头浏览器中通过 13 项检查，覆盖多行输入、单选互斥与跨页隔离、下拉/列表选择、设置文本动作、文本转义及重叠矢量填充像素。记录见 [library-browser-qa.json](library-browser-qa.json)，复查脚本为 [verify-library.mjs](../scripts/verify-library.mjs)。这是 Ingenaro 自有播放器的运行验证，没有执行参考编辑器，也不证明全部原生保存和编辑行为已对齐。

原生导入区分 `RadioButton`、`TextArea`、`ComboBox` 与 `ListBox`，没有再统一降级成复选框或单行输入。在 `.rp` 工程中新增这四类表单时，当前文件需已有同类原生对象供模板复制；无模板会拒绝保存。`.rplib` 仍不支持新增库条目，已有表单可编辑并保存。独立 `.ingenaro` 工程无此模板限制。现有基础容器不能新增写回成动态面板。表格、菜单和树组合写回也不会变成原厂的专用复合对象。

内置常用图标共 28 项，使用可编辑的矢量路径。界面组合共 12 项，包括信息卡片、个人资料卡、统计卡片、登录表单、搜索栏、标签页、分页栏、面包屑、确认弹窗、提示横幅、空状态和进度条。组合保留内部文字、表单控件及图形的编辑能力，按钮动作可以在交互面板中配置。

## 后续对齐重点

1. 目录与查找：用分类、搜索和稳定的预设 ID 承载组件，不把每种标题或颜色都扩成底层类型；保留窄侧栏、拖入和独立滚动。
2. 编辑与保存：持续核对默认值、画布、属性、复制粘贴、撤销、独立工程、HTML/SVG 和原生文件读写的一致性；补足新表单的原生模板限制和专用对象转换。
3. 真实语义：表格、树、菜单从可编辑组合升级为专用模型；动态面板、中继器、框架和快照按各自的数据/状态模型实现。
4. 图标与扩展库：920 项原厂图标清单只用于了解范围；当前原创常用图标集不宣称与其数量、形状或许可相同。用户库导入、实例复用和外部库管理仍是单独能力。

文件身份：Default SHA-256 `9b4f398eab76f6e081f3275824ad15396f670fd2448e7e36b4e4399c40f85d50`；Flow `95d85ffaddac36144ae8ebd645341b2e557f0a6157b092319e7dac7b71d9d451`；Icons `83f450088f81f6047dfb196919fff3a4aae2f3adb56668da4d4c8cfd3b813bda`；Sample form patterns `1044b386c12c170876c0423ccd07b515d9c9e063d4cde108cb6a4bbe96131179`；Sample UI patterns `cf1787fded7a74278209f9aee4b07a266a10360f2e0dd552c49cca158f65e36b`。

REA 摘要与原始父证据 ID 保存在 [library-rea-evidence.json](library-rea-evidence.json)。本机完整静态图证据包保留在 `target/library-rea-full-evidence.json`，不把体积较大的分析中间产物作为组件实现或图形资产分发。
