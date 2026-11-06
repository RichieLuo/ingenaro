"""Click only the launched RPShop HWND and request app-rendered screenshots.
Usage: python -I scripts/ai-desktop-qa.py target/debug/rpshop.exe target/ai-desktop-qa
Uses stdlib; screenshot mode disables real session/config/recovery writes.
"""
import ctypes
from ctypes import wintypes
import json
import os
from pathlib import Path
import subprocess
import sys
import time

user = ctypes.windll.user32
user.PostMessageW.argtypes = [wintypes.HWND, wintypes.UINT, wintypes.WPARAM, wintypes.LPARAM]
user.GetWindowRect.argtypes = [wintypes.HWND, ctypes.POINTER(wintypes.RECT)]


def main():
    executable = Path(sys.argv[1]).resolve()
    output = Path(sys.argv[2]).resolve()
    output.mkdir(parents=True, exist_ok=True)
    request = output / "capture-request.txt"
    environment = dict(os.environ, RPSHOP_QA_CAPTURE_REQUEST=str(request))
    first = output / "right-ai-canvas.png"
    process = subprocess.Popen([str(executable), "--screenshot", str(first), "--ai-design", "--qa-interactive"], env=environment)
    window = None
    try:
        def wait_for(path):
            deadline = time.monotonic() + 20
            while time.monotonic() < deadline:
                if path.is_file() and path.stat().st_size > 100:
                    return
                if process.poll() is not None:
                    raise RuntimeError("RPShop exited before screenshot")
                time.sleep(0.05)
            raise RuntimeError(f"RPShop did not render {path.name}")
        wait_for(first)
        callback_type = ctypes.WINFUNCTYPE(wintypes.BOOL, wintypes.HWND, wintypes.LPARAM)
        found = []
        @callback_type
        def visit(hwnd, unused):
            pid = wintypes.DWORD()
            user.GetWindowThreadProcessId(hwnd, ctypes.byref(pid))
            if pid.value == process.pid and user.IsWindowVisible(hwnd):
                found.append(hwnd)
            return True
        user.EnumWindows(visit, 0)
        if not found:
            raise RuntimeError("RPShop HWND not found by process ID")
        window = found[0]
        rect = wintypes.RECT()
        user.GetWindowRect(window, ctypes.byref(rect))
        width, height = rect.right - rect.left, rect.bottom - rect.top
        def capture(name):
            path = output / name
            request.write_text(str(path), encoding="utf-8")
            wait_for(path)
        def click(x, y, delay=0.3):
            point = (int(y) << 16) | int(x)
            user.PostMessageW(window, 0x0200, 0, point)
            user.PostMessageW(window, 0x0201, 1, point)
            time.sleep(0.1)
            user.PostMessageW(window, 0x0202, 0, point)
            time.sleep(delay)
        # Replay the true canvas reveal without touching the committed document.
        click(width - 40, 107)
        capture("composer-menu.png")
        click(width - 65, 214, delay=0.12)
        capture("drawing-early.png")
        time.sleep(0.65)
        capture("drawing-middle.png")
        time.sleep(2.2)
        capture("drawing-complete.png")
        if "--dialogs" in sys.argv[3:]:
            click(width - 300, height - 97)
            capture("permission-options.png")
            click(width - 270, height - 207)
            capture("full-authorization-dialog.png")
            # Close any open permission UI before opening settings.
            user.PostMessageW(window, 0x0100, 0x1B, 0)
            user.PostMessageW(window, 0x0101, 0x1B, 0)
            time.sleep(0.3)
        # Toolbar settings entry is stable across composer widths.
        click(293, 56)
        capture("ai-settings.png")
        if "--catalog" in sys.argv[3:]:
            # This only inspects login/version/model metadata, never sends a user prompt.
            click(330, 279)
            time.sleep(10.0)
            capture("local-agents-detected.png")
            click(800, 448)
            capture("local-model-list.png")
        print(json.dumps({"pid": process.pid, "size": [width, height], "output": str(output)}, ensure_ascii=False))
    finally:
        if process.poll() is None:
            if window:
                user.PostMessageW(window, 0x0010, 0, 0)
            try:
                process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                process.terminate()
                process.wait(timeout=5)
        request.unlink(missing_ok=True)


if __name__ == "__main__":
    main()
