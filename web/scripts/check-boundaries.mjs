import { readFileSync } from 'node:fs';
const source = readFileSync(new URL('../src/main.tsx', import.meta.url), 'utf8');
for (const required of ['AuthMiniProvider', 'LinkitProvider', 'custody execution disabled', 'Withdraw']) if (!source.includes(required)) throw new Error(`Missing required v1 boundary: ${required}`);
console.log('Midas web boundary check passed');
