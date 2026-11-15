<script lang="ts">
  // Iconos de trazo decorativos (aria-hidden): el texto de al lado da el significado.
  let { name, size = 20 }: { name: string; size?: number } = $props();
  const paths: Record<string, string> = {
    lock: 'M5 11h14v10H5z M8 11V8a4 4 0 0 1 8 0v3',
    unlock: 'M5 11h14v10H5z M8 11V8a4 4 0 0 1 7.5-2',
    moon: 'M20 14.5A8 8 0 1 1 9.5 4a6.5 6.5 0 0 0 10.5 10.5z',
    check: 'M5 12.5l4.5 4.5L19 7.5',
    clock: 'M12 3.5a8.5 8.5 0 1 0 0 17a8.5 8.5 0 1 0 0-17z M12 7.5V12l3 2',
    link: 'M10 14a4 4 0 0 0 5.7 0l3-3a4 4 0 0 0-5.7-5.7l-1 1 M14 10a4 4 0 0 0-5.7 0l-3 3a4 4 0 0 0 5.7 5.7l1-1',
    alert: 'M12 4l9 16H3z M12 10v4 M12 17v1',
    target: 'M12 3.5a8.5 8.5 0 1 0 0 17a8.5 8.5 0 1 0 0-17z M12 8a4 4 0 1 0 0 8a4 4 0 1 0 0-8z',
    leaf: 'M5 19c0-8 6-14 14-14 0 8-6 14-14 14z M5 19l7-7',
    sync: 'M4 12a8 8 0 0 1 14-5.3L20 9 M20 4v5h-5 M20 12a8 8 0 0 1-14 5.3L4 15 M4 20v-5h5',
    sun: 'M12 8a4 4 0 1 0 0 8a4 4 0 1 0 0-8z M12 2v2 M12 20v2 M2 12h2 M20 12h2 M5 5l1.5 1.5 M17.5 17.5L19 19 M5 19l1.5-1.5 M17.5 6.5L19 5',
    plus: 'M12 5v14 M5 12h14',
    home: 'M4 11l8-7 8 7 M6 9.5V20h12V9.5',
  };
</script>

<svg
  width={size}
  height={size}
  viewBox="0 0 24 24"
  fill="none"
  stroke="currentColor"
  stroke-width="1.8"
  stroke-linecap="round"
  stroke-linejoin="round"
  aria-hidden="true"
>
  <path d={paths[name] ?? ''} />
</svg>
