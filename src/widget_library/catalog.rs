use super::*;

macro_rules! item {
    ($id:literal,$label:literal,$category:ident,$preset:expr,$keys:literal) => {
        LibraryItem {
            id: $id,
            label: $label,
            category: Category::$category,
            preset: $preset,
            keywords: $keys,
        }
    };
}

pub const ITEMS: &[LibraryItem] = &[
    item!(
        "rectangle",
        "矩形",
        Basic,
        Preset::Kind(WidgetKind::Rectangle),
        "box rectangle"
    ),
    item!(
        "ellipse",
        "椭圆",
        Basic,
        Preset::Kind(WidgetKind::Ellipse),
        "circle ellipse 圆形"
    ),
    item!(
        "text",
        "文本",
        Basic,
        Preset::Kind(WidgetKind::Text),
        "label text 文字"
    ),
    item!(
        "button",
        "按钮",
        Basic,
        Preset::Kind(WidgetKind::Button),
        "primary button 主要按钮"
    ),
    item!(
        "heading1",
        "一级标题",
        Basic,
        Preset::Heading(1),
        "h1 heading title"
    ),
    item!(
        "heading2",
        "二级标题",
        Basic,
        Preset::Heading(2),
        "h2 heading title"
    ),
    item!(
        "heading3",
        "三级标题",
        Basic,
        Preset::Heading(3),
        "h3 heading title"
    ),
    item!(
        "paragraph",
        "段落",
        Basic,
        Preset::Paragraph,
        "paragraph 段落文本"
    ),
    item!("link", "文本链接", Basic, Preset::Link, "link 链接"),
    item!(
        "outline-button",
        "次要按钮",
        Basic,
        Preset::OutlineButton,
        "secondary outline button"
    ),
    item!("rounded", "圆角矩形", Basic, Preset::Rounded, "rounded box"),
    item!("box2", "矩形 2", Basic, Preset::Box(2), "box2 box 2"),
    item!("box3", "矩形 3", Basic, Preset::Box(3), "box3 box 3"),
    item!(
        "line",
        "直线",
        Basic,
        Preset::Kind(WidgetKind::Line),
        "line 分隔线"
    ),
    item!(
        "vertical-line",
        "垂直线",
        Basic,
        Preset::VerticalLine,
        "vertical line 竖线"
    ),
    item!(
        "image",
        "图片",
        Basic,
        Preset::Kind(WidgetKind::Image),
        "image picture 上传"
    ),
    item!(
        "placeholder",
        "占位符",
        Basic,
        Preset::Placeholder,
        "placeholder 占位图"
    ),
    item!(
        "hotspot",
        "热区",
        Basic,
        Preset::Hotspot,
        "hot spot hotspot 点击区域"
    ),
    item!(
        "triangle",
        "三角形",
        Basic,
        Preset::Shape("triangle"),
        "triangle"
    ),
    item!(
        "diamond",
        "菱形",
        Basic,
        Preset::Shape("diamond"),
        "diamond"
    ),
    item!(
        "pentagon",
        "五边形",
        Basic,
        Preset::Shape("pentagon"),
        "pentagon"
    ),
    item!(
        "hexagon",
        "六边形",
        Basic,
        Preset::Shape("hexagon"),
        "hexagon"
    ),
    item!(
        "octagon",
        "八边形",
        Basic,
        Preset::Shape("octagon"),
        "octagon"
    ),
    item!("star", "星形", Basic, Preset::Shape("star"), "star 五角星"),
    item!(
        "arrow-right",
        "右箭头",
        Basic,
        Preset::Shape("arrow"),
        "right arrow"
    ),
    item!(
        "arrow-left",
        "左箭头",
        Basic,
        Preset::Shape("left-arrow"),
        "left arrow"
    ),
    item!(
        "arrow-double",
        "双向箭头",
        Basic,
        Preset::Shape("double-arrow"),
        "double arrow"
    ),
    item!(
        "arrow-up",
        "垂直箭头",
        Basic,
        Preset::Shape("up-arrow"),
        "vertical arrow 上箭头"
    ),
    item!(
        "container",
        "容器",
        Basic,
        Preset::Kind(WidgetKind::Panel),
        "panel container 基础容器"
    ),
    item!(
        "input",
        "输入框",
        Forms,
        Preset::Kind(WidgetKind::Input),
        "text field input 文本框"
    ),
    item!(
        "textarea",
        "文本域",
        Forms,
        Preset::Kind(WidgetKind::TextArea),
        "textarea text area 多行输入框"
    ),
    item!(
        "checkbox",
        "复选框",
        Forms,
        Preset::Kind(WidgetKind::Checkbox),
        "checkbox check box"
    ),
    item!(
        "radio",
        "单选按钮",
        Forms,
        Preset::Kind(WidgetKind::Radio),
        "radio button 单选框"
    ),
    item!(
        "droplist",
        "下拉列表",
        Forms,
        Preset::Kind(WidgetKind::Droplist),
        "select dropdown droplist 下拉框"
    ),
    item!(
        "listbox",
        "列表框",
        Forms,
        Preset::Kind(WidgetKind::ListBox),
        "listbox list box"
    ),
    item!("table", "表格", Forms, Preset::Table, "table grid 数据表格"),
    item!(
        "horizontal-menu",
        "横向菜单",
        Forms,
        Preset::Menu(false),
        "horizontal menu 导航栏"
    ),
    item!(
        "vertical-menu",
        "纵向菜单",
        Forms,
        Preset::Menu(true),
        "vertical menu 菜单栏"
    ),
    item!("tree", "树", Forms, Preset::Tree, "tree 树形菜单 目录"),
    item!(
        "sticky1",
        "便签 1",
        Markup,
        Preset::Sticky(1),
        "sticky note 注释 标注"
    ),
    item!(
        "sticky2",
        "便签 2",
        Markup,
        Preset::Sticky(2),
        "sticky note 注释 标注"
    ),
    item!(
        "sticky3",
        "便签 3",
        Markup,
        Preset::Sticky(3),
        "sticky note 注释 标注"
    ),
    item!(
        "sticky4",
        "便签 4",
        Markup,
        Preset::Sticky(4),
        "sticky note 注释 标注"
    ),
    item!(
        "circle-marker",
        "圆形标记",
        Markup,
        Preset::Marker(false),
        "circle marker 序号"
    ),
    item!(
        "drop-marker",
        "水滴标记",
        Markup,
        Preset::Marker(true),
        "drop marker 序号"
    ),
    item!(
        "flow-process",
        "流程",
        Flow,
        Preset::Shape("process"),
        "flow process 过程"
    ),
    item!(
        "flow-decision",
        "判断",
        Flow,
        Preset::Shape("decision"),
        "flow decision 条件"
    ),
    item!(
        "flow-start",
        "开始/结束",
        Flow,
        Preset::Shape("start"),
        "flow terminator start end"
    ),
    item!(
        "flow-data",
        "数据",
        Flow,
        Preset::Shape("data"),
        "flow data 输入输出 平行四边形"
    ),
    item!(
        "flow-document",
        "文档",
        Flow,
        Preset::Shape("document"),
        "flow document"
    ),
    item!(
        "flow-database",
        "数据库",
        Flow,
        Preset::Shape("database"),
        "flow database cylinder 圆柱"
    ),
    item!(
        "flow-predefined",
        "子流程",
        Flow,
        Preset::Shape("predefined"),
        "flow predefined subprocess"
    ),
    item!(
        "flow-manual",
        "手动输入",
        Flow,
        Preset::Shape("manual"),
        "flow manual input"
    ),
    item!(
        "flow-preparation",
        "准备",
        Flow,
        Preset::Shape("preparation"),
        "flow preparation"
    ),
    item!(
        "flow-connector",
        "连接点",
        Flow,
        Preset::Shape("connector"),
        "flow connector 圆"
    ),
    item!(
        "flow-actor",
        "人物",
        Flow,
        Preset::Shape("actor"),
        "flow actor"
    ),
    item!(
        "flow-beveled",
        "斜角矩形",
        Flow,
        Preset::Shape("beveled"),
        "flow beveled rectangle"
    ),
    item!(
        "flow-bracket",
        "括号",
        Flow,
        Preset::Shape("bracket"),
        "flow bracket"
    ),
    item!(
        "flow-collate",
        "校对",
        Flow,
        Preset::Shape("collate"),
        "flow collate"
    ),
    item!(
        "flow-delay",
        "延迟",
        Flow,
        Preset::Shape("delay"),
        "flow delay"
    ),
    item!(
        "flow-display",
        "显示",
        Flow,
        Preset::Shape("display"),
        "flow display"
    ),
    item!(
        "flow-file",
        "文件",
        Flow,
        Preset::Shape("file"),
        "flow file"
    ),
    item!(
        "flow-ram",
        "内部存储",
        Flow,
        Preset::Shape("ram"),
        "flow internal storage ram"
    ),
    item!(
        "flow-keying",
        "键控",
        Flow,
        Preset::Shape("keying"),
        "flow keying"
    ),
    item!(
        "flow-or",
        "逻辑或",
        Flow,
        Preset::Shape("or"),
        "flow logical or"
    ),
    item!(
        "flow-tape",
        "磁带",
        Flow,
        Preset::Shape("tape"),
        "flow magnetic tape"
    ),
    item!(
        "flow-multidoc",
        "多文档",
        Flow,
        Preset::Shape("multidoc"),
        "flow multi document"
    ),
    item!(
        "flow-offpage",
        "离页连接",
        Flow,
        Preset::Shape("offpage"),
        "flow off page connector"
    ),
    item!(
        "flow-offline",
        "离线存储",
        Flow,
        Preset::Shape("offline"),
        "flow offline storage"
    ),
    item!(
        "flow-card",
        "穿孔卡",
        Flow,
        Preset::Shape("card"),
        "flow punched card"
    ),
    item!(
        "flow-punched",
        "穿孔带",
        Flow,
        Preset::Shape("punched"),
        "flow punched tape"
    ),
    item!(
        "flow-semicircle",
        "半圆",
        Flow,
        Preset::Shape("semicircle"),
        "flow semicircle"
    ),
    item!(
        "flow-sort",
        "排序",
        Flow,
        Preset::Shape("sort"),
        "flow sort"
    ),
    item!(
        "flow-stack-file",
        "堆叠文件",
        Flow,
        Preset::Shape("stack-file"),
        "flow stacked file"
    ),
    item!(
        "flow-stack-rect",
        "堆叠矩形",
        Flow,
        Preset::Shape("stack-rect"),
        "flow stacked rectangle"
    ),
    item!(
        "flow-stack-rounded",
        "堆叠圆角",
        Flow,
        Preset::Shape("stack-rounded"),
        "flow stacked rounded"
    ),
    item!(
        "flow-stored",
        "存储数据",
        Flow,
        Preset::Shape("stored"),
        "flow stored data"
    ),
    item!(
        "flow-sum",
        "求和连接",
        Flow,
        Preset::Shape("sum"),
        "flow summing junction"
    ),
    item!(
        "flow-trapezoid",
        "梯形",
        Flow,
        Preset::Shape("trapezoid"),
        "flow trapezoid"
    ),
    item!(
        "icon-search",
        "搜索",
        Icons,
        Preset::Icon(Icon::Search),
        "search find 搜索 放大镜"
    ),
    item!(
        "icon-close",
        "关闭",
        Icons,
        Preset::Icon(Icon::Close),
        "close x 删除"
    ),
    item!(
        "icon-menu",
        "菜单",
        Icons,
        Preset::Icon(Icon::Menu),
        "menu hamburger"
    ),
    item!(
        "icon-home",
        "首页",
        Icons,
        Preset::Icon(Icon::Home),
        "home house"
    ),
    item!(
        "icon-user",
        "用户",
        Icons,
        Preset::Icon(Icon::User),
        "user person 人像"
    ),
    item!(
        "icon-check",
        "勾选",
        Icons,
        Preset::Icon(Icon::Check),
        "check tick 对勾"
    ),
    item!(
        "icon-plus",
        "添加",
        Icons,
        Preset::Icon(Icon::Plus),
        "add plus 加号"
    ),
    item!(
        "icon-chevron",
        "下一步",
        Icons,
        Preset::Icon(Icon::Next),
        "chevron next 向右"
    ),
    item!(
        "icon-arrow-left",
        "向左",
        Icons,
        Preset::Icon(Icon::Left),
        "left arrow back 返回"
    ),
    item!(
        "icon-arrow-right",
        "向右",
        Icons,
        Preset::Icon(Icon::Right),
        "right arrow forward 前进"
    ),
    item!(
        "icon-chevron-down",
        "向下展开",
        Icons,
        Preset::Icon(Icon::Down),
        "down chevron expand 展开"
    ),
    item!(
        "icon-minus",
        "减号",
        Icons,
        Preset::Icon(Icon::Minus),
        "minus remove 减少"
    ),
    item!(
        "icon-edit",
        "编辑",
        Icons,
        Preset::Icon(Icon::Edit),
        "edit pencil 修改 铅笔"
    ),
    item!(
        "icon-trash",
        "删除",
        Icons,
        Preset::Icon(Icon::Trash),
        "trash delete bin 回收站"
    ),
    item!(
        "icon-copy",
        "复制",
        Icons,
        Preset::Icon(Icon::Copy),
        "copy duplicate clone 拷贝"
    ),
    item!(
        "icon-settings",
        "设置",
        Icons,
        Preset::Icon(Icon::Settings),
        "settings gear preferences 齿轮"
    ),
    item!(
        "icon-filter",
        "筛选",
        Icons,
        Preset::Icon(Icon::Filter),
        "filter funnel 过滤 漏斗"
    ),
    item!(
        "icon-download",
        "下载",
        Icons,
        Preset::Icon(Icon::Download),
        "download save 导出"
    ),
    item!(
        "icon-upload",
        "上传",
        Icons,
        Preset::Icon(Icon::Upload),
        "upload import 导入"
    ),
    item!(
        "icon-folder",
        "文件夹",
        Icons,
        Preset::Icon(Icon::Folder),
        "folder directory 目录"
    ),
    item!(
        "icon-bell",
        "通知",
        Icons,
        Preset::Icon(Icon::Bell),
        "bell notification 铃铛"
    ),
    item!(
        "icon-calendar",
        "日历",
        Icons,
        Preset::Icon(Icon::Calendar),
        "calendar date 日期"
    ),
    item!(
        "icon-heart",
        "喜欢",
        Icons,
        Preset::Icon(Icon::Heart),
        "heart like favorite 爱心"
    ),
    item!(
        "icon-star",
        "收藏",
        Icons,
        Preset::Icon(Icon::Star),
        "star bookmark favorite 星标"
    ),
    item!(
        "icon-info",
        "信息",
        Icons,
        Preset::Icon(Icon::Info),
        "information info 帮助 说明"
    ),
    item!(
        "icon-alert",
        "警告",
        Icons,
        Preset::Icon(Icon::Alert),
        "alert warning caution 提醒"
    ),
    item!(
        "icon-lock",
        "锁定",
        Icons,
        Preset::Icon(Icon::Lock),
        "lock privacy security 锁"
    ),
    item!(
        "icon-mail",
        "邮件",
        Icons,
        Preset::Icon(Icon::Mail),
        "mail email envelope 信封"
    ),
    item!(
        "pattern-card",
        "信息卡片",
        Patterns,
        Preset::Pattern(Pattern::Card),
        "card resource 内容卡片 资源卡片"
    ),
    item!(
        "pattern-profile",
        "个人资料卡",
        Patterns,
        Preset::Pattern(Pattern::Profile),
        "profile user avatar 用户卡片 头像 个人信息"
    ),
    item!(
        "pattern-stat",
        "统计卡片",
        Patterns,
        Preset::Pattern(Pattern::Stat),
        "stat metric KPI analytics dashboard 指标 数据看板 数据卡片"
    ),
    item!(
        "pattern-login",
        "登录表单",
        Patterns,
        Preset::Pattern(Pattern::Login),
        "login sign in form email 验证码 邮箱"
    ),
    item!(
        "pattern-search",
        "搜索栏",
        Patterns,
        Preset::Pattern(Pattern::Search),
        "search bar field 查找 搜索框"
    ),
    item!(
        "pattern-tabs",
        "标签页",
        Patterns,
        Preset::Pattern(Pattern::Tabs),
        "tabs tab bar navigation 标签栏 选项卡"
    ),
    item!(
        "pattern-pagination",
        "分页栏",
        Patterns,
        Preset::Pattern(Pattern::Pagination),
        "pagination pager page navigation 页码"
    ),
    item!(
        "pattern-breadcrumb",
        "面包屑",
        Patterns,
        Preset::Pattern(Pattern::Breadcrumb),
        "breadcrumb path navigation 路径导航"
    ),
    item!(
        "pattern-dialog",
        "确认弹窗",
        Patterns,
        Preset::Pattern(Pattern::Dialog),
        "modal confirmation dialog 确认框 对话框"
    ),
    item!(
        "pattern-alert",
        "提示横幅",
        Patterns,
        Preset::Pattern(Pattern::Alert),
        "alert banner notification 通知 信息提示"
    ),
    item!(
        "pattern-empty",
        "空状态",
        Patterns,
        Preset::Pattern(Pattern::Empty),
        "empty state placeholder 无数据 暂无内容"
    ),
    item!(
        "pattern-progress",
        "进度条",
        Patterns,
        Preset::Pattern(Pattern::Progress),
        "progress bar loading 进度 加载"
    ),
];
