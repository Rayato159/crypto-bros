import { spawnSync } from 'node:child_process';
import { cpSync, existsSync, lstatSync, mkdirSync, readdirSync, realpathSync, rmSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = realpathSync(join(dirname(fileURLToPath(import.meta.url)), '..'));
function run(command, args) {
  const result = spawnSync(command, args, { cwd: root, stdio: 'inherit', windowsHide: true });
  if (result.error) throw result.error;
  if (result.status !== 0) throw new Error(`${command} failed (${result.status ?? result.signal})`);
}

run('cargo', ['build', '--release', '-p', 'crypto-bros', '--locked']);
const worker = process.platform === 'win32' ? 'crypto-bros.exe' : 'crypto-bros';
mkdirSync(join(root, 'src-tauri', 'binaries'), { recursive: true });
cpSync(join(root, 'target', 'release', worker), join(root, 'src-tauri', 'binaries', worker));
const output = join(root, 'target', 'dx', 'crypto-bros-ui', 'release', 'web', 'public');
function cleanGenerated(path) {
  if (!existsSync(path)) return;
  if (realpathSync(path) !== path) throw new Error('Refusing cleanup through a linked generated directory');
  function checkLinks(dir) {
    if (lstatSync(dir).isSymbolicLink()) throw new Error('Refusing linked generated content');
    for (const entry of readdirSync(dir, { withFileTypes: true })) {
      if (entry.isSymbolicLink()) throw new Error('Refusing linked generated content');
      if (entry.isDirectory()) checkLinks(join(dir, entry.name));
    }
  }
  checkLinks(path);
  rmSync(path, { recursive: true });
}
// Dioxus retains previous hashed assets between builds. Never bundle those stale copies.
cleanGenerated(output);
run('dx', ['build', '--platform', 'web', '--release', '--package', 'crypto-bros-ui']);
if (!existsSync(join(output, 'index.html'))) throw new Error('Dioxus frontend output is missing');
const dist = join(root, 'dist');
cleanGenerated(dist);
cpSync(output, dist, { recursive: true });
