// Stand-in engine for extension tests. Behaviour is chosen by FAKE_ENGINE_MODE.
import { readFileSync } from 'node:fs';

const mode = process.env.FAKE_ENGINE_MODE ?? 'ok';
const input = readFileSync(0, 'utf8');
const request = JSON.parse(input);
const response = {
  schema_version: '1.0',
  request_id: request.request_id,
  status: 'ok',
  engine: { name: 'bigolens-engine', version: '0.0.0-fake', protocol_version: '1.0', frontend: null },
  results: { actions: ['capabilities'], features: { benchmark: 'not_implemented' }, stl_cost_model: [] },
  diagnostics: [],
  errors: [],
};

switch (mode) {
  case 'ok':
    process.stdout.write(`${JSON.stringify(response)}\n`);
    break;
  case 'garbage':
    process.stdout.write('this is not json');
    break;
  case 'schema-violation':
    process.stdout.write(JSON.stringify({ ...response, status: 'maybe' }));
    break;
  case 'wrong-id':
    process.stdout.write(JSON.stringify({ ...response, request_id: 'someone-else' }));
    break;
  case 'exit-1':
    process.stderr.write('fatal: simulated failure\n');
    process.exit(1);
    break;
  case 'hang':
    setInterval(() => {}, 1000);
    break;
  default:
    process.stderr.write(`unknown FAKE_ENGINE_MODE ${mode}\n`);
    process.exit(3);
}
