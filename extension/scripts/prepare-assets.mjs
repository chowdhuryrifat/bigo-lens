// Copies build-time assets into the extension: the protocol schemas (so the
// packaged extension validates engine responses with the exact contract) and
// the repository licence (required in the .vsix).
import { copyFileSync, mkdirSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const extensionRoot = join(dirname(fileURLToPath(import.meta.url)), '..');
const repoRoot = join(extensionRoot, '..');
const target = join(extensionRoot, 'schemas');
mkdirSync(target, { recursive: true });
for (const name of ['request.schema.json', 'response.schema.json']) {
  copyFileSync(join(repoRoot, 'shared', 'schemas', name), join(target, name));
}
copyFileSync(join(repoRoot, 'LICENSE'), join(extensionRoot, 'LICENSE'));
