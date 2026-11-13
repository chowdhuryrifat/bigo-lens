import { accessSync, chmodSync, constants, statSync } from 'node:fs';
import { isAbsolute, join } from 'node:path';

export interface LocatorEnvironment {
  /** Value of the bigoLens.enginePath setting. */
  configuredPath: string;
  /** Value of the BIGOLENS_ENGINE_PATH environment variable, if any. */
  environmentPath: string | undefined;
  /** Root directory of the installed extension. */
  extensionRoot: string;
  platform: NodeJS.Platform;
  /** Injectable for tests. */
  isExecutable?: (path: string) => boolean;
  /** Whether the active workspace is trusted (defaults to true). */
  isWorkspaceTrusted?: boolean;
  /** Whether the configured path originated from workspace or folder settings. */
  isWorkspaceConfiguredPath?: boolean;
}

export interface LocatorResult {
  path?: string;
  /** Every candidate considered, in order, for error messages. */
  searched: string[];
  /** Set when an explicitly configured path is unusable. */
  problem?: string;
}

const BUILD_PRESETS = ['dev', 'release', 'ci', 'no-clang'];
const CONFIGURATIONS = ['', 'Debug', 'Release'];

export function defaultIsExecutable(path: string): boolean {
  try {
    if (!statSync(path).isFile()) {
      return false;
    }
    accessSync(path, process.platform === 'win32' ? constants.F_OK : constants.X_OK);
    return true;
  } catch {
    return false;
  }
}

/** Path of the engine shipped inside the extension package. */
export function bundledEnginePath(extensionRoot: string, platform: NodeJS.Platform): string {
  return join(extensionRoot, 'bin', platform === 'win32' ? 'bigolens-engine.exe' : 'bigolens-engine');
}

/**
 * Archive extraction does not always preserve the executable bit. Restores it
 * on the bundled engine only (never on user-configured paths). Returns true if
 * the file is executable afterwards.
 */
export function ensureBundledEngineExecutable(extensionRoot: string, platform: NodeJS.Platform): boolean {
  const path = bundledEnginePath(extensionRoot, platform);
  try {
    if (!statSync(path).isFile()) {
      return false;
    }
    if (platform !== 'win32') {
      try {
        accessSync(path, constants.X_OK);
      } catch {
        chmodSync(path, 0o755);
      }
    }
    return defaultIsExecutable(path);
  } catch {
    return false;
  }
}

/**
 * Resolves the engine executable. Explicit configuration wins and is never
 * silently replaced by a fallback: an unusable configured path is an error.
 */
export function locateEngine(env: LocatorEnvironment): LocatorResult {
  const isExecutable = env.isExecutable ?? defaultIsExecutable;
  const binary = env.platform === 'win32' ? 'bigolens-engine.exe' : 'bigolens-engine';
  const searched: string[] = [];

  const isTrusted = env.isWorkspaceTrusted ?? true;
  const isWorkspaceConfig = env.isWorkspaceConfiguredPath ?? false;
  const ignoreWorkspaceConfig = !isTrusted && isWorkspaceConfig;

  if (ignoreWorkspaceConfig && env.configuredPath.trim() !== '') {
    searched.push(`${env.configuredPath.trim()} (ignored: workspace-level setting in untrusted workspace)`);
  }

  for (const [label, value] of [
    ['setting bigoLens.enginePath', ignoreWorkspaceConfig ? '' : env.configuredPath.trim()],
    ['environment variable BIGOLENS_ENGINE_PATH', (env.environmentPath ?? '').trim()],
  ] as const) {
    if (value === '') {
      continue;
    }
    searched.push(value);
    if (!isAbsolute(value)) {
      return { searched, problem: `${label} must be an absolute path (got '${value}')` };
    }
    if (!isExecutable(value)) {
      return { searched, problem: `${label} does not point to an executable file: ${value}` };
    }
    return { path: value, searched };
  }

  const candidates = [bundledEnginePath(env.extensionRoot, env.platform)];
  // Development layout: <repo>/extension is the extension root, builds live in <repo>/build/<preset>.
  const repoRoot = join(env.extensionRoot, '..');
  for (const preset of BUILD_PRESETS) {
    for (const configuration of CONFIGURATIONS) {
      candidates.push(join(repoRoot, 'build', preset, 'engine', configuration, binary));
    }
  }
  for (const candidate of candidates) {
    searched.push(candidate);
    if (isExecutable(candidate)) {
      return { path: candidate, searched };
    }
  }
  return { searched };
}
