import { test } from 'node:test';
import assert from 'node:assert/strict';
import vm from 'node:vm';
import { webcrypto } from 'node:crypto';
import { build } from 'esbuild';

const bundle = await build({ entryPoints: ['src/live-wallet.js'], bundle: true, platform: 'browser', format: 'iife', write: false });
const source = bundle.outputFiles[0].text;
// Synthetic, unfunded public account and unsigned packet. No real wallet or network is involved.
const address = '11111111111111111111111111111111';
const message = [128, 1, 0, 1, 2, ...new Uint8Array(32), ...new Uint8Array(32).fill(2),
  ...new Uint8Array(32).fill(9), 1, 1, 0, 8, ...new Uint8Array(8), 0];
const packet = Uint8Array.from([1, ...new Uint8Array(64), ...message]);
const encoded = Buffer.from(packet).toString('base64');
async function page({ wrongWallet = false, interrupted = false } = {}) {
  const elements = new Map();
  function element(id) {
    if (!elements.has(id)) elements.set(id, { disabled: true, hidden: true, textContent: '', addEventListener(name, fn) { this[name] = fn; } });
    return elements.get(id);
  }
  const requests = []; let signatures = 0;
  const preview = { address, order: { id: 'synthetic', side: 'buy', input: 1_000_000, minimum_output: 9_000_000,
    network_fee: 5_000, rent_delta: 0, expires_at: Math.floor(Date.now() / 1000) + 60, unsigned_transaction: encoded } };
  const publicKey = { toString: () => wrongWallet ? 'different-public-wallet' : address };
  const sandbox = { Uint8Array, ArrayBuffer, TextEncoder, TextDecoder, URL, crypto: webcrypto,
    setTimeout, clearTimeout, setInterval() {}, atob, btoa,
    location: { hash: '#isolated-test-ticket', pathname: '/live' }, history: { replaceState() {} },
    document: { querySelector: element },
    window: { phantom: { solana: { isPhantom: true, publicKey, async connect() { return { publicKey }; },
      async signTransaction(transaction) { signatures++; assert.deepEqual(Array.from(transaction.serialize()), Array.from(packet)); return transaction; } } } },
    async fetch(path, init) {
      requests.push(path);
      assert.equal(init.method, 'POST');
      const body = JSON.parse(init.body);
      assert.equal(body.ticket, 'isolated-test-ticket');
      if (path === '/live/submit') {
        assert.equal(body.signed_transaction, encoded);
        if (interrupted) throw new Error('synthetic interrupted response');
        return { ok: true, async json() { return { signature: 'synthetic-public-signature', message: 'Pending confirmation' }; } };
      }
      assert.equal(path, '/live/preview');
      return { ok: true, async json() { return preview; } };
    },
  };
  vm.runInNewContext(source, sandbox, { timeout: 5000 });
  await new Promise(resolve => setImmediate(resolve));
  return { element, requests, signatureCount: () => signatures };
}
test('wallet connection never signs until the explicit approval click; serialized message stays unchanged', async () => {
  const p = await page();
  assert.equal(p.signatureCount(), 0);
  assert.equal(p.element('#sign').disabled, false);
  await p.element('#sign').click();
  assert.equal(p.signatureCount(), 1);
  assert.equal(p.requests.filter(p => p === '/live/submit').length, 1);
  await p.element('#sign').click();
  assert.equal(p.signatureCount(), 1);
});
test('a different Phantom wallet cannot sign the prepared order', async () => {
  const p = await page({ wrongWallet: true });
  await p.element('#sign').click();
  assert.equal(p.signatureCount(), 0);
  assert.equal(p.requests.includes('/live/submit'), false);
  assert.match(p.element('#status').textContent, /different wallet/);
});
test('an interrupted submission remains disabled rather than signing or sending twice', async () => {
  const p = await page({ interrupted: true });
  await p.element('#sign').click();
  await p.element('#sign').click();
  assert.equal(p.signatureCount(), 1);
  assert.equal(p.requests.filter(p => p === '/live/submit').length, 1);
  assert.equal(p.element('#sign').disabled, true);
  assert.match(p.element('#status').textContent, /reconcile/);
});
