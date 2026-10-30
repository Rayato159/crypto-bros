import { build } from 'esbuild';
import { join } from 'node:path';
const output = process.argv[2];
if (!output) throw new Error('Cargo output directory is required');
await build({
  entryPoints: ['src/live-wallet.js'],
  outfile: join(output, 'live-wallet.js'),
  bundle: true, platform: 'browser', format: 'iife', target: ['es2022'],
  minify: true, legalComments: 'inline', sourcemap: false,
});
