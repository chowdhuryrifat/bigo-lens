/* BigO Lens Report Client Script */
(function () {
  const vscode = acquireVsCodeApi();

  // Notify extension that the report view is mounted and ready
  vscode.postMessage({ type: 'ready' });

  // Handle user interaction with source navigation links
  document.addEventListener('click', (event) => {
    const target = event.target;
    if (!target || !(target instanceof Element)) {
      return;
    }

    const button = target.closest('[data-action="navigate"]');
    if (!button) {
      return;
    }

    const file = button.getAttribute('data-file') || undefined;
    const lineAttr = button.getAttribute('data-line');
    const colAttr = button.getAttribute('data-column');

    const line = lineAttr ? parseInt(lineAttr, 10) : NaN;
    const column = colAttr ? parseInt(colAttr, 10) : undefined;

    if (!Number.isNaN(line) && line >= 1) {
      vscode.postMessage({
        type: 'navigate',
        location: {
          file,
          line,
          column: column && !Number.isNaN(column) && column >= 1 ? column : undefined,
        },
      });
    }
  });
})();
