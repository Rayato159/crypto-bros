import { spawnSync } from 'node:child_process';
import { cpSync, existsSync, lstatSync, mkdirSync, realpathSync, rmSync } from 'node:fs';
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
run('dx', ['build', '--platform', 'web', '--release', '--package', 'crypto-bros-ui']);

const output = join(root, 'target', 'dx', 'crypto-bros-ui', 'release', 'web', 'public');
if (!existsSync(join(output, 'index.html'))) throw new Error('Dioxus frontend output is missing');
const dist = join(root, 'dist');
if (existsSync(dist)) {
  if (lstatSync(dist).isSymbolicLink() || realpathSync(dist) !== dist) {
    throw new Error('Refusing cleanup outside the generated dist directory');
  }
  rmSync(dist, { recursive: true });
}
cpSync(output, dist, { recursive: true });
