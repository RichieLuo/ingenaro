use super::*;

fn cli(source: &str, kind: EngineKind) -> Option<(EngineConfig, PathBuf)> {
    if std::process::Command::new("node")
        .arg("--version")
        .output()
        .is_err()
    {
        return None;
    }
    let directory =
        std::env::temp_dir().join(format!("rpshop-catalog-test-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&directory).unwrap();
    let file = directory.join("agent.js");
    std::fs::write(&file, source).unwrap();
    let mut config = EngineConfig::new(kind);
    config.executable = Some(file);
    Some((config, directory))
}
#[test]
fn actual_codex_protocol_paginates_models_without_starting_a_turn() {
    let Some((config, directory)) = cli(
        r#"
const readline=require('readline');
readline.createInterface({input:process.stdin}).on('line',line=>{const m=JSON.parse(line);if(m.method==='initialize')console.log(JSON.stringify({id:m.id,result:{}}));else if(m.method==='model/list')console.log(JSON.stringify({id:m.id,result:m.params.cursor?{data:[{model:'two',displayName:'Second'}],nextCursor:null}:{data:[{model:'one',displayName:'First',isDefault:true,supportedReasoningEfforts:[{reasoningEffort:'low'},{reasoningEffort:'xhigh'}]},{model:'hidden',hidden:true}],nextCursor:'page2'}}));else if(m.method!=='initialized')process.exit(3);});
"#,
        EngineKind::Codex,
    ) else {
        return;
    };
    let models = discover_models(&config).unwrap();
    assert_eq!(models.models.len(), 2);
    assert_eq!(models.models[0].reasoning_efforts, ["low", "xhigh"]);
    assert_eq!(models.models[1].id, "two");
    std::fs::remove_dir_all(directory).unwrap();
}
#[test]
fn actual_claude_initialize_returns_models_without_user_message() {
    let Some((config, directory)) = cli(
        r#"
const readline=require('readline');readline.createInterface({input:process.stdin}).on('line',line=>{const m=JSON.parse(line);if(m.type==='user')process.exit(3);if(m.type==='control_request')console.log(JSON.stringify({type:'control_response',response:{subtype:'success',request_id:m.request_id,response:{models:[{value:'local-alias',displayName:'Configured model',description:'from CLI',supportsEffort:true,supportedEffortLevels:['low','medium','high','xhigh','max']}],account:{private:'not included'}}}}));});
"#,
        EngineKind::Claude,
    ) else {
        return;
    };
    let models = discover_models(&config).unwrap();
    assert_eq!(models.models[0].id, "local-alias");
    assert_eq!(
        models.models[0].reasoning_efforts,
        ["low", "medium", "high", "xhigh", "max"]
    );
    assert!(!serde_json::to_string(&models).unwrap().contains("private"));
    std::fs::remove_dir_all(directory).unwrap();
}
#[test]
fn repeated_cursor_fails_without_exposing_protocol_error_bodies() {
    let Some((config, directory)) = cli(
        r#"
const readline=require('readline');readline.createInterface({input:process.stdin}).on('line',line=>{const m=JSON.parse(line);if(m.method==='initialize')console.log(JSON.stringify({id:m.id,result:{}}));else if(m.method==='model/list')console.log(JSON.stringify({id:m.id,result:{data:[],nextCursor:'again'}}));});
"#,
        EngineKind::Codex,
    ) else {
        return;
    };
    assert!(discover_models(&config).is_err());
    std::fs::remove_dir_all(directory).unwrap();
}
