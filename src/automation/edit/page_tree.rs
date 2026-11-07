use super::*;

#[derive(Clone, Copy)]
pub(super) enum Entry<'a> {
    Folder(&'a str),
    Page(u64),
}

impl Entry<'_> {
    fn matches(self, node: &PageTreeNode) -> bool {
        match (self, node) {
            (Self::Folder(key), PageTreeNode::Folder { id, .. }) => key == id,
            (Self::Page(key), PageTreeNode::Page { page_id, .. }) => key == *page_id,
            _ => false,
        }
    }

    fn find(self, nodes: &[PageTreeNode]) -> Option<&PageTreeNode> {
        nodes.iter().find_map(|node| {
            if self.matches(node) {
                Some(node)
            } else {
                self.find(node.children())
            }
        })
    }

    fn locate(self, nodes: &mut Vec<PageTreeNode>) -> Option<(&mut Vec<PageTreeNode>, usize)> {
        if let Some(index) = nodes.iter().position(|node| self.matches(node)) {
            return Some((nodes, index));
        }
        nodes
            .iter_mut()
            .find_map(|node| self.locate(children_mut(node)))
    }
}

fn children_mut(node: &mut PageTreeNode) -> &mut Vec<PageTreeNode> {
    match node {
        PageTreeNode::Folder { children, .. } | PageTreeNode::Page { children, .. } => children,
    }
}

fn destination<'a>(
    tree: &'a mut Vec<PageTreeNode>,
    folder_id: Option<&str>,
) -> Result<&'a mut Vec<PageTreeNode>> {
    match folder_id {
        Some(id) => {
            let (nodes, index) = Entry::Folder(id)
                .locate(tree)
                .context("目标页面文件夹不存在")?;
            Ok(children_mut(&mut nodes[index]))
        }
        None => Ok(tree),
    }
}

pub(super) fn append_page(
    tree: &mut Vec<PageTreeNode>,
    page_id: u64,
    folder_id: Option<&str>,
) -> Result<()> {
    if !tree.is_empty() || folder_id.is_some() {
        destination(tree, folder_id)?.push(PageTreeNode::Page {
            page_id,
            children: Vec::new(),
        });
    }
    Ok(())
}

pub(super) fn create_folder(
    project: &mut Project,
    id: Option<String>,
    name: String,
    parent_id: Option<&str>,
) -> Result<String> {
    let id = id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    ensure!(
        !id.trim().is_empty()
            && id.len() <= 1024
            && Entry::Folder(&id).find(&project.page_tree).is_none(),
        "页面文件夹 ID 为空、过长或重复"
    );
    let name = folder_name(name)?;
    if project.page_tree.is_empty() && parent_id.is_none() {
        project.page_tree = project
            .pages
            .iter()
            .map(|page| PageTreeNode::Page {
                page_id: page.id,
                children: Vec::new(),
            })
            .collect();
    }
    destination(&mut project.page_tree, parent_id)?.push(PageTreeNode::Folder {
        id: id.clone(),
        name,
        children: Vec::new(),
    });
    Ok(id)
}

fn folder_name(name: String) -> Result<String> {
    let name = name.trim();
    ensure!(
        !name.is_empty() && name.len() <= 1024,
        "页面文件夹名称不能为空或超过 1024 字节"
    );
    Ok(name.to_owned())
}

pub(super) fn rename_folder(
    tree: &mut Vec<PageTreeNode>,
    id: &str,
    new_name: String,
) -> Result<()> {
    let (nodes, index) = Entry::Folder(id).locate(tree).context("页面文件夹不存在")?;
    if let PageTreeNode::Folder { name, .. } = &mut nodes[index] {
        *name = folder_name(new_name)?;
    }
    Ok(())
}

pub(super) fn move_entry(
    project: &mut Project,
    entry: Entry<'_>,
    folder_id: Option<&str>,
) -> Result<()> {
    let tree = &mut project.page_tree;
    if tree.is_empty()
        && folder_id.is_none()
        && matches!(entry, Entry::Page(id) if project.pages.iter().any(|page| page.id == id))
    {
        return Ok(());
    }
    let node = entry.find(tree).context("页面目录节点不存在")?;
    if let Some(id) = folder_id {
        let target = Entry::Folder(id);
        ensure!(
            !target.matches(node) && target.find(node.children()).is_none(),
            "不能将目录节点移入自身或其下级文件夹"
        );
    }
    if destination(tree, folder_id)?
        .iter()
        .any(|node| entry.matches(node))
    {
        return Ok(());
    }
    let (siblings, index) = entry.locate(tree).context("页面目录节点不存在")?;
    let node = siblings.remove(index);
    destination(tree, folder_id)?.push(node);
    Ok(())
}

pub(super) fn remove_entry(tree: &mut Vec<PageTreeNode>, entry: Entry<'_>) -> bool {
    let Some((siblings, index)) = entry.locate(tree) else {
        return false;
    };
    let mut node = siblings.remove(index);
    siblings.splice(index..index, std::mem::take(children_mut(&mut node)));
    true
}
