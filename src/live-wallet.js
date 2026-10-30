import { VersionedTransaction } from '@solana/web3.js';

const ticket = location.hash.slice(1);
history.replaceState(null, '', location.pathname);
const status = document.querySelector('#status');
const button = document.querySelector('#sign');
let preview;
let submitting = false;
function message(text) { status.textContent = text; }
function amount(value, decimals) { return (Number(value) / 10 ** decimals).toLocaleString(undefined, { maximumFractionDigits: decimals }); }
async function call(path, body) {
  const response = await fetch(path, { method: 'POST', cache: 'no-store', credentials: 'omit',
    headers: { 'content-type': 'application/json' }, body: JSON.stringify({ ticket, ...body }) });
  const result = await response.json();
  if (!response.ok) throw new Error(result.error || 'The local worker rejected this request.');
  return result;
}
async function load() {
  if (!ticket) throw new Error('Open a fresh approval link from the installed Crypto Bros app.');
  preview = await call('/live/preview', {});
  const o = preview.order;
  document.querySelector('#wallet').textContent = preview.address;
  document.querySelector('#direction').textContent = o.side === 'buy' ? 'Buy SOL with USDC' : 'Sell SOL for USDC';
  document.querySelector('#spend').textContent = `${amount(o.input, o.side === 'buy' ? 6 : 9)} ${o.side === 'buy' ? 'USDC' : 'SOL'}`;
  document.querySelector('#receive').textContent = `${amount(o.minimum_output, o.side === 'buy' ? 9 : 6)} ${o.side === 'buy' ? 'SOL' : 'USDC'} minimum`;
  document.querySelector('#fee').textContent = `${amount(o.network_fee, 9)} SOL estimated network fee`;
  document.querySelector('#rent').textContent = `${amount(o.rent_delta, 9)} SOL account-rent change (separate from trading fees)`;
  document.querySelector('#expiry').textContent = new Date(o.expires_at * 1000).toLocaleTimeString();
  document.querySelector('#review').hidden = false;
  button.disabled = false;
  message('Check the amounts and wallet. Signing authorizes a real mainnet trade.');
  setInterval(() => {
    if (Date.now() >= o.expires_at * 1000 && !submitting) {
      button.disabled = true;
      message('Quote expired. Cancel it in the app and prepare a fresh order.');
    }
  }, 1000);
}
button.addEventListener('click', async () => {
  if (submitting || !preview) return;
  submitting = true; button.disabled = true;
  let signed = false;
  try {
    const provider = window.phantom?.solana;
    if (!provider?.isPhantom) throw new Error('Open this page in the Chrome profile where Phantom is installed.');
    const connected = await provider.connect();
    if (connected.publicKey.toString() !== preview.address) throw new Error('Phantom selected a different wallet. Choose the dedicated bot wallet and reopen this approval link.');
    // Refresh the ticket before asking the wallet to sign. No connection permission grants trading authority.
    const current = await call('/live/preview', {});
    if (current.order.id !== preview.order.id) throw new Error('Order changed. Open a new approval link.');
    const bytes = Uint8Array.from(atob(current.order.unsigned_transaction), c => c.charCodeAt(0));
    const transaction = VersionedTransaction.deserialize(bytes);
    message('Review and confirm this transaction inside Phantom.');
    const approved = await provider.signTransaction(transaction);
    signed = true;
    if (provider.publicKey?.toString() !== preview.address) throw new Error('Wallet changed. Transaction was not sent by this page.');
    const encoded = btoa(String.fromCharCode(...approved.serialize()));
    message('Sending the signed order. Keep the app open while it confirms the chain result.');
    const result = await call('/live/submit', { signed_transaction: encoded });
    message(result.message);
    document.querySelector('#signature').textContent = result.signature;
    button.textContent = 'Return to Crypto Bros to check confirmation';
  } catch (error) {
    // Never display wallet/provider objects or transaction contents in diagnostics.
    message(signed
      ? 'Signing completed, but submission was not confirmed here. Check Pending order in Crypto Bros and reconcile before trying another order.'
      : (error instanceof Error ? error.message : 'Phantom did not approve this transaction.'));
    if (!signed) { submitting = false; button.disabled = Date.now() >= preview.order.expires_at * 1000; }
  }
});
load().catch(error => message(error instanceof Error ? error.message : 'Cannot load this approval link.'));
