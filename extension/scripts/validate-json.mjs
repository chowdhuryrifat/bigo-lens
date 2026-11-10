// Validates the protocol schemas themselves and every example under
// shared/examples against them. Exit status 1 on any failure.
import Ajv2020 from 'ajv/dist/2020.js';
import { readdirSync, readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const repoRoot = join(dirname(fileURLToPath(import.meta.url)), '..', '..');
const schemaDir = join(repoRoot, 'shared', 'schemas');
const exampleDir = join(repoRoot, 'shared', 'examples');

const ajv = new Ajv2020({ allErrors: true, strict: true, allowUnionTypes: true });
const load = (path) => JSON.parse(readFileSync(path, 'utf8'));
const validateRequest = ajv.compile(load(join(schemaDir, 'request.schema.json')));
const validateResponse = ajv.compile(load(join(schemaDir, 'response.schema.json')));

let failures = 0;
let checked = 0;
for (const name of readdirSync(exampleDir).sort()) {
  const path = join(exampleDir, name);
  if (name.endsWith('.txt')) {
    // Deliberately malformed input for negative tests: must not parse.
    let parsed = true;
    try {
      JSON.parse(readFileSync(path, 'utf8'));
    } catch {
      parsed = false;
    }
    if (parsed) {
      console.error(`FAIL ${name}: expected malformed JSON`);
      failures++;
    }
    checked++;
    continue;
  }
  if (!name.endsWith('.json')) continue;
  const document = load(path);
  const isRequest = name.endsWith('.request.json');
  const validate = isRequest ? validateRequest : validateResponse;
  const valid = validate(document);
  const expectValid = !name.startsWith('invalid.');
  if (valid !== expectValid) {
    console.error(`FAIL ${name}: expected ${expectValid ? 'valid' : 'invalid'}`, validate.errors ?? '');
    failures++;
  } else {
    console.log(`ok   ${name}`);
  }
  checked++;
}
if (checked === 0) {
  console.error('no examples found');
  process.exit(1);
}
console.log(`${checked} file(s) checked, ${failures} failure(s)`);
process.exit(failures === 0 ? 0 : 1);
