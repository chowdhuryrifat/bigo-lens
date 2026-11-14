/**
 * Types and validation for communication between the extension and webview reports.
 */

export interface WebviewLocation {
  file?: string;
  line: number;
  column?: number;
}

export interface NavigateMessage {
  type: 'navigate';
  location: WebviewLocation;
}

export interface ReadyMessage {
  type: 'ready';
}

export interface RunBenchmarkMessage {
  type: 'runBenchmark';
}

export type WebviewIncomingMessage = NavigateMessage | ReadyMessage | RunBenchmarkMessage;

export type MessageValidationResult =
  { ok: true; message: WebviewIncomingMessage } | { ok: false; error: string };

/**
 * Strict validator for messages sent from the untrusted/less-trusted webview context.
 * Rejects unknown types, non-objects, and malformed payloads.
 */
export function validateWebviewMessage(raw: unknown): MessageValidationResult {
  if (typeof raw !== 'object' || raw === null || Array.isArray(raw)) {
    return { ok: false, error: 'Message must be a non-null object' };
  }

  const candidate = raw as Record<string, unknown>;
  if (typeof candidate.type !== 'string' || candidate.type.trim() === '') {
    return { ok: false, error: 'Message must contain a non-empty string "type" property' };
  }

  switch (candidate.type) {
    case 'ready': {
      return { ok: true, message: { type: 'ready' } };
    }
    case 'runBenchmark': {
      return { ok: true, message: { type: 'runBenchmark' } };
    }
    case 'navigate': {
      if (
        typeof candidate.location !== 'object' ||
        candidate.location === null ||
        Array.isArray(candidate.location)
      ) {
        return { ok: false, error: 'Navigate message requires a non-null object "location"' };
      }
      const loc = candidate.location as Record<string, unknown>;
      if (
        typeof loc.line !== 'number' ||
        !Number.isInteger(loc.line) ||
        loc.line < 1 ||
        !Number.isFinite(loc.line)
      ) {
        return { ok: false, error: 'Location "line" must be a positive integer >= 1' };
      }

      let file: string | undefined;
      if (loc.file !== undefined) {
        if (typeof loc.file !== 'string' || loc.file.trim() === '') {
          return { ok: false, error: 'Location "file" must be a non-empty string when provided' };
        }
        file = loc.file;
      }

      let column: number | undefined;
      if (loc.column !== undefined) {
        if (
          typeof loc.column !== 'number' ||
          !Number.isInteger(loc.column) ||
          loc.column < 1 ||
          !Number.isFinite(loc.column)
        ) {
          return { ok: false, error: 'Location "column" must be a positive integer >= 1 when provided' };
        }
        column = loc.column;
      }

      const location: WebviewLocation = { line: loc.line };
      if (file !== undefined) {
        location.file = file;
      }
      if (column !== undefined) {
        location.column = column;
      }

      return {
        ok: true,
        message: {
          type: 'navigate',
          location,
        },
      };
    }
    default: {
      return { ok: false, error: `Unknown message type: "${candidate.type}"` };
    }
  }
}

export function isWebviewIncomingMessage(raw: unknown): raw is WebviewIncomingMessage {
  return validateWebviewMessage(raw).ok;
}
