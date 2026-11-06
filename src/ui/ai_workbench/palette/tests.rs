use super::*;
use crate::ui::tests::{click, fixture, frame, text_center};

fn app() -> App {
    let mut app = fixture(Project::blank());
    app.ai.document_id = uuid::Uuid::new_v4().to_string();
    app.ai.last_project_key = app.ai_project_key();
    app.ai.visible = true;
    app
}

fn press(app: &mut App, ctx: &egui::Context, label: &str) {
    frame(app, ctx, vec![]);
    let output = frame(app, ctx, vec![]);
    click(app, ctx, text_center(&output, label));
}

fn wait_for(app: &mut App, ctx: &egui::Context, done: impl Fn(&App) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(12);
    while !done(app) && Instant::now() < deadline {
        frame(app, ctx, vec![]);
        std::thread::sleep(Duration::from_millis(2));
    }
    assert!(done(app), "{} · {}", app.ai.state, app.ai.palette.notice);
}

fn reference_card() -> ai::ImageAttachment {
    let pixels: Vec<_> = (0..48)
        .flat_map(|i| {
            if i < 24 {
                [46, 105, 186, 255]
            } else {
                [255, 71, 31, 255]
            }
        })
        .collect();
    ai::ImageAttachment::from_rgba("card", 48, 1, &pixels).unwrap()
}

fn finish_image_upload(app: &mut App, ctx: &egui::Context) {
    let (tx, rx) = mpsc::channel();
    tx.send(extract_colors(&[reference_card()]).map(Some))
        .unwrap();
    app.ai.palette.worker = Some(Worker::Image(rx));
    app.ai_poll_palette(ctx);
}

#[test]
fn manual_image_and_text_inputs_share_one_confirmable_project_draft() {
    let Some(files) = MockFiles::new() else {
        return;
    };
    let mut app = app();
    files.configure(&mut app);
    let ctx = egui::Context::default();
    press(&mut app, &ctx, "项目配色");
    press(&mut app, &ctx, "＋ 添加色块");
    app.ai.palette.draft.as_mut().unwrap()[0].hex = "1a53af".into();
    press(&mut app, &ctx, "锁定");
    press(&mut app, &ctx, "＋ 添加色块");
    let original = app.ai.palette.valid_palette().unwrap();
    assert_eq!(original.0.len(), 2);
    assert!(original.0[0].locked);
    finish_image_upload(&mut app, &ctx);
    assert_eq!(app.ai.palette.valid_palette().unwrap(), original);
    app.ai.palette.text = "从图里补一个强调色，主色保留，背景暖一点".into();
    app.ai_generate_palette(false).unwrap();
    wait_for(&mut app, &ctx, |app| app.ai.palette.worker.is_none());
    assert!(!app.ai.palette.error, "{}", app.ai.palette.notice);
    assert_eq!(app.ai.palette.changes.len(), 2);
    let draft = app.ai.palette.valid_palette().unwrap();
    assert_eq!(draft.0[0], original.0[0]);
    assert_eq!(draft.0[1].hex, HexColor([255, 244, 224]));
    assert_eq!(draft.0[2].role, ColorRole::Accent);
    assert_eq!(draft.0[2].hex, HexColor([255, 71, 31]));
    assert!(app.ai_confirmed_palette().is_none());
    press(&mut app, &ctx, "确认项目配色");
    assert_eq!(app.ai_confirmed_palette(), Some(&draft));
    assert!(app.project.pages[0].widgets.is_empty());
}

#[test]
fn palette_image_candidates_require_review() {
    let mut app = app();
    let ctx = egui::Context::default();
    press(&mut app, &ctx, "项目配色");
    finish_image_upload(&mut app, &ctx);
    assert!(app.ai.palette.has_colors());
    assert!(app.ai_confirmed_palette().is_none());
    app.ai_confirm_palette().unwrap();
    let active = app.ai_confirmed_palette().cloned();
    press(&mut app, &ctx, "项目配色");
    finish_image_upload(&mut app, &ctx);
    assert_eq!(app.ai_confirmed_palette().cloned(), active);
    assert_eq!(Some(app.ai.palette.valid_palette().unwrap()), active);
    assert!(app.ai.palette.notice.contains("请选择"));
}

#[test]
fn palette_panel_research_attributes_observed_sources() {
    let Some(files) = MockFiles::new() else {
        return;
    };
    let mut app = app();
    let ctx = egui::Context::default();
    files.configure(&mut app);
    press(&mut app, &ctx, "项目配色");
    app.ai.palette.text = "做一个清爽的海岸风格网站".into();
    press(&mut app, &ctx, "生成配色建议");
    wait_for(&mut app, &ctx, |app| app.ai.palette.worker.is_none());
    assert!(!app.ai.palette.error, "{}", app.ai.palette.notice);
    assert!(app.project.pages[0].widgets.is_empty());
    assert!(app.ai.running.is_none());
    assert!(app.ai_confirmed_palette().is_none());
    assert_eq!(
        app.ai.palette.basis.links,
        ["https://example.com/visual-reference"]
    );
    assert!(app.ai.palette.basis.rationale.contains("蓝白"));
    press(&mut app, &ctx, "确认项目配色");
    assert_eq!(
        app.ai_confirmed_palette()
            .unwrap()
            .first(ColorRole::Primary),
        Some(HexColor([35, 121, 205]))
    );
    assert!(app.ai.running.is_none());
    assert!(app.project.pages[0].widgets.is_empty());
}

#[test]
fn search_failure_falls_back_to_model_knowledge_and_keeps_explicitly_preserved_roles() {
    let Some(files) = MockFiles::new() else {
        return;
    };
    let mut app = app();
    let ctx = egui::Context::default();
    files.configure(&mut app);
    app.ai_open_palette();
    app.ai.palette.text = "做一个复古风格的网站".into();
    app.ai_generate_palette(true).unwrap();
    wait_for(&mut app, &ctx, |app| app.ai.palette.worker.is_none());
    assert!(!app.ai.palette.error, "{}", app.ai.palette.notice);
    assert!(app.ai.palette.basis.text.contains("已有知识"));
    assert!(app.ai.palette.basis.links.is_empty());
    let original = app.ai.palette.valid_palette().unwrap();
    app.ai.palette.text = "主色保留，背景再暖一点".into();
    app.ai_generate_palette(false).unwrap();
    wait_for(&mut app, &ctx, |app| app.ai.palette.worker.is_none());
    assert!(!app.ai.palette.error, "{}", app.ai.palette.notice);
    let next = app.ai.palette.valid_palette().unwrap();
    assert_eq!(
        next.first(ColorRole::Primary),
        original.first(ColorRole::Primary)
    );
    assert_eq!(
        next.first(ColorRole::Background),
        Some(HexColor([255, 244, 224]))
    );
    assert_eq!(app.ai.palette.changes.len(), 1);
    assert!(app.ai_confirmed_palette().is_none());
}

#[test]
fn chat_applies_confirmed_palette_and_scoped_edits_without_opening_settings() {
    let Some(files) = MockFiles::new() else {
        return;
    };
    let ctx = egui::Context::default();
    let project_path = files.0.join("project.rpshop");
    let settings_path = files.0.join("ai-settings.json");
    let mut original = app();
    files.configure(&mut original);
    original.project.save(&project_path).unwrap();
    original.path = Some(project_path.clone());
    original.ai_open_palette();
    let draft = Palette(vec![
        Swatch {
            locked: true,
            ..Swatch::new(HexColor([32, 80, 167]), ColorRole::Primary)
        },
        Swatch::new(HexColor([248, 242, 230]), ColorRole::Background),
        Swatch::new(HexColor([18, 25, 36]), ColorRole::Text),
    ]);
    original.ai.palette.set_draft(&draft);
    original.ai_confirm_palette().unwrap();
    original.ai_grant_full_permission();
    original.ai.prompt = "整个项目的配色方案我确定好了，你现在修改一下原型吧".into();
    original.ai_submit();
    assert!(original.ai.running.is_some());
    assert!(!original.ai.palette.is_open());
    assert!(original.ai.palette.worker.is_none());
    wait_for(&mut original, &ctx, |app| app.ai.running.is_none());
    assert!(original.ai.task_error.is_none(), "{}", original.ai.state);
    assert_eq!(original.project.pages[0].background, "#F8F2E6");
    assert_eq!(original.project.pages[0].widgets[0].fill, "#2050A7");
    assert_eq!(original.project.pages[0].widgets[1].text_color, "#121924");
    original.project.save(&project_path).unwrap();
    original.ai.settings.save(&settings_path).unwrap();
    let key = original.ai_project_key();
    drop(original);

    let mut reopened = app();
    reopened.project = Project::load(&project_path).unwrap();
    reopened.path = Some(project_path);
    reopened.ai.settings = ai::AiSettings::load(&settings_path).unwrap();
    reopened.ai_bind_document_path(true);
    assert_eq!(reopened.ai_project_key(), key);
    reopened.ai.last_project_key = key;
    reopened.ai.selected_engine = reopened.ai.settings.default_engine_id.clone().unwrap();
    reopened.ai_new_conversation();
    assert_eq!(reopened.ai_confirmed_palette(), Some(&draft));
    reopened.ai_grant_full_permission();
    for prompt in [
        "用这张图的配色",
        "这个页面的按钮改成紫色",
        "请生成第二个页面",
    ] {
        if prompt == "用这张图的配色" {
            reopened.ai.images.push(reference_card());
        }
        reopened.ai.prompt = prompt.into();
        reopened.ai_submit();
        assert!(reopened.ai.running.is_some());
        assert!(!reopened.ai.palette.is_open());
        assert!(reopened.ai.palette.worker.is_none());
        wait_for(&mut reopened, &ctx, |app| app.ai.running.is_none());
        assert!(reopened.ai.task_error.is_none(), "{}", reopened.ai.state);
        if prompt == "用这张图的配色" {
            assert_eq!(reopened.project.pages[0].widgets[0].fill, "#FF471F");
        }
        assert_eq!(reopened.ai_confirmed_palette(), Some(&draft));
    }
    assert_eq!(reopened.project.pages[0].widgets[0].fill, "#8844CC");
    assert_eq!(reopened.project.pages[1].background, "#F8F2E6");
    assert_eq!(reopened.project.pages[1].widgets[0].fill, "#2050A7");
    assert_eq!(reopened.project.pages[1].widgets[1].text_color, "#121924");
    reopened.ai.kind = design::ArtifactKind::Html;
    assert!(
        reopened
            .ai_palette_context()
            .contains("--rpshop-primary:#2050A7")
    );
    reopened.ai_document_replaced();
    assert!(reopened.ai_confirmed_palette().is_none());
}

struct MockFiles(PathBuf);

impl MockFiles {
    fn new() -> Option<Self> {
        if std::process::Command::new("node")
            .arg("--version")
            .output()
            .is_err()
        {
            eprintln!("Node.js unavailable; skipped local Codex palette protocol test");
            return None;
        }
        let dir =
            std::env::temp_dir().join(format!("rpshop-palette-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&dir).unwrap();
        std::fs::write(dir.join("mock.js"), MOCK_CODEX).unwrap();
        Some(Self(dir))
    }

    fn configure(&self, app: &mut App) {
        let mut engine = ai::EngineConfig::new(ai::EngineKind::Codex);
        engine.executable = Some(self.0.join("mock.js"));
        app.ai.selected_engine = engine.id.clone();
        app.ai.settings.default_engine_id = Some(engine.id.clone());
        app.ai.settings.engines.push(engine);
    }
}

impl Drop for MockFiles {
    fn drop(&mut self) {
        for name in ["mock.js", "project.rpshop", "ai-settings.json"] {
            let _ = std::fs::remove_file(self.0.join(name));
        }
        let _ = std::fs::remove_dir(&self.0);
    }
}

const MOCK_CODEX: &str = r##"
const assert = require('node:assert/strict');
const send = value => console.log(JSON.stringify(value));
const search = process.argv.includes('web_search="live"');
const source = 'https://example.com/visual-reference';
let palette = false;
let thread = '';
let prompt = '';
let paletteInput = '';
const complete = (text) => {
  send({method:'item/agentMessage/delta',params:{threadId:thread,delta:text}});
  send({method:'turn/completed',params:{turn:{status:'completed'}}});
};
require('node:readline').createInterface({input:process.stdin}).on('line', line => {
  const msg = JSON.parse(line);
  switch (msg.method) {
    case 'initialize': send({id:msg.id,result:{}}); break;
    case 'mcpServerStatus/list': send({id:msg.id,result:{data:[]}}); break;
    case 'thread/start':
    case 'thread/resume':
      palette = msg.params.ephemeral === true;
      thread = palette ? 'palette' : 'design';
      if (palette) assert.deepEqual(msg.params.dynamicTools, []);
      else assert(!search, 'only palette workers can search');
      send({id:msg.id,result:{thread:{id:thread}}});
      break;
    case 'turn/start': {
      send({id:msg.id,result:{turn:{id:'turn'}}});
      const input = msg.params.input[0].text;
      if (palette) {
        assert.equal(msg.params.input.length, 1);
        paletteInput = input;
        prompt = input.split('\n\n用户描述：\n')[1];
        if (prompt.includes('复古') && search) {
          send({method:'error',params:{message:'search unavailable'}});
          break;
        }
        if (search) {
          assert(prompt.includes('海岸'));
          const item = {type:'webSearch',query:'海岸 清爽 蓝白',action:{type:'openPage',url:source}};
          send({method:'item/completed',params:{threadId:thread,item}});
        }
        // A palette-only turn must reject even a forged project-tool request.
        send({id:41,method:'item/tool/call',params:{threadId:thread,tool:'rpshop_batch_edit',callId:'forbidden',
          arguments:{operations:[{op:'update_project',name:'unexpected edit'}]}}});
        break;
      }
      const match = input.match(/当前已确认的项目配色[^：]+：(\[[^\n]+?\])。主色/u);
      assert(match, 'design context must contain the confirmed palette');
      const roles = Object.fromEntries(JSON.parse(match[1]).map(row => [row.role,row.hex]));
      const user = input.split('\n\nUser request:\n')[1];
      const imageRequest = user.includes('用这张图的配色');
      if (imageRequest) {
        assert.equal(msg.params.input.length, 2);
        assert.equal(msg.params.input[1].type, 'image');
      }
      const operations = imageRequest || user.includes('紫色')
        ? [{op:'update_widget',page_id:1,widget_id:2,properties:{fill:imageRequest ? '#FF471F' : '#8844CC'}}]
        : (user.includes('第二个') ? [
          {op:'create_page',id:4,name:'第二页',background:roles.background},
          {op:'create_widget',page_id:4,kind:'Button',properties:{fill:roles.primary}},
          {op:'create_widget',page_id:4,kind:'Text',properties:{text_color:roles.text}},
        ] : [
          {op:'update_page',page_id:1,properties:{background:roles.background}},
          {op:'create_widget',page_id:1,kind:'Button',properties:{fill:roles.primary}},
          {op:'create_widget',page_id:1,kind:'Text',properties:{text_color:roles.text}},
        ]);
      send({id:42,method:'item/tool/call',params:{threadId:thread,tool:'rpshop_batch_edit',callId:'design',arguments:{operations}}});
      break;
    }
    default:
      if (msg.id === 41) {
        assert.equal(msg.result.success, false);
        const edits = prompt.includes('主色保留')
          ? [{op:'update',index:1,hex:'#D03041'},{op:'update',index:2,hex:'#FFF4E0'}]
          : Object.entries({primary:'#2379CD',background:'#F7FBFF',text:'#182536',accent:'#EB4448'})
              .map(([role,hex])=>({op:'add',role,hex}));
        if (prompt.includes('从图里')) {
          assert(paletteInput.includes('#FF471F'));
          edits.push({op:'add',role:'accent',hex:'#FF471F'});
        }
        complete(JSON.stringify({edits,sources:[source,'https://example.com/unobserved'],rationale:'蓝白基调配红色强调'}));
      }
      if (msg.id === 42) {
        assert.equal(msg.result.success, true);
        complete('已更新画布。');
      }
  }
});
"##;
