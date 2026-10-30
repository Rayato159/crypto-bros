// Tauri reads its signing key itself; this script never opens signing material.
import {existsSync} from 'node:fs';
import {resolve} from 'node:path';
import {spawnSync} from 'node:child_process';
const key=resolve('.publisher/updater.key');
if(!existsSync(key))throw Error('Publisher key is not configured. Keep it private; never paste it into chat. See docs/RELEASING.md.');
process.env.TAURI_SIGNING_PRIVATE_KEY=key;
process.env.TAURI_SIGNING_PRIVATE_KEY_PASSWORD??='';
const result=spawnSync(process.execPath,['node_modules/@tauri-apps/cli/tauri.js','build',...process.argv.slice(2)],{stdio:'inherit',windowsHide:true});
if(result.error)throw Error('Could not start Tauri build');
process.exitCode=result.status??1;
