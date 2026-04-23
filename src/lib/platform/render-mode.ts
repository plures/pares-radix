/**
 * Render Mode — controls how pares-radix renders components
 *
 * Three modes:
 * 1. gui (default) — Standard Svelte rendering in Tauri webview
 * 2. tui-css — Same webview with terminal-aesthetic CSS theme (design-dojo TUI theme)
 * 3. tui-native — svelte-ratatui compiled to native ratatui widgets (no webview)
 *
 * Detection:
 * - TAURI_TUI=1 env var → tui-native (if svelte-ratatui plugin active)
 * - RADIX_RENDER_MODE=tui-css → tui-css
 * - --tui CLI arg → tui-css (fallback if native not available)
 * - Default → gui
 *
 * The render mode is a praxis fact (render.mode) so it can be toggled
 * from design mode and persisted across sessions.
 */

export type RenderMode = 'gui' | 'tui-css' | 'tui-native';

/** Detect the initial render mode from environment */
export function detectRenderMode(): RenderMode {
  if (typeof process !== 'undefined') {
    if (process.env.TAURI_TUI === '1') return 'tui-native';
    if (process.env.RADIX_RENDER_MODE === 'tui-css') return 'tui-css';
  }

  // Check URL params (useful for dev/testing)
  if (typeof window !== 'undefined') {
    const params = new URLSearchParams(window.location.search);
    const mode = params.get('render');
    if (mode === 'tui-css' || mode === 'tui-native') return mode;
  }

  return 'gui';
}

/** CSS class applied to the root element based on render mode */
export function renderModeClass(mode: RenderMode): string {
  switch (mode) {
    case 'tui-css': return 'render-tui-css';
    case 'tui-native': return 'render-tui-native';
    default: return 'render-gui';
  }
}

/**
 * TUI CSS theme overrides — applied when render mode is 'tui-css'.
 * These make the webview look like a terminal application.
 */
export const tuiCssOverrides = `
  .render-tui-css {
    font-family: 'JetBrains Mono', 'Fira Code', 'Cascadia Code', monospace !important;
    font-size: 14px;
    line-height: 1.4;
    -webkit-font-smoothing: none;
    image-rendering: pixelated;
  }

  .render-tui-css * {
    font-family: inherit !important;
    border-radius: 0 !important;
    box-shadow: none !important;
    transition: none !important;
  }

  .render-tui-css button,
  .render-tui-css input,
  .render-tui-css textarea,
  .render-tui-css select {
    border: 1px solid var(--color-border) !important;
    background: var(--color-bg) !important;
  }

  .render-tui-css button:hover,
  .render-tui-css button:focus {
    background: var(--color-accent-bg) !important;
    outline: 1px solid var(--color-accent) !important;
  }

  /* Box-drawing borders for panels */
  .render-tui-css .app > aside,
  .render-tui-css .filter-sidebar,
  .render-tui-css .detail-panel,
  .render-tui-css .schema-detail,
  .render-tui-css .rule-editor {
    border: 1px solid var(--color-border) !important;
    border-radius: 0 !important;
  }

  /* Cursor-style focus indicators */
  .render-tui-css .schema-item.selected,
  .render-tui-css .kind-btn.active {
    background: var(--color-accent) !important;
    color: var(--color-bg) !important;
  }

  /* Scanline effect (subtle) */
  .render-tui-css::after {
    content: '';
    position: fixed;
    top: 0;
    left: 0;
    right: 0;
    bottom: 0;
    pointer-events: none;
    background: repeating-linear-gradient(
      transparent 0px,
      transparent 1px,
      rgba(0, 0, 0, 0.03) 1px,
      rgba(0, 0, 0, 0.03) 2px
    );
    z-index: 9999;
  }
`;
