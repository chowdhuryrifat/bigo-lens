import { spawn } from 'node:child_process';

import type { EngineRequest, EngineResponse } from '../models/protocol';
import type { SchemaValidator } from './schemaValidator';

/** Upper bound on engine output accepted by the extension. */
export const MAX_RESPONSE_BYTES = 16 * 1024 * 1024;

export class EngineError extends Error {
  constructor(
    message: string,
    readonly kind: 'spawn' | 'timeout' | 'exit' | 'output' | 'protocol' | 'cancelled',
    readonly stderr = '',
  ) {
    super(message);
    this.name = 'EngineError';
  }
}

export interface EngineClientOptions {
  enginePath: string;
  /**
   * Fixed arguments placed before nothing else; the production engine takes
   * none. Used by tests to run a script engine via the Node executable.
   */
  launcherArgs?: readonly string[];
  timeoutMs: number;
  validator: SchemaValidator;
}

/**
 * Runs one engine process per request. The executable is spawned directly
 * (never through a shell) with no arguments; the request travels on stdin.
 */
export class EngineClient {
  constructor(private readonly options: EngineClientOptions) {}

  async send(request: EngineRequest, signal?: AbortSignal): Promise<EngineResponse> {
    const checked = this.options.validator.validateRequest(request);
    if (!checked.value) {
      throw new EngineError(`refusing to send an invalid request: ${checked.errors.join('; ')}`, 'protocol');
    }
    const raw = await this.run(JSON.stringify(request), signal);
    let document: unknown;
    try {
      document = JSON.parse(raw);
    } catch {
      throw new EngineError('engine output is not valid JSON', 'output');
    }
    const validated = this.options.validator.validateResponse(document);
    if (!validated.value) {
      throw new EngineError(
        `engine response violates the protocol schema: ${validated.errors.slice(0, 5).join('; ')}`,
        'protocol',
      );
    }
    if (validated.value.request_id !== null && validated.value.request_id !== request.request_id) {
      throw new EngineError('engine response does not match the request id', 'protocol');
    }
    return validated.value;
  }

  private run(input: string, signal?: AbortSignal): Promise<string> {
    return new Promise((resolve, reject) => {
      if (signal?.aborted) {
        reject(new EngineError('analysis cancelled', 'cancelled'));
        return;
      }
      const child = spawn(this.options.enginePath, [...(this.options.launcherArgs ?? [])], {
        shell: false,
        windowsHide: true,
        stdio: ['pipe', 'pipe', 'pipe'],
      });
      const stdout: Buffer[] = [];
      let stdoutBytes = 0;
      let stderr = '';
      let settled = false;

      const finish = (error: EngineError | undefined, output?: string): void => {
        if (settled) {
          return;
        }
        settled = true;
        clearTimeout(timer);
        signal?.removeEventListener('abort', onAbort);
        if (error) {
          child.kill('SIGKILL');
          reject(error);
        } else {
          resolve(output ?? '');
        }
      };
      const onAbort = (): void => {
        finish(new EngineError('analysis cancelled', 'cancelled', stderr));
      };
      const timer = setTimeout(() => {
        finish(
          new EngineError(`engine did not respond within ${this.options.timeoutMs} ms`, 'timeout', stderr),
        );
      }, this.options.timeoutMs);
      signal?.addEventListener('abort', onAbort);

      child.on('error', (err) => {
        finish(new EngineError(`could not start the engine: ${err.message}`, 'spawn'));
      });
      child.stdout.on('data', (chunk: Buffer) => {
        stdoutBytes += chunk.length;
        if (stdoutBytes > MAX_RESPONSE_BYTES) {
          finish(new EngineError('engine output exceeds the size limit', 'output', stderr));
          return;
        }
        stdout.push(chunk);
      });
      child.stderr.on('data', (chunk: Buffer) => {
        if (stderr.length < 64 * 1024) {
          stderr += chunk.toString('utf8');
        }
      });
      child.on('close', (code) => {
        if (code !== 0) {
          finish(new EngineError(`engine exited with status ${String(code)}`, 'exit', stderr));
          return;
        }
        finish(undefined, Buffer.concat(stdout).toString('utf8'));
      });
      child.stdin.on('error', () => {
        // The process exited before reading its input; reported via 'close'.
      });
      child.stdin.end(input, 'utf8');
    });
  }
}
