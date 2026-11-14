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

export const window = {
  createStatusBarItem: (alignment = 2, priority = 0): MockStatusBarItem => {
    return new MockStatusBarItem(alignment, priority);
  },
  activeTextEditor: undefined as
    | {
        document: { uri: MockUri };
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
};

export const workspace = {
  workspaceFolders: undefined as { uri: MockUri }[] | undefined,
  openTextDocument: (uri: MockUri) => Promise.resolve({ uri }),
};
