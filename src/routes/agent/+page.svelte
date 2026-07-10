<script lang="ts">
  // Thin wrapper: the /agent route is a full-screen presentation of the shared
  // AgensSurface. The identical surface also mounts as a right-dock pane
  // (see +layout.svelte paneBody) — ONE implementation, two mount points
  // (C-NOSTUB-001: honest desktop-only empty-state in browser, real agent-api
  // in Tauri; no duplication, no drift).
  import AgensSurface from '$lib/plugins/agens/AgensSurface.svelte';
</script>

<svelte:head><title>Agens — Radix</title></svelte:head>

<AgensSurface />
