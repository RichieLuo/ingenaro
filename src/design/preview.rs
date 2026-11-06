use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result, ensure};

use super::validate_html;

static NEXT_PREVIEW: AtomicU64 = AtomicU64::new(1);

// The parent cannot load anything but the initial srcdoc/about document, including
// when child script tries to navigate its own frame to a remote/local URL.
const PARENT_POLICY: &str = "default-src 'none'; script-src 'unsafe-inline'; style-src 'unsafe-inline'; img-src data:; frame-src about:; connect-src 'none'; object-src 'none'; base-uri 'none'; form-action 'none'";
const CHILD_POLICY: &str = "default-src 'none'; script-src 'unsafe-inline'; style-src 'unsafe-inline'; img-src data:; font-src data:; media-src data:; connect-src 'none'; frame-src 'none'; worker-src 'none'; object-src 'none'; base-uri 'none'; form-action 'none'; webrtc 'block'";
// CSP connect-src does not cover WebRTC on all browser versions. Remove the
// constructors before generated script runs, in this opaque, script-only frame.
const CHILD_GUARD: &str = "for(const key of ['RTCPeerConnection','webkitRTCPeerConnection','RTCIceGatherer']){try{Object.defineProperty(window,key,{value:undefined,writable:false,configurable:false})}catch{}}";

/// Self-contained viewer usable by a browser or WebView2 `load_html`.
/// Never pass the raw source directly to a webview. The generated document lives
/// only in an escaped srcdoc attribute, in an opaque-origin script-only sandbox.
/// Host webviews should additionally deny permission/download requests and new
/// windows, and cancel all navigation away from the initial viewer document.
pub fn preview_document(source: &str) -> Result<String> {
    validate_html(source)?;
    let child = format!(
        "<!doctype html><meta charset=\"utf-8\"><meta http-equiv=\"Content-Security-Policy\" content=\"{CHILD_POLICY}\"><meta name=\"referrer\" content=\"no-referrer\"><meta http-equiv=\"x-dns-prefetch-control\" content=\"off\"><script>{CHILD_GUARD}</script>{source}"
    );
    let srcdoc = escape_attribute(&child);
    Ok(format!(
        r##"<!doctype html>
<html lang="zh-CN"><head><meta charset="utf-8">
<meta http-equiv="Content-Security-Policy" content="{PARENT_POLICY}">
<meta name="viewport" content="width=device-width,initial-scale=1">
<title>RPShop · 设计预览</title>
<style>
*{{box-sizing:border-box}}body{{margin:0;background:#eeede8;color:#343b34;font:13px system-ui,sans-serif}}header{{min-height:58px;display:flex;align-items:center;gap:20px;padding:10px 22px;border-bottom:1px solid #d9dcd3;background:#fafbf7}}strong{{font-size:14px}}nav{{display:flex;gap:4px}}button{{font:inherit;color:inherit;cursor:pointer;border:1px solid transparent;border-radius:6px;background:none;padding:7px 12px}}button:hover{{background:#e5e9df}}button[aria-pressed="true"]{{background:#dae4d4;color:#24492f}}button:focus-visible{{outline:2px solid #386847;outline-offset:2px}}small{{margin-left:auto;color:#626d60}}main{{height:calc(100dvh - 59px);padding:16px;overflow:auto}}iframe{{display:block;width:100%;height:100%;min-height:360px;margin:auto;border:1px solid #dce0d5;border-radius:10px;background:#fff;box-shadow:0 10px 45px #293e3110}}iframe[data-size="tablet"]{{width:768px;max-width:100%}}iframe[data-size="mobile"]{{width:390px;max-width:100%}}@media(max-width:680px){{header{{padding:10px;gap:8px}}small{{display:none}}button{{padding:6px 8px}}main{{padding:8px}}}}@media(prefers-reduced-motion:reduce){{*{{scroll-behavior:auto!important}}}}
</style></head><body>
<header><strong>RPShop · 设计预览</strong><nav aria-label="预览尺寸"><button data-size="desktop" aria-pressed="true">桌面</button><button data-size="tablet" aria-pressed="false">平板</button><button data-size="mobile" aria-pressed="false">手机</button></nav><small>隔离预览 · 无外部资源</small></header>
<main><iframe id="design" title="设计作品预览" sandbox="allow-scripts" referrerpolicy="no-referrer" allow="camera 'none'; microphone 'none'; geolocation 'none'; clipboard-read 'none'; clipboard-write 'none'; display-capture 'none'; usb 'none'" srcdoc="{srcdoc}"></iframe></main>
<script>document.querySelectorAll('button[data-size]').forEach(button=>button.addEventListener('click',()=>{{document.getElementById('design').dataset.size=button.dataset.size;document.querySelectorAll('button[data-size]').forEach(item=>item.setAttribute('aria-pressed',String(item===button)))}}));</script>
</body></html>"##
    ))
}

fn escape_attribute(source: &str) -> String {
    let mut output = String::with_capacity(source.len());
    for character in source.chars() {
        match character {
            '&' => output.push_str("&amp;"),
            '"' => output.push_str("&quot;"),
            '\'' => output.push_str("&#39;"),
            '<' => output.push_str("&lt;"),
            '>' => output.push_str("&gt;"),
            _ => output.push(character),
        }
    }
    output
}

/// Write a safe wrapper, never a raw generated file, in a unique temp directory.
/// The returned directory is owned by this preview; callers may remove it once
/// their browser/webview is finished. Existing previews are never overwritten.
pub fn write_preview(source: &str) -> Result<PathBuf> {
    let document = preview_document(source)?;
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    for _ in 0..8 {
        let serial = NEXT_PREVIEW.fetch_add(1, Ordering::Relaxed);
        let directory = std::env::temp_dir().join(format!(
            "rpshop-design-{}-{timestamp}-{serial}",
            std::process::id()
        ));
        match fs::create_dir(&directory) {
            Ok(()) => {
                let path = directory.join("index.html");
                if let Err(error) = write_new(&path, document.as_bytes()) {
                    let _ = fs::remove_file(&path);
                    let _ = fs::remove_dir(&directory);
                    return Err(error);
                }
                return Ok(path);
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error).context("无法创建隔离预览目录"),
        }
    }
    anyhow::bail!("无法创建唯一的预览目录")
}

/// Open a local safe viewer without invoking a shell. Return its path for cleanup.
pub fn open_preview(source: &str) -> Result<PathBuf> {
    let path = write_preview(source)?;
    #[cfg(target_os = "windows")]
    let result = {
        use std::os::windows::process::CommandExt;
        Command::new("explorer.exe")
            .arg(&path)
            .creation_flags(0x0800_0000)
            .spawn()
    };
    #[cfg(target_os = "macos")]
    let result = Command::new("open").arg(&path).spawn();
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    let result = Command::new("xdg-open").arg(&path).spawn();
    if let Err(error) = result {
        let _ = fs::remove_file(&path);
        if let Some(directory) = path.parent() {
            let _ = fs::remove_dir(directory);
        }
        return Err(error).context("无法打开系统浏览器");
    }
    Ok(path)
}

/// Explicit standalone HTML export. The exported viewer is self-contained and
/// retains the same sandbox/CSP as in-app preview, including scripts/interactions.
/// It never overwrites a file: after UI confirmation, remove/rename the old file
/// through the UI's existing save workflow, or choose a new path and retry.
/// Returning a viewer rather than raw source avoids making export a sandbox bypass.
pub fn export_html(path: &Path, source: &str) -> Result<()> {
    ensure!(
        path.extension()
            .and_then(|v| v.to_str())
            .is_some_and(|v| v.eq_ignore_ascii_case("html") || v.eq_ignore_ascii_case("htm")),
        "导出文件名需要使用 .html 或 .htm 扩展名"
    );
    let document = preview_document(source)?;
    write_new(path, document.as_bytes())
}

fn write_new(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(path)
        .with_context(|| format!("无法创建 HTML 文件（不会覆盖已有文件）：{}", path.display()))?;
    if let Err(error) = file.write_all(bytes).and_then(|_| file.sync_all()) {
        drop(file);
        let _ = fs::remove_file(path);
        return Err(error).context("HTML 写入失败");
    }
    Ok(())
}
