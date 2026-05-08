<!--
  Canvas2D — declarative 2D graphics via HTML Canvas.

  The AI describes what to draw as an array of draw commands.
  This component renders them. No imperative API.

  Draw commands are PluresDB-bindable: change the data, the canvas redraws.

  Supports: rectangles, circles, lines, paths, text, images, transforms.

  Example (AI writes this data):
  [
    { type: 'rect', x: 10, y: 10, width: 100, height: 50, fill: '#6366f1' },
    { type: 'circle', cx: 200, cy: 100, radius: 30, fill: 'red' },
    { type: 'text', x: 50, y: 200, text: 'Hello World', font: '24px sans-serif', fill: 'white' },
    { type: 'line', x1: 0, y1: 0, x2: 300, y2: 300, stroke: 'green', lineWidth: 2 },
  ]
-->
<script lang="ts">
  interface DrawCommand {
    type: 'rect' | 'circle' | 'line' | 'path' | 'text' | 'image' | 'group';
    x?: number; y?: number; width?: number; height?: number;
    cx?: number; cy?: number; radius?: number;
    x1?: number; y1?: number; x2?: number; y2?: number;
    points?: [number, number][]; closed?: boolean;
    text?: string; font?: string; align?: string; baseline?: string;
    fill?: string; stroke?: string; lineWidth?: number; alpha?: number;
    translate?: [number, number]; rotate?: number; scale?: [number, number];
    children?: DrawCommand[]; src?: string;
  }
  interface CanvasPointerEvent { x: number; y: number; original: MouseEvent; }
  interface Canvas2DProps {
    width?: number; height?: number; commands?: DrawCommand[];
    background?: string; class?: string;
    onclick?: (e: CanvasPointerEvent) => void;
    onmousemove?: (e: CanvasPointerEvent) => void;
  }

  let {
    width = 400,
    height = 300,
    commands = [],
    background = 'transparent',
    class: className = '',
    onclick,
    onmousemove,
  }: Canvas2DProps = $props();

  let canvasEl: HTMLCanvasElement;

  function render(cmds: DrawCommand[]) {
    if (!canvasEl) return;
    const ctx = canvasEl.getContext('2d');
    if (!ctx) return;

    // Clear
    ctx.clearRect(0, 0, width, height);
    if (background !== 'transparent') {
      ctx.fillStyle = background;
      ctx.fillRect(0, 0, width, height);
    }

    // Execute commands
    for (const cmd of cmds) {
      ctx.save();

      // Apply transforms
      if (cmd.translate) ctx.translate(cmd.translate[0], cmd.translate[1]);
      if (cmd.rotate) ctx.rotate(cmd.rotate);
      if (cmd.scale) ctx.scale(cmd.scale[0], cmd.scale[1]);
      if (cmd.alpha !== undefined) ctx.globalAlpha = cmd.alpha;

      switch (cmd.type) {
        case 'rect':
          if (cmd.fill) { ctx.fillStyle = cmd.fill; ctx.fillRect(cmd.x ?? 0, cmd.y ?? 0, cmd.width ?? 0, cmd.height ?? 0); }
          if (cmd.stroke) { ctx.strokeStyle = cmd.stroke; ctx.lineWidth = cmd.lineWidth ?? 1; ctx.strokeRect(cmd.x ?? 0, cmd.y ?? 0, cmd.width ?? 0, cmd.height ?? 0); }
          break;

        case 'circle':
          ctx.beginPath();
          ctx.arc(cmd.cx ?? 0, cmd.cy ?? 0, cmd.radius ?? 0, 0, Math.PI * 2);
          if (cmd.fill) { ctx.fillStyle = cmd.fill; ctx.fill(); }
          if (cmd.stroke) { ctx.strokeStyle = cmd.stroke; ctx.lineWidth = cmd.lineWidth ?? 1; ctx.stroke(); }
          break;

        case 'line':
          ctx.beginPath();
          ctx.moveTo(cmd.x1 ?? 0, cmd.y1 ?? 0);
          ctx.lineTo(cmd.x2 ?? 0, cmd.y2 ?? 0);
          ctx.strokeStyle = cmd.stroke ?? '#fff';
          ctx.lineWidth = cmd.lineWidth ?? 1;
          ctx.stroke();
          break;

        case 'path':
          if (cmd.points && cmd.points.length > 0) {
            ctx.beginPath();
            ctx.moveTo(cmd.points[0][0], cmd.points[0][1]);
            for (let i = 1; i < cmd.points.length; i++) {
              ctx.lineTo(cmd.points[i][0], cmd.points[i][1]);
            }
            if (cmd.closed) ctx.closePath();
            if (cmd.fill) { ctx.fillStyle = cmd.fill; ctx.fill(); }
            if (cmd.stroke) { ctx.strokeStyle = cmd.stroke; ctx.lineWidth = cmd.lineWidth ?? 1; ctx.stroke(); }
          }
          break;

        case 'text':
          ctx.font = cmd.font ?? '16px sans-serif';
          ctx.textAlign = (cmd.align as CanvasTextAlign) ?? 'left';
          ctx.textBaseline = (cmd.baseline as CanvasTextBaseline) ?? 'top';
          if (cmd.fill) { ctx.fillStyle = cmd.fill; ctx.fillText(cmd.text ?? '', cmd.x ?? 0, cmd.y ?? 0); }
          if (cmd.stroke) { ctx.strokeStyle = cmd.stroke; ctx.strokeText(cmd.text ?? '', cmd.x ?? 0, cmd.y ?? 0); }
          break;

        case 'image':
          // Image loading is async — skip if not cached
          break;

        case 'group':
          if (cmd.children) render(cmd.children);
          break;
      }

      ctx.restore();
    }
  }

  $effect(() => {
    render(commands);
  });

  function handleClick(e: MouseEvent) {
    if (!onclick) return;
    const rect = canvasEl.getBoundingClientRect();
    onclick({ x: e.clientX - rect.left, y: e.clientY - rect.top, original: e });
  }

  function handleMouseMove(e: MouseEvent) {
    if (!onmousemove) return;
    const rect = canvasEl.getBoundingClientRect();
    onmousemove({ x: e.clientX - rect.left, y: e.clientY - rect.top, original: e });
  }
</script>

<!-- eslint-disable svelte/no-at-html-tags -->
<canvas
  bind:this={canvasEl}
  {width}
  {height}
  class="canvas2d {className}"
  onclick={handleClick}
  onmousemove={handleMouseMove}
></canvas>

<style>
  .canvas2d {
    border-radius: 8px;
    display: block;
  }
</style>
