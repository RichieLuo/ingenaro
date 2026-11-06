//! No-model diagnostic: lists CLI model metadata, never sends a user prompt or starts a turn.
fn main() {
    for config in rpshop::ai::local_defaults() {
        match rpshop::ai::discover_models(&config) {
            Ok(catalog) => println!(
                "{}",
                serde_json::json!({"engine":config.name,"catalog":catalog})
            ),
            Err(error) => println!(
                "{}",
                serde_json::json!({"engine":config.name,"error":error})
            ),
        }
    }
}
