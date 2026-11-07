//! Read-only catalog inspection of locally supplied RP libraries.
//! Compile against the workspace's rpshop and serde_json libraries.
use ingenaro::rp_format::{binary, container};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

fn parts(index: &Value, out: &mut BTreeMap<String, u64>) {
    if let Some(children) = index.get("children").and_then(Value::as_object) {
        for (id, child) in children {
            if let Some(offset) = child.pointer("/parts/package").and_then(Value::as_u64) {
                out.insert(id.clone(), offset);
            }
            parts(child, out);
        }
    }
}

fn categories(graph: &binary::Graph) -> BTreeMap<String, String> {
    let mut output = BTreeMap::new();
    let Some(map) = graph
        .root()
        .and_then(|n| n.properties.get("sitemap"))
        .and_then(|r| graph.resolve(r))
    else {
        return output;
    };
    let Some(table) = map.properties.get("node-table").and_then(Value::as_object) else {
        return output;
    };
    fn visit(
        graph: &binary::Graph,
        table: &serde_json::Map<String, Value>,
        id: &str,
        category: &str,
        seen: &mut BTreeSet<String>,
        output: &mut BTreeMap<String, String>,
    ) {
        if !seen.insert(id.to_owned()) {
            return;
        }
        let mut next = category.to_owned();
        if let Some(package) = table.get(id).and_then(binary::reference_id) {
            if let Some(folder) = graph.node(package).filter(|n| n.kind.contains("Folder")) {
                let name = folder
                    .properties
                    .get("name")
                    .or_else(|| folder.properties.get("Name"))
                    .and_then(Value::as_str)
                    .unwrap_or("");
                if !name.is_empty() {
                    next = if category.is_empty() {
                        name.to_owned()
                    } else {
                        format!("{category}/{name}")
                    };
                }
            } else {
                output.insert(package.to_owned(), category.to_owned());
            }
        }
        if let Some(children) = graph
            .node(id)
            .and_then(|n| n.properties.get("children"))
            .and_then(Value::as_array)
        {
            for child in children {
                if let Some(child_id) = binary::reference_id(child) {
                    visit(graph, table, child_id, &next, seen, output);
                }
            }
        }
    }
    if let Some(root) = map.properties.get("tree").and_then(binary::reference_id) {
        visit(graph, table, root, "", &mut BTreeSet::new(), &mut output);
    }
    output
}

fn inspect(path: &Path, summary: bool) -> Result<Value, Box<dyn std::error::Error>> {
    let container = container::open(path)?;
    let offset = container
        .index
        .pointer("/root/parts/DesignDocument")
        .and_then(Value::as_u64)
        .ok_or("missing DesignDocument")?;
    let graph = binary::parse(&container.part(offset)?)?;
    let categories = categories(&graph);
    let mut offsets = BTreeMap::new();
    parts(&container.index["root"], &mut offsets);
    let mut entries = Vec::new();
    for node in graph
        .objects
        .values()
        .filter(|n| n.kind == "Axure:PackageInfo")
    {
        let Some(id) = node
            .properties
            .get("package-id")
            .and_then(binary::reference_id)
        else {
            continue;
        };
        if node
            .properties
            .pointer("/package-id/$type")
            .and_then(Value::as_str)
            != Some("Axure:Page")
        {
            continue;
        }
        let mut kinds = BTreeMap::<String, usize>::new();
        let mut shapes = BTreeMap::<String, usize>::new();
        if let Some(offset) = offsets.get(id).filter(|_| !summary) {
            let page = binary::parse(&container.part(*offset)?)?;
            for object in page
                .objects
                .values()
                .filter(|n| n.kind.starts_with("Axure:DiagramObject:"))
            {
                *kinds.entry(object.kind.clone()).or_default() += 1;
                if let Some(shape) = object.properties.get("shape-type") {
                    *shapes.entry(shape.to_string()).or_default() += 1;
                }
            }
        }
        entries.push(json!({"id":id,"name":node.properties.get("package-name"),"category":categories.get(id),"object_kinds":kinds,"shape_types":shapes}));
    }
    entries.sort_by(|a, b| a["name"].as_str().cmp(&b["name"].as_str()));
    let folders: Vec<_> = graph.objects.values()
        .filter(|n| n.kind.contains("Folder"))
        .map(|n| json!({"kind":n.kind,"name":n.properties.get("name").or_else(|| n.properties.get("Name"))}))
        .collect();
    Ok(
        json!({"path":path,"version":container.version,"entry_count":entries.len(),"entries":entries,"folders":folders}),
    )
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut libraries = Vec::new();
    let args: Vec<_> = std::env::args().skip(1).collect();
    let summary = args.iter().any(|arg| arg == "--summary");
    for path in args.iter().filter(|arg| arg.as_str() != "--summary") {
        libraries.push(inspect(Path::new(path), summary)?);
    }
    println!(
        "{}",
        serde_json::to_string_pretty(
            &json!({"authority":"local-shipped-library-parsed-by-rpshop","runtime_observed":false,"libraries":libraries})
        )?
    );
    Ok(())
}
