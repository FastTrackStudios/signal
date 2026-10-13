//! JS-backed drag ghost overlay for smooth cursor-following and drop/cancel animations.

use crate::prelude::*;

pub fn start_drag_ghost(source_id: &str, label: &str) {
    let source_id = escape_js(source_id);
    let label = escape_js(label);
    let js = format!(
        r#"(function() {{
  const src = document.getElementById('{source_id}');
  if (!src) return;

  if (window.__dockGhostState?.cleanup) {{
    window.__dockGhostState.cleanup();
  }}

  const srcRect = src.getBoundingClientRect();
  const ghost = document.createElement('div');
  ghost.id = 'dock-drag-ghost';
  ghost.textContent = '{label}';
  ghost.style.position = 'fixed';
  ghost.style.left = srcRect.left + 'px';
  ghost.style.top = srcRect.top + 'px';
  ghost.style.width = srcRect.width + 'px';
  ghost.style.height = srcRect.height + 'px';
  ghost.style.pointerEvents = 'none';
  ghost.style.zIndex = '10000';
  ghost.style.border = '1px solid rgba(96,165,250,0.8)';
  ghost.style.background = 'rgba(30,41,59,0.72)';
  ghost.style.backdropFilter = 'blur(4px)';
  ghost.style.borderRadius = '6px';
  ghost.style.padding = '4px 8px';
  ghost.style.color = 'rgb(226,232,240)';
  ghost.style.fontSize = '12px';
  ghost.style.display = 'flex';
  ghost.style.alignItems = 'center';
  ghost.style.boxShadow = '0 10px 30px rgba(0,0,0,0.35)';
  ghost.style.opacity = '0.9';
  ghost.style.transition = 'none';

  document.body.appendChild(ghost);

  let raf = 0;
  let x = srcRect.left;
  let y = srcRect.top;

  function paint() {{
    raf = 0;
    ghost.style.transform = `translate(${{x - srcRect.left}}px, ${{y - srcRect.top}}px)`;
  }}

  function onDragOver(e) {{
    x = e.clientX - srcRect.width * 0.5;
    y = e.clientY - srcRect.height * 0.5;
    if (!raf) raf = requestAnimationFrame(paint);
  }}

  document.addEventListener('dragover', onDragOver, true);

  window.__dockGhostState = {{
    ghost,
    sourceId: '{source_id}',
    sourceRect: srcRect,
    cleanup: () => {{
      document.removeEventListener('dragover', onDragOver, true);
      if (raf) cancelAnimationFrame(raf);
      if (ghost.parentNode) ghost.parentNode.removeChild(ghost);
      window.__dockGhostState = null;
    }}
  }};
}})();"#,
    );
    document::eval(&js);
}

pub fn animate_drop_to(target_id: &str) {
    let target_id = escape_js(target_id);
    let js = format!(
        r#"(function() {{
  const st = window.__dockGhostState;
  if (!st || !st.ghost) return;
  const target = document.getElementById('{target_id}');
  if (!target) {{ st.cleanup(); return; }}

  const tr = target.getBoundingClientRect();
  const ghost = st.ghost;

  ghost.style.transition = 'all 150ms ease-out';
  ghost.style.left = tr.left + 'px';
  ghost.style.top = tr.top + 'px';
  ghost.style.width = Math.max(tr.width, 80) + 'px';
  ghost.style.height = Math.max(tr.height * 0.2, 28) + 'px';
  ghost.style.opacity = '0.0';
  ghost.style.transform = 'translate(0px, 0px) scale(0.96)';

  setTimeout(() => st.cleanup(), 170);
}})();"#,
    );
    document::eval(&js);
}

pub fn animate_cancel_back() {
    let js = r#"(function() {
  const st = window.__dockGhostState;
  if (!st || !st.ghost) return;
  const src = document.getElementById(st.sourceId);
  const sr = src ? src.getBoundingClientRect() : st.sourceRect;
  const ghost = st.ghost;

  ghost.style.transition = 'all 150ms ease-out';
  ghost.style.left = sr.left + 'px';
  ghost.style.top = sr.top + 'px';
  ghost.style.width = sr.width + 'px';
  ghost.style.height = sr.height + 'px';
  ghost.style.opacity = '0.0';
  ghost.style.transform = 'translate(0px, 0px) scale(0.96)';

  setTimeout(() => st.cleanup(), 170);
})();"#;
    document::eval(js);
}

fn escape_js(input: &str) -> String {
    input
        .replace('\\', "\\\\")
        .replace('\'', "\\'")
        .replace('\n', "\\n")
}
