export class MockUri {
  public readonly fsPath: string;
  public readonly scheme: string;
  public readonly path: string;

  constructor(fsPath: string, scheme = 'file') {
    this.fsPath = fsPath;
    this.scheme = scheme;
    this.path = fsPath;
  }

  static file(fsPath: string): MockUri {
    return new MockUri(fsPath, 'file');
  }

  static joinPath(base: MockUri, ...segments: string[]): MockUri {
    const cleanBase = base.fsPath.replace(/\/+$/, '');
    const joined = [cleanBase, ...segments].join('/');
    return new MockUri(joined, base.scheme);
  }

  static parse(uriStr: string): MockUri {
    return new MockUri(uriStr, 'vscode-webview');
  }

  toString(): string {
    return `${this.scheme}://${this.path}`;
  }
}

export class MockWebview {
  public cspSource = 'vscode-webview:';
  public html = '';
  public messageListeners: ((msg: unknown) => void)[] = [];
  public postedMessages: unknown[] = [];

  asWebviewUri(uri: MockUri): MockUri {
    return new MockUri(`vscode-webview://media/${uri.path}`, 'vscode-webview');
  }

  onDidReceiveMessage(
    listener: (msg: unknown) => void,
    _thisArgs?: unknown,
    disposables?: { dispose: () => void }[],
  ): { dispose: () => void } {
    this.messageListeners.push(listener);
    const disposable = {
      dispose: () => {
        const index = this.messageListeners.indexOf(listener);
        if (index >= 0) {
          this.messageListeners.splice(index, 1);
        }
      },
    };
    if (disposables) {
      disposables.push(disposable);
    }
    return disposable;
  }

  postMessage(msg: unknown): Promise<boolean> {
    this.postedMessages.push(msg);
    return Promise.resolve(true);
  }

  simulateMessage(msg: unknown): void {
    for (const listener of [...this.messageListeners]) {
      listener(msg);
    }
  }
}

export interface WebviewPanelCreationOptions {
  enableScripts?: boolean | undefined;
  retainContextWhenHidden?: boolean | undefined;
  localResourceRoots?: readonly MockUri[] | undefined;
}

export class MockWebviewPanel {
  public viewType: string;
  public title: string;
  public showOptions: unknown;
  public options: WebviewPanelCreationOptions;
  public webview: MockWebview = new MockWebview();
  public isDisposed = false;
  public revealedColumn?: number | undefined;
  public disposeListeners: (() => void)[] = [];

  constructor(
    viewType: string,
    title: string,
    showOptions: unknown,
    options: WebviewPanelCreationOptions = {},
  ) {
    this.viewType = viewType;
    this.title = title;
    this.showOptions = showOptions;
    this.options = options;
  }

  reveal(column?: number): void {
    this.revealedColumn = column;
  }

  onDidDispose(
    listener: () => void,
    _thisArgs?: unknown,
    disposables?: { dispose: () => void }[],
  ): { dispose: () => void } {
    this.disposeListeners.push(listener);
    const disposable = {
      dispose: () => {
        const index = this.disposeListeners.indexOf(listener);
        if (index >= 0) {
          this.disposeListeners.splice(index, 1);
        }
      },
    };
    if (disposables) {
      disposables.push(disposable);
    }
    return disposable;
  }

  dispose(): void {
    if (this.isDisposed) {
      return;
    }
    this.isDisposed = true;
    for (const listener of [...this.disposeListeners]) {
      listener();
    }
  }
}

export const createdPanels: MockWebviewPanel[] = [];

export function resetCreatedPanels(): void {
  createdPanels.length = 0;
}

export const Uri = MockUri;

export const ViewColumn = {
  Active: -1,
  Beside: -2,
  One: 1,
  Two: 2,
} as const;

export const StatusBarAlignment = {
  Left: 1,
  Right: 2,
} as const;

export class MarkdownString {
  public value = '';
  public isTrusted = false;

  constructor(value = '', isTrusted = false) {
    this.value = value;
    this.isTrusted = isTrusted;
  }

  appendMarkdown(value: string): this {
    this.value += value;
    return this;
  }
}

export class MockStatusBarItem {
  public alignment: number;
  public priority: number;
  public text = '';
  public tooltip: unknown = '';
  public command?: string;
  public name?: string;
  public accessibilityInformation?: { label: string; role?: string } | undefined;
  public isVisible = false;
  public isDisposed = false;

  constructor(alignment = 2, priority = 0) {
    this.alignment = alignment;
    this.priority = priority;
  }

  show(): void {
    this.isVisible = true;
  }

  hide(): void {
    this.isVisible = false;
  }

  dispose(): void {
    this.isDisposed = true;
    this.isVisible = false;
  }
}

export const TextEditorRevealType = {
  Default: 0,
  InCenter: 1,
  InCenterIfOutsideViewport: 2,
  AtTop: 3,
} as const;

export class Position {
  constructor(
    public readonly line: number,
    public readonly character: number,
  ) {}
}

export class Range {
  constructor(
    public readonly start: Position,
    public readonly end: Position,
  ) {}
}

export class Selection extends Range {
  constructor(
    public readonly anchor: Position,
    public readonly active: Position,
  ) {
    super(anchor, active);
  }
}

export interface NavigationRecord {
  documentUri: MockUri;
  selection: Selection;
}

export const navigationRecords: NavigationRecord[] = [];

export const ProgressLocation = {
  SourceControl: 1,
  Window: 10,
  Notification: 15,
} as const;

export class EventEmitter<T> {
  private readonly listeners: ((e: T) => unknown)[] = [];

  public readonly event = (listener: (e: T) => unknown): { dispose: () => void } => {
    this.listeners.push(listener);
    return {
      dispose: () => {
        const index = this.listeners.indexOf(listener);
        if (index >= 0) {
          this.listeners.splice(index, 1);
        }
      },
    };
  };

  public fire(data: T): void {
    for (const listener of this.listeners) {
      listener(data);
    }
  }

  public dispose(): void {
    this.listeners.length = 0;
  }
}

export class TreeItem {
  public iconPath?: unknown;
  public description?: string;
  public tooltip?: unknown;
  public command?: unknown;
  public accessibilityInformation?: { label: string; role?: string } | undefined;

  constructor(
    public label: string,
    public collapsibleState?: unknown,
  ) {}
}

export const TreeItemCollapsibleState = {
  None: 0,
  Collapsed: 1,
  Expanded: 2,
} as const;

export class ThemeIcon {
  constructor(public id: string) {}
}

export const DiagnosticSeverity = {
  Error: 0,
  Warning: 1,
  Information: 2,
  Hint: 3,
} as const;

export class Diagnostic {
  public source?: string;
  public code?: unknown;

  constructor(
    public range: Range,
    public message: string,
    public severity?: unknown,
  ) {}
}

export class MockDiagnosticCollection {
  public readonly entries = new Map<string, Diagnostic[]>();

  public set(uri: { fsPath?: string; toString?: () => string }, diagnostics: Diagnostic[]): void {
    const key = uri.fsPath ?? (typeof uri.toString === 'function' ? uri.toString() : '');
    this.entries.set(key, diagnostics);
  }

  public delete(uri: { fsPath?: string; toString?: () => string }): void {
    const key = uri.fsPath ?? (typeof uri.toString === 'function' ? uri.toString() : '');
    this.entries.delete(key);
  }

  public clear(): void {
    this.entries.clear();
  }

  public dispose(): void {
    this.clear();
  }
}

export const languages = {
  createDiagnosticCollection: (): MockDiagnosticCollection => {
    return new MockDiagnosticCollection();
  },
};

export const window = {
  createStatusBarItem: (alignment = 2, priority = 0): MockStatusBarItem => {
    return new MockStatusBarItem(alignment, priority);
  },
  activeTextEditor: undefined as
    | {
        document: { uri: MockUri; languageId?: string; fileName?: string; getText?: () => string };
        selection?: Selection;
        revealRange?: (range: Range, type: unknown) => void;
      }
    | undefined,
  createWebviewPanel: (
    viewType: string,
    title: string,
    showOptions: unknown,
    options?: WebviewPanelCreationOptions,
  ): MockWebviewPanel => {
    const panel = new MockWebviewPanel(viewType, title, showOptions, options ?? {});
    createdPanels.push(panel);
    return panel;
  },
  showTextDocument: (doc: {
    uri: MockUri;
  }): Promise<{
    document: { uri: MockUri };
    selection: Selection;
    revealRange: (range: Range, type: unknown) => void;
  }> => {
    const editor = {
      document: doc,
      selection: new Selection(new Position(0, 0), new Position(0, 0)),
      revealRange: () => {},
    };
    navigationRecords.push({
      documentUri: doc.uri,
      get selection() {
        return editor.selection;
      },
    });
    return Promise.resolve(editor);
  },
  showInformationMessage: (): Promise<string | undefined> => Promise.resolve(undefined),
  showWarningMessage: (): Promise<string | undefined> => Promise.resolve(undefined),
  showErrorMessage: (): Promise<string | undefined> => Promise.resolve(undefined),
  withProgress: <R>(
    _options: { location: unknown; title?: string; cancellable?: boolean },
    task: (
      progress: { report: (value: { message?: string; increment?: number }) => void },
      token: {
        isCancellationRequested: boolean;
        onCancellationRequested: (listener: () => void) => { dispose: () => void };
      },
    ) => Promise<R>,
  ): Promise<R> => {
    let cancelFn: (() => void) | undefined;
    const token = {
      isCancellationRequested: false,
      onCancellationRequested: (listener: () => void) => {
        cancelFn = listener;
        return {
          dispose: () => {
            cancelFn = undefined;
          },
        };
      },
      cancel: () => {
        token.isCancellationRequested = true;
        cancelFn?.();
      },
    };
    return task({ report: () => {} }, token);
  },
};

export const workspace = {
  isTrusted: true,
  workspaceFolders: undefined as { uri: MockUri }[] | undefined,
  openTextDocument: (uri: MockUri) => Promise.resolve({ uri }),
  asRelativePath: (uri: unknown): string => {
    if (typeof uri === 'object' && uri !== null && 'fsPath' in uri) {
      return (uri as { fsPath: string }).fsPath;
    }
    return String(uri);
  },
  getConfiguration: () => ({
    get: <T>(_section: string, defaultValue?: T): T | undefined => defaultValue,
    inspect: <T>() => ({
      globalValue: undefined as T | undefined,
      workspaceValue: undefined as T | undefined,
      workspaceFolderValue: undefined as T | undefined,
    }),
  }),
};

export const commands = {
  executeCommand: (): Promise<unknown> => Promise.resolve(undefined),
};
